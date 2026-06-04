#include "xmms.h"
#include "playlistwin.h"

#define PLWIN_WIDTH   275
#define PLWIN_HEIGHT  232
#define PLWIN_ENTRY_HEIGHT 11

GtkWidget *playlistwin = NULL;
static GtkWidget *plwin_drawing_area = NULL;
static GList *plwin_wlist = NULL;

static gint plwin_scroll_offset = 0;
static gint plwin_selected = -1;

/* Forward declarations */
static void plwin_queue_draw(void);

/* ---- Playlist rendering ---- */

static void
draw_playlist_entries(cairo_t *cr)
{
    gint list_x = 12, list_y = 20;
    gint list_w = PLWIN_WIDTH - 32, list_h = PLWIN_HEIGHT - 58;
    gint visible = list_h / PLWIN_ENTRY_HEIGHT;
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

    /* Scrollbar track */
    gint sb_x = PLWIN_WIDTH - 20;
    gint sb_h = list_h;
    cairo_set_source_rgb(cr, 0.15, 0.15, 0.15);
    cairo_rectangle(cr, sb_x, list_y, 8, sb_h);
    cairo_fill(cr);

    /* Scrollbar thumb */
    if (total > visible) {
        gint thumb_h = MAX(20, (visible * sb_h) / total);
        gint thumb_y = list_y + (plwin_scroll_offset * (sb_h - thumb_h)) /
                       (total - visible);
        cairo_set_source_rgb(cr, 0.5, 0.5, 0.5);
        cairo_rectangle(cr, sb_x, thumb_y, 8, thumb_h);
        cairo_fill(cr);
    }
}

static void
draw_playlist_frame(cairo_t *cr)
{
    gint w = PLWIN_WIDTH, h = PLWIN_HEIGHT;
    SkinIndex src = SKIN_PLEDIT;
    gint y = 0; /* focused titlebar; would be 21 for unfocused */

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
    for (gint i = 0; i < (h - 58) / 29; i++) {
        skin_draw_pixmap(cr, src, 0, 42, 0, (i * 29) + 20, 12, 29);
        skin_draw_pixmap(cr, src, 32, 42, w - 19, (i * 29) + 20, 19, 29);
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
    gint list_x = 12, list_y = 20;
    gint list_w = PLWIN_WIDTH - 32, list_h = PLWIN_HEIGHT - 58;

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
        GdkSurface *surface = gtk_native_get_surface(GTK_NATIVE(playlistwin));
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
plwin_scroll(GtkEventControllerScroll *controller,
             double dx, double dy, gpointer data)
{
    (void)controller; (void)dx; (void)data;

    gint list_h = PLWIN_HEIGHT - 58;
    gint visible = list_h / PLWIN_ENTRY_HEIGHT;
    gint total = playlist_get_length();

    plwin_scroll_offset += (gint)(dy * 3);
    plwin_scroll_offset = CLAMP(plwin_scroll_offset, 0,
                                MAX(0, total - visible));
    plwin_queue_draw();
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

void
playlistwin_create(GtkApplication *app)
{
    playlistwin = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(playlistwin), "XMMS Resuscitated - Playlist");
    gtk_window_set_decorated(GTK_WINDOW(playlistwin), FALSE);
    gtk_window_set_resizable(GTK_WINDOW(playlistwin), FALSE);

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

    gtk_window_set_child(GTK_WINDOW(playlistwin), plwin_drawing_area);

    /* Click events */
    GtkGesture *click = gtk_gesture_click_new();
    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(click), 0);
    g_signal_connect(click, "pressed", G_CALLBACK(plwin_click_pressed), NULL);
    gtk_widget_add_controller(plwin_drawing_area,
                              GTK_EVENT_CONTROLLER(click));

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

void
playlistwin_show(gboolean show)
{
    if (!playlistwin)
        return;
    if (show)
        gtk_window_present(GTK_WINDOW(playlistwin));
    else
        gtk_widget_set_visible(playlistwin, FALSE);
}

gboolean
playlistwin_is_visible(void)
{
    return playlistwin && gtk_widget_get_visible(playlistwin);
}

void
playlistwin_update(void)
{
    if (playlistwin && gtk_widget_get_visible(playlistwin))
        plwin_queue_draw();
}
