#ifndef SKIN_H
#define SKIN_H

#include <gtk/gtk.h>
#include <cairo.h>

typedef enum {
    SKIN_MAIN,
    SKIN_CBUTTONS,
    SKIN_TITLEBAR,
    SKIN_SHUFREP,
    SKIN_TEXT,
    SKIN_VOLUME,
    SKIN_BALANCE,
    SKIN_MONOSTEREO,
    SKIN_PLAYPAUSE,
    SKIN_NUMBERS,
    SKIN_POSBAR,
    SKIN_PLEDIT,
    SKIN_EQMAIN,
    SKIN_EQ_EX,
    SKIN_PIXMAP_COUNT
} SkinIndex;

typedef enum {
    SKIN_MASK_MAIN,
    SKIN_MASK_MAIN_SHADE,
    SKIN_MASK_EQ,
    SKIN_MASK_EQ_SHADE
} SkinMaskIndex;

typedef struct {
    cairo_surface_t *surface;
    cairo_surface_t *def_surface;
    gint width, height;
    gint current_width, current_height;
} SkinPixmap;

typedef struct {
    gchar *path;
    SkinPixmap pixmaps[SKIN_PIXMAP_COUNT];
    GdkRGBA pledit_normal;
    GdkRGBA pledit_current;
    GdkRGBA pledit_normalbg;
    GdkRGBA pledit_selectedbg;
    guchar vis_color[24][3];
    gint id;
} Skin;

extern Skin *skin;

void skin_init(void);
void skin_free(void);
gboolean skin_load(const gchar *path);
void skin_draw_pixmap(cairo_t *cr, SkinIndex index,
                      gint xsrc, gint ysrc,
                      gint xdest, gint ydest,
                      gint width, gint height);
cairo_surface_t *skin_get_surface(SkinIndex index);
gint skin_get_id(void);

#endif /* SKIN_H */
