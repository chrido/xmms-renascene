#include "xmms.h"
#include "playlistwin.h"

#define PLWIN_WIDTH   275
#define PLWIN_HEIGHT  232
#define PLWIN_ENTRY_HEIGHT 11
#define PLWIN_LIST_X  12
#define PLWIN_LIST_Y  20
#define PLWIN_LIST_W  (PLWIN_WIDTH - 32)
#define PLWIN_LIST_H  (PLWIN_HEIGHT - 58)
#define PLWIN_SCROLLBAR_X  (PLWIN_WIDTH - 16)
#define PLWIN_SCROLLBAR_W  8
#define PLWIN_SCROLL_THUMB_H 18
#define PLWIN_DETACH_BTN_X 250
#define PLWIN_DETACH_BTN_Y 3
#define PLWIN_DETACH_BTN_W 13
#define PLWIN_DETACH_BTN_H 10

GtkWidget *playlistwin = NULL;
static GtkWidget *plwin_drawing_area = NULL;
static GtkWidget *plwin_floating_window = NULL;
static GList *plwin_wlist = NULL;

static gint plwin_scroll_offset = 0;
static gint plwin_selected = -1;
static gboolean plwin_scrollbar_dragging = FALSE;
static gint plwin_scrollbar_drag_offset = 0;
static gdouble plwin_scroll_delta = 0.0;

/* Forward declarations */
static void plwin_queue_draw(void);
static void plwin_set_scroll_offset(gint offset);
static gint plwin_visible_entries(void);
static gint plwin_max_scroll_offset(void);
static gboolean plwin_scrollbar_geometry(gint *thumb_y, gint *thumb_h);
static void plwin_scrollbar_set_from_y(gint y);

/* ---- Playlist rendering ---- */

static gint
plwin_visible_entries(void)
{
    return PLWIN_LIST_H / PLWIN_ENTRY_HEIGHT;
}

static gint
plwin_max_scroll_offset(void)
{
    return MAX(0, playlist_get_length() - plwin_visible_entries());
}

static void
plwin_set_scroll_offset(gint offset)
{
    plwin_scroll_offset = CLAMP(offset, 0, plwin_max_scroll_offset());
}

static gboolean
plwin_scrollbar_geometry(gint *thumb_y, gint *thumb_h)
{
    gint total = playlist_get_length();
    gint visible = plwin_visible_entries();
    if (total <= visible)
        return FALSE;

    gint track_h = PLWIN_LIST_H;
    gint max_scroll = total - visible;
    gint max_thumb_y = PLWIN_LIST_Y + track_h - PLWIN_SCROLL_THUMB_H;

    if (thumb_h)
        *thumb_h = PLWIN_SCROLL_THUMB_H;
    if (thumb_y) {
        *thumb_y = PLWIN_LIST_Y +
            (plwin_scroll_offset * (track_h - PLWIN_SCROLL_THUMB_H)) /
            max_scroll;
        *thumb_y = CLAMP(*thumb_y, PLWIN_LIST_Y, max_thumb_y);
    }
    return TRUE;
}

static void
plwin_scrollbar_set_from_y(gint y)
{
    gint total = playlist_get_length();
    gint visible = plwin_visible_entries();
    if (total <= visible)
        return;

    gint track_h = PLWIN_LIST_H;
    gint max_scroll = total - visible;
    gint max_thumb_pos = track_h - PLWIN_SCROLL_THUMB_H;
    gint thumb_pos = CLAMP(y - PLWIN_LIST_Y - plwin_scrollbar_drag_offset,
                           0, max_thumb_pos);

    if (max_thumb_pos <= 0)
        plwin_set_scroll_offset(0);
    else
        plwin_set_scroll_offset((thumb_pos * max_scroll +
                                 max_thumb_pos / 2) / max_thumb_pos);
}

