#include "xmms.h"
#include <sys/stat.h>
#include <gst/pbutils/pbutils.h>

static GList *playlist = NULL;
static gint playlist_position = -1;
static GList *shuffle_list = NULL;
static gboolean shuffle = FALSE;
static gboolean repeat = FALSE;
static gboolean no_advance = FALSE;
static gboolean duration_index_running = FALSE;
static gboolean duration_index_rescan_requested = FALSE;

typedef struct {
    gint index;
    gchar *uri;
} DurationIndexItem;

typedef struct {
    GPtrArray *items;
} DurationIndexJob;

typedef struct {
    gint index;
    gchar *uri;
    gint64 length;
    gchar *title;
} DurationIndexResult;

static void playlist_index_item_free(gpointer data);
static void playlist_index_job_free(DurationIndexJob *job);
static gpointer playlist_duration_index_thread(gpointer data);
static gboolean playlist_duration_index_result_cb(gpointer data);
static gboolean playlist_duration_index_finished_cb(gpointer data);

static void
playlist_refresh_position(PlaylistEntry *current)
{
    g_list_free(shuffle_list);
    shuffle_list = NULL;

    if (!current) {
        playlist_position = playlist ? 0 : -1;
        return;
    }

    gint pos = g_list_index(playlist, current);
    playlist_position = pos >= 0 ? pos : (playlist ? 0 : -1);
}

static gchar *
entry_path_for_compare(PlaylistEntry *entry)
{
    if (!entry || !entry->filename)
        return g_strdup("");

    gchar *path = uri_to_filename(entry->filename);
    if (path)
        return path;
    return g_strdup(entry->filename);
}

static gint
playlist_sort_by_title_cmpfunc(gconstpointer a, gconstpointer b)
{
    const PlaylistEntry *ea = a;
    const PlaylistEntry *eb = b;
    const gchar *ta = (ea && ea->title) ? ea->title : "";
    const gchar *tb = (eb && eb->title) ? eb->title : "";
    return g_ascii_strcasecmp(ta, tb);
}

static void
playlist_index_item_free(gpointer data)
{
    DurationIndexItem *item = data;
    g_free(item->uri);
    g_free(item);
}

static void
playlist_index_job_free(DurationIndexJob *job)
{
    if (!job)
        return;
    g_ptr_array_free(job->items, TRUE);
    g_free(job);
}

static gboolean
playlist_duration_index_result_cb(gpointer data)
{
    DurationIndexResult *result = data;
    PlaylistEntry *entry = playlist_get_entry(result->index);

    gboolean changed = FALSE;
    if (entry && entry->filename &&
        g_strcmp0(entry->filename, result->uri) == 0) {
        if (result->length > 0 && entry->length != result->length) {
            entry->length = result->length;
            changed = TRUE;
        }
        if (result->title && result->title[0] &&
            g_strcmp0(entry->title, result->title) != 0) {
            g_free(entry->title);
            entry->title = g_strdup(result->title);
            changed = TRUE;
        }
    }

    if (changed)
        playlistwin_update();

    g_free(result->uri);
    g_free(result->title);
    g_free(result);
    return G_SOURCE_REMOVE;
}

static gchar *
playlist_title_from_tags(const GstTagList *tags)
{
    if (!tags)
        return NULL;

    gchar *artist = NULL;
    gchar *title = NULL;
    gst_tag_list_get_string(tags, GST_TAG_ARTIST, &artist);
    gst_tag_list_get_string(tags, GST_TAG_TITLE, &title);

    gchar *formatted = NULL;
    if (artist && artist[0] && title && title[0])
        formatted = g_strdup_printf("%s - %s", artist, title);
    else if (title && title[0])
        formatted = g_strdup(title);
    else if (artist && artist[0])
        formatted = g_strdup(artist);

    g_free(artist);
    g_free(title);
    return formatted;
}

static gboolean
playlist_duration_index_finished_cb(gpointer data)
{
    (void)data;
    duration_index_running = FALSE;

    if (duration_index_rescan_requested) {
        duration_index_rescan_requested = FALSE;
        playlist_index_missing_durations();
    } else {
        playlistwin_update();
    }

    return G_SOURCE_REMOVE;
}

