#include "xmms.h"
#include "equalizerwin.h"

#define EQWIN_WIDTH  275
#define EQWIN_HEIGHT 116
#define EQWIN_DETACH_BTN_X 250
#define EQWIN_DETACH_BTN_Y 3
#define EQWIN_DETACH_BTN_W 13
#define EQWIN_DETACH_BTN_H 10
#define EQWIN_ON_X 14
#define EQWIN_ON_Y 18
#define EQWIN_ON_W 25
#define EQWIN_ON_H 12
#define EQWIN_AUTO_X 39
#define EQWIN_AUTO_Y 18
#define EQWIN_AUTO_W 33
#define EQWIN_AUTO_H 12
#define EQWIN_PRESETS_X 217
#define EQWIN_PRESETS_Y 18
#define EQWIN_PRESETS_W 44
#define EQWIN_PRESETS_H 12

typedef enum {
    EQ_CONTROL_NONE,
    EQ_CONTROL_ON,
    EQ_CONTROL_AUTO,
    EQ_CONTROL_PRESETS
} EqControl;

GtkWidget *equalizerwin = NULL;
static GtkWidget *eqwin_drawing_area = NULL;
static GtkWidget *eqwin_floating_window = NULL;
static GtkWidget *eq_presets_popover = NULL;
static GList *eqwin_wlist = NULL;

static gboolean eq_active = TRUE;
static gboolean eq_auto = FALSE;
static EqControl eq_pressed_control = EQ_CONTROL_NONE;
static gboolean eq_pressed_inside = FALSE;
static gfloat eq_preamp = 0.0;
static gfloat eq_bands[10] = { 0 };

/* EQ slider positions (0=top/+20dB, 50=center/0dB, 100=bottom/-20dB) */
static gint eq_slider_pos[10] = { 50, 50, 50, 50, 50, 50, 50, 50, 50, 50 };
static gint eq_preamp_pos = 50;
static gint eq_dragging = -1; /* -1=none, 0=preamp, 1-10=bands */

static void eqwin_queue_draw(void);
static void eqwin_attach_widget(void);
static void eqwin_detach_widget(void);
static void eqwin_show_presets_menu(void);

static gboolean
eqwin_point_in_rect(gint x, gint y, gint rx, gint ry, gint rw, gint rh)
{
    return x >= rx && x < rx + rw && y >= ry && y < ry + rh;
}

static EqControl
eqwin_control_at(gint x, gint y)
{
    if (eqwin_point_in_rect(x, y, EQWIN_ON_X, EQWIN_ON_Y,
                            EQWIN_ON_W, EQWIN_ON_H))
        return EQ_CONTROL_ON;
    if (eqwin_point_in_rect(x, y, EQWIN_AUTO_X, EQWIN_AUTO_Y,
                            EQWIN_AUTO_W, EQWIN_AUTO_H))
        return EQ_CONTROL_AUTO;
    if (eqwin_point_in_rect(x, y, EQWIN_PRESETS_X, EQWIN_PRESETS_Y,
                            EQWIN_PRESETS_W, EQWIN_PRESETS_H))
        return EQ_CONTROL_PRESETS;
    return EQ_CONTROL_NONE;
}

static void
eqwin_draw_toggle_button(cairo_t *cr, gboolean selected, gboolean pressed,
                         gint nux, gint nuy, gint pux, gint puy,
                         gint nsx, gint nsy, gint psx, gint psy,
                         gint dx, gint dy, gint w, gint h)
{
    gint sx, sy;

    if (selected) {
        sx = pressed ? psx : nsx;
        sy = pressed ? psy : nsy;
    } else {
        sx = pressed ? pux : nux;
        sy = pressed ? puy : nuy;
    }

    skin_draw_pixmap(cr, SKIN_EQMAIN, sx, sy, dx, dy, w, h);
}

/* ---- EQ drawing ---- */

