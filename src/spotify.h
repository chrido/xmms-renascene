#ifndef SPOTIFY_H
#define SPOTIFY_H

#include <gtk/gtk.h>

typedef struct {
    gchar *id;
    gchar *name;
    gint   total_tracks;
    gchar *uri;
} SpotifyPlaylist;

typedef struct {
    gchar  *id;
    gchar  *name;
    gchar  *artist;
    gchar  *album;
    gchar  *uri;
    gint    duration_ms;
} SpotifyTrack;

/* Initialization / auth */
void     spotify_init(void);
void     spotify_free(void);
gboolean spotify_is_authenticated(void);
void     spotify_authenticate(GtkWindow *parent);

/* API calls — all async, results delivered via callbacks */
typedef void (*SpotifyPlaylistsCb)(GList *playlists, gpointer data);
typedef void (*SpotifyTracksCb)(GList *tracks, gpointer data);

void spotify_get_playlists(SpotifyPlaylistsCb cb, gpointer data);
void spotify_get_playlist_tracks(const gchar *playlist_id,
                                  SpotifyTracksCb cb, gpointer data);

/* Playback control via Web API */
gboolean spotify_play_track(const gchar *track_uri, const gchar *context_uri,
                             gint offset);
void spotify_play(void);
void spotify_pause(void);
void spotify_next(void);
void spotify_previous(void);

/* Playback state polling */
typedef struct {
    gboolean is_playing;
    gint64   progress_ms;
    gint64   duration_ms;
    gchar   *track_name;
    gchar   *artist_name;
} SpotifyPlaybackState;

gboolean spotify_get_playback_state(SpotifyPlaybackState *state);
void     spotify_playback_state_clear(SpotifyPlaybackState *state);

/* Device management */
typedef struct {
    gchar    *id;
    gchar    *name;
    gchar    *type;      /* e.g. "Computer", "Smartphone", "Speaker" */
    gboolean  is_active;
} SpotifyDevice;

GList   *spotify_get_devices(void);
gboolean spotify_set_device(const gchar *device_id);
void     spotify_device_free(SpotifyDevice *dev);
void     spotify_device_list_free(GList *list);

/* Free helpers */
void spotify_playlist_free(SpotifyPlaylist *p);
void spotify_track_free(SpotifyTrack *t);
void spotify_playlist_list_free(GList *list);
void spotify_track_list_free(GList *list);

#endif /* SPOTIFY_H */
