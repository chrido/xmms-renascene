#ifndef PLAYLIST_H
#define PLAYLIST_H

#include <glib.h>

typedef struct _PlaylistEntry {
    gchar *filename;
    gchar *title;
    gint64 length;  /* in milliseconds, -1 if unknown */
    gboolean selected;
    gboolean is_podcast;
    gchar *podcast_feed;
    gchar *podcast_guid;
    gboolean podcast_downloading;
} PlaylistEntry;

void playlist_init(void);
void playlist_free(void);

void playlist_add(const gchar *filename);
void playlist_add_uri(const gchar *uri);
void playlist_add_url_checked(const gchar *url);
void playlist_add_podcast_entry(const gchar *uri, const gchar *title,
                                const gchar *feed, const gchar *guid);
GList *playlist_get_podcast_feeds(void);
void playlist_add_dir(const gchar *dir);
void playlist_add_spotify(const gchar *spotify_uri, const gchar *title,
                           gint duration_ms);
void playlist_remove(gint pos);
void playlist_clear(void);
void playlist_index_missing_durations(void);

gint playlist_get_length(void);
PlaylistEntry *playlist_get_entry(gint pos);
const gchar *playlist_get_filename(gint pos);
const gchar *playlist_get_title(gint pos);
void playlist_set_length(gint pos, gint64 length_ms);
void playlist_podcast_cache_ready(const gchar *uri, gint64 length_ms);

gint playlist_get_position(void);
void playlist_set_position(gint pos);

void playlist_next(void);
void playlist_prev(void);

void playlist_play(void);
void playlist_eof_reached(void);

void playlist_shuffle_toggle(void);
void playlist_repeat_toggle(void);
void playlist_set_shuffle(gboolean enabled);
void playlist_set_repeat(gboolean enabled);
gboolean playlist_get_shuffle(void);
gboolean playlist_get_repeat(void);
void playlist_set_no_advance(gboolean enabled);
gboolean playlist_get_no_advance(void);

GList *playlist_get_entries(void);

void playlist_sort_by_title(void);
void playlist_sort_by_filename(void);
void playlist_sort_by_path(void);
void playlist_sort_by_date(void);
void playlist_sort_selected_by_title(void);
void playlist_sort_selected_by_filename(void);
void playlist_sort_selected_by_path(void);
void playlist_sort_selected_by_date(void);
void playlist_reverse(void);
void playlist_random(void);

gboolean playlist_load(const gchar *filename);
gboolean playlist_save(const gchar *filename);

#endif /* PLAYLIST_H */