static gpointer
playlist_duration_index_thread(gpointer data)
{
    DurationIndexJob *job = data;

    if (!gst_is_initialized())
        gst_init(NULL, NULL);

    GError *error = NULL;
    GstDiscoverer *discoverer = gst_discoverer_new(5 * GST_SECOND, &error);
    if (!discoverer) {
        g_warning("Could not create duration indexer: %s",
                  error ? error->message : "unknown error");
        g_clear_error(&error);
        playlist_index_job_free(job);
        g_idle_add(playlist_duration_index_finished_cb, NULL);
        return NULL;
    }

    for (guint i = 0; i < job->items->len; i++) {
        DurationIndexItem *item = g_ptr_array_index(job->items, i);
        GstDiscovererInfo *info =
            gst_discoverer_discover_uri(discoverer, item->uri, &error);
        if (!info) {
            g_clear_error(&error);
            continue;
        }

        GstClockTime duration = gst_discoverer_info_get_duration(info);
        const GstTagList *tags = gst_discoverer_info_get_tags(info);
        gchar *title = playlist_title_from_tags(tags);
        if ((GST_CLOCK_TIME_IS_VALID(duration) && duration > 0) || title) {
            DurationIndexResult *result = g_new0(DurationIndexResult, 1);
            result->index = item->index;
            result->uri = g_strdup(item->uri);
            result->length = (GST_CLOCK_TIME_IS_VALID(duration) && duration > 0) ?
                (gint64)(duration / GST_MSECOND) : -1;
            result->title = title;
            g_idle_add(playlist_duration_index_result_cb, result);
        } else {
            g_free(title);
        }

        gst_discoverer_info_unref(info);
    }

    g_object_unref(discoverer);
    playlist_index_job_free(job);
    g_idle_add(playlist_duration_index_finished_cb, NULL);
    return NULL;
}

static gint
playlist_sort_by_filename_cmpfunc(gconstpointer a, gconstpointer b)
{
    gchar *pa = entry_path_for_compare((PlaylistEntry *)a);
    gchar *pb = entry_path_for_compare((PlaylistEntry *)b);
    gchar *ba = g_path_get_basename(pa);
    gchar *bb = g_path_get_basename(pb);
    gint result = g_ascii_strcasecmp(ba, bb);

    g_free(ba);
    g_free(bb);
    g_free(pa);
    g_free(pb);
    return result;
}

static gint
playlist_sort_by_path_cmpfunc(gconstpointer a, gconstpointer b)
{
    gchar *pa = entry_path_for_compare((PlaylistEntry *)a);
    gchar *pb = entry_path_for_compare((PlaylistEntry *)b);
    gint result = g_ascii_strcasecmp(pa, pb);
    g_free(pa);
    g_free(pb);
    return result;
}

static gint
playlist_sort_by_date_cmpfunc(gconstpointer a, gconstpointer b)
{
    gchar *pa = entry_path_for_compare((PlaylistEntry *)a);
    gchar *pb = entry_path_for_compare((PlaylistEntry *)b);
    struct stat sa;
    struct stat sb;
    gboolean hasa = (stat(pa, &sa) == 0);
    gboolean hasb = (stat(pb, &sb) == 0);
    gint result;

    if (hasa && hasb) {
        if (sa.st_mtime == sb.st_mtime)
            result = 0;
        else
            result = sb.st_mtime > sa.st_mtime ? -1 : 1;
    } else if (hasa) {
        result = -1;
    } else if (hasb) {
        result = 1;
    } else {
        result = playlist_sort_by_filename_cmpfunc(a, b);
    }

    g_free(pa);
    g_free(pb);
    return result;
}

static void
playlist_sort_all(GCompareFunc cmpfunc)
{
    PlaylistEntry *current = playlist_get_entry(playlist_position);
    playlist = g_list_sort(playlist, cmpfunc);
    playlist_refresh_position(current);
}

static void
playlist_sort_selected(GCompareFunc cmpfunc)
{
    PlaylistEntry *current = playlist_get_entry(playlist_position);
    GList *selected = NULL;
    GArray *indices = g_array_new(FALSE, FALSE, sizeof(gint));

    for (GList *l = g_list_last(playlist); l; ) {
        GList *prev = l->prev;
        PlaylistEntry *entry = l->data;
        if (entry && entry->selected) {
            gint idx = g_list_position(playlist, l);
            g_array_prepend_val(indices, idx);
            playlist = g_list_remove_link(playlist, l);
            l->prev = NULL;
            l->next = NULL;
            selected = g_list_concat(l, selected);
        }
        l = prev;
    }

    selected = g_list_sort(selected, cmpfunc);

    GList *sel = selected;
    for (guint i = 0; i < indices->len && sel; i++, sel = sel->next) {
        gint idx = g_array_index(indices, gint, i);
        playlist = g_list_insert(playlist, sel->data, idx);
    }

    g_array_free(indices, TRUE);
    g_list_free(selected);
    playlist_refresh_position(current);
}

