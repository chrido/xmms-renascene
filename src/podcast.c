#include "xmms.h"
#include <gio/gio.h>
#include <glib/gstdio.h>
#include <gst/pbutils/pbutils.h>
#include <libsoup/soup.h>
#include <libxml/parser.h>
#include <libxml/tree.h>

#define PODCAST_FETCH_LIMIT (5 * 1024 * 1024)
#define PODCAST_BUFFER_SIZE 8192

typedef struct {
    gchar *url;
    gchar *title;
    gchar *feed;
    gchar *guid;
} PodcastItem;

typedef struct {
    gchar *url;
    GPtrArray *items;
    gboolean add_as_stream;
} PodcastUrlResult;

typedef struct {
    gchar *url;
    gchar *cache_path;
} PodcastDownloadJob;

typedef struct {
    gchar *url;
    gchar *cache_path;
    gint64 length;
} PodcastDownloadResult;

static GHashTable *downloads = NULL;
static GHashTable *refreshes = NULL;
static guint refresh_timeout_id = 0;

static gboolean podcast_content_type_is_xml(const gchar *content_type);
static GByteArray *podcast_read_limited(GInputStream *stream);
static gboolean podcast_bytes_look_like_feed(GByteArray *bytes);

static void
podcast_item_free(gpointer data)
{
    PodcastItem *item = data;
    if (!item)
        return;
    g_free(item->url);
    g_free(item->title);
    g_free(item->feed);
    g_free(item->guid);
    g_free(item);
}

static void
podcast_url_result_free(PodcastUrlResult *result)
{
    if (!result)
        return;
    g_free(result->url);
    if (result->items)
        g_ptr_array_free(result->items, TRUE);
    g_free(result);
}

static gchar *
podcast_cache_dir(void)
{
    gchar *config_dir = xmms_get_config_dir();
    gchar *dir = g_build_filename(config_dir, "podcast-cache", NULL);
    g_free(config_dir);
    return dir;
}

static gchar *
podcast_cache_path_for_url(const gchar *url)
{
    gchar *hash = g_compute_checksum_for_string(G_CHECKSUM_SHA256, url, -1);
    gchar *dir = podcast_cache_dir();
    gchar *path = g_build_filename(dir, hash, NULL);
    g_free(dir);
    g_free(hash);
    return path;
}

static gchar *
podcast_file_uri_for_cache(const gchar *path)
{
    return g_filename_to_uri(path, NULL, NULL);
}

gboolean
podcast_cache_is_fresh_for_url(const gchar *url)
{
    if (!url)
        return FALSE;

    gchar *path = podcast_cache_path_for_url(url);
    GStatBuf st;
    gboolean fresh = FALSE;
    if (g_stat(path, &st) == 0 && st.st_size > 0) {
        gint ttl = cfg.podcast_cache_ttl_days > 0 ?
            cfg.podcast_cache_ttl_days : 60;
        gint64 age = g_get_real_time() / G_USEC_PER_SEC - st.st_mtime;
        fresh = age <= (gint64)ttl * 24 * 60 * 60;
    }
    g_free(path);
    return fresh;
}

static gchar *
podcast_resolve_url(const gchar *base, const gchar *url)
{
    if (!url || !url[0])
        return NULL;
    if (g_str_has_prefix(url, "http://") || g_str_has_prefix(url, "https://"))
        return g_strdup(url);
    return g_uri_resolve_relative(base, url, G_URI_FLAGS_NONE, NULL);
}

static gboolean
xml_name_is(xmlNode *node, const gchar *name)
{
    return node && node->type == XML_ELEMENT_NODE &&
        g_ascii_strcasecmp((const gchar *)node->name, name) == 0;
}

static gchar *
xml_child_text(xmlNode *node, const gchar *name)
{
    for (xmlNode *child = node ? node->children : NULL; child; child = child->next) {
        if (xml_name_is(child, name)) {
            xmlChar *content = xmlNodeGetContent(child);
            gchar *text = content ? g_strdup((const gchar *)content) : NULL;
            if (content)
                xmlFree(content);
            if (text)
                g_strstrip(text);
            return text;
        }
    }
    return NULL;
}

static gchar *
xml_prop(xmlNode *node, const gchar *name)
{
    xmlChar *value = xmlGetProp(node, (const xmlChar *)name);
    gchar *copy = value ? g_strdup((const gchar *)value) : NULL;
    if (value)
        xmlFree(value);
    return copy;
}

