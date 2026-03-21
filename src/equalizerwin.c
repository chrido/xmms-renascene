#include "xmms.h"
#include "equalizerwin.h"

#define EQWIN_WIDTH  275
#define EQWIN_HEIGHT 116

GtkWidget *equalizerwin = NULL;
static GtkWidget *eqwin_drawing_area = NULL;
static GList *eqwin_wlist = NULL;

static gboolean eq_active = TRUE;
static gfloat eq_preamp = 0.0;
static gfloat eq_bands[10] = { 0 };

/* EQ slider positions (0=top/+20dB, 50=center/0dB, 100=bottom/-20dB) */
static gint eq_slider_pos[10] = { 50, 50, 50, 50, 50, 50, 50, 50, 50, 50 };
static gint eq_preamp_pos = 50;
static gint eq_dragging = -1; /* -1=none, 0=preamp, 1-10=bands */

static void eqwin_queue_draw(void);

/* ---- EQ drawing ---- */

static void
draw_eq_slider(cairo_t *cr, gint x, gint pos)
{
    /* Slider track */
    gint track_x = x;
    gint track_y = 38;
    gint track_h = 63;

    /* Draw track background from skin */
    skin_draw_pixmap(cr, SKIN_EQMAIN,
                     13, 164, track_x, track_y, 14, track_h);

    /* Draw knob at position */
    gint knob_y = track_y + (pos * (track_h - 11)) / 100;
    skin_draw_pixmap(cr, SKIN_EQMAIN,
                     0, 164, track_x, knob_y, 14, 11);
}

static void
draw_eq_graph(cairo_t *cr)
{
    /* Simple EQ response curve */
    gint graph_x = 86, graph_y = 17;
    gint graph_w = 113, graph_h = 19;

    /* Background - source is at y=132 in eqmain, only available in full skins */
    if (skin->pixmaps[SKIN_EQMAIN].current_height >= 151) {
        skin_draw_pixmap(cr, SKIN_EQMAIN,
                         66, 132, graph_x, graph_y, graph_w, graph_h);
    } else {
        cairo_set_source_rgb(cr, 0.0, 0.0, 0.0);
        cairo_rectangle(cr, graph_x, graph_y, graph_w, graph_h);
        cairo_fill(cr);
    }

    /* Draw response curve */
    cairo_set_source_rgb(cr, 0.0, 1.0, 0.0);
    cairo_set_line_width(cr, 1.0);

    for (gint i = 0; i < 10; i++) {
        gdouble x = graph_x + (i * graph_w) / 9.0;
        gdouble val = (50 - eq_slider_pos[i]) / 50.0; /* -1 to 1 */
        gdouble y = graph_y + graph_h / 2.0 - val * (graph_h / 2.0 - 1);

        if (i == 0)
            cairo_move_to(cr, x, y);
        else
            cairo_line_to(cr, x, y);
    }
    cairo_stroke(cr);
}

static void
draw_equalizer_window(GtkDrawingArea *area, cairo_t *cr,
                      int width, int height, gpointer data)
{
    (void)area; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    cairo_scale(cr, (double)width / EQWIN_WIDTH,
                    (double)height / EQWIN_HEIGHT);

    /* Draw EQ background from skin */
    skin_draw_pixmap(cr, SKIN_EQMAIN,
                     0, 0, 0, 0, EQWIN_WIDTH, EQWIN_HEIGHT);

    /* Overlay titlebar (focused at y=134, unfocused at y=149)
     * Only if the skin image is tall enough (real Winamp skins are 164+ px) */
    if (skin->pixmaps[SKIN_EQMAIN].current_height >= 148)
        skin_draw_pixmap(cr, SKIN_EQMAIN,
                         0, 134, 0, 0, EQWIN_WIDTH, 14);

    /* On/Off button state */
    if (eq_active) {
        skin_draw_pixmap(cr, SKIN_EQMAIN,
                         69, 119, 14, 18, 25, 12);
    } else {
        skin_draw_pixmap(cr, SKIN_EQMAIN,
                         187, 119, 14, 18, 25, 12);
    }

    /* Preamp slider */
    draw_eq_slider(cr, 21, eq_preamp_pos);

    /* 10 band sliders */
    for (gint i = 0; i < 10; i++) {
        draw_eq_slider(cr, 78 + i * 18, eq_slider_pos[i]);
    }

    /* EQ graph */
    draw_eq_graph(cr);

    /* Draw widgets */
    widget_list_draw(eqwin_wlist, cr);
}

/* ---- Apply EQ values ---- */

static void
apply_eq(void)
{
    eq_preamp = (50 - eq_preamp_pos) * 20.0 / 50.0; /* -20 to +20 dB */
    for (gint i = 0; i < 10; i++) {
        eq_bands[i] = (50 - eq_slider_pos[i]) * 20.0 / 50.0;
    }

    if (eq_active)
        player_set_equalizer(eq_preamp, eq_bands);
    else {
        gfloat zeros[10] = { 0 };
        player_set_equalizer(0, zeros);
    }
}

