#ifndef PLAYLISTWIN_H
#define PLAYLISTWIN_H

#include <gtk/gtk.h>

void playlistwin_create(GtkApplication *app);
GtkWidget *playlistwin_get_widget(void);
void playlistwin_show(gboolean show);
gboolean playlistwin_is_visible(void);
void playlistwin_set_detached(gboolean detached);
gboolean playlistwin_is_detached(void);
gint playlistwin_height(void);
void playlistwin_update(void);
void playlistwin_show_menu(const gchar *menu);
void playlistwin_shutdown(void);

#endif /* PLAYLISTWIN_H */