static gchar *
podcast_find_enclosure(xmlNode *item, const gchar *feed_url)
{
    for (xmlNode *child = item ? item->children : NULL; child; child = child->next) {
        if (!child || child->type != XML_ELEMENT_NODE)
            continue;

        if (xml_name_is(child, "enclosure")) {
            gchar *url = xml_prop(child, "url");
            gchar *resolved = podcast_resolve_url(feed_url, url);
            g_free(url);
            if (resolved)
                return resolved;
        } else if (xml_name_is(child, "content")) {
            gchar *url = xml_prop(child, "url");
            gchar *resolved = podcast_resolve_url(feed_url, url);
            g_free(url);
            if (resolved)
                return resolved;
        } else if (xml_name_is(child, "link")) {
            gchar *rel = xml_prop(child, "rel");
            if (rel && g_ascii_strcasecmp(rel, "enclosure") == 0) {
                gchar *href = xml_prop(child, "href");
                gchar *resolved = podcast_resolve_url(feed_url, href);
                g_free(href);
                g_free(rel);
                if (resolved)
                    return resolved;
            }
            g_free(rel);
        }
    }
    return NULL;
}

static void
podcast_parse_item(xmlNode *node, const gchar *feed_url, GPtrArray *items)
{
    gchar *url = podcast_find_enclosure(node, feed_url);
    if (!url)
        return;

    PodcastItem *item = g_new0(PodcastItem, 1);
    item->url = url;
    item->title = xml_child_text(node, "title");
    item->guid = xml_child_text(node, "guid");
    if (!item->guid)
        item->guid = xml_child_text(node, "id");
    item->feed = g_strdup(feed_url);
    if (!item->title || !item->title[0]) {
        g_free(item->title);
        item->title = g_strdup(url);
    }
    g_ptr_array_add(items, item);
}

static void
podcast_collect_items(xmlNode *node, const gchar *feed_url, GPtrArray *items)
{
    for (xmlNode *child = node; child; child = child->next) {
        if (xml_name_is(child, "item") || xml_name_is(child, "entry"))
            podcast_parse_item(child, feed_url, items);
        podcast_collect_items(child->children, feed_url, items);
    }
}

static GPtrArray *
podcast_parse_feed(const guint8 *data, gsize len, const gchar *feed_url)
{
    xmlDoc *doc = xmlReadMemory((const char *)data, (int)len, feed_url, NULL,
                                XML_PARSE_RECOVER | XML_PARSE_NOERROR |
                                XML_PARSE_NOWARNING | XML_PARSE_NONET);
    if (!doc)
        return NULL;

    GPtrArray *items = g_ptr_array_new_with_free_func(podcast_item_free);
    podcast_collect_items(xmlDocGetRootElement(doc), feed_url, items);
    xmlFreeDoc(doc);

    if (items->len == 0) {
        g_ptr_array_free(items, TRUE);
        return NULL;
    }
    return items;
}

static GPtrArray *
podcast_fetch_feed_items(const gchar *url)
{
    SoupSession *session = soup_session_new_with_options("timeout", 12, NULL);
    SoupMessage *msg = soup_message_new("GET", url);
    SoupMessageHeaders *headers = soup_message_get_request_headers(msg);
    soup_message_headers_replace(headers, "User-Agent", "XMMS Resuscitated");
    soup_message_headers_replace(headers, "Accept",
                                 "application/rss+xml, application/atom+xml, application/xml, text/xml, */*");

    GError *error = NULL;
    GInputStream *stream = soup_session_send(session, msg, NULL, &error);
    guint status = soup_message_get_status(msg);
    GPtrArray *items = NULL;

    if (stream && status >= 200 && status < 300) {
        SoupMessageHeaders *response_headers =
            soup_message_get_response_headers(msg);
        const gchar *content_type =
            soup_message_headers_get_content_type(response_headers, NULL);
        GByteArray *bytes = podcast_read_limited(stream);
        if (podcast_content_type_is_xml(content_type) ||
            podcast_bytes_look_like_feed(bytes))
            items = podcast_parse_feed(bytes->data, bytes->len, url);
        g_byte_array_free(bytes, TRUE);
    }

    if (stream)
        g_object_unref(stream);
    g_clear_error(&error);
    g_object_unref(msg);
    g_object_unref(session);
    return items;
}