static void
draw_playlist_entries(cairo_t *cr)
{
    gint list_x = PLWIN_LIST_X, list_y = PLWIN_LIST_Y;
    gint list_w = PLWIN_LIST_W, list_h = PLWIN_LIST_H;
    gint visible = plwin_visible_entries();
    gint total = playlist_get_length();
    gint current = playlist_get_position();

    /* Background */
    gdk_cairo_set_source_rgba(cr, &skin->pledit_normalbg);
    cairo_rectangle(cr, list_x, list_y, list_w, list_h);
    cairo_fill(cr);

    /* Entries */
    cairo_select_font_face(cr, "Sans", CAIRO_FONT_SLANT_NORMAL,
                           CAIRO_FONT_WEIGHT_NORMAL);
    cairo_set_font_size(cr, 9);

    for (gint i = 0; i < visible && (i + plwin_scroll_offset) < total; i++) {
        gint idx = i + plwin_scroll_offset;
        gint y = list_y + i * PLWIN_ENTRY_HEIGHT;

        /* Selection highlight */
        if (idx == plwin_selected) {
            gdk_cairo_set_source_rgba(cr, &skin->pledit_selectedbg);
            cairo_rectangle(cr, list_x, y, list_w, PLWIN_ENTRY_HEIGHT);
            cairo_fill(cr);
        }

        /* Text color */
        if (idx == current)
            gdk_cairo_set_source_rgba(cr, &skin->pledit_current);
        else
            gdk_cairo_set_source_rgba(cr, &skin->pledit_normal);

        const gchar *title = playlist_get_title(idx);
        if (title) {
            gchar *display = g_strdup_printf("%d. %s", idx + 1, title);

            /* Clip to list area (leave room for duration) */
            cairo_save(cr);
            cairo_rectangle(cr, list_x, y, list_w - 40, PLWIN_ENTRY_HEIGHT);
            cairo_clip(cr);
            cairo_move_to(cr, list_x + 2, y + 9);
            cairo_show_text(cr, display);
            cairo_restore(cr);

            g_free(display);
        }

        /* Duration */
        PlaylistEntry *entry = playlist_get_entry(idx);
        if (entry && entry->length > 0) {
            gchar *dur = time_to_string(entry->length);
            cairo_text_extents_t ext;
            cairo_text_extents(cr, dur, &ext);
            cairo_move_to(cr, list_x + list_w - ext.width - 4, y + 9);
            cairo_show_text(cr, dur);
            g_free(dur);
        }
    }

    /* Scrollbar thumb */
    gint thumb_y, thumb_h;
    if (plwin_scrollbar_geometry(&thumb_y, &thumb_h)) {
        skin_draw_pixmap(cr, SKIN_PLEDIT,
                         plwin_scrollbar_dragging ? 52 : 61, 53,
                         PLWIN_SCROLLBAR_X, thumb_y,
                         PLWIN_SCROLLBAR_W, thumb_h);
    }
}

static void
draw_playlist_frame(cairo_t *cr)
{
    gint w = PLWIN_WIDTH, h = PLWIN_HEIGHT;
    SkinIndex src = SKIN_PLEDIT;
    gint y = 0; /* focused titlebar; would be 21 for unfocused */

    gdk_cairo_set_source_rgba(cr, &skin->pledit_normalbg);
    cairo_rectangle(cr, 0, 0, w, h);
    cairo_fill(cr);

    /* Titlebar left corner */
    skin_draw_pixmap(cr, src, 0, y, 0, 0, 25, 20);

    /* Titlebar tiled fill */
    gint c = (w - 150) / 25;
    for (gint i = 0; i < c / 2; i++) {
        skin_draw_pixmap(cr, src, 127, y, (i * 25) + 25, 0, 25, 20);
        skin_draw_pixmap(cr, src, 127, y, (i * 25) + (w / 2) + 50, 0, 25, 20);
    }
    if (c & 1) {
        skin_draw_pixmap(cr, src, 127, y, ((c / 2) * 25) + 25, 0, 12, 20);
        skin_draw_pixmap(cr, src, 127, y, (w / 2) + ((c / 2) * 25) + 50, 0, 13, 20);
    }

    /* Titlebar title */
    skin_draw_pixmap(cr, src, 26, y, (w / 2) - 50, 0, 100, 20);

    /* Titlebar right corner */
    skin_draw_pixmap(cr, src, 153, y, w - 25, 0, 25, 20);

    /* Left and right sides */
    const gint side_src_y = 52;
    const gint side_y = 20;
    const gint side_h = h - 58;
    for (gint ydest = side_y; ydest < side_y + side_h; ydest++) {
        skin_draw_pixmap(cr, src, 0, side_src_y, 0, ydest, 12, 1);
        skin_draw_pixmap(cr, src, 32, side_src_y, w - 19, ydest, 19, 1);
    }

    /* Bottom left corner (menu buttons) */
    skin_draw_pixmap(cr, src, 0, 72, 0, h - 38, 125, 38);

    /* Bottom blank filler */
    c = (w - 275) / 25;
    if (c >= 3) {
        c -= 3;
        skin_draw_pixmap(cr, src, 205, 0, w - 225, h - 38, 75, 38);
    }
    for (gint i = 0; i < c; i++)
        skin_draw_pixmap(cr, src, 179, 0, (i * 25) + 125, h - 38, 25, 38);

    /* Bottom right corner */
    skin_draw_pixmap(cr, src, 126, 72, w - 150, h - 38, 150, 38);

    /* The original playlist overlays blank time text fields here. */
    cairo_set_source_rgb(cr, 10.0 / 255.0, 18.0 / 255.0, 26.0 / 255.0);
    cairo_rectangle(cr, w - 82, h - 15, 28, 9);
    cairo_fill(cr);
}