static void
draw_eq_slider(cairo_t *cr, gint x, gint pos)
{
    gint track_x = x;
    gint track_y = 38;
    gint track_h = 63;

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

    eqwin_draw_toggle_button(cr, eq_active,
                             eq_pressed_control == EQ_CONTROL_ON &&
                             eq_pressed_inside,
                             10, 119, 128, 119, 69, 119, 187, 119,
                             EQWIN_ON_X, EQWIN_ON_Y,
                             EQWIN_ON_W, EQWIN_ON_H);
    eqwin_draw_toggle_button(cr, eq_auto,
                             eq_pressed_control == EQ_CONTROL_AUTO &&
                             eq_pressed_inside,
                             35, 119, 153, 119, 94, 119, 212, 119,
                             EQWIN_AUTO_X, EQWIN_AUTO_Y,
                             EQWIN_AUTO_W, EQWIN_AUTO_H);
    skin_draw_pixmap(cr, SKIN_EQMAIN,
                     224, (eq_pressed_control == EQ_CONTROL_PRESETS &&
                           eq_pressed_inside) ? 176 : 164,
                     EQWIN_PRESETS_X, EQWIN_PRESETS_Y,
                     EQWIN_PRESETS_W, EQWIN_PRESETS_H);

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

    if (button == 1 &&
        eqwin_point_in_rect(sx, sy, EQWIN_DETACH_BTN_X, EQWIN_DETACH_BTN_Y,
                            EQWIN_DETACH_BTN_W, EQWIN_DETACH_BTN_H)) {
        equalizerwin_set_detached(!cfg.equalizer_detached);
        return;
    }

    EqControl control = eqwin_control_at(sx, sy);
    if (control != EQ_CONTROL_NONE) {
        eq_pressed_control = control;
        eq_pressed_inside = TRUE;
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
        GtkWidget *move_window = cfg.equalizer_detached ?
            eqwin_floating_window : mainwin;
        GdkSurface *surface = move_window ?
            gtk_native_get_surface(GTK_NATIVE(move_window)) : NULL;
        if (surface && GDK_IS_TOPLEVEL(surface)) {
            GdkDevice *device = gtk_gesture_get_device(GTK_GESTURE(gesture));
            guint32 timestamp = gtk_event_controller_get_current_event_time(
                GTK_EVENT_CONTROLLER(gesture));
            graphene_point_t point = GRAPHENE_POINT_INIT(x, y);
            graphene_point_t translated = point;
            if (!cfg.equalizer_detached && mainwin &&
                !gtk_widget_compute_point(eqwin_drawing_area, mainwin,
                                          &point, &translated))
                translated = point;
            gdk_toplevel_begin_move(GDK_TOPLEVEL(surface), device,
                                    button, translated.x, translated.y, timestamp);
        }
    }
}

static void
eqwin_click_released(GtkGestureClick *gesture, int n_press,
                     double x, double y, gpointer data)
{
    (void)n_press; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);
    (void)gesture;

    if (eq_pressed_control != EQ_CONTROL_NONE) {
        EqControl released_control = eqwin_control_at(sx, sy);
        EqControl pressed_control = eq_pressed_control;
        gboolean activate = released_control == pressed_control;

        eq_pressed_control = EQ_CONTROL_NONE;
        eq_pressed_inside = FALSE;

        if (activate) {
            if (pressed_control == EQ_CONTROL_ON) {
                eq_active = !eq_active;
                apply_eq();
            } else if (pressed_control == EQ_CONTROL_AUTO) {
                eq_auto = !eq_auto;
            } else if (pressed_control == EQ_CONTROL_PRESETS) {
                eqwin_show_presets_menu();
            }
        }
        eqwin_queue_draw();
        return;
    }

    eq_dragging = -1;
}

static void
eqwin_motion(GtkEventControllerMotion *controller,
             double x, double y, gpointer data)
{
    (void)controller; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);

    if (eq_pressed_control != EQ_CONTROL_NONE) {
        gboolean was_inside = eq_pressed_inside;
        eq_pressed_inside = eqwin_control_at(sx, sy) == eq_pressed_control;
        if (was_inside != eq_pressed_inside)
            eqwin_queue_draw();
        return;
    }

    if (eq_dragging < 0)
        return;
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
equalizerwin_set_state(gboolean active, gboolean automatic,
                       gint preamp_pos, const gint band_pos[10])
{
    eq_active = active;
    eq_auto = automatic;
    eq_preamp_pos = CLAMP(preamp_pos, 0, 100);
    for (gint i = 0; i < 10; i++)
        eq_slider_pos[i] = CLAMP(band_pos[i], 0, 100);

    apply_eq();
    eqwin_queue_draw();
}

