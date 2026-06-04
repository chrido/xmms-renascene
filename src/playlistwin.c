#include "xmms.h"
#include "playlistwin.h"
#include <glib/gstdio.h>

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
#define PLWIN_BUTTON_Y      (PLWIN_HEIGHT - 29)
#define PLWIN_BUTTON_H      18
#define PLWIN_BUTTON_W      25
#define PLWIN_BUTTON_SRC_W  22
#define PLWIN_MENU_BORDER_W 3
#define PLWIN_MENU_W        (PLWIN_MENU_BORDER_W + PLWIN_BUTTON_SRC_W)
#define PLWIN_FONT_SIZE     9.0

GtkWidget *playlistwin = NULL;
static GtkWidget *plwin_drawing_area = NULL;
static GtkWidget *plwin_floating_window = NULL;
static GtkWidget *plwin_url_window = NULL;
static GtkWidget *plwin_url_entry = NULL;
static GtkWidget *plwin_sort_popover = NULL;
static GtkWidget *plwin_context_popover = NULL;
static GList *plwin_wlist = NULL;
static TextBox *plwin_time_min = NULL;
static TextBox *plwin_time_sec = NULL;
static TextBox *plwin_info = NULL;
static TextBox *plwin_sinfo = NULL;

typedef enum {
    PLWIN_BUTTON_NONE,
    PLWIN_BUTTON_ADD,
    PLWIN_BUTTON_REMOVE,
    PLWIN_BUTTON_SELECT,
    PLWIN_BUTTON_MISC,
    PLWIN_BUTTON_LIST,
    PLWIN_BUTTON_PREV,
    PLWIN_BUTTON_PLAY,
    PLWIN_BUTTON_PAUSE,
    PLWIN_BUTTON_STOP,
    PLWIN_BUTTON_NEXT,
    PLWIN_BUTTON_EJECT,
    PLWIN_BUTTON_SCROLL_UP,
    PLWIN_BUTTON_SCROLL_DOWN
} PlwinButton;

static gint plwin_scroll_offset = 0;
static gint plwin_selected = -1;
static gint plwin_selection_anchor = -1;
static gboolean plwin_scrollbar_dragging = FALSE;
static gboolean plwin_shaded = FALSE;
static gint plwin_scrollbar_drag_offset = 0;
static gdouble plwin_scroll_delta = 0.0;
static PlwinButton plwin_pressed_button = PLWIN_BUTTON_NONE;
static gboolean plwin_pressed_inside = FALSE;
static Widget *plwin_pressed_widget = NULL;

/* Forward declarations */
static void plwin_queue_draw(void);
static void plwin_set_scroll_offset(gint offset);
static gint plwin_visible_entries(void);
static gint plwin_max_scroll_offset(void);
static gboolean plwin_scrollbar_geometry(gint *thumb_y, gint *thumb_h);
static void plwin_scrollbar_set_from_y(gint y);
static void plwin_activate_button(PlwinButton button);
static void plwin_close_menu(void);
static void plwin_update_info(void);
static void plwin_update_shaded_info(void);
static void plwin_open_files(gboolean replace);
static void plwin_remove_selected(void);
static void plwin_show_context_menu(gint x, gint y);

typedef enum {
    PLWIN_ACTION_ADD_URL,
    PLWIN_ACTION_ADD_FILE,
    PLWIN_ACTION_ADD_DIR,
    PLWIN_ACTION_REMOVE_MISC,
    PLWIN_ACTION_REMOVE_SELECTED,
    PLWIN_ACTION_REMOVE_CROP,
    PLWIN_ACTION_REMOVE_ALL,
    PLWIN_ACTION_REMOVE_DEAD,
    PLWIN_ACTION_PHYSICALLY_DELETE,
    PLWIN_ACTION_SELECT_ALL,
    PLWIN_ACTION_SELECT_NONE,
    PLWIN_ACTION_SELECT_INVERT,
    PLWIN_ACTION_READ_EXTENDED_INFO,
    PLWIN_ACTION_MISC_SORT,
    PLWIN_ACTION_MISC_FILE_INFO,
    PLWIN_ACTION_MISC_OPTIONS,
    PLWIN_ACTION_LIST_NEW,
    PLWIN_ACTION_LIST_LOAD,
    PLWIN_ACTION_LIST_SAVE
} PlwinAction;

static void plwin_menu_action_activate(PlwinAction action);

typedef enum {
    PLWIN_SORT_BY_TITLE,
    PLWIN_SORT_BY_FILENAME,
    PLWIN_SORT_BY_PATH,
    PLWIN_SORT_BY_DATE,
    PLWIN_SORT_SEL_BY_TITLE,
    PLWIN_SORT_SEL_BY_FILENAME,
    PLWIN_SORT_SEL_BY_PATH,
    PLWIN_SORT_SEL_BY_DATE,
    PLWIN_SORT_RANDOMIZE,
    PLWIN_SORT_REVERSE
} PlwinSortAction;

typedef struct {
    PlwinAction action;
    gint normal_x, normal_y;
    gint selected_x, selected_y;
} PlwinMenuItem;

typedef struct {
    gboolean open;
    gboolean pressed;
    PlwinButton button;
    const PlwinMenuItem *items;
    guint n_items;
    gint x, y;
    gint border_x, border_y;
    gint hover;
} PlwinMenu;

static PlwinMenu plwin_menu = { 0 };

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
plwin_set_playlist_font(cairo_t *cr)
{
    /* Original XMMS defaults to -adobe-helvetica-bold-r-*-*-10-*. */
    cairo_select_font_face(cr, cfg.playlist_font && cfg.playlist_font[0] ?
                           cfg.playlist_font : "Helvetica",
                           CAIRO_FONT_SLANT_NORMAL,
                           CAIRO_FONT_WEIGHT_BOLD);
    cairo_set_font_size(cr, PLWIN_FONT_SIZE);
}

static gchar *
plwin_normalize_playlist_text(const gchar *text)
{
    gchar *normalized = g_strdup(text ? text : "");
    gchar *pos;

    if (cfg.convert_underscore) {
        while ((pos = strchr(normalized, '_')) != NULL)
            *pos = ' ';
    }

    if (cfg.convert_twenty) {
        while ((pos = strstr(normalized, "%20")) != NULL) {
            gchar *tail = pos + 3;
            *(pos++) = ' ';
            memmove(pos, tail, strlen(tail) + 1);
        }
    }

    return normalized;
}

static void
plwin_ellipsize_to_width(cairo_t *cr, gchar *text, gint width)
{
    gint len = strlen(text);
    cairo_text_extents_t ext;

    while (len > 4) {
        cairo_text_extents(cr, text, &ext);
        if (ext.width <= width)
            return;

        len--;
        text[len - 3] = '.';
        text[len - 2] = '.';
        text[len - 1] = '.';
        text[len] = '\0';
    }
}

static gchar *
plwin_format_duration(gint64 milliseconds, gboolean more)
{
    if (milliseconds <= 0 && more)
        return g_strdup("?");

    gint64 seconds = MAX((gint64)0, milliseconds / 1000);
    if (seconds > 3600)
        return g_strdup_printf("%" G_GINT64_FORMAT ":%02" G_GINT64_FORMAT ":%02" G_GINT64_FORMAT "%s",
                               seconds / 3600, (seconds / 60) % 60,
                               seconds % 60, more ? "+" : "");
    return g_strdup_printf("%" G_GINT64_FORMAT ":%02" G_GINT64_FORMAT "%s",
                           seconds / 60, seconds % 60, more ? "+" : "");
}

