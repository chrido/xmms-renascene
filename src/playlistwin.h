#ifndef PLAYLISTWIN_H
#define PLAYLISTWIN_H

#include <gtk/gtk.h>

void playlistwin_create(GtkApplication *app);
void playlistwin_show(gboolean show);
gboolean playlistwin_is_visible(void);
void playlistwin_update(void);

extern GtkWidget *playlistwin;

#endif /* PLAYLISTWIN_H */
