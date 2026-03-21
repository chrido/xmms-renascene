#ifndef MPRIS_H
#define MPRIS_H

void mpris_init(void);
void mpris_free(void);
void mpris_update_metadata(const gchar *title, const gchar *uri, gint64 length_us);
void mpris_update_playback_status(void);
void mpris_update_position(gint64 position_us);

#endif /* MPRIS_H */