static void
plwin_update_info(void)
{
    if (!plwin_info)
        return;

    gint64 total = 0;
    gint64 selected = 0;
    gboolean total_more = FALSE;
    gboolean selected_more = FALSE;

    for (gint i = 0; i < playlist_get_length(); i++) {
        PlaylistEntry *entry = playlist_get_entry(i);
        if (!entry)
            continue;

        if (entry->length >= 0)
            total += entry->length;
        else
            total_more = TRUE;

        if (entry->selected || i == plwin_selected) {
            if (entry->length >= 0)
                selected += entry->length;
            else
                selected_more = TRUE;
        }
    }

    gchar *selected_text = plwin_format_duration(selected, selected_more);
    gchar *total_text = plwin_format_duration(total, total_more);
    gchar *text = g_strconcat(selected_text, "/", total_text, NULL);

    textbox_set_text(plwin_info, text);

    g_free(text);
    g_free(total_text);
    g_free(selected_text);
}

static void
plwin_update_shaded_info(void)
{
    if (!plwin_sinfo)
        return;

    gint pos = playlist_get_position();
    if (pos < 0) {
        textbox_set_text(plwin_sinfo, "");
        return;
    }

    const gchar *title = playlist_get_title(pos);
    if (!title) {
        textbox_set_text(plwin_sinfo, "");
        return;
    }

    PlaylistEntry *entry = playlist_get_entry(pos);
    gchar *normalized = plwin_normalize_playlist_text(title);
    gchar *posstr = cfg.show_numbers_in_pl ?
        g_strdup_printf("%d. ", pos + 1) : g_strdup("");
    gchar *timestr = NULL;
    gint max_len = (PLWIN_WIDTH - 35) / 5 - (gint)strlen(posstr);

    if (entry && entry->length >= 0) {
        timestr = time_to_string(entry->length);
        max_len -= (gint)strlen(timestr) + 1;
    } else {
        timestr = g_strdup("");
    }

    max_len = MAX(0, max_len);
    if ((gint)strlen(normalized) > max_len) {
        gint title_len = MAX(0, max_len - 3);
        gchar *short_title = g_strndup(normalized, title_len);
        gchar *info = g_strdup_printf("%s%s...%s%s",
                                      posstr, short_title,
                                      timestr[0] ? " " : "", timestr);
        textbox_set_text(plwin_sinfo, info);
        g_free(info);
        g_free(short_title);
    } else {
        gchar *info = g_strdup_printf("%s%s%s%s",
                                      posstr, normalized,
                                      timestr[0] ? " " : "", timestr);
        textbox_set_text(plwin_sinfo, info);
        g_free(info);
    }

    g_free(timestr);
    g_free(posstr);
    g_free(normalized);
}

static void
plwin_update_time(void)
{
    if (!plwin_time_min || !plwin_time_sec)
        return;

    PlayerState state = player_get_state();
    if (state == PLAYER_STOPPED) {
        textbox_set_text(plwin_time_min, "   ");
        textbox_set_text(plwin_time_sec, "  ");
        return;
    }

    gint64 time = player_get_position();
    gint64 length = player_get_duration();
    if (cfg.timer_mode == TIMER_REMAINING && length > 0)
        time = length - time;
    time = MAX((gint64)0, time / 1000);
    if (time > 99 * 60)
        time /= 60;

    gchar *mins = g_strdup_printf("%c%02" G_GINT64_FORMAT,
                                  cfg.timer_mode == TIMER_REMAINING && length > 0 ? '-' : ' ',
                                  time / 60);
    gchar *secs = g_strdup_printf("%02" G_GINT64_FORMAT, time % 60);

    textbox_set_text(plwin_time_min, mins);
    textbox_set_text(plwin_time_sec, secs);

    g_free(secs);
    g_free(mins);
}

static void
plwin_set_scroll_offset(gint offset)
{
    plwin_scroll_offset = CLAMP(offset, 0, plwin_max_scroll_offset());
}

static void
plwin_play_pushed(void)
{
    if (player_get_state() == PLAYER_PAUSED)
        player_unpause();
    else if (player_get_state() == PLAYER_STOPPED)
        playlist_play();
}

static void
plwin_eject_pushed(void)
{
    plwin_open_files(TRUE);
}

static void
plwin_scroll_up_pushed(void)
{
    plwin_set_scroll_offset(plwin_scroll_offset - 3);
    plwin_queue_draw();
}