static gboolean
podcast_content_type_is_audio(const gchar *content_type)
{
    return content_type &&
        (g_str_has_prefix(content_type, "audio/") ||
         g_str_has_prefix(content_type, "video/") ||
         g_strrstr(content_type, "ogg") ||
         g_strrstr(content_type, "mpegurl") ||
         g_strrstr(content_type, "pls"));
}

static gboolean
podcast_content_type_is_xml(const gchar *content_type)
{
    return content_type &&
        (g_strrstr(content_type, "xml") ||
         g_strrstr(content_type, "rss") ||
         g_strrstr(content_type, "atom"));
}

static GByteArray *
podcast_read_limited(GInputStream *stream)
{
    GByteArray *bytes = g_byte_array_new();
    guint8 buffer[PODCAST_BUFFER_SIZE];
    GError *error = NULL;

    while (bytes->len < PODCAST_FETCH_LIMIT) {
        gsize room = MIN((gsize)PODCAST_BUFFER_SIZE,
                         (gsize)PODCAST_FETCH_LIMIT - bytes->len);
        gssize n = g_input_stream_read(stream, buffer, room, NULL, &error);
        if (n <= 0)
            break;
        g_byte_array_append(bytes, buffer, (guint)n);
    }

    g_clear_error(&error);
    return bytes;
}

static gboolean
podcast_bytes_look_like_feed(GByteArray *bytes)
{
    if (!bytes || bytes->len == 0)
        return FALSE;
    gchar *prefix = g_strndup((const gchar *)bytes->data,
                              MIN(bytes->len, (guint)4096));
    gchar *folded = g_utf8_strdown(prefix, -1);
    gboolean looks = g_strrstr(folded, "<rss") || g_strrstr(folded, "<feed");
    g_free(folded);
    g_free(prefix);
    return looks;
}

static gboolean
podcast_url_result_cb(gpointer data)
{
    PodcastUrlResult *result = data;
    if (result->items && result->items->len > 0) {
        for (guint i = 0; i < result->items->len; i++) {
            PodcastItem *item = g_ptr_array_index(result->items, i);
            playlist_add_podcast_entry(item->url, item->title,
                                       item->feed, item->guid);
        }
    } else if (result->add_as_stream) {
        playlist_add_uri(result->url);
    }
    playlistwin_update();
    podcast_url_result_free(result);
    return G_SOURCE_REMOVE;
}

static gpointer
podcast_add_url_thread(gpointer data)
{
    gchar *url = data;
    PodcastUrlResult *result = g_new0(PodcastUrlResult, 1);
    result->url = g_strdup(url);

    SoupSession *session = soup_session_new_with_options("timeout", 12, NULL);
    SoupMessage *msg = soup_message_new("GET", url);
    SoupMessageHeaders *headers = soup_message_get_request_headers(msg);
    soup_message_headers_replace(headers, "User-Agent", "XMMS Resuscitated");
    soup_message_headers_replace(headers, "Accept",
                                 "application/rss+xml, application/atom+xml, application/xml, text/xml, audio/*, */*");

    GError *error = NULL;
    GInputStream *stream = soup_session_send(session, msg, NULL, &error);
    guint status = soup_message_get_status(msg);
    if (!stream || status < 200 || status >= 300) {
        result->add_as_stream = TRUE;
        g_clear_error(&error);
        goto done;
    }

    SoupMessageHeaders *response_headers = soup_message_get_response_headers(msg);
    const gchar *content_type =
        soup_message_headers_get_content_type(response_headers, NULL);
    if (podcast_content_type_is_audio(content_type) ||
        soup_message_headers_get_one(response_headers, "icy-name")) {
        result->add_as_stream = TRUE;
        goto done;
    }

    GByteArray *bytes = podcast_read_limited(stream);
    if (podcast_content_type_is_xml(content_type) ||
        podcast_bytes_look_like_feed(bytes)) {
        result->items = podcast_parse_feed(bytes->data, bytes->len, url);
        result->add_as_stream = result->items == NULL;
    } else {
        result->add_as_stream = TRUE;
    }
    g_byte_array_free(bytes, TRUE);

done:
    if (stream)
        g_object_unref(stream);
    g_object_unref(msg);
    g_object_unref(session);
    g_idle_add(podcast_url_result_cb, result);
    g_free(url);
    return NULL;
}

void
podcast_add_url(const gchar *url)
{
    if (!url || !url[0])
        return;
    GThread *thread = g_thread_new("podcast-add-url",
                                   podcast_add_url_thread, g_strdup(url));
    g_thread_unref(thread);
}