void
playlist_init(void)
{
    playlist = NULL;
    playlist_position = -1;
}

void
playlist_free(void)
{
    playlist_clear();
}

void
playlist_add(const gchar *filename)
{
    gchar *uri = filename_to_uri(filename);
    playlist_add_uri(uri);
    g_free(uri);
}

void
playlist_add_uri(const gchar *uri)
{
    PlaylistEntry *entry = g_new0(PlaylistEntry, 1);
    entry->filename = g_strdup(uri);
    entry->title = NULL;
    entry->length = -1;

    /* Extract title from filename */
    gchar *fn = uri_to_filename(uri);
    if (fn) {
        entry->title = format_title(fn, NULL);
        g_free(fn);
    } else {
        entry->title = g_strdup(uri);
    }

    playlist = g_list_append(playlist, entry);
    playlist_index_missing_durations();
}

void
playlist_add_url_checked(const gchar *url)
{
    if (!url || !url[0])
        return;
    if (g_str_has_prefix(url, "http://") || g_str_has_prefix(url, "https://"))
        podcast_add_url(url);
    else
        playlist_add_uri(url);
}

void
playlist_add_podcast_entry(const gchar *uri, const gchar *title,
                           const gchar *feed, const gchar *guid)
{
    if (!uri || !uri[0])
        return;

    for (GList *l = playlist; l; l = l->next) {
        PlaylistEntry *existing = l->data;
        gboolean same_guid = guid && guid[0] && existing->podcast_guid &&
            g_strcmp0(existing->podcast_guid, guid) == 0 &&
            g_strcmp0(existing->podcast_feed, feed) == 0;
        gboolean same_url = g_strcmp0(existing->filename, uri) == 0;
        if (!existing->is_podcast || (!same_guid && !same_url))
            continue;

        if (title && title[0]) {
            gboolean failed =
                existing->title && g_str_has_prefix(existing->title,
                                                    "failed: ");
            const gchar *current = failed ?
                existing->title + strlen("failed: ") : existing->title;
            if (g_strcmp0(current, title) != 0) {
                g_free(existing->title);
                existing->title = failed ?
                    g_strdup_printf("failed: %s", title) : g_strdup(title);
            }
        }
        if (feed && feed[0] && g_strcmp0(existing->podcast_feed, feed) != 0) {
            g_free(existing->podcast_feed);
            existing->podcast_feed = g_strdup(feed);
        }
        if (guid && guid[0] && g_strcmp0(existing->podcast_guid, guid) != 0) {
            g_free(existing->podcast_guid);
            existing->podcast_guid = g_strdup(guid);
        }
        return;
    }

    PlaylistEntry *entry = g_new0(PlaylistEntry, 1);
    entry->filename = g_strdup(uri);
    entry->title = title && title[0] ? g_strdup(title) : g_strdup(uri);
    entry->length = -1;
    entry->is_podcast = TRUE;
    entry->podcast_feed = g_strdup(feed);
    entry->podcast_guid = g_strdup(guid);

    if (podcast_cache_is_fresh_for_url(uri)) {
        gchar *play_uri = podcast_prepare_playback_uri(entry);
        if (play_uri && g_str_has_prefix(play_uri, "file://")) {
            entry->length = -1;
            entry->podcast_downloading = FALSE;
        }
        g_free(play_uri);
    }

    playlist = g_list_append(playlist, entry);
}

GList *
playlist_get_podcast_feeds(void)
{
    GList *feeds = NULL;
    for (GList *l = playlist; l; l = l->next) {
        PlaylistEntry *entry = l->data;
        if (!entry || !entry->is_podcast || !entry->podcast_feed ||
            !entry->podcast_feed[0])
            continue;
        gboolean exists = FALSE;
        for (GList *f = feeds; f; f = f->next) {
            if (g_strcmp0(f->data, entry->podcast_feed) == 0) {
                exists = TRUE;
                break;
            }
        }
        if (!exists)
            feeds = g_list_prepend(feeds, g_strdup(entry->podcast_feed));
    }
    return g_list_reverse(feeds);
}