static void
plwin_scroll_down_pushed(void)
{
    plwin_set_scroll_offset(plwin_scroll_offset + 3);
    plwin_queue_draw();
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
    plwin_set_playlist_font(cr);
    cairo_font_extents_t font_ext;
    cairo_font_extents(cr, &font_ext);
    gint baseline = (gint)(font_ext.ascent + 0.999);

    for (gint i = 0; i < visible && (i + plwin_scroll_offset) < total; i++) {
        gint idx = i + plwin_scroll_offset;
        gint y = list_y + i * PLWIN_ENTRY_HEIGHT;

        /* Selection highlight */
        PlaylistEntry *entry = playlist_get_entry(idx);
        if (idx == plwin_selected || (entry && entry->selected)) {
            gdk_cairo_set_source_rgba(cr, &skin->pledit_selectedbg);
            cairo_rectangle(cr, list_x, y, list_w, PLWIN_ENTRY_HEIGHT);
            cairo_fill(cr);
        }

        /* Text color */
        if (idx == current)
            gdk_cairo_set_source_rgba(cr, &skin->pledit_current);
        else
            gdk_cairo_set_source_rgba(cr, &skin->pledit_normal);

        gint text_w = list_w;
        if (entry && entry->length > 0) {
            gchar *dur = time_to_string(entry->length);
            cairo_text_extents_t ext;
            cairo_text_extents(cr, dur, &ext);
            cairo_move_to(cr, list_x + list_w - ext.width - 2,
                          y + baseline);
            cairo_show_text(cr, dur);
            text_w = list_w - ext.width - 5;
            g_free(dur);
        }

        const gchar *title = playlist_get_title(idx);
        if (title) {
            gchar *normalized = plwin_normalize_playlist_text(title);
            gchar *display = cfg.show_numbers_in_pl ?
                g_strdup_printf("%d. %s", idx + 1, normalized) :
                g_strdup(normalized);

            plwin_ellipsize_to_width(cr, display, text_w);
            cairo_move_to(cr, list_x, y + baseline);
            cairo_show_text(cr, display);

            g_free(display);
            g_free(normalized);
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
draw_playlist_pressed_button(cairo_t *cr, PlwinButton button,
                             gint pressed_x, gint pressed_y,
                             gint dest_x, gint dest_y)
{
    if (plwin_pressed_button != button || !plwin_pressed_inside)
        return;

    skin_draw_pixmap(cr, SKIN_PLEDIT, pressed_x, pressed_y,
                     dest_x, dest_y, PLWIN_BUTTON_SRC_W, PLWIN_BUTTON_H);
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

    draw_playlist_pressed_button(cr, PLWIN_BUTTON_ADD,
                                 23, 149, 12, PLWIN_BUTTON_Y);
    draw_playlist_pressed_button(cr, PLWIN_BUTTON_REMOVE,
                                 77, 149, 41, PLWIN_BUTTON_Y);
    draw_playlist_pressed_button(cr, PLWIN_BUTTON_SELECT,
                                 127, 149, 70, PLWIN_BUTTON_Y);
    draw_playlist_pressed_button(cr, PLWIN_BUTTON_MISC,
                                 177, 149, 99, PLWIN_BUTTON_Y);
    draw_playlist_pressed_button(cr, PLWIN_BUTTON_LIST,
                                 227, 149, w - 46, PLWIN_BUTTON_Y);
}

static gboolean
plwin_point_in_rect(gint x, gint y, gint rx, gint ry, gint rw, gint rh)
{
    return x >= rx && x < rx + rw && y >= ry && y < ry + rh;
}

static gboolean
plwin_button_rect(PlwinButton button, gint *x, gint *y, gint *w, gint *h)
{
    switch (button) {
    case PLWIN_BUTTON_ADD:
        *x = 12; *y = PLWIN_BUTTON_Y; *w = PLWIN_BUTTON_W; *h = PLWIN_BUTTON_H;
        return TRUE;
    case PLWIN_BUTTON_REMOVE:
        *x = 41; *y = PLWIN_BUTTON_Y; *w = PLWIN_BUTTON_W; *h = PLWIN_BUTTON_H;
        return TRUE;
    case PLWIN_BUTTON_SELECT:
        *x = 70; *y = PLWIN_BUTTON_Y; *w = PLWIN_BUTTON_W; *h = PLWIN_BUTTON_H;
        return TRUE;
    case PLWIN_BUTTON_MISC:
        *x = 99; *y = PLWIN_BUTTON_Y; *w = PLWIN_BUTTON_W; *h = PLWIN_BUTTON_H;
        return TRUE;
    case PLWIN_BUTTON_LIST:
        *x = PLWIN_WIDTH - 46; *y = PLWIN_BUTTON_Y; *w = 23; *h = PLWIN_BUTTON_H;
        return TRUE;
    case PLWIN_BUTTON_PREV:
        *x = PLWIN_WIDTH - 144; *y = PLWIN_HEIGHT - 16; *w = 8; *h = 7;
        return TRUE;
    case PLWIN_BUTTON_PLAY:
        *x = PLWIN_WIDTH - 138; *y = PLWIN_HEIGHT - 16; *w = 10; *h = 7;
        return TRUE;
    case PLWIN_BUTTON_PAUSE:
        *x = PLWIN_WIDTH - 128; *y = PLWIN_HEIGHT - 16; *w = 10; *h = 7;
        return TRUE;
    case PLWIN_BUTTON_STOP:
        *x = PLWIN_WIDTH - 118; *y = PLWIN_HEIGHT - 16; *w = 9; *h = 7;
        return TRUE;
    case PLWIN_BUTTON_NEXT:
        *x = PLWIN_WIDTH - 109; *y = PLWIN_HEIGHT - 16; *w = 8; *h = 7;
        return TRUE;
    case PLWIN_BUTTON_EJECT:
        *x = PLWIN_WIDTH - 100; *y = PLWIN_HEIGHT - 16; *w = 9; *h = 7;
        return TRUE;
    case PLWIN_BUTTON_SCROLL_UP:
        *x = PLWIN_WIDTH - 14; *y = PLWIN_HEIGHT - 35; *w = 8; *h = 5;
        return TRUE;
    case PLWIN_BUTTON_SCROLL_DOWN:
        *x = PLWIN_WIDTH - 14; *y = PLWIN_HEIGHT - 30; *w = 8; *h = 5;
        return TRUE;
    default:
        return FALSE;
    }
}

static PlwinButton
plwin_button_at(gint x, gint y)
{
    static const PlwinButton buttons[] = {
        PLWIN_BUTTON_ADD, PLWIN_BUTTON_REMOVE, PLWIN_BUTTON_SELECT,
        PLWIN_BUTTON_MISC, PLWIN_BUTTON_LIST, PLWIN_BUTTON_PREV,
        PLWIN_BUTTON_PLAY, PLWIN_BUTTON_PAUSE, PLWIN_BUTTON_STOP,
        PLWIN_BUTTON_NEXT, PLWIN_BUTTON_EJECT, PLWIN_BUTTON_SCROLL_UP,
        PLWIN_BUTTON_SCROLL_DOWN
    };

    for (guint i = 0; i < G_N_ELEMENTS(buttons); i++) {
        gint bx, by, bw, bh;
        if (plwin_button_rect(buttons[i], &bx, &by, &bw, &bh) &&
            plwin_point_in_rect(x, y, bx, by, bw, bh))
            return buttons[i];
    }
    return PLWIN_BUTTON_NONE;
}

static gint
plwin_menu_item_at(gint x, gint y)
{
    if (!plwin_menu.open)
        return -1;

    if (x < plwin_menu.x || x >= plwin_menu.x + PLWIN_MENU_W ||
        y < plwin_menu.y ||
        y >= plwin_menu.y + (gint)plwin_menu.n_items * PLWIN_BUTTON_H)
        return -1;

    return (y - plwin_menu.y) / PLWIN_BUTTON_H;
}

static void
draw_playlist_menu(cairo_t *cr)
{
    if (!plwin_menu.open)
        return;

    for (guint i = 0; i < plwin_menu.n_items; i++) {
        const PlwinMenuItem *item = &plwin_menu.items[i];
        gboolean selected = plwin_menu.hover == (gint)i;
        skin_draw_pixmap(cr, SKIN_PLEDIT,
                         selected ? item->selected_x : item->normal_x,
                         selected ? item->selected_y : item->normal_y,
                         plwin_menu.x + PLWIN_MENU_BORDER_W,
                         plwin_menu.y + i * PLWIN_BUTTON_H,
                         PLWIN_BUTTON_SRC_W, PLWIN_BUTTON_H);
    }

    skin_draw_pixmap(cr, SKIN_PLEDIT,
                     plwin_menu.border_x, plwin_menu.border_y,
                     plwin_menu.x, plwin_menu.y,
                     PLWIN_MENU_BORDER_W,
                     plwin_menu.n_items * PLWIN_BUTTON_H);
}

static void
draw_playlist_window(GtkDrawingArea *area, cairo_t *cr,
                     int width, int height, gpointer data)
{
    (void)area; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    cairo_scale(cr, (double)width / PLWIN_WIDTH,
                    (double)height / playlistwin_height());

    /* Draw assembled playlist frame from skin pieces */
    draw_playlist_frame(cr);
    if (plwin_shaded) {
        widget_list_draw(plwin_wlist, cr);
        return;
    }

    /* Draw entries */
    draw_playlist_entries(cr);

    /* Draw all custom widgets */
    widget_list_draw(plwin_wlist, cr);

    draw_playlist_menu(cr);

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
    if (button == 1)
        gtk_widget_grab_focus(plwin_drawing_area);

    /* Check list area click */
    gint list_x = PLWIN_LIST_X, list_y = PLWIN_LIST_Y;
    gint list_w = PLWIN_LIST_W, list_h = PLWIN_LIST_H;

    if (button == 3) {
        if (sx >= list_x && sx < list_x + list_w &&
            sy >= list_y && sy < list_y + list_h) {
            gint entry_idx = (sy - list_y) / PLWIN_ENTRY_HEIGHT + plwin_scroll_offset;
            if (entry_idx < playlist_get_length())
                plwin_selected = entry_idx;
        }
        plwin_show_context_menu(sx, sy);
        return;
    }

    if (plwin_shaded && sy >= 14)
        return;

    if (button == 1 && n_press == 2 && sy < 14) {
        playlistwin_set_shaded(!plwin_shaded);
        return;
    }

    gint thumb_y, thumb_h;
    gboolean has_scrollbar = plwin_scrollbar_geometry(&thumb_y, &thumb_h);

    if (button == 1 && plwin_menu.open) {
        gint item = plwin_menu_item_at(sx, sy);
        if (item >= 0) {
            plwin_menu.hover = item;
            plwin_menu.pressed = TRUE;
            plwin_queue_draw();
            return;
        }
        plwin_close_menu();
        plwin_queue_draw();
    }

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

    Widget *widget = button == 1 ? widget_list_find(plwin_wlist, sx, sy) : NULL;
    if (widget && widget->button_press) {
        plwin_pressed_widget = widget;
        widget->button_press(widget, sx, sy, button);
        plwin_queue_draw();
        return;
    }

    if (button == 1 &&
        sx >= PLWIN_DETACH_BTN_X && sx < PLWIN_DETACH_BTN_X + PLWIN_DETACH_BTN_W &&
        sy >= PLWIN_DETACH_BTN_Y && sy < PLWIN_DETACH_BTN_Y + PLWIN_DETACH_BTN_H) {
        playlistwin_set_detached(!cfg.playlist_detached);
        return;
    }

    PlwinButton pl_button = button == 1 ? plwin_button_at(sx, sy) :
        PLWIN_BUTTON_NONE;
    if (pl_button != PLWIN_BUTTON_NONE) {
        plwin_pressed_button = pl_button;
        plwin_pressed_inside = TRUE;
        plwin_queue_draw();
        return;
    }

    if (sx >= list_x && sx < list_x + list_w &&
        sy >= list_y && sy < list_y + list_h) {
        gint entry_idx = (sy - list_y) / PLWIN_ENTRY_HEIGHT + plwin_scroll_offset;
        if (entry_idx < playlist_get_length()) {
            GdkModifierType state =
                gtk_event_controller_get_current_event_state(
                    GTK_EVENT_CONTROLLER(gesture));
            PlaylistEntry *entry = playlist_get_entry(entry_idx);

            if ((state & GDK_SHIFT_MASK) && plwin_selection_anchor >= 0) {
                gint start = MIN(plwin_selection_anchor, entry_idx);
                gint end = MAX(plwin_selection_anchor, entry_idx);
                for (gint i = start; i <= end; i++) {
                    PlaylistEntry *range_entry = playlist_get_entry(i);
                    if (range_entry)
                        range_entry->selected = TRUE;
                }
            } else if ((state & GDK_CONTROL_MASK) && entry) {
                entry->selected = !entry->selected;
                plwin_selection_anchor = entry_idx;
            } else {
                for (gint i = 0; i < playlist_get_length(); i++) {
                    PlaylistEntry *other = playlist_get_entry(i);
                    if (other)
                        other->selected = FALSE;
                }
                if (entry)
                    entry->selected = TRUE;
                plwin_selection_anchor = entry_idx;
            }
            plwin_selected = entry_idx;
            plwin_update_info();
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
    (void)gesture; (void)n_press; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);
    gint button = gtk_gesture_single_get_current_button(
        GTK_GESTURE_SINGLE(gesture));

    if (plwin_menu.open && plwin_menu.pressed) {
        gint item = plwin_menu_item_at(sx, sy);
        gboolean activate = item >= 0 && item == plwin_menu.hover;
        PlwinAction action = activate ?
            plwin_menu.items[item].action : PLWIN_ACTION_MISC_OPTIONS;
        plwin_close_menu();
        plwin_queue_draw();
        if (activate)
            plwin_menu_action_activate(action);
        return;
    }

    if (plwin_pressed_widget) {
        if (plwin_pressed_widget->button_release)
            plwin_pressed_widget->button_release(plwin_pressed_widget,
                                                 sx, sy, button);
        plwin_pressed_widget = NULL;
        plwin_queue_draw();
        return;
    }

    if (plwin_pressed_button != PLWIN_BUTTON_NONE) {
        PlwinButton pressed = plwin_pressed_button;
        gboolean activate = plwin_button_at(sx, sy) == pressed;
        plwin_pressed_button = PLWIN_BUTTON_NONE;
        plwin_pressed_inside = FALSE;
        plwin_queue_draw();
        if (activate)
            plwin_activate_button(pressed);
        return;
    }

    if (plwin_scrollbar_dragging) {
        plwin_scrollbar_dragging = FALSE;
        plwin_queue_draw();
    }
}

static void
plwin_motion(GtkEventControllerMotion *controller,
             double x, double y, gpointer data)
{
    (void)controller; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);

    if (plwin_menu.open) {
        gint item = plwin_menu_item_at(sx, sy);
        if (item != plwin_menu.hover) {
            plwin_menu.hover = item;
            plwin_queue_draw();
        }
        return;
    }

    if (plwin_pressed_widget && plwin_pressed_widget->motion) {
        plwin_pressed_widget->motion(plwin_pressed_widget, sx, sy);
        return;
    }

    if (plwin_pressed_button != PLWIN_BUTTON_NONE) {
        gboolean was_inside = plwin_pressed_inside;
        plwin_pressed_inside = plwin_button_at(sx, sy) == plwin_pressed_button;
        if (was_inside != plwin_pressed_inside)
            plwin_queue_draw();
        return;
    }

    if (!plwin_scrollbar_dragging)
        return;
    plwin_scrollbar_set_from_y(sy);
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

static gboolean
plwin_key_pressed(GtkEventControllerKey *controller, guint keyval,
                  guint keycode, GdkModifierType state, gpointer data)
{
    (void)controller; (void)keycode; (void)state; (void)data;

    if (keyval == GDK_KEY_Delete) {
        plwin_remove_selected();
        return GDK_EVENT_STOP;
    }

    return GDK_EVENT_PROPAGATE;
}

/* ---- Public API ---- */

static void
plwin_queue_draw(void)
{
    plwin_update_info();
    plwin_update_shaded_info();
    plwin_update_time();
    if (plwin_drawing_area)
        gtk_widget_queue_draw(plwin_drawing_area);
}

static void
plwin_select_all(gboolean selected)
{
    for (gint i = 0; i < playlist_get_length(); i++) {
        PlaylistEntry *entry = playlist_get_entry(i);
        if (entry)
            entry->selected = selected;
    }
    if (!selected)
        plwin_selected = -1;
    plwin_queue_draw();
}

static void
plwin_select_invert(void)
{
    for (gint i = 0; i < playlist_get_length(); i++) {
        PlaylistEntry *entry = playlist_get_entry(i);
        if (entry)
            entry->selected = !entry->selected;
    }
    plwin_queue_draw();
}

static void
plwin_remove_selected(void)
{
    gboolean removed = FALSE;
    for (gint i = playlist_get_length() - 1; i >= 0; i--) {
        PlaylistEntry *entry = playlist_get_entry(i);
        if (entry && (entry->selected || i == plwin_selected)) {
            playlist_remove(i);
            removed = TRUE;
        }
    }

    if (!removed && playlist_get_position() >= 0)
        playlist_remove(playlist_get_position());

    plwin_selected = -1;
    plwin_set_scroll_offset(plwin_scroll_offset);
    plwin_queue_draw();
}

static gchar *
plwin_entry_local_path(PlaylistEntry *entry)
{
    if (!entry || !entry->filename)
        return NULL;

    gchar *path = uri_to_filename(entry->filename);
    if (path)
        return path;

    if (g_path_is_absolute(entry->filename))
        return g_strdup(entry->filename);

    return NULL;
}

static gboolean
plwin_entry_is_selected(gint idx, PlaylistEntry *entry)
{
    return entry && (entry->selected || idx == plwin_selected);
}

static void
plwin_read_extended_info(void)
{
    for (gint i = 0; i < playlist_get_length(); i++) {
        PlaylistEntry *entry = playlist_get_entry(i);
        if (!plwin_entry_is_selected(i, entry))
            continue;

        gchar *path = plwin_entry_local_path(entry);
        if (path) {
            gchar *title = format_title(path, NULL);
            if (title && title[0]) {
                g_free(entry->title);
                entry->title = title;
            } else {
                g_free(title);
            }
            g_free(path);
        }

        if (i == playlist_get_position()) {
            gint64 duration = player_get_duration();
            if (duration > 0)
                entry->length = duration;
        }
    }
    plwin_update_info();
    plwin_queue_draw();
}

static void
plwin_remove_dead_files(void)
{
    for (gint i = playlist_get_length() - 1; i >= 0; i--) {
        PlaylistEntry *entry = playlist_get_entry(i);
        gchar *path = plwin_entry_local_path(entry);
        if (path) {
            if (!g_file_test(path, G_FILE_TEST_EXISTS))
                playlist_remove(i);
            g_free(path);
        }
    }
    plwin_selected = -1;
    plwin_set_scroll_offset(plwin_scroll_offset);
    plwin_queue_draw();
}

static gint
plwin_selected_local_file_count(void)
{
    gint count = 0;
    for (gint i = 0; i < playlist_get_length(); i++) {
        PlaylistEntry *entry = playlist_get_entry(i);
        if (!plwin_entry_is_selected(i, entry))
            continue;
        gchar *path = plwin_entry_local_path(entry);
        if (path) {
            count++;
            g_free(path);
        }
    }
    return count;
}

static void
plwin_physically_delete_confirmed(GtkButton *button, gpointer data)
{
    (void)button;
    GtkWindow *window = GTK_WINDOW(data);

    for (gint i = playlist_get_length() - 1; i >= 0; i--) {
        PlaylistEntry *entry = playlist_get_entry(i);
        if (!plwin_entry_is_selected(i, entry))
            continue;

        gchar *path = plwin_entry_local_path(entry);
        if (path) {
            if (g_remove(path) == 0)
                playlist_remove(i);
            else
                g_warning("Failed to delete file: %s", path);
            g_free(path);
        }
    }

    gtk_window_destroy(window);
    plwin_selected = -1;
    plwin_set_scroll_offset(plwin_scroll_offset);
    plwin_queue_draw();
}

static void
plwin_dialog_cancel_clicked(GtkButton *button, gpointer data)
{
    (void)button;
    gtk_window_destroy(GTK_WINDOW(data));
}

static void
plwin_physically_delete_selected(void)
{
    gint count = plwin_selected_local_file_count();
    if (count == 0) {
        g_message("No selected local files to delete");
        return;
    }

    GtkWindow *parent = GTK_WINDOW(cfg.playlist_detached && plwin_floating_window ?
                                  plwin_floating_window : mainwin);
    GtkWidget *window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(window), "Delete Files");
    gtk_window_set_transient_for(GTK_WINDOW(window), parent);
    gtk_window_set_modal(GTK_WINDOW(window), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(window), FALSE);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    gtk_widget_set_margin_top(box, 12);
    gtk_widget_set_margin_bottom(box, 12);
    gtk_widget_set_margin_start(box, 12);
    gtk_widget_set_margin_end(box, 12);
    gtk_window_set_child(GTK_WINDOW(window), box);

    gchar *message = g_strdup_printf("Delete %d selected local file%s from disk?",
                                     count, count == 1 ? "" : "s");
    gtk_box_append(GTK_BOX(box), gtk_label_new(message));
    g_free(message);

    GtkWidget *buttons = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_box_append(GTK_BOX(box), buttons);
    GtkWidget *cancel = gtk_button_new_with_label("Cancel");
    GtkWidget *delete = gtk_button_new_with_label("Delete");
    g_signal_connect(cancel, "clicked",
                     G_CALLBACK(plwin_dialog_cancel_clicked), window);
    g_signal_connect(delete, "clicked",
                     G_CALLBACK(plwin_physically_delete_confirmed), window);
    gtk_box_append(GTK_BOX(buttons), cancel);
    gtk_box_append(GTK_BOX(buttons), delete);
    gtk_window_present(GTK_WINDOW(window));
}

static void
plwin_open_files_cb(GObject *source, GAsyncResult *result, gpointer data)
{
    gboolean replace = GPOINTER_TO_INT(data);
    GtkFileDialog *dlg = GTK_FILE_DIALOG(source);
    GListModel *files = gtk_file_dialog_open_multiple_finish(dlg, result, NULL);
    if (!files)
        return;

    if (replace)
        playlist_clear();

    for (guint i = 0; i < g_list_model_get_n_items(files); i++) {
        GFile *file = g_list_model_get_item(files, i);
        gchar *uri = g_file_get_uri(file);
        if (uri) {
            playlist_add_uri(uri);
            g_free(uri);
        }
        g_object_unref(file);
    }
    g_object_unref(files);

    if (replace)
        playlist_play();
    plwin_queue_draw();
}

static void
plwin_open_files(gboolean replace)
{
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, replace ? "Open Files" : "Add Files");

    GtkFileFilter *filter = gtk_file_filter_new();
    gtk_file_filter_set_name(filter, "Audio Files");
    gtk_file_filter_add_mime_type(filter, "audio/*");

    GtkFileFilter *all_filter = gtk_file_filter_new();
    gtk_file_filter_set_name(all_filter, "All Files");
    gtk_file_filter_add_pattern(all_filter, "*");

    GListStore *filters = g_list_store_new(GTK_TYPE_FILE_FILTER);
    g_list_store_append(filters, filter);
    g_list_store_append(filters, all_filter);
    gtk_file_dialog_set_filters(dialog, G_LIST_MODEL(filters));

    GtkWindow *parent = GTK_WINDOW(cfg.playlist_detached && plwin_floating_window ?
                                  plwin_floating_window : mainwin);
    gtk_file_dialog_open_multiple(dialog, parent, NULL,
                                  plwin_open_files_cb,
                                  GINT_TO_POINTER(replace));

    g_object_unref(filters);
    g_object_unref(filter);
    g_object_unref(all_filter);
}

static void
plwin_open_folder_cb(GObject *source, GAsyncResult *result, gpointer data)
{
    (void)data;
    GtkFileDialog *dlg = GTK_FILE_DIALOG(source);
    GFile *folder = gtk_file_dialog_select_folder_finish(dlg, result, NULL);
    if (!folder)
        return;

    gchar *path = g_file_get_path(folder);
    if (path) {
        playlist_add_dir(path);
        g_free(path);
    }
    g_object_unref(folder);
    plwin_queue_draw();
}

static void
plwin_open_folder(void)
{
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, "Add Directory");
    GtkWindow *parent = GTK_WINDOW(cfg.playlist_detached && plwin_floating_window ?
                                  plwin_floating_window : mainwin);
    gtk_file_dialog_select_folder(dialog, parent, NULL, plwin_open_folder_cb, NULL);
}

static void
plwin_load_cb(GObject *source, GAsyncResult *result, gpointer data)
{
    (void)data;
    GtkFileDialog *dlg = GTK_FILE_DIALOG(source);
    GFile *file = gtk_file_dialog_open_finish(dlg, result, NULL);
    if (!file)
        return;

    gchar *path = g_file_get_path(file);
    if (path) {
        playlist_clear();
        playlist_load(path);
        g_free(path);
    }
    g_object_unref(file);
    plwin_queue_draw();
}

static void
plwin_save_cb(GObject *source, GAsyncResult *result, gpointer data)
{
    (void)data;
    GtkFileDialog *dlg = GTK_FILE_DIALOG(source);
    GFile *file = gtk_file_dialog_save_finish(dlg, result, NULL);
    if (!file)
        return;

    gchar *path = g_file_get_path(file);
    if (path) {
        playlist_save(path);
        g_free(path);
    }
    g_object_unref(file);
}

static void
plwin_load_playlist(void)
{
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, "Load Playlist");
    GtkWindow *parent = GTK_WINDOW(cfg.playlist_detached && plwin_floating_window ?
                                  plwin_floating_window : mainwin);
    gtk_file_dialog_open(dialog, parent, NULL, plwin_load_cb, NULL);
}

