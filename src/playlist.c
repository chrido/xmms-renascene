#include "xmms.h"

static GList *playlist = NULL;
static gint playlist_position = -1;
static GList *shuffle_list = NULL;
static gboolean shuffle = FALSE;
static gboolean repeat = FALSE;

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
}

void
playlist_clear(void)
{
    g_list_free_full(playlist, (GDestroyNotify)entry_free);
    playlist = NULL;
    playlist_position = -1;
    g_list_free(shuffle_list);
    shuffle_list = NULL;
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
playlist_play(void)
{
    gint len = playlist_get_length();
    if (len == 0)
        return;

    if (playlist_position < 0)
        playlist_position = 0;

    const gchar *uri = playlist_get_filename(playlist_position);
    if (uri)
        player_play(uri);
}

void
playlist_eof_reached(void)
{
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
    shuffle = !shuffle;
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
    repeat = !repeat;
}

GList *
playlist_get_entries(void)
{
    return playlist;
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

    for (int i = 0; lines[i]; i++) {
        gchar *line = g_strstrip(lines[i]);
        if (line[0] == '\0' || line[0] == '#')
            continue;

        if (g_path_is_absolute(line) ||
            g_str_has_prefix(line, "file://") ||
            g_str_has_prefix(line, "http://") ||
            g_str_has_prefix(line, "https://")) {
            playlist_add(line);
        } else {
            gchar *path = g_build_filename(base_dir, line, NULL);
            playlist_add(path);
            g_free(path);
        }
    }

    g_strfreev(lines);
    g_free(base_dir);
    return TRUE;
}

gboolean
playlist_save(const gchar *filename)
{
    GString *str = g_string_new("");

    for (GList *l = playlist; l; l = l->next) {
        PlaylistEntry *entry = l->data;
        g_string_append(str, entry->filename);
        g_string_append_c(str, '\n');
    }

    gboolean ok = g_file_set_contents(filename, str->str, str->len, NULL);
    g_string_free(str, TRUE);
    return ok;
}