/* ---- Event handling ---- */

static void
eqwin_click_pressed(GtkGestureClick *gesture, int n_press,
                    double x, double y, gpointer data)
{
    (void)data; (void)n_press;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);
    gint button = gtk_gesture_single_get_current_button(
        GTK_GESTURE_SINGLE(gesture));

    /* On/Off button (14,18 size 25x12) */
    if (sx >= 14 && sx < 39 && sy >= 18 && sy < 30) {
        eq_active = !eq_active;
        apply_eq();
        eqwin_queue_draw();
        return;
    }

    /* Check preamp slider area (x=21, y=38, h=63) */
    if (sx >= 21 && sx < 35 && sy >= 38 && sy < 101) {
        eq_dragging = 0;
        eq_preamp_pos = CLAMP((sy - 38) * 100 / 63, 0, 100);
        apply_eq();
        eqwin_queue_draw();
        return;
    }

    /* Check band sliders (x=78+i*18) */
    for (gint i = 0; i < 10; i++) {
        gint bx = 78 + i * 18;
        if (sx >= bx && sx < bx + 14 && sy >= 38 && sy < 101) {
            eq_dragging = i + 1;
            eq_slider_pos[i] = CLAMP((sy - 38) * 100 / 63, 0, 100);
            apply_eq();
            eqwin_queue_draw();
            return;
        }
    }

    /* Titlebar close button (x=264, y=3, 9x9) */
    if (sx >= 264 && sx < 273 && sy >= 3 && sy < 12) {
        equalizerwin_show(FALSE);
        return;
    }

    /* Titlebar drag */
    if (sy < 14) {
        GdkSurface *surface = gtk_native_get_surface(GTK_NATIVE(equalizerwin));
        if (surface && GDK_IS_TOPLEVEL(surface)) {
            GdkDevice *device = gtk_gesture_get_device(GTK_GESTURE(gesture));
            guint32 timestamp = gtk_event_controller_get_current_event_time(
                GTK_EVENT_CONTROLLER(gesture));
            gdk_toplevel_begin_move(GDK_TOPLEVEL(surface), device,
                                    button, x, y, timestamp);
        }
    }
}

static void
eqwin_click_released(GtkGestureClick *gesture, int n_press,
                     double x, double y, gpointer data)
{
    (void)gesture; (void)n_press; (void)x; (void)y; (void)data;
    eq_dragging = -1;
}

static void
eqwin_motion(GtkEventControllerMotion *controller,
             double x, double y, gpointer data)
{
    (void)controller; (void)data;

    if (eq_dragging < 0)
        return;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    gint sy = (gint)(y / scale);
    gint pos = CLAMP((sy - 38) * 100 / 63, 0, 100);

    if (eq_dragging == 0) {
        eq_preamp_pos = pos;
    } else {
        eq_slider_pos[eq_dragging - 1] = pos;
    }

    apply_eq();
    eqwin_queue_draw();
}

/* ---- Public API ---- */

static void
eqwin_queue_draw(void)
{
    if (eqwin_drawing_area)
        gtk_widget_queue_draw(eqwin_drawing_area);
}

void
equalizerwin_create(GtkApplication *app)
{
    equalizerwin = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(equalizerwin), "XMMS Equalizer");
    gtk_window_set_decorated(GTK_WINDOW(equalizerwin), FALSE);
    gtk_window_set_resizable(GTK_WINDOW(equalizerwin), FALSE);

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 2;

    eqwin_drawing_area = gtk_drawing_area_new();
    gtk_drawing_area_set_content_width(
        GTK_DRAWING_AREA(eqwin_drawing_area), EQWIN_WIDTH * scale);
    gtk_drawing_area_set_content_height(
        GTK_DRAWING_AREA(eqwin_drawing_area), EQWIN_HEIGHT * scale);
    gtk_drawing_area_set_draw_func(
        GTK_DRAWING_AREA(eqwin_drawing_area),
        draw_equalizer_window, NULL, NULL);

    gtk_window_set_child(GTK_WINDOW(equalizerwin), eqwin_drawing_area);

    /* Click events */
    GtkGesture *click = gtk_gesture_click_new();
    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(click), 0);
    g_signal_connect(click, "pressed", G_CALLBACK(eqwin_click_pressed), NULL);
    g_signal_connect(click, "released", G_CALLBACK(eqwin_click_released), NULL);
    gtk_widget_add_controller(eqwin_drawing_area,
                              GTK_EVENT_CONTROLLER(click));

    /* Motion events */
    GtkEventController *motion = gtk_event_controller_motion_new();
    g_signal_connect(motion, "motion", G_CALLBACK(eqwin_motion), NULL);
    gtk_widget_add_controller(eqwin_drawing_area, motion);
}

void
equalizerwin_show(gboolean show)
{
    if (!equalizerwin)
        return;
    gtk_widget_set_visible(equalizerwin, show);
}

gboolean
equalizerwin_is_visible(void)
{
    return equalizerwin && gtk_widget_get_visible(equalizerwin);
}