static void
plwin_save_playlist(void)
{
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, "Save Playlist");
    GtkWindow *parent = GTK_WINDOW(cfg.playlist_detached && plwin_floating_window ?
                                  plwin_floating_window : mainwin);
    gtk_file_dialog_save(dialog, parent, NULL, plwin_save_cb, NULL);
}

static gboolean
plwin_url_close_cb(GtkWindow *window, gpointer data)
{
    (void)window; (void)data;
    plwin_url_window = NULL;
    plwin_url_entry = NULL;
    return FALSE;
}

static void
plwin_url_add_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    const gchar *url = plwin_url_entry ?
        gtk_editable_get_text(GTK_EDITABLE(plwin_url_entry)) : NULL;
    if (url && *url) {
        playlist_add_uri(url);
        plwin_queue_draw();
    }
    if (plwin_url_window)
        gtk_window_destroy(GTK_WINDOW(plwin_url_window));
}

static void
plwin_url_cancel_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    if (plwin_url_window)
        gtk_window_destroy(GTK_WINDOW(plwin_url_window));
}

static void
plwin_show_add_url_window(void)
{
    if (plwin_url_window) {
        gtk_window_present(GTK_WINDOW(plwin_url_window));
        return;
    }

    GtkWindow *parent = GTK_WINDOW(cfg.playlist_detached && plwin_floating_window ?
                                  plwin_floating_window : mainwin);
    plwin_url_window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(plwin_url_window), "Add URL");
    gtk_window_set_transient_for(GTK_WINDOW(plwin_url_window), parent);
    gtk_window_set_modal(GTK_WINDOW(plwin_url_window), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(plwin_url_window), FALSE);
    g_signal_connect(plwin_url_window, "close-request",
                     G_CALLBACK(plwin_url_close_cb), NULL);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    gtk_widget_set_margin_top(box, 12);
    gtk_widget_set_margin_bottom(box, 12);
    gtk_widget_set_margin_start(box, 12);
    gtk_widget_set_margin_end(box, 12);
    gtk_window_set_child(GTK_WINDOW(plwin_url_window), box);

    plwin_url_entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(plwin_url_entry), "https://...");
    gtk_box_append(GTK_BOX(box), plwin_url_entry);

    GtkWidget *buttons = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_box_append(GTK_BOX(box), buttons);

    GtkWidget *cancel = gtk_button_new_with_label("Cancel");
    GtkWidget *add = gtk_button_new_with_label("Add");
    g_signal_connect(cancel, "clicked",
                     G_CALLBACK(plwin_url_cancel_clicked), NULL);
    g_signal_connect(add, "clicked",
                     G_CALLBACK(plwin_url_add_clicked), NULL);
    gtk_box_append(GTK_BOX(buttons), cancel);
    gtk_box_append(GTK_BOX(buttons), add);

    gtk_window_present(GTK_WINDOW(plwin_url_window));
}