void
equalizerwin_get_state(gboolean *active, gboolean *automatic,
                       gint *preamp_pos, gint band_pos[10])
{
    if (active)
        *active = eq_active;
    if (automatic)
        *automatic = eq_auto;
    if (preamp_pos)
        *preamp_pos = eq_preamp_pos;
    if (band_pos) {
        for (gint i = 0; i < 10; i++)
            band_pos[i] = eq_slider_pos[i];
    }
}

static void
eqwin_apply_preset(gint preset)
{
    eq_preamp_pos = 50;
    for (gint i = 0; i < 10; i++)
        eq_slider_pos[i] = 50;

    switch (preset) {
    case 1: /* Bass boost */
        eq_slider_pos[0] = 25;
        eq_slider_pos[1] = 30;
        eq_slider_pos[2] = 40;
        break;
    case 2: /* Treble boost */
        eq_slider_pos[7] = 40;
        eq_slider_pos[8] = 30;
        eq_slider_pos[9] = 25;
        break;
    case 3: /* Rock */
        eq_slider_pos[0] = 30;
        eq_slider_pos[1] = 35;
        eq_slider_pos[4] = 60;
        eq_slider_pos[5] = 60;
        eq_slider_pos[8] = 35;
        eq_slider_pos[9] = 30;
        break;
    default:
        break;
    }

    apply_eq();
    eqwin_queue_draw();
}

static void
eqwin_preset_clicked(GtkButton *button, gpointer data)
{
    (void)button;
    eqwin_apply_preset(GPOINTER_TO_INT(data));
    if (eq_presets_popover) {
        gtk_popover_popdown(GTK_POPOVER(eq_presets_popover));
        gtk_widget_unparent(eq_presets_popover);
        eq_presets_popover = NULL;
    }
}

static void
eqwin_show_presets_menu(void)
{
    if (!eqwin_drawing_area)
        return;

    if (eq_presets_popover) {
        gtk_popover_popdown(GTK_POPOVER(eq_presets_popover));
        gtk_widget_unparent(eq_presets_popover);
        eq_presets_popover = NULL;
    }

    eq_presets_popover = gtk_popover_new();
    gtk_widget_set_parent(eq_presets_popover, eqwin_drawing_area);
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_popover_set_child(GTK_POPOVER(eq_presets_popover), box);

    static const struct {
        const gchar *label;
        gint preset;
    } presets[] = {
        { "Flat", 0 },
        { "Bass Boost", 1 },
        { "Treble Boost", 2 },
        { "Rock", 3 },
    };

    for (guint i = 0; i < G_N_ELEMENTS(presets); i++) {
        GtkWidget *button = gtk_button_new_with_label(presets[i].label);
        g_signal_connect(button, "clicked",
                         G_CALLBACK(eqwin_preset_clicked),
                         GINT_TO_POINTER(presets[i].preset));
        gtk_box_append(GTK_BOX(box), button);
    }

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    GdkRectangle rect = {
        EQWIN_PRESETS_X * scale,
        (EQWIN_PRESETS_Y + EQWIN_PRESETS_H) * scale,
        EQWIN_PRESETS_W * scale,
        1
    };
    gtk_popover_set_pointing_to(GTK_POPOVER(eq_presets_popover), &rect);
    gtk_popover_popup(GTK_POPOVER(eq_presets_popover));
}

static gboolean
eqwin_floating_close_cb(GtkWindow *window, gpointer data)
{
    (void)window; (void)data;
    equalizerwin_show(FALSE);
    return TRUE;
}

static void
eqwin_ensure_floating_window(void)
{
    if (eqwin_floating_window)
        return;

    eqwin_floating_window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(eqwin_floating_window),
                         "XMMS Resuscitated - Equalizer");
    gtk_window_set_decorated(GTK_WINDOW(eqwin_floating_window), FALSE);
    gtk_window_set_resizable(GTK_WINDOW(eqwin_floating_window), FALSE);
    if (mainwin) {
        gtk_window_set_transient_for(GTK_WINDOW(eqwin_floating_window),
                                     GTK_WINDOW(mainwin));
        gtk_window_set_destroy_with_parent(GTK_WINDOW(eqwin_floating_window),
                                           TRUE);
    }
    g_signal_connect(eqwin_floating_window, "close-request",
                     G_CALLBACK(eqwin_floating_close_cb), NULL);
}