static void
draw_playlist_window(GtkDrawingArea *area, cairo_t *cr,
                     int width, int height, gpointer data)
{
    (void)area; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    cairo_scale(cr, (double)width / PLWIN_WIDTH,
                    (double)height / PLWIN_HEIGHT);

    /* Draw assembled playlist frame from skin pieces */
    draw_playlist_frame(cr);

    /* Draw entries */
    draw_playlist_entries(cr);

    /* Draw all custom widgets */
    widget_list_draw(plwin_wlist, cr);
}

/* ---- Event handling ---- */

static void
plwin_click_pressed(GtkGestureClick *gesture, int n_press,
                    double x, double y, gpointer data)
{
    (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);
    gint button = gtk_gesture_single_get_current_button(
        GTK_GESTURE_SINGLE(gesture));

    /* Check list area click */
    gint list_x = PLWIN_LIST_X, list_y = PLWIN_LIST_Y;
    gint list_w = PLWIN_LIST_W, list_h = PLWIN_LIST_H;

    gint thumb_y, thumb_h;
    gboolean has_scrollbar = plwin_scrollbar_geometry(&thumb_y, &thumb_h);

    if (button == 1 && has_scrollbar &&
        sx >= PLWIN_SCROLLBAR_X && sx < PLWIN_SCROLLBAR_X + PLWIN_SCROLLBAR_W &&
        sy >= list_y && sy < list_y + list_h) {
        plwin_scrollbar_dragging = TRUE;
        if (sy >= thumb_y && sy < thumb_y + thumb_h)
            plwin_scrollbar_drag_offset = sy - thumb_y;
        else {
            plwin_scrollbar_drag_offset = thumb_h / 2;
            plwin_scrollbar_set_from_y(sy);
        }
        plwin_queue_draw();
        return;
    }

    if (button == 1 &&
        sx >= PLWIN_DETACH_BTN_X && sx < PLWIN_DETACH_BTN_X + PLWIN_DETACH_BTN_W &&
        sy >= PLWIN_DETACH_BTN_Y && sy < PLWIN_DETACH_BTN_Y + PLWIN_DETACH_BTN_H) {
        playlistwin_set_detached(!cfg.playlist_detached);
        return;
    }

    if (sx >= list_x && sx < list_x + list_w &&
        sy >= list_y && sy < list_y + list_h) {
        gint entry_idx = (sy - list_y) / PLWIN_ENTRY_HEIGHT + plwin_scroll_offset;
        if (entry_idx < playlist_get_length()) {
            plwin_selected = entry_idx;
            if (n_press == 2 && button == 1) {
                /* Double click - play this entry */
                playlist_set_position(entry_idx);
                playlist_play();
            }
            plwin_queue_draw();
        }
        return;
    }

    /* Titlebar close button (top-right corner, 9x9) */
    if (sx >= PLWIN_WIDTH - 11 && sx < PLWIN_WIDTH - 2 && sy >= 3 && sy < 12) {
        playlistwin_show(FALSE);
        return;
    }

    /* Titlebar drag */
    if (sy < 20) {
        GtkWidget *move_window = cfg.playlist_detached ?
            plwin_floating_window : mainwin;
        GdkSurface *surface = move_window ?
            gtk_native_get_surface(GTK_NATIVE(move_window)) : NULL;
        if (surface && GDK_IS_TOPLEVEL(surface)) {
            GdkDevice *device = gtk_gesture_get_device(GTK_GESTURE(gesture));
            guint32 timestamp = gtk_event_controller_get_current_event_time(
                GTK_EVENT_CONTROLLER(gesture));
            graphene_point_t point = GRAPHENE_POINT_INIT(x, y);
            graphene_point_t translated = point;
            if (!cfg.playlist_detached && mainwin &&
                !gtk_widget_compute_point(plwin_drawing_area, mainwin,
                                          &point, &translated))
                translated = point;
            gdk_toplevel_begin_move(GDK_TOPLEVEL(surface), device,
                                    button, translated.x, translated.y, timestamp);
        }
    }
}

