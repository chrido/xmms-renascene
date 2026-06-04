#ifndef EQUALIZERWIN_H
#define EQUALIZERWIN_H

#include <gtk/gtk.h>

void equalizerwin_create(GtkApplication *app);
GtkWidget *equalizerwin_get_widget(void);
void equalizerwin_show(gboolean show);
gboolean equalizerwin_is_visible(void);
void equalizerwin_set_detached(gboolean detached);
gboolean equalizerwin_is_detached(void);
gint equalizerwin_height(void);

extern GtkWidget *equalizerwin;

#endif /* EQUALIZERWIN_H */