static gboolean
podcast_refresh_result_cb(gpointer data)
{
    PodcastUrlResult *result = data;
    if (refreshes)
        g_hash_table_remove(refreshes, result->url);
    if (result->items && result->items->len > 0) {
        for (guint i = 0; i < result->items->len; i++) {
            PodcastItem *item = g_ptr_array_index(result->items, i);
            playlist_add_podcast_entry(item->url, item->title,
                                       item->feed, item->guid);
        }
        playlistwin_update();
    }
    podcast_url_result_free(result);
    return G_SOURCE_REMOVE;
}

static gpointer
podcast_refresh_feed_thread(gpointer data)
{
    gchar *url = data;
    PodcastUrlResult *result = g_new0(PodcastUrlResult, 1);
    result->url = g_strdup(url);
    result->items = podcast_fetch_feed_items(url);
    g_idle_add(podcast_refresh_result_cb, result);
    g_free(url);
    return NULL;
}

static void
podcast_refresh_feed(const gchar *url)
{
    if (!url || !url[0])
        return;
    if (!refreshes)
        refreshes = g_hash_table_new_full(g_str_hash, g_str_equal,
                                          g_free, NULL);
    if (g_hash_table_contains(refreshes, url))
        return;
    g_hash_table_add(refreshes, g_strdup(url));
    GThread *thread = g_thread_new("podcast-refresh-feed",
                                   podcast_refresh_feed_thread, g_strdup(url));
    g_thread_unref(thread);
}

void
podcast_refresh_all_feeds(void)
{
    GList *feeds = playlist_get_podcast_feeds();
    for (GList *l = feeds; l; l = l->next)
        podcast_refresh_feed(l->data);
    g_list_free_full(feeds, g_free);
}

static gboolean podcast_refresh_timeout_cb(gpointer data);

static void
podcast_schedule_refresh_timer(void)
{
    if (refresh_timeout_id)
        g_source_remove(refresh_timeout_id);
    if (!refreshes)
        return;
    gint minutes = cfg.podcast_refresh_interval_minutes > 0 ?
        cfg.podcast_refresh_interval_minutes : 60;
    refresh_timeout_id = g_timeout_add_seconds((guint)minutes * 60,
                                               podcast_refresh_timeout_cb,
                                               NULL);
}

void
podcast_update_refresh_timer(void)
{
    podcast_schedule_refresh_timer();
}

static gboolean
podcast_refresh_timeout_cb(gpointer data)
{
    (void)data;
    refresh_timeout_id = 0;
    podcast_refresh_all_feeds();
    podcast_schedule_refresh_timer();
    return G_SOURCE_REMOVE;
}

static gint64
podcast_discover_length(const gchar *cache_path)
{
    gchar *uri = podcast_file_uri_for_cache(cache_path);
    if (!uri)
        return -1;

    GError *error = NULL;
    GstDiscoverer *discoverer = gst_discoverer_new(5 * GST_SECOND, &error);
    if (!discoverer) {
        g_clear_error(&error);
        g_free(uri);
        return -1;
    }

    GstDiscovererInfo *info =
        gst_discoverer_discover_uri(discoverer, uri, &error);
    gint64 length = -1;
    if (info) {
        GstClockTime duration = gst_discoverer_info_get_duration(info);
        if (GST_CLOCK_TIME_IS_VALID(duration) && duration > 0)
            length = (gint64)(duration / GST_MSECOND);
        gst_discoverer_info_unref(info);
    }
    g_clear_error(&error);
    g_object_unref(discoverer);
    g_free(uri);
    return length;
}

static gboolean
podcast_download_result_cb(gpointer data)
{
    PodcastDownloadResult *result = data;
    if (downloads)
        g_hash_table_remove(downloads, result->url);
    playlist_podcast_cache_ready(result->url, result->length);
    playlistwin_update();
    g_free(result->url);
    g_free(result->cache_path);
    g_free(result);
    return G_SOURCE_REMOVE;
}