static void
plwin_context_button_clicked(GtkButton *button, gpointer data)
{
    (void)button;
    if (plwin_context_popover)
        gtk_popover_popdown(GTK_POPOVER(plwin_context_popover));
    plwin_menu_action_activate(GPOINTER_TO_INT(data));
}

static void
plwin_context_add_button(GtkWidget *box, const gchar *label,
                         PlwinAction action)
{
    GtkWidget *button = gtk_button_new_with_label(label);
    gtk_widget_set_halign(button, GTK_ALIGN_FILL);
    g_signal_connect(button, "clicked",
                     G_CALLBACK(plwin_context_button_clicked),
                     GINT_TO_POINTER(action));
    gtk_box_append(GTK_BOX(box), button);
}

static void
plwin_show_context_menu(gint x, gint y)
{
    if (!plwin_context_popover) {
        GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
        plwin_context_add_button(box, "View File Info",
                                 PLWIN_ACTION_MISC_FILE_INFO);
        plwin_context_add_button(box, "Add File",
                                 PLWIN_ACTION_ADD_FILE);
        plwin_context_add_button(box, "Add Directory",
                                 PLWIN_ACTION_ADD_DIR);
        plwin_context_add_button(box, "Add URL",
                                 PLWIN_ACTION_ADD_URL);
        plwin_context_add_button(box, "Remove Selected",
                                 PLWIN_ACTION_REMOVE_SELECTED);
        plwin_context_add_button(box, "Remove Dead Files",
                                 PLWIN_ACTION_REMOVE_DEAD);
        plwin_context_add_button(box, "Physically Delete Files",
                                 PLWIN_ACTION_PHYSICALLY_DELETE);
        plwin_context_add_button(box, "Select All",
                                 PLWIN_ACTION_SELECT_ALL);
        plwin_context_add_button(box, "Select None",
                                 PLWIN_ACTION_SELECT_NONE);
        plwin_context_add_button(box, "Invert Selection",
                                 PLWIN_ACTION_SELECT_INVERT);
        plwin_context_add_button(box, "Read Extended Info",
                                 PLWIN_ACTION_READ_EXTENDED_INFO);

        plwin_context_popover = gtk_popover_new();
        gtk_popover_set_child(GTK_POPOVER(plwin_context_popover), box);
        gtk_widget_set_parent(plwin_context_popover, plwin_drawing_area);
    }

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    GdkRectangle rect = { x * scale, y * scale, scale, scale };
    gtk_popover_set_pointing_to(GTK_POPOVER(plwin_context_popover), &rect);
    gtk_popover_popup(GTK_POPOVER(plwin_context_popover));
}

