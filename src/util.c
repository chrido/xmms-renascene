#include "xmms.h"

gchar *
xmms_get_config_dir(void)
{
    return g_build_filename(g_get_user_config_dir(), "xmms", NULL);
}

gchar *
xmms_get_skin_dir(void)
{
    return g_build_filename(XMMS_DATADIR, "Skins", NULL);
}

gchar *
filename_to_uri(const gchar *filename)
{
    if (g_str_has_prefix(filename, "file://") ||
        g_str_has_prefix(filename, "http://") ||
        g_str_has_prefix(filename, "https://"))
        return g_strdup(filename);

    gchar *abs = NULL;
    if (!g_path_is_absolute(filename)) {
        gchar *cwd = g_get_current_dir();
        abs = g_build_filename(cwd, filename, NULL);
        g_free(cwd);
    } else {
        abs = g_strdup(filename);
    }

    gchar *uri = g_filename_to_uri(abs, NULL, NULL);
    g_free(abs);
    return uri;
}

gchar *
uri_to_filename(const gchar *uri)
{
    return g_filename_from_uri(uri, NULL, NULL);
}

gchar *
time_to_string(gint64 ms)
{
    gint secs = (gint)(ms / 1000);
    gint mins = secs / 60;
    secs %= 60;
    return g_strdup_printf("%d:%02d", mins, secs);
}

gchar *
format_title(const gchar *filename, const gchar *title)
{
    if (title && title[0])
        return g_strdup(title);

    gchar *base = g_path_get_basename(filename);
    /* Remove file extension */
    gchar *dot = strrchr(base, '.');
    if (dot)
        *dot = '\0';
    /* Replace underscores with spaces */
    for (gchar *p = base; *p; p++) {
        if (*p == '_')
            *p = ' ';
    }
    return base;
}

cairo_surface_t *
pixbuf_to_surface(GdkPixbuf *pixbuf)
{
    if (!pixbuf)
        return NULL;

    gint width = gdk_pixbuf_get_width(pixbuf);
    gint height = gdk_pixbuf_get_height(pixbuf);

    cairo_surface_t *surface = cairo_image_surface_create(
        CAIRO_FORMAT_ARGB32, width, height);

    gint n_channels = gdk_pixbuf_get_n_channels(pixbuf);
    gint rowstride = gdk_pixbuf_get_rowstride(pixbuf);
    guchar *pixels = gdk_pixbuf_get_pixels(pixbuf);
    guchar *cairo_data = cairo_image_surface_get_data(surface);
    gint cairo_stride = cairo_image_surface_get_stride(surface);

    cairo_surface_flush(surface);
    for (gint y = 0; y < height; y++) {
        guchar *src = pixels + y * rowstride;
        guint32 *dst = (guint32 *)(cairo_data + y * cairo_stride);
        for (gint x = 0; x < width; x++) {
            guchar r = src[0], g = src[1], b = src[2];
            guchar a = (n_channels == 4) ? src[3] : 255;
            /* Pre-multiply alpha for Cairo ARGB32 */
            dst[x] = ((guint32)a << 24) |
                     ((guint32)((r * a + 127) / 255) << 16) |
                     ((guint32)((g * a + 127) / 255) << 8) |
                     ((guint32)((b * a + 127) / 255));
            src += n_channels;
        }
    }
    cairo_surface_mark_dirty(surface);

    return surface;
}
