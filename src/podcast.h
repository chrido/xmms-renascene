#ifndef PODCAST_H
#define PODCAST_H

#include <glib.h>

typedef struct _PlaylistEntry PlaylistEntry;

void podcast_init(void);
void podcast_shutdown(void);
void podcast_add_url(const gchar *url);
gchar *podcast_prepare_playback_uri(PlaylistEntry *entry);
gboolean podcast_cache_is_fresh_for_url(const gchar *url);

#endif /* PODCAST_H */