static void
plwin_sort_action_clicked(GtkButton *button, gpointer data)
{
    (void)button;

    switch (GPOINTER_TO_INT(data)) {
    case PLWIN_SORT_BY_TITLE:
        playlist_sort_by_title();
        break;
    case PLWIN_SORT_BY_FILENAME:
        playlist_sort_by_filename();
        break;
    case PLWIN_SORT_BY_PATH:
        playlist_sort_by_path();
        break;
    case PLWIN_SORT_BY_DATE:
        playlist_sort_by_date();
        break;
    case PLWIN_SORT_SEL_BY_TITLE:
        playlist_sort_selected_by_title();
        break;
    case PLWIN_SORT_SEL_BY_FILENAME:
        playlist_sort_selected_by_filename();
        break;
    case PLWIN_SORT_SEL_BY_PATH:
        playlist_sort_selected_by_path();
        break;
    case PLWIN_SORT_SEL_BY_DATE:
        playlist_sort_selected_by_date();
        break;
    case PLWIN_SORT_RANDOMIZE:
        playlist_random();
        break;
    case PLWIN_SORT_REVERSE:
        playlist_reverse();
        break;
    }

    if (plwin_sort_popover)
        gtk_popover_popdown(GTK_POPOVER(plwin_sort_popover));
    plwin_selected = -1;
    plwin_set_scroll_offset(plwin_scroll_offset);
    plwin_queue_draw();
}