static gpointer
podcast_download_thread(gpointer data)
{
    PodcastDownloadJob *job = data;
    PodcastDownloadResult *result = g_new0(PodcastDownloadResult, 1);
    result->url = g_strdup(job->url);
    result->cache_path = g_strdup(job->cache_path);
    result->length = -1;

    gchar *dir = podcast_cache_dir();
    g_mkdir_with_parents(dir, 0755);
    g_free(dir);

    gchar *tmp = g_strdup_printf("%s.part", job->cache_path);
    SoupSession *session = soup_session_new_with_options("timeout", 0, NULL);
    SoupMessage *msg = soup_message_new("GET", job->url);
    GError *error = NULL;
    GInputStream *stream = soup_session_send(session, msg, NULL, &error);
    guint status = soup_message_get_status(msg);
    FILE *file = NULL;

    if (!stream || status < 200 || status >= 300)
        goto done;

    file = g_fopen(tmp, "wb");
    if (!file)
        goto done;

    guint8 buffer[PODCAST_BUFFER_SIZE];
    while (TRUE) {
        gssize n = g_input_stream_read(stream, buffer, sizeof(buffer),
                                       NULL, &error);
        if (n <= 0)
            break;
        if (fwrite(buffer, 1, (size_t)n, file) != (size_t)n)
            break;
    }

    if (file) {
        fclose(file);
        file = NULL;
    }

    if (!error && g_rename(tmp, job->cache_path) == 0)
        result->length = podcast_discover_length(job->cache_path);

done:
    if (file)
        fclose(file);
    if (stream)
        g_object_unref(stream);
    g_clear_error(&error);
    g_object_unref(msg);
    g_object_unref(session);
    g_unlink(tmp);
    g_free(tmp);
    g_free(job->url);
    g_free(job->cache_path);
    g_free(job);
    g_idle_add(podcast_download_result_cb, result);
    return NULL;
}

static void
podcast_start_download(PlaylistEntry *entry)
{
    if (!entry || !entry->is_podcast || !entry->filename)
        return;
    if (podcast_cache_is_fresh_for_url(entry->filename))
        return;

    if (!downloads)
        downloads = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL);
    if (g_hash_table_contains(downloads, entry->filename))
        return;

    gchar *cache_path = podcast_cache_path_for_url(entry->filename);
    PodcastDownloadJob *job = g_new0(PodcastDownloadJob, 1);
    job->url = g_strdup(entry->filename);
    job->cache_path = g_strdup(cache_path);
    g_hash_table_add(downloads, g_strdup(entry->filename));
    entry->podcast_downloading = TRUE;

    GThread *thread = g_thread_new("podcast-download",
                                   podcast_download_thread, job);
    g_thread_unref(thread);
    g_free(cache_path);
}

gchar *
podcast_prepare_playback_uri(PlaylistEntry *entry)
{
    if (!entry || !entry->filename)
        return NULL;
    if (!entry->is_podcast)
        return g_strdup(entry->filename);

    if (podcast_cache_is_fresh_for_url(entry->filename)) {
        gchar *path = podcast_cache_path_for_url(entry->filename);
        gchar *uri = podcast_file_uri_for_cache(path);
        g_free(path);
        return uri ? uri : g_strdup(entry->filename);
    }

    podcast_start_download(entry);
    return g_strdup(entry->filename);
}

static gpointer
podcast_cleanup_thread(gpointer data)
{
    (void)data;
    gchar *dir = podcast_cache_dir();
    GDir *gdir = g_dir_open(dir, 0, NULL);
    if (gdir) {
        gint ttl = cfg.podcast_cache_ttl_days > 0 ?
            cfg.podcast_cache_ttl_days : 60;
        gint64 max_age = (gint64)ttl * 24 * 60 * 60;
        gint64 now = g_get_real_time() / G_USEC_PER_SEC;
        const gchar *name;
        while ((name = g_dir_read_name(gdir))) {
            if (g_str_has_suffix(name, ".part"))
                continue;
            gchar *path = g_build_filename(dir, name, NULL);
            GStatBuf st;
            if (g_stat(path, &st) == 0 && now - st.st_mtime > max_age)
                g_unlink(path);
            g_free(path);
        }
        g_dir_close(gdir);
    }
    g_free(dir);
    return NULL;
}

void
podcast_init(void)
{
    downloads = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL);
    refreshes = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, NULL);
    GThread *thread = g_thread_new("podcast-cache-cleanup",
                                   podcast_cleanup_thread, NULL);
    g_thread_unref(thread);
    podcast_refresh_all_feeds();
    podcast_schedule_refresh_timer();
}

void
podcast_shutdown(void)
{
    if (refresh_timeout_id) {
        g_source_remove(refresh_timeout_id);
        refresh_timeout_id = 0;
    }
    g_clear_pointer(&downloads, g_hash_table_destroy);
    g_clear_pointer(&refreshes, g_hash_table_destroy);
}