void
playlist_add_spotify(const gchar *spotify_uri, const gchar *title,
                      gint duration_ms)
{
    PlaylistEntry *entry = g_new0(PlaylistEntry, 1);
    entry->filename = g_strdup(spotify_uri);
    entry->title = g_strdup(title);
    entry->length = duration_ms;
    playlist = g_list_append(playlist, entry);
}

void
playlist_add_dir(const gchar *dir)
{
    GDir *d = g_dir_open(dir, 0, NULL);
    if (!d)
        return;

    const gchar *name;
    while ((name = g_dir_read_name(d))) {
        gchar *path = g_build_filename(dir, name, NULL);

        if (g_file_test(path, G_FILE_TEST_IS_DIR)) {
            playlist_add_dir(path);
        } else if (g_file_test(path, G_FILE_TEST_IS_REGULAR)) {
            /* Add if it looks like a media file */
            gchar *lower = g_ascii_strdown(name, -1);
            if (g_str_has_suffix(lower, ".mp3") ||
                g_str_has_suffix(lower, ".ogg") ||
                g_str_has_suffix(lower, ".flac") ||
                g_str_has_suffix(lower, ".wav") ||
                g_str_has_suffix(lower, ".m4a") ||
                g_str_has_suffix(lower, ".aac") ||
                g_str_has_suffix(lower, ".opus") ||
                g_str_has_suffix(lower, ".wma") ||
                g_str_has_suffix(lower, ".mp4") ||
                g_str_has_suffix(lower, ".webm")) {
                playlist_add(path);
            }
            g_free(lower);
        }

        g_free(path);
    }

    g_dir_close(d);
}

static void
entry_free(PlaylistEntry *entry)
{
    g_free(entry->filename);
    g_free(entry->title);
    g_free(entry->podcast_feed);
    g_free(entry->podcast_guid);
    g_free(entry);
}

void
playlist_remove(gint pos)
{
    GList *node = g_list_nth(playlist, pos);
    if (!node)
        return;

    entry_free(node->data);
    playlist = g_list_delete_link(playlist, node);

    if (pos < playlist_position)
        playlist_position--;
    else if (pos == playlist_position)
        playlist_position = -1;
    if (duration_index_running)
        duration_index_rescan_requested = TRUE;
}

void
playlist_clear(void)
{
    g_list_free_full(playlist, (GDestroyNotify)entry_free);
    playlist = NULL;
    playlist_position = -1;
    g_list_free(shuffle_list);
    shuffle_list = NULL;
    if (duration_index_running)
        duration_index_rescan_requested = TRUE;
}

void
playlist_index_missing_durations(void)
{
    if (duration_index_running) {
        duration_index_rescan_requested = TRUE;
        return;
    }

    DurationIndexJob *job = g_new0(DurationIndexJob, 1);
    job->items = g_ptr_array_new_with_free_func(playlist_index_item_free);

    gint index = 0;
    for (GList *l = playlist; l; l = l->next, index++) {
        PlaylistEntry *entry = l->data;
        if (!entry || entry->length >= 0 || !entry->filename ||
            entry->is_podcast ||
            g_str_has_prefix(entry->filename, "spotify:"))
            continue;

        DurationIndexItem *item = g_new0(DurationIndexItem, 1);
        item->index = index;
        item->uri = g_strdup(entry->filename);
        g_ptr_array_add(job->items, item);
    }

    if (job->items->len == 0) {
        playlist_index_job_free(job);
        return;
    }

    duration_index_running = TRUE;
    GThread *thread = g_thread_new("playlist-duration-index",
                                   playlist_duration_index_thread, job);
    g_thread_unref(thread);
}

gint
playlist_get_length(void)
{
    return g_list_length(playlist);
}

PlaylistEntry *
playlist_get_entry(gint pos)
{
    return g_list_nth_data(playlist, pos);
}

const gchar *
playlist_get_filename(gint pos)
{
    PlaylistEntry *entry = playlist_get_entry(pos);
    return entry ? entry->filename : NULL;
}

const gchar *
playlist_get_title(gint pos)
{
    PlaylistEntry *entry = playlist_get_entry(pos);
    return entry ? entry->title : NULL;
}