static void
plwin_sort_menu_add_button(GtkWidget *box, const gchar *label,
                           PlwinSortAction action)
{
    GtkWidget *button = gtk_button_new_with_label(label);
    gtk_widget_set_halign(button, GTK_ALIGN_FILL);
    g_signal_connect(button, "clicked",
                     G_CALLBACK(plwin_sort_action_clicked),
                     GINT_TO_POINTER(action));
    gtk_box_append(GTK_BOX(box), button);
}

static void
plwin_show_sort_menu(void)
{
    if (!plwin_sort_popover) {
        GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);

        plwin_sort_menu_add_button(box, "Sort List: By Title",
                                   PLWIN_SORT_BY_TITLE);
        plwin_sort_menu_add_button(box, "Sort List: By Filename",
                                   PLWIN_SORT_BY_FILENAME);
        plwin_sort_menu_add_button(box, "Sort List: By Path + Filename",
                                   PLWIN_SORT_BY_PATH);
        plwin_sort_menu_add_button(box, "Sort List: By Date",
                                   PLWIN_SORT_BY_DATE);
        plwin_sort_menu_add_button(box, "Sort Selection: By Title",
                                   PLWIN_SORT_SEL_BY_TITLE);
        plwin_sort_menu_add_button(box, "Sort Selection: By Filename",
                                   PLWIN_SORT_SEL_BY_FILENAME);
        plwin_sort_menu_add_button(box, "Sort Selection: By Path + Filename",
                                   PLWIN_SORT_SEL_BY_PATH);
        plwin_sort_menu_add_button(box, "Sort Selection: By Date",
                                   PLWIN_SORT_SEL_BY_DATE);
        plwin_sort_menu_add_button(box, "Randomize List",
                                   PLWIN_SORT_RANDOMIZE);
        plwin_sort_menu_add_button(box, "Reverse List",
                                   PLWIN_SORT_REVERSE);

        plwin_sort_popover = gtk_popover_new();
        gtk_popover_set_child(GTK_POPOVER(plwin_sort_popover), box);
        gtk_widget_set_parent(plwin_sort_popover, plwin_drawing_area);
        gtk_popover_set_position(GTK_POPOVER(plwin_sort_popover),
                                 GTK_POS_TOP);
    }

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;
    GdkRectangle rect = {
        (99 - 1) * scale,
        (PLWIN_BUTTON_Y - 2 * PLWIN_BUTTON_H - 1) * scale,
        PLWIN_MENU_W * scale,
        PLWIN_BUTTON_H * scale
    };
    gtk_popover_set_pointing_to(GTK_POPOVER(plwin_sort_popover), &rect);
    gtk_popover_popup(GTK_POPOVER(plwin_sort_popover));
}

static void
plwin_menu_action_activate(PlwinAction action)
{
    switch (action) {
    case PLWIN_ACTION_ADD_URL:
        plwin_show_add_url_window();
        break;
    case PLWIN_ACTION_ADD_FILE:
        plwin_open_files(FALSE);
        break;
    case PLWIN_ACTION_ADD_DIR:
        plwin_open_folder();
        break;
    case PLWIN_ACTION_REMOVE_MISC:
        plwin_show_context_menu(41, PLWIN_BUTTON_Y - PLWIN_BUTTON_H);
        break;
    case PLWIN_ACTION_REMOVE_DEAD:
        plwin_remove_dead_files();
        break;
    case PLWIN_ACTION_PHYSICALLY_DELETE:
        plwin_physically_delete_selected();
        break;
    case PLWIN_ACTION_REMOVE_SELECTED:
        plwin_remove_selected();
        break;
    case PLWIN_ACTION_REMOVE_CROP:
        for (gint i = playlist_get_length() - 1; i >= 0; i--) {
            PlaylistEntry *entry = playlist_get_entry(i);
            if (!entry || (!entry->selected && i != plwin_selected))
                playlist_remove(i);
        }
        plwin_selected = -1;
        plwin_set_scroll_offset(0);
        plwin_queue_draw();
        break;
    case PLWIN_ACTION_REMOVE_ALL:
        playlist_clear();
        plwin_selected = -1;
        plwin_set_scroll_offset(0);
        plwin_queue_draw();
        break;
    case PLWIN_ACTION_SELECT_ALL:
        plwin_select_all(TRUE);
        break;
    case PLWIN_ACTION_SELECT_NONE:
        plwin_select_all(FALSE);
        break;
    case PLWIN_ACTION_SELECT_INVERT:
        plwin_select_invert();
        break;
    case PLWIN_ACTION_READ_EXTENDED_INFO:
        plwin_read_extended_info();
        break;
    case PLWIN_ACTION_MISC_SORT:
        plwin_show_sort_menu();
        break;
    case PLWIN_ACTION_MISC_FILE_INFO: {
        gint idx = plwin_selected >= 0 ? plwin_selected : playlist_get_position();
        const gchar *title = playlist_get_title(idx);
        g_message("Playlist entry: %s", title ? title : "none");
        break;
    }
    case PLWIN_ACTION_MISC_OPTIONS:
        break;
    case PLWIN_ACTION_LIST_NEW:
        playlist_clear();
        plwin_selected = -1;
        plwin_set_scroll_offset(0);
        plwin_queue_draw();
        break;
    case PLWIN_ACTION_LIST_LOAD:
        plwin_load_playlist();
        break;
    case PLWIN_ACTION_LIST_SAVE:
        plwin_save_playlist();
        break;
    }
}

static void
plwin_close_menu(void)
{
    plwin_menu.open = FALSE;
    plwin_menu.pressed = FALSE;
    plwin_menu.hover = -1;
}

static void
plwin_show_menu(PlwinButton button, const PlwinMenuItem *items, guint n_items,
                gint x, gint border_x, gint border_y)
{
    plwin_close_menu();

    plwin_menu.open = TRUE;
    plwin_menu.pressed = FALSE;
    plwin_menu.button = button;
    plwin_menu.items = items;
    plwin_menu.n_items = n_items;
    plwin_menu.x = x - 1;
    plwin_menu.y = PLWIN_BUTTON_Y - (((gint)n_items - 1) * PLWIN_BUTTON_H) - 1;
    plwin_menu.border_x = border_x;
    plwin_menu.border_y = border_y;
    plwin_menu.hover = n_items > 0 ? (gint)n_items - 1 : -1;
    plwin_queue_draw();
}

