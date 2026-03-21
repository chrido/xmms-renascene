#ifndef UTIL_H
#define UTIL_H

#include <glib.h>
#include <cairo.h>

gchar *xmms_get_config_dir(void);
gchar *xmms_get_skin_dir(void);

gchar *filename_to_uri(const gchar *filename);
gchar *uri_to_filename(const gchar *uri);

gchar *time_to_string(gint64 ms);
gchar *format_title(const gchar *filename, const gchar *title);

cairo_surface_t *pixbuf_to_surface(GdkPixbuf *pixbuf);

#endif /* UTIL_H */