void
playlist_set_length(gint pos, gint64 length_ms)
{
    PlaylistEntry *entry = playlist_get_entry(pos);
    if (entry && length_ms >= 0)
        entry->length = length_ms;
}

void
playlist_podcast_cache_ready(const gchar *uri, gint64 length_ms)
{
    for (GList *l = playlist; l; l = l->next) {
        PlaylistEntry *entry = l->data;
        if (!entry || !entry->is_podcast ||
            g_strcmp0(entry->filename, uri) != 0)
            continue;
        entry->podcast_downloading = FALSE;
        if (entry->title && g_str_has_prefix(entry->title, "failed: ")) {
            gchar *title = g_strdup(entry->title + strlen("failed: "));
            g_free(entry->title);
            entry->title = title;
        }
        if (length_ms >= 0)
            entry->length = length_ms;
    }
}

gboolean
playlist_podcast_cache_failed(const gchar *uri)
{
    gboolean current_failed = FALSE;
    gint index = 0;

    for (GList *l = playlist; l; l = l->next) {
        PlaylistEntry *entry = l->data;
        if (!entry || !entry->is_podcast ||
            g_strcmp0(entry->filename, uri) != 0) {
            index++;
            continue;
        }

        entry->podcast_downloading = FALSE;
        if (!entry->title || !entry->title[0]) {
            g_free(entry->title);
            entry->title = g_strdup("failed: podcast download");
        } else if (!g_str_has_prefix(entry->title, "failed: ")) {
            gchar *title = g_strdup_printf("failed: %s", entry->title);
            g_free(entry->title);
            entry->title = title;
        }
        if (index == playlist_position)
            current_failed = TRUE;
        index++;
    }

    return current_failed;
}

gint
playlist_get_position(void)
{
    return playlist_position;
}

void
playlist_set_position(gint pos)
{
    gint len = playlist_get_length();
    if (pos >= 0 && pos < len)
        playlist_position = pos;
}

static void
generate_shuffle_list(void)
{
    g_list_free(shuffle_list);
    shuffle_list = NULL;

    gint len = playlist_get_length();
    if (len == 0)
        return;

    /* Create array of indices and shuffle */
    gint *indices = g_new(gint, len);
    for (gint i = 0; i < len; i++)
        indices[i] = i;

    for (gint i = len - 1; i > 0; i--) {
        gint j = g_random_int_range(0, i + 1);
        gint tmp = indices[i];
        indices[i] = indices[j];
        indices[j] = tmp;
    }

    for (gint i = 0; i < len; i++)
        shuffle_list = g_list_append(shuffle_list, GINT_TO_POINTER(indices[i]));

    g_free(indices);
}

static gint
get_next_position(void)
{
    gint len = playlist_get_length();
    if (len == 0)
        return -1;

    if (shuffle) {
        if (!shuffle_list)
            generate_shuffle_list();

        /* Find current position in shuffle list and get next */
        for (GList *l = shuffle_list; l; l = l->next) {
            if (GPOINTER_TO_INT(l->data) == playlist_position) {
                if (l->next)
                    return GPOINTER_TO_INT(l->next->data);
                else if (repeat) {
                    generate_shuffle_list();
                    return GPOINTER_TO_INT(shuffle_list->data);
                }
                return -1;
            }
        }
        /* Not found, start from beginning */
        return GPOINTER_TO_INT(shuffle_list->data);
    }

    gint next = playlist_position + 1;
    if (next >= len) {
        if (repeat)
            return 0;
        return -1;
    }
    return next;
}

static gint
get_prev_position(void)
{
    gint len = playlist_get_length();
    if (len == 0)
        return -1;

    gint prev = playlist_position - 1;
    if (prev < 0) {
        if (repeat)
            return len - 1;
        return 0;
    }
    return prev;
}

void
playlist_next(void)
{
    gint next = get_next_position();
    if (next >= 0) {
        playlist_position = next;
        playlist_play();
    }
}

void
playlist_prev(void)
{
    gint prev = get_prev_position();
    if (prev >= 0) {
        playlist_position = prev;
        playlist_play();
    }
}

void
playlist_skip_failed_current(void)
{
    gint next = get_next_position();
    if (next >= 0 && next != playlist_position) {
        playlist_position = next;
        playlist_play();
    } else {
        player_stop();
    }
}

