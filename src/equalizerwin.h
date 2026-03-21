#ifndef EQUALIZERWIN_H
#define EQUALIZERWIN_H

#include <gtk/gtk.h>

void equalizerwin_create(GtkApplication *app);
void equalizerwin_show(gboolean show);
gboolean equalizerwin_is_visible(void);

extern GtkWidget *equalizerwin;

#endif /* EQUALIZERWIN_H */