static void
plwin_click_released(GtkGestureClick *gesture, int n_press,
                     double x, double y, gpointer data)
{
    (void)gesture; (void)n_press; (void)x; (void)y; (void)data;

    if (plwin_scrollbar_dragging) {
        plwin_scrollbar_dragging = FALSE;
        plwin_queue_draw();
    }
}

static void
plwin_motion(GtkEventControllerMotion *controller,
             double x, double y, gpointer data)
{
    (void)controller; (void)x; (void)data;

    if (!plwin_scrollbar_dragging)
        return;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    plwin_scrollbar_set_from_y((gint)(y / scale));
    plwin_queue_draw();
}

static gboolean
plwin_scroll(GtkEventControllerScroll *controller,
             double dx, double dy, gpointer data)
{
    (void)controller; (void)dx; (void)data;

    plwin_scroll_delta += dy * 3.0;
    gint scroll_steps = (gint)plwin_scroll_delta;
    if (scroll_steps != 0) {
        plwin_set_scroll_offset(plwin_scroll_offset + scroll_steps);
        plwin_scroll_delta -= scroll_steps;
        plwin_queue_draw();
    }

    return GDK_EVENT_STOP;
}

/* ---- Public API ---- */

static void
plwin_queue_draw(void)
{
    if (plwin_drawing_area)
        gtk_widget_queue_draw(plwin_drawing_area);
}

static gboolean
plwin_drop_cb(GtkDropTarget *target, const GValue *value,
              double x, double y, gpointer data)
{
    (void)target; (void)x; (void)y; (void)data;

    if (!G_VALUE_HOLDS(value, GDK_TYPE_FILE_LIST))
        return FALSE;

    GSList *files = g_value_get_boxed(value);
    for (GSList *l = files; l; l = l->next) {
        GFile *file = l->data;
        gchar *path = g_file_get_path(file);
        if (path) {
            if (g_file_test(path, G_FILE_TEST_IS_DIR))
                playlist_add_dir(path);
            else
                playlist_add(path);
            g_free(path);
        }
    }
    plwin_queue_draw();
    return TRUE;
}

static gboolean
plwin_floating_close_cb(GtkWindow *window, gpointer data)
{
    (void)window; (void)data;
    playlistwin_show(FALSE);
    return TRUE;
}

static void
plwin_ensure_floating_window(void)
{
    if (plwin_floating_window)
        return;

    plwin_floating_window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(plwin_floating_window),
                         "XMMS Resuscitated - Playlist");
    gtk_window_set_decorated(GTK_WINDOW(plwin_floating_window), FALSE);
    gtk_window_set_resizable(GTK_WINDOW(plwin_floating_window), FALSE);
    if (mainwin) {
        gtk_window_set_transient_for(GTK_WINDOW(plwin_floating_window),
                                     GTK_WINDOW(mainwin));
        gtk_window_set_destroy_with_parent(GTK_WINDOW(plwin_floating_window),
                                           TRUE);
    }
    g_signal_connect(plwin_floating_window, "close-request",
                     G_CALLBACK(plwin_floating_close_cb), NULL);
}

static void
plwin_attach_widget(void)
{
    if (!plwin_drawing_area || !mainwin_container)
        return;

    GtkWidget *parent = gtk_widget_get_parent(plwin_drawing_area);
    if (parent == mainwin_container)
        return;

    g_object_ref(plwin_drawing_area);
    if (parent == plwin_floating_window)
        gtk_window_set_child(GTK_WINDOW(plwin_floating_window), NULL);
    else if (parent)
        gtk_widget_unparent(plwin_drawing_area);

    gtk_box_append(GTK_BOX(mainwin_container), plwin_drawing_area);
    g_object_unref(plwin_drawing_area);
}

static void
plwin_detach_widget(void)
{
    if (!plwin_drawing_area)
        return;

    plwin_ensure_floating_window();
    GtkWidget *parent = gtk_widget_get_parent(plwin_drawing_area);
    if (parent == plwin_floating_window)
        return;

    g_object_ref(plwin_drawing_area);
    if (parent == mainwin_container)
        gtk_box_remove(GTK_BOX(mainwin_container), plwin_drawing_area);
    else if (parent)
        gtk_widget_unparent(plwin_drawing_area);

    gtk_window_set_child(GTK_WINDOW(plwin_floating_window), plwin_drawing_area);
    g_object_unref(plwin_drawing_area);
}