void
playlist_play(void)
{
    gint len = playlist_get_length();
    if (len == 0)
        return;

    if (playlist_position < 0)
        playlist_position = 0;

    PlaylistEntry *entry = playlist_get_entry(playlist_position);
    gchar *uri = podcast_prepare_playback_uri(entry);
    if (uri) {
        player_play(uri);
        g_free(uri);
    }
}

void
playlist_eof_reached(void)
{
    if (no_advance) {
        player_stop();
        return;
    }

    gint next = get_next_position();
    if (next >= 0) {
        playlist_position = next;
        playlist_play();
    } else {
        player_stop();
    }
}

void
playlist_shuffle_toggle(void)
{
    playlist_set_shuffle(!shuffle);
}

void
playlist_set_shuffle(gboolean enabled)
{
    shuffle = enabled;
    if (shuffle)
        generate_shuffle_list();
    else {
        g_list_free(shuffle_list);
        shuffle_list = NULL;
    }
}

void
playlist_repeat_toggle(void)
{
    playlist_set_repeat(!repeat);
}

void
playlist_set_repeat(gboolean enabled)
{
    repeat = enabled;
}

gboolean
playlist_get_shuffle(void)
{
    return shuffle;
}

gboolean
playlist_get_repeat(void)
{
    return repeat;
}

void
playlist_set_no_advance(gboolean enabled)
{
    no_advance = enabled;
}

gboolean
playlist_get_no_advance(void)
{
    return no_advance;
}

GList *
playlist_get_entries(void)
{
    return playlist;
}

void
playlist_sort_by_title(void)
{
    playlist_sort_all((GCompareFunc)playlist_sort_by_title_cmpfunc);
}

void
playlist_sort_by_filename(void)
{
    playlist_sort_all((GCompareFunc)playlist_sort_by_filename_cmpfunc);
}

void
playlist_sort_by_path(void)
{
    playlist_sort_all((GCompareFunc)playlist_sort_by_path_cmpfunc);
}

void
playlist_sort_by_date(void)
{
    playlist_sort_all((GCompareFunc)playlist_sort_by_date_cmpfunc);
}

void
playlist_sort_selected_by_title(void)
{
    playlist_sort_selected((GCompareFunc)playlist_sort_by_title_cmpfunc);
}

void
playlist_sort_selected_by_filename(void)
{
    playlist_sort_selected((GCompareFunc)playlist_sort_by_filename_cmpfunc);
}

void
playlist_sort_selected_by_path(void)
{
    playlist_sort_selected((GCompareFunc)playlist_sort_by_path_cmpfunc);
}

void
playlist_sort_selected_by_date(void)
{
    playlist_sort_selected((GCompareFunc)playlist_sort_by_date_cmpfunc);
}

void
playlist_reverse(void)
{
    PlaylistEntry *current = playlist_get_entry(playlist_position);
    playlist = g_list_reverse(playlist);
    playlist_refresh_position(current);
}

void
playlist_random(void)
{
    PlaylistEntry *current = playlist_get_entry(playlist_position);
    GList *shuffled = NULL;

    while (playlist) {
        gint idx = g_random_int_range(0, g_list_length(playlist));
        GList *node = g_list_nth(playlist, idx);
        playlist = g_list_remove_link(playlist, node);
        node->prev = NULL;
        node->next = NULL;
        shuffled = g_list_concat(node, shuffled);
    }

    playlist = shuffled;
    playlist_refresh_position(current);
}