static void
eqwin_attach_widget(void)
{
    if (!eqwin_drawing_area || !mainwin_container)
        return;

    GtkWidget *parent = gtk_widget_get_parent(eqwin_drawing_area);
    if (parent == mainwin_container)
        return;

    g_object_ref(eqwin_drawing_area);
    if (parent == eqwin_floating_window)
        gtk_window_set_child(GTK_WINDOW(eqwin_floating_window), NULL);
    else if (parent)
        gtk_widget_unparent(eqwin_drawing_area);

    gtk_box_insert_child_after(GTK_BOX(mainwin_container), eqwin_drawing_area,
                               mainwin_drawing_area);
    g_object_unref(eqwin_drawing_area);
}

static void
eqwin_detach_widget(void)
{
    if (!eqwin_drawing_area)
        return;

    eqwin_ensure_floating_window();
    GtkWidget *parent = gtk_widget_get_parent(eqwin_drawing_area);
    if (parent == eqwin_floating_window)
        return;

    g_object_ref(eqwin_drawing_area);
    if (parent == mainwin_container)
        gtk_box_remove(GTK_BOX(mainwin_container), eqwin_drawing_area);
    else if (parent)
        gtk_widget_unparent(eqwin_drawing_area);

    gtk_window_set_child(GTK_WINDOW(eqwin_floating_window), eqwin_drawing_area);
    g_object_unref(eqwin_drawing_area);
}

void
equalizerwin_create(GtkApplication *app)
{
    (void)app;

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
    gtk_widget_set_visible(eqwin_drawing_area, FALSE);
    equalizerwin = eqwin_drawing_area;

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

GtkWidget *
equalizerwin_get_widget(void)
{
    return eqwin_drawing_area;
}

void
equalizerwin_show(gboolean show)
{
    if (!eqwin_drawing_area)
        return;
    cfg.equalizer_visible = show;
    if (cfg.equalizer_detached) {
        eqwin_detach_widget();
        gtk_widget_set_visible(eqwin_drawing_area, TRUE);
        if (show)
            gtk_window_present(GTK_WINDOW(eqwin_floating_window));
        else if (eqwin_floating_window)
            gtk_widget_set_visible(eqwin_floating_window, FALSE);
    } else {
        eqwin_attach_widget();
        if (eqwin_floating_window)
            gtk_widget_set_visible(eqwin_floating_window, FALSE);
        gtk_widget_set_visible(eqwin_drawing_area, show);
    }
    mainwin_update_attached_size();
    mainwin_update_panel_toggles();
    eqwin_queue_draw();
}

gboolean
equalizerwin_is_visible(void)
{
    if (!eqwin_drawing_area)
        return FALSE;
    if (cfg.equalizer_detached)
        return eqwin_floating_window &&
            gtk_widget_get_visible(eqwin_floating_window);
    return gtk_widget_get_visible(eqwin_drawing_area);
}

void
equalizerwin_set_detached(gboolean detached)
{
    if (!eqwin_drawing_area) {
        cfg.equalizer_detached = detached;
        return;
    }

    gboolean visible = equalizerwin_is_visible();
    cfg.equalizer_detached = detached;
    if (detached) {
        eqwin_detach_widget();
        gtk_widget_set_visible(eqwin_drawing_area, TRUE);
        if (visible)
            gtk_window_present(GTK_WINDOW(eqwin_floating_window));
    } else {
        if (eqwin_floating_window)
            gtk_widget_set_visible(eqwin_floating_window, FALSE);
        eqwin_attach_widget();
        gtk_widget_set_visible(eqwin_drawing_area, visible);
    }
    cfg.equalizer_visible = visible;
    mainwin_update_attached_size();
    mainwin_update_panel_toggles();
    eqwin_queue_draw();
}

gboolean
equalizerwin_is_detached(void)
{
    return cfg.equalizer_detached;
}

gint
equalizerwin_height(void)
{
    return EQWIN_HEIGHT;
}