void
playlistwin_create(GtkApplication *app)
{
    (void)app;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 2;

    plwin_drawing_area = gtk_drawing_area_new();
    gtk_drawing_area_set_content_width(
        GTK_DRAWING_AREA(plwin_drawing_area), PLWIN_WIDTH * scale);
    gtk_drawing_area_set_content_height(
        GTK_DRAWING_AREA(plwin_drawing_area), PLWIN_HEIGHT * scale);
    gtk_drawing_area_set_draw_func(
        GTK_DRAWING_AREA(plwin_drawing_area),
        draw_playlist_window, NULL, NULL);
    gtk_widget_set_visible(plwin_drawing_area, FALSE);
    playlistwin = plwin_drawing_area;

    /* Click events */
    GtkGesture *click = gtk_gesture_click_new();
    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(click), 0);
    g_signal_connect(click, "pressed", G_CALLBACK(plwin_click_pressed), NULL);
    g_signal_connect(click, "released", G_CALLBACK(plwin_click_released), NULL);
    gtk_widget_add_controller(plwin_drawing_area,
                              GTK_EVENT_CONTROLLER(click));

    GtkEventController *motion = gtk_event_controller_motion_new();
    g_signal_connect(motion, "motion", G_CALLBACK(plwin_motion), NULL);
    gtk_widget_add_controller(plwin_drawing_area, motion);

    /* Scroll events */
    GtkEventController *scroll = gtk_event_controller_scroll_new(
        GTK_EVENT_CONTROLLER_SCROLL_VERTICAL);
    g_signal_connect(scroll, "scroll", G_CALLBACK(plwin_scroll), NULL);
    gtk_widget_add_controller(plwin_drawing_area, scroll);

    /* Drag and drop */
    GtkDropTarget *drop = gtk_drop_target_new(GDK_TYPE_FILE_LIST,
                                               GDK_ACTION_COPY);
    g_signal_connect(drop, "drop", G_CALLBACK(plwin_drop_cb), NULL);
    gtk_widget_add_controller(plwin_drawing_area,
                              GTK_EVENT_CONTROLLER(drop));
}

GtkWidget *
playlistwin_get_widget(void)
{
    return plwin_drawing_area;
}

void
playlistwin_show(gboolean show)
{
    if (!plwin_drawing_area)
        return;
    cfg.playlist_visible = show;
    if (cfg.playlist_detached) {
        plwin_detach_widget();
        gtk_widget_set_visible(plwin_drawing_area, TRUE);
        if (show)
            gtk_window_present(GTK_WINDOW(plwin_floating_window));
        else if (plwin_floating_window)
            gtk_widget_set_visible(plwin_floating_window, FALSE);
    } else {
        plwin_attach_widget();
        if (plwin_floating_window)
            gtk_widget_set_visible(plwin_floating_window, FALSE);
        gtk_widget_set_visible(plwin_drawing_area, show);
    }
    mainwin_update_attached_size();
    mainwin_update_panel_toggles();
    plwin_queue_draw();
}

gboolean
playlistwin_is_visible(void)
{
    if (!plwin_drawing_area)
        return FALSE;
    if (cfg.playlist_detached)
        return plwin_floating_window &&
            gtk_widget_get_visible(plwin_floating_window);
    return gtk_widget_get_visible(plwin_drawing_area);
}

void
playlistwin_set_detached(gboolean detached)
{
    if (!plwin_drawing_area) {
        cfg.playlist_detached = detached;
        return;
    }

    gboolean visible = playlistwin_is_visible();
    cfg.playlist_detached = detached;
    if (detached) {
        plwin_detach_widget();
        gtk_widget_set_visible(plwin_drawing_area, TRUE);
        if (visible)
            gtk_window_present(GTK_WINDOW(plwin_floating_window));
    } else {
        if (plwin_floating_window)
            gtk_widget_set_visible(plwin_floating_window, FALSE);
        plwin_attach_widget();
        gtk_widget_set_visible(plwin_drawing_area, visible);
    }
    cfg.playlist_visible = visible;
    mainwin_update_attached_size();
    mainwin_update_panel_toggles();
    plwin_queue_draw();
}

gboolean
playlistwin_is_detached(void)
{
    return cfg.playlist_detached;
}

gint
playlistwin_height(void)
{
    return PLWIN_HEIGHT;
}

void
playlistwin_update(void)
{
    if (playlistwin_is_visible())
        plwin_queue_draw();
}