gboolean
playlist_load(const gchar *filename)
{
    gchar *contents = NULL;
    if (!g_file_get_contents(filename, &contents, NULL, NULL))
        return FALSE;

    gchar **lines = g_strsplit(contents, "\n", -1);
    g_free(contents);

    gchar *base_dir = g_path_get_dirname(filename);
    gint64 pending_length = -1;
    gchar *pending_title = NULL;
    gchar *pending_feed = NULL;
    gchar *pending_guid = NULL;
    gboolean pending_podcast = FALSE;

    for (int i = 0; lines[i]; i++) {
        gchar *line = g_strstrip(lines[i]);
        if (line[0] == '\0')
            continue;

        if (g_strcmp0(line, "#XMMSPODCAST") == 0) {
            pending_podcast = TRUE;
            continue;
        }
        if (g_str_has_prefix(line, "#XMMSPODCAST:")) {
            pending_podcast = TRUE;
            g_free(pending_feed);
            g_free(pending_guid);
            pending_feed = NULL;
            pending_guid = NULL;
            const gchar *payload = line + strlen("#XMMSPODCAST:");
            gchar **parts = g_strsplit(payload, "\t", -1);
            for (gint j = 0; parts[j]; j++) {
                if (g_str_has_prefix(parts[j], "feed="))
                    pending_feed = g_uri_unescape_string(parts[j] + 5, NULL);
                else if (g_str_has_prefix(parts[j], "guid="))
                    pending_guid = g_uri_unescape_string(parts[j] + 5, NULL);
            }
            g_strfreev(parts);
            continue;
        }

        if (g_str_has_prefix(line, "#EXTINF:")) {
            gchar *comma = strchr(line, ',');
            if (comma) {
                *comma = '\0';
                gint seconds = atoi(line + 8);
                pending_length = seconds >= 0 ? (gint64)seconds * 1000 : -1;
                g_free(pending_title);
                pending_title = g_strdup(comma + 1);
            }
            continue;
        }

        if (line[0] == '#')
            continue;

        if (g_str_has_prefix(line, "file://") ||
            g_str_has_prefix(line, "http://") ||
            g_str_has_prefix(line, "https://") ||
            g_str_has_prefix(line, "spotify:")) {
            if (pending_podcast) {
                playlist_add_podcast_entry(line, pending_title,
                                           pending_feed, pending_guid);
            } else {
                playlist_add_uri(line);
            }
        } else if (g_path_is_absolute(line)) {
            playlist_add(line);
        } else {
            gchar *path = g_build_filename(base_dir, line, NULL);
            playlist_add(path);
            g_free(path);
        }

        PlaylistEntry *entry = g_list_last(playlist) ?
            g_list_last(playlist)->data : NULL;
        if (entry) {
            if (pending_length >= 0)
                entry->length = pending_length;
            if (pending_title && pending_title[0]) {
                g_free(entry->title);
                entry->title = g_strdup(pending_title);
            }
            if (pending_podcast) {
                entry->is_podcast = TRUE;
                if (!podcast_cache_is_fresh_for_url(entry->filename))
                    entry->length = -1;
            }
        }
        pending_length = -1;
        pending_podcast = FALSE;
        g_clear_pointer(&pending_title, g_free);
        g_clear_pointer(&pending_feed, g_free);
        g_clear_pointer(&pending_guid, g_free);
    }

    g_strfreev(lines);
    g_free(base_dir);
    g_free(pending_title);
    g_free(pending_feed);
    g_free(pending_guid);
    return TRUE;
}

gboolean
playlist_save(const gchar *filename)
{
    GString *str = g_string_new("#EXTM3U\n");

    for (GList *l = playlist; l; l = l->next) {
        PlaylistEntry *entry = l->data;
        if (entry->is_podcast) {
            gboolean has_feed = entry->podcast_feed && entry->podcast_feed[0];
            gboolean has_guid = entry->podcast_guid && entry->podcast_guid[0];
            if (has_feed || has_guid) {
                g_string_append(str, "#XMMSPODCAST:");
                if (has_feed) {
                    gchar *escaped = g_uri_escape_string(entry->podcast_feed,
                                                         NULL, TRUE);
                    g_string_append_printf(str, "feed=%s", escaped);
                    g_free(escaped);
                }
                if (has_guid) {
                    gchar *escaped = g_uri_escape_string(entry->podcast_guid,
                                                         NULL, TRUE);
                    g_string_append_printf(str, "%sguid=%s",
                                           has_feed ? "\t" : "", escaped);
                    g_free(escaped);
                }
                g_string_append_c(str, '\n');
            } else {
                g_string_append(str, "#XMMSPODCAST\n");
            }
        }
        gboolean save_length = entry->length >= 0 &&
            (!entry->is_podcast ||
             podcast_cache_is_fresh_for_url(entry->filename));
        if (save_length || (entry->is_podcast && entry->title)) {
            g_string_append_printf(str, "#EXTINF:%" G_GINT64_FORMAT ",%s\n",
                                   save_length ? entry->length / 1000 : -1,
                                   entry->title ? entry->title : "");
        }
        g_string_append(str, entry->filename);
        g_string_append_c(str, '\n');
    }

    gboolean ok = g_file_set_contents(filename, str->str, str->len, NULL);
    g_string_free(str, TRUE);
    return ok;
}