static void
plwin_activate_button(PlwinButton button)
{
    switch (button) {
    case PLWIN_BUTTON_ADD: {
        static const PlwinMenuItem items[] = {
            { PLWIN_ACTION_ADD_URL,  0, 111,  23, 111 },
            { PLWIN_ACTION_ADD_DIR,  0, 130,  23, 130 },
            { PLWIN_ACTION_ADD_FILE, 0, 149,  23, 149 },
        };
        plwin_show_menu(button, items, G_N_ELEMENTS(items), 12, 48, 111);
        break;
    }
    case PLWIN_BUTTON_REMOVE: {
        static const PlwinMenuItem items[] = {
            { PLWIN_ACTION_REMOVE_MISC,     54, 168,  77, 168 },
            { PLWIN_ACTION_REMOVE_ALL,      54, 111,  77, 111 },
            { PLWIN_ACTION_REMOVE_CROP,     54, 130,  77, 130 },
            { PLWIN_ACTION_REMOVE_SELECTED, 54, 149,  77, 149 },
        };
        plwin_show_menu(button, items, G_N_ELEMENTS(items), 41, 100, 111);
        break;
    }
    case PLWIN_BUTTON_SELECT: {
        static const PlwinMenuItem items[] = {
            { PLWIN_ACTION_SELECT_INVERT, 104, 111, 127, 111 },
            { PLWIN_ACTION_SELECT_NONE,   104, 130, 127, 130 },
            { PLWIN_ACTION_SELECT_ALL,    104, 149, 127, 149 },
        };
        plwin_show_menu(button, items, G_N_ELEMENTS(items), 70, 150, 111);
        break;
    }
    case PLWIN_BUTTON_MISC: {
        static const PlwinMenuItem items[] = {
            { PLWIN_ACTION_MISC_SORT,      154, 111, 177, 111 },
            { PLWIN_ACTION_MISC_FILE_INFO, 154, 130, 177, 130 },
            { PLWIN_ACTION_MISC_OPTIONS,   154, 149, 177, 149 },
        };
        plwin_show_menu(button, items, G_N_ELEMENTS(items), 99, 200, 111);
        break;
    }
    case PLWIN_BUTTON_LIST: {
        static const PlwinMenuItem items[] = {
            { PLWIN_ACTION_LIST_NEW,  204, 111, 227, 111 },
            { PLWIN_ACTION_LIST_SAVE, 204, 130, 227, 130 },
            { PLWIN_ACTION_LIST_LOAD, 204, 149, 227, 149 },
        };
        plwin_show_menu(button, items, G_N_ELEMENTS(items),
                        PLWIN_WIDTH - 46, 250, 111);
        break;
    }
    case PLWIN_BUTTON_PREV:
        playlist_prev();
        break;
    case PLWIN_BUTTON_PLAY:
        if (player_get_state() == PLAYER_PAUSED)
            player_unpause();
        else if (player_get_state() == PLAYER_STOPPED)
            playlist_play();
        break;
    case PLWIN_BUTTON_PAUSE:
        player_toggle_pause();
        break;
    case PLWIN_BUTTON_STOP:
        player_stop();
        break;
    case PLWIN_BUTTON_NEXT:
        playlist_next();
        break;
    case PLWIN_BUTTON_EJECT:
        plwin_open_files(TRUE);
        break;
    case PLWIN_BUTTON_SCROLL_UP:
        plwin_set_scroll_offset(plwin_scroll_offset - 1);
        plwin_queue_draw();
        break;
    case PLWIN_BUTTON_SCROLL_DOWN:
        plwin_set_scroll_offset(plwin_scroll_offset + 1);
        plwin_queue_draw();
        break;
    default:
        break;
    }
}

void
playlistwin_show_menu(const gchar *menu)
{
    if (g_strcmp0(menu, "add") == 0)
        plwin_activate_button(PLWIN_BUTTON_ADD);
    else if (g_strcmp0(menu, "remove") == 0)
        plwin_activate_button(PLWIN_BUTTON_REMOVE);
    else if (g_strcmp0(menu, "select") == 0)
        plwin_activate_button(PLWIN_BUTTON_SELECT);
    else if (g_strcmp0(menu, "misc") == 0)
        plwin_activate_button(PLWIN_BUTTON_MISC);
    else if (g_strcmp0(menu, "list") == 0)
        plwin_activate_button(PLWIN_BUTTON_LIST);
}

void
playlistwin_shutdown(void)
{
    plwin_close_menu();
    if (plwin_sort_popover) {
        gtk_widget_unparent(plwin_sort_popover);
        plwin_sort_popover = NULL;
    }
    if (plwin_context_popover) {
        gtk_widget_unparent(plwin_context_popover);
        plwin_context_popover = NULL;
    }
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
    gtk_widget_set_focusable(plwin_drawing_area, TRUE);
    gtk_widget_set_visible(plwin_drawing_area, FALSE);
    playlistwin = plwin_drawing_area;

    plwin_time_min = textbox_new(&plwin_wlist, PLWIN_WIDTH - 82,
                                 PLWIN_HEIGHT - 15, 15, FALSE, SKIN_TEXT);
    plwin_time_sec = textbox_new(&plwin_wlist, PLWIN_WIDTH - 64,
                                 PLWIN_HEIGHT - 15, 10, FALSE, SKIN_TEXT);
    plwin_info = textbox_new(&plwin_wlist, PLWIN_WIDTH - 143,
                             PLWIN_HEIGHT - 28, 85, FALSE, SKIN_TEXT);
    plwin_sinfo = textbox_new(&plwin_wlist, 4, 4,
                              PLWIN_WIDTH - 35, FALSE, SKIN_TEXT);
    ((Widget *)plwin_sinfo)->visible = FALSE;

    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 144, PLWIN_HEIGHT - 16,
                8, 7, playlist_prev);
    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 138, PLWIN_HEIGHT - 16,
                10, 7, plwin_play_pushed);
    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 128, PLWIN_HEIGHT - 16,
                10, 7, player_toggle_pause);
    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 118, PLWIN_HEIGHT - 16,
                9, 7, player_stop);
    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 109, PLWIN_HEIGHT - 16,
                8, 7, playlist_next);
    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 100, PLWIN_HEIGHT - 16,
                9, 7, plwin_eject_pushed);
    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 14, PLWIN_HEIGHT - 35,
                8, 5, plwin_scroll_up_pushed);
    sbutton_new(&plwin_wlist, PLWIN_WIDTH - 14, PLWIN_HEIGHT - 30,
                8, 5, plwin_scroll_down_pushed);

    plwin_update_info();
    plwin_update_shaded_info();
    plwin_update_time();

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

    GtkEventController *key = gtk_event_controller_key_new();
    g_signal_connect(key, "key-pressed", G_CALLBACK(plwin_key_pressed), NULL);
    gtk_widget_add_controller(plwin_drawing_area, key);

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
    if (show)
        gtk_widget_grab_focus(plwin_drawing_area);
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

void
playlistwin_set_shaded(gboolean shaded)
{
    plwin_shaded = shaded;
    if (plwin_time_min)
        ((Widget *)plwin_time_min)->visible = !shaded;
    if (plwin_time_sec)
        ((Widget *)plwin_time_sec)->visible = !shaded;
    if (plwin_info)
        ((Widget *)plwin_info)->visible = !shaded;
    if (plwin_sinfo)
        ((Widget *)plwin_sinfo)->visible = shaded;

    if (plwin_drawing_area) {
        gint scale = cfg.scale_factor;
        if (scale < 1) scale = 2;
        gtk_drawing_area_set_content_width(
            GTK_DRAWING_AREA(plwin_drawing_area), PLWIN_WIDTH * scale);
        gtk_drawing_area_set_content_height(
            GTK_DRAWING_AREA(plwin_drawing_area),
            playlistwin_height() * scale);
    }
    mainwin_update_attached_size();
    plwin_queue_draw();
}

gboolean
playlistwin_is_shaded(void)
{
    return plwin_shaded;
}

gint
playlistwin_height(void)
{
    return plwin_shaded ? 14 : PLWIN_HEIGHT;
}

void
playlistwin_update(void)
{
    if (playlistwin_is_visible())
        plwin_queue_draw();
}
