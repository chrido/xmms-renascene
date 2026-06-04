#include "xmms.h"
#include <stdarg.h>

Config cfg;

GtkWidget *mainwin = NULL;
GtkWidget *mainwin_drawing_area = NULL;
GtkWidget *mainwin_container = NULL;

static GList *mainwin_wlist = NULL;

/* Main window widgets */
static PButton *mainwin_menubtn, *mainwin_minimize, *mainwin_shade, *mainwin_close;
static PButton *mainwin_rew, *mainwin_play, *mainwin_pause, *mainwin_stop;
static PButton *mainwin_fwd, *mainwin_eject;
static TButton *mainwin_shuffle, *mainwin_repeat, *mainwin_eq, *mainwin_pl;
static TextBox *mainwin_info;
static TextBox *mainwin_rate_text, *mainwin_freq_text;
/* Shaded mode time text - TODO */
static HSlider *mainwin_volume, *mainwin_balance, *mainwin_position;
static MonoStereo *mainwin_monostereo;
static PlayStatus *mainwin_playstatus;
static Number *mainwin_minus_num, *mainwin_10min_num, *mainwin_min_num;
static Number *mainwin_10sec_num, *mainwin_sec_num;
static Vis *mainwin_vis;

static Widget *pressed_widget = NULL;

static guint update_timeout_tag = 0;
static gboolean app_initialized = FALSE;
static gboolean startup_reset = FALSE;
static gboolean mainwin_shaded = FALSE;
static gint vis_update_divisor = 1;
static gint vis_update_counter = 0;

static gint mainwin_current_height(void);
static void mainwin_set_shaded(gboolean shaded);
static void mainwin_update_time_display(void);
static void mainwin_apply_scale_factor(void);
static void mainwin_reload_skin(void);
static void mainwin_set_doublesize(gboolean enabled);
static void mainwin_set_always_on_top(gboolean enabled);
static void mainwin_set_sticky(gboolean enabled);
static void mainwin_set_easy_move(gboolean enabled);
static void mainwin_show_message(const gchar *title, const gchar *message);

typedef enum {
    MAINWIN_PROMPT_PLAY_LOCATION,
    MAINWIN_PROMPT_JUMP_TIME,
    MAINWIN_PROMPT_JUMP_FILE
} MainwinPromptAction;

typedef struct {
    GtkWidget *window;
    GtkWidget *entry;
    MainwinPromptAction action;
} MainwinPrompt;

static const GOptionEntry app_option_entries[] = {
    { "playlist", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Show the playlist window on startup", NULL },
    { "equalizer", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Show the equalizer window on startup", NULL },
    { "reset", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Start with default settings and an empty playlist", NULL },
    { "playlist-menu-add", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Show the playlist Add menu on startup", NULL },
    { "playlist-menu-remove", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Show the playlist Remove menu on startup", NULL },
    { "playlist-menu-select", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Show the playlist Select menu on startup", NULL },
    { "playlist-menu-misc", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Show the playlist Misc menu on startup", NULL },
    { "playlist-menu-list", 0, 0, G_OPTION_ARG_NONE, NULL,
      "Show the playlist List menu on startup", NULL },
    { NULL }
};

/* Forward declarations */
static void open_files_cb(GObject *source, GAsyncResult *result, gpointer data);
static void open_directory_cb(GObject *source, GAsyncResult *result, gpointer data);

static gchar *
playlist_state_file(void)
{
    gchar *config_dir = xmms_get_config_dir();
    gchar *playlist_file = g_build_filename(config_dir, "playlist.m3u", NULL);
    g_free(config_dir);
    return playlist_file;
}

static gboolean
open_playlist_menu_idle(gpointer data)
{
    gchar *menu = data;
    playlistwin_show(TRUE);
    playlistwin_show_menu(menu);
    g_free(menu);
    return G_SOURCE_REMOVE;
}

static const gchar *
playlist_menu_option(GVariantDict *options)
{
    if (g_variant_dict_contains(options, "playlist-menu-add"))
        return "add";
    if (g_variant_dict_contains(options, "playlist-menu-remove"))
        return "remove";
    if (g_variant_dict_contains(options, "playlist-menu-select"))
        return "select";
    if (g_variant_dict_contains(options, "playlist-menu-misc"))
        return "misc";
    if (g_variant_dict_contains(options, "playlist-menu-list"))
        return "list";
    return NULL;
}

static gboolean
session_debug_enabled(void)
{
    const gchar *debug = g_getenv("XMMS_DEBUG_SESSION");
    return debug && debug[0] && g_strcmp0(debug, "0") != 0;
}

static void
session_debug(const gchar *format, ...)
{
    if (!session_debug_enabled())
        return;

    va_list args;
    va_start(args, format);
    gchar *message = g_strdup_vprintf(format, args);
    va_end(args);

    g_printerr("xmms-session: %s\n", message);
    g_free(message);
}

/* ---- Callbacks ---- */

static void
mainwin_play_pushed(void)
{
    PlayerState state = player_get_state();
    if (state == PLAYER_PAUSED) {
        player_unpause();
    } else if (state == PLAYER_STOPPED) {
        playlist_play();
    }
}

static void
mainwin_pause_pushed(void)
{
    player_toggle_pause();
}

static void
mainwin_stop_pushed(void)
{
    player_stop();
}

static void
mainwin_rew_pushed(void)
{
    playlist_prev();
}

static void
mainwin_fwd_pushed(void)
{
    playlist_next();
}

static void
mainwin_eject_pushed(void)
{
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, "Open Files");

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

    gtk_file_dialog_open_multiple(dialog,
        GTK_WINDOW(mainwin), NULL, open_files_cb, NULL);

    /* dialog is owned by the async operation — do not unref it here */
    g_object_unref(filters);
    g_object_unref(filter);
    g_object_unref(all_filter);
}

static void
mainwin_shuffle_pushed(gboolean toggled)
{
    (void)toggled;
    playlist_shuffle_toggle();
}

static void
mainwin_repeat_pushed(gboolean toggled)
{
    (void)toggled;
    playlist_repeat_toggle();
}

static void
mainwin_eq_pushed(gboolean toggled)
{
    (void)toggled;
    equalizerwin_show(!equalizerwin_is_visible());
}

static void
mainwin_pl_pushed(gboolean toggled)
{
    (void)toggled;
    playlistwin_show(!playlistwin_is_visible());
}

static void mainwin_close_pushed(void) {
    GApplication *app = g_application_get_default();
    if (app) g_application_quit(app);
}
static void mainwin_minimize_pushed(void) { gtk_window_minimize(GTK_WINDOW(mainwin)); }
static void mainwin_shade_pushed(void) { mainwin_set_shaded(!mainwin_shaded); }
static void
mainwin_menu_skin_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    skinwin_show();
}

static void
mainwin_menu_spotify_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    spotifywin_show(GTK_WINDOW(mainwin));
}

static void
mainwin_menu_output_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    outputwin_show(GTK_WINDOW(mainwin));
}

static void
mainwin_menu_prefs_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    mainwin_show_message("Preferences",
                         "The full XMMS preferences window is not implemented yet.");
}

static void
mainwin_menu_reload_skin_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    mainwin_reload_skin();
}

static void
mainwin_menu_repeat_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    playlist_repeat_toggle();
    tbutton_set_toggled(mainwin_repeat, playlist_get_repeat());
    mainwin_queue_draw();
}

static void
mainwin_menu_shuffle_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    playlist_shuffle_toggle();
    tbutton_set_toggled(mainwin_shuffle, playlist_get_shuffle());
    mainwin_queue_draw();
}

static void
mainwin_menu_no_advance_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    playlist_set_no_advance(!playlist_get_no_advance());
}

static void
mainwin_menu_time_elapsed_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    cfg.timer_mode = TIMER_ELAPSED;
    mainwin_update_time_display();
    playlistwin_update();
    mainwin_queue_draw();
}

static void
mainwin_menu_time_remaining_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    cfg.timer_mode = TIMER_REMAINING;
    mainwin_update_time_display();
    playlistwin_update();
    mainwin_queue_draw();
}

static void
mainwin_menu_always_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    mainwin_set_always_on_top(!cfg.always_on_top);
}

static void
mainwin_menu_sticky_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    mainwin_set_sticky(!cfg.sticky);
}

static void
mainwin_menu_doublesize_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    mainwin_set_doublesize(!cfg.doublesize);
}

static void
mainwin_menu_easy_move_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    mainwin_set_easy_move(!cfg.easy_move);
}

static void
mainwin_menu_vis_analyzer_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_set_mode(mainwin_vis, VIS_MODE_ANALYZER);
    mainwin_queue_draw();
}

static void
mainwin_menu_vis_scope_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_set_mode(mainwin_vis, VIS_MODE_SCOPE);
    mainwin_queue_draw();
}

static void
mainwin_menu_vis_off_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_set_mode(mainwin_vis, VIS_MODE_OFF);
    mainwin_queue_draw();
}

static void
mainwin_menu_vis_bars_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_set_analyzer_style(mainwin_vis, VIS_ANALYZER_BARS);
    mainwin_queue_draw();
}

static void
mainwin_menu_vis_lines_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_set_analyzer_style(mainwin_vis, VIS_ANALYZER_LINES);
    mainwin_queue_draw();
}

static void
mainwin_menu_vis_peaks_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    static gboolean enabled = TRUE;
    (void)action; (void)param; (void)data;
    enabled = !enabled;
    vis_set_peaks_enabled(mainwin_vis, enabled);
    mainwin_queue_draw();
}

static void
mainwin_menu_vis_falloff_slow_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_set_falloff(mainwin_vis, 0.015f);
}

static void
mainwin_menu_vis_falloff_fast_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_set_falloff(mainwin_vis, 0.08f);
}

static void
mainwin_menu_vis_refresh_full_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_update_divisor = 1;
}

static void
mainwin_menu_vis_refresh_half_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_update_divisor = 2;
}

static void
mainwin_menu_vis_refresh_quarter_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    vis_update_divisor = 4;
}

static void
mainwin_menu_windowshade_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    mainwin_set_shaded(!mainwin_shaded);
}

static void
mainwin_menu_playlist_shade_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    playlistwin_set_shaded(!playlistwin_is_shaded());
}

static void
mainwin_menu_equalizer_shade_cb(GSimpleAction *action, GVariant *param, gpointer data)
{
    (void)action; (void)param; (void)data;
    equalizerwin_set_shaded(!equalizerwin_is_shaded());
}

static void
mainwin_menubtn_pushed(void)
{
    /* Build a popover menu */
    GMenu *menu = g_menu_new();
    GMenu *options = g_menu_new();
    GMenu *vis = g_menu_new();
    GMenu *shade = g_menu_new();
    g_menu_append(options, "Preferences", "win.preferences");
    g_menu_append(menu, "Skin Browser...", "win.skin-browser");
    g_menu_append(options, "Reload Skin", "win.reload-skin");
    g_menu_append(options, "Repeat", "win.repeat");
    g_menu_append(options, "Shuffle", "win.shuffle");
    g_menu_append(options, "No Playlist Advance", "win.no-advance");
    g_menu_append(options, "Time Elapsed", "win.time-elapsed");
    g_menu_append(options, "Time Remaining", "win.time-remaining");
    g_menu_append(options, "Always On Top", "win.always-on-top");
    g_menu_append(options, "Sticky", "win.sticky");
    g_menu_append(options, "DoubleSize", "win.doublesize");
    g_menu_append(options, "Easy Move", "win.easy-move");
    g_menu_append_submenu(menu, "Options", G_MENU_MODEL(options));
    g_menu_append(menu, "Spotify Playlists...", "win.spotify");
    g_menu_append(menu, "Output Device...", "win.output");
    g_menu_append(vis, "Analyzer", "win.vis-analyzer");
    g_menu_append(vis, "Scope", "win.vis-scope");
    g_menu_append(vis, "Off", "win.vis-off");
    g_menu_append(vis, "Analyzer Bars", "win.vis-bars");
    g_menu_append(vis, "Analyzer Lines", "win.vis-lines");
    g_menu_append(vis, "Toggle Peaks", "win.vis-peaks");
    g_menu_append(vis, "Slow Falloff", "win.vis-falloff-slow");
    g_menu_append(vis, "Fast Falloff", "win.vis-falloff-fast");
    g_menu_append(vis, "Refresh Full", "win.vis-refresh-full");
    g_menu_append(vis, "Refresh Half", "win.vis-refresh-half");
    g_menu_append(vis, "Refresh Quarter", "win.vis-refresh-quarter");
    g_menu_append_submenu(menu, "Visualization", G_MENU_MODEL(vis));
    g_menu_append(shade, "WindowShade Mode", "win.windowshade");
    g_menu_append(shade, "Playlist WindowShade Mode", "win.playlist-shade");
    g_menu_append(shade, "Equalizer WindowShade Mode", "win.equalizer-shade");
    g_menu_append_submenu(menu, "WindowShade", G_MENU_MODEL(shade));

    GtkWidget *popover = gtk_popover_menu_new_from_model(G_MENU_MODEL(menu));
    gtk_widget_set_parent(popover, mainwin_drawing_area);

    GdkRectangle rect = { 6, 3, 9, 9 };
    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 2;
    rect.x *= scale; rect.y *= scale;
    rect.width *= scale; rect.height *= scale;
    gtk_popover_set_pointing_to(GTK_POPOVER(popover), &rect);

    gtk_popover_popup(GTK_POPOVER(popover));
    g_object_unref(options);
    g_object_unref(vis);
    g_object_unref(shade);
    g_object_unref(menu);
}

static void
mainwin_dialog_close_clicked(GtkButton *button, gpointer data)
{
    (void)button;
    gtk_window_destroy(GTK_WINDOW(data));
}

static void
mainwin_show_message(const gchar *title, const gchar *message)
{
    GtkWidget *window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(window), title);
    gtk_window_set_transient_for(GTK_WINDOW(window), GTK_WINDOW(mainwin));
    gtk_window_set_modal(GTK_WINDOW(window), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(window), FALSE);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    gtk_widget_set_margin_top(box, 12);
    gtk_widget_set_margin_bottom(box, 12);
    gtk_widget_set_margin_start(box, 12);
    gtk_widget_set_margin_end(box, 12);
    gtk_window_set_child(GTK_WINDOW(window), box);

    gtk_box_append(GTK_BOX(box), gtk_label_new(message));
    GtkWidget *ok = gtk_button_new_with_label("OK");
    g_signal_connect(ok, "clicked",
                     G_CALLBACK(mainwin_dialog_close_clicked), window);
    gtk_box_append(GTK_BOX(box), ok);
    gtk_window_present(GTK_WINDOW(window));
}

static gint64
mainwin_parse_time_ms(const gchar *text)
{
    if (!text || !text[0])
        return -1;

    gchar **parts = g_strsplit(text, ":", 3);
    gint64 result = -1;
    if (parts[0] && parts[1] && !parts[2]) {
        gint64 minutes = g_ascii_strtoll(parts[0], NULL, 10);
        gint64 seconds = g_ascii_strtoll(parts[1], NULL, 10);
        result = (minutes * 60 + seconds) * 1000;
    } else if (parts[0] && !parts[1]) {
        result = g_ascii_strtoll(parts[0], NULL, 10) * 1000;
    }
    g_strfreev(parts);
    return result;
}

static void
mainwin_prompt_free(MainwinPrompt *prompt)
{
    g_free(prompt);
}

static void
mainwin_prompt_destroyed(GtkWidget *widget, gpointer data)
{
    (void)widget;
    mainwin_prompt_free(data);
}

static void
mainwin_prompt_accept(GtkButton *button, gpointer data)
{
    (void)button;
    MainwinPrompt *prompt = data;
    const gchar *text = gtk_editable_get_text(GTK_EDITABLE(prompt->entry));

    switch (prompt->action) {
    case MAINWIN_PROMPT_PLAY_LOCATION:
        if (text && text[0]) {
            playlist_add_uri(text);
            playlist_set_position(playlist_get_length() - 1);
            playlist_play();
        }
        break;
    case MAINWIN_PROMPT_JUMP_TIME: {
        gint64 ms = mainwin_parse_time_ms(text);
        if (ms >= 0)
            player_seek(ms);
        break;
    }
    case MAINWIN_PROMPT_JUMP_FILE:
        if (text && text[0]) {
            for (gint i = 0; i < playlist_get_length(); i++) {
                const gchar *title = playlist_get_title(i);
                const gchar *filename = playlist_get_filename(i);
                if ((title && g_strrstr(title, text)) ||
                    (filename && g_strrstr(filename, text))) {
                    playlist_set_position(i);
                    playlist_play();
                    playlistwin_show(TRUE);
                    break;
                }
            }
        }
        break;
    }

    gtk_window_destroy(GTK_WINDOW(prompt->window));
}

static void
mainwin_show_prompt(const gchar *title, const gchar *placeholder,
                    MainwinPromptAction action)
{
    MainwinPrompt *prompt = g_new0(MainwinPrompt, 1);
    prompt->action = action;
    prompt->window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(prompt->window), title);
    gtk_window_set_transient_for(GTK_WINDOW(prompt->window), GTK_WINDOW(mainwin));
    gtk_window_set_modal(GTK_WINDOW(prompt->window), TRUE);
    gtk_window_set_resizable(GTK_WINDOW(prompt->window), FALSE);
    g_signal_connect(prompt->window, "destroy",
                     G_CALLBACK(mainwin_prompt_destroyed), prompt);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    gtk_widget_set_margin_top(box, 12);
    gtk_widget_set_margin_bottom(box, 12);
    gtk_widget_set_margin_start(box, 12);
    gtk_widget_set_margin_end(box, 12);
    gtk_window_set_child(GTK_WINDOW(prompt->window), box);

    prompt->entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(prompt->entry), placeholder);
    gtk_box_append(GTK_BOX(box), prompt->entry);

    GtkWidget *buttons = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_box_append(GTK_BOX(box), buttons);
    GtkWidget *cancel = gtk_button_new_with_label("Cancel");
    GtkWidget *ok = gtk_button_new_with_label("OK");
    g_signal_connect(cancel, "clicked",
                     G_CALLBACK(mainwin_dialog_close_clicked), prompt->window);
    g_signal_connect(ok, "clicked", G_CALLBACK(mainwin_prompt_accept), prompt);
    gtk_box_append(GTK_BOX(buttons), cancel);
    gtk_box_append(GTK_BOX(buttons), ok);

    gtk_window_present(GTK_WINDOW(prompt->window));
}

static void
mainwin_show_file_info(void)
{
    gint pos = playlist_get_position();
    PlaylistEntry *entry = playlist_get_entry(pos);
    if (!entry) {
        mainwin_show_message("File Info", "No playlist entry is playing.");
        return;
    }

    gchar *duration = entry->length > 0 ? time_to_string(entry->length) :
        g_strdup("unknown");
    gchar *message = g_strdup_printf("Title: %s\nLocation: %s\nLength: %s",
                                     entry->title ? entry->title : "",
                                     entry->filename ? entry->filename : "",
                                     duration);
    mainwin_show_message("File Info", message);
    g_free(message);
    g_free(duration);
}

static void
mainwin_open_directory(void)
{
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog, "Open Directory");
    gtk_file_dialog_select_folder(dialog, GTK_WINDOW(mainwin), NULL,
                                  open_directory_cb, NULL);
}

/* ---- Volume/Balance/Position callbacks ---- */

static gint
mainwin_volume_framecb(gint pos)
{
    return (gint)((pos / 51.0) * 27);
}

static void
mainwin_volume_motioncb(gint pos)
{
    gint vol = (gint)((pos * 100.0) / 51.0);
    player_set_volume(vol);
}

static void
mainwin_volume_releasecb(gint pos)
{
    mainwin_volume_motioncb(pos);
}

static gint
mainwin_balance_framecb(gint pos)
{
    return (gint)((pos / 24.0) * 27);
}

static void
mainwin_balance_motioncb(gint pos)
{
    gint bal = (gint)(((pos - 12) * 100.0) / 12.0);
    player_set_balance(bal);
}

static void
mainwin_balance_releasecb(gint pos)
{
    mainwin_balance_motioncb(pos);
}

static void
mainwin_position_motioncb(gint pos)
{
    /* Seek on release only */
    (void)pos;
}

static void
mainwin_position_releasecb(gint pos)
{
    gint64 dur = player_get_duration();
    if (dur > 0) {
        gint64 target = (gint64)((pos * dur) / 219.0);
        player_seek(target);
    }
}

/* ---- Drawing ---- */

void
mainwin_queue_draw(void)
{
    if (mainwin_drawing_area)
        gtk_widget_queue_draw(mainwin_drawing_area);
}

void
mainwin_update_attached_size(void)
{
    if (!mainwin)
        return;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 2;

    gint height = mainwin_current_height();
    if (equalizerwin_is_visible() && !equalizerwin_is_detached())
        height += equalizerwin_height();
    if (playlistwin_is_visible() && !playlistwin_is_detached())
        height += playlistwin_height();

    gtk_window_set_default_size(GTK_WINDOW(mainwin),
                                MAINWIN_WIDTH * scale, height * scale);
    gtk_widget_queue_resize(mainwin);
}

void
mainwin_update_panel_toggles(void)
{
    if (mainwin_eq)
        tbutton_set_toggled(mainwin_eq, equalizerwin_is_visible());
    if (mainwin_pl)
        tbutton_set_toggled(mainwin_pl, playlistwin_is_visible());
}

static void
draw_mainwin_titlebar(cairo_t *cr, gboolean focused)
{
    /* titlebar.bmp layout: source x=0-26 holds button graphics (not background),
       source x=27+ holds the actual titlebar strip (275 pixels wide).
       Draw source (27,y) to dest (0,0) covering full window width.
       PButtons draw button icons on top at their screen positions. */
    gint ty = focused ? 0 : 15;
    skin_draw_pixmap(cr, SKIN_TITLEBAR,
                     27, ty, 0, 0, MAINWIN_WIDTH, 14);
}

static gint
mainwin_current_height(void)
{
    return mainwin_shaded ? 14 : MAINWIN_HEIGHT;
}

static void
mainwin_set_shaded(gboolean shaded)
{
    mainwin_shaded = shaded;
    if (mainwin_shade) {
        mainwin_shade->ny = shaded ? 27 : 18;
        mainwin_shade->py = shaded ? 27 : 18;
    }

    if (mainwin_drawing_area) {
        gint scale = cfg.scale_factor;
        if (scale < 1) scale = 2;
        gtk_drawing_area_set_content_height(
            GTK_DRAWING_AREA(mainwin_drawing_area),
            mainwin_current_height() * scale);
    }

    mainwin_update_attached_size();
    mainwin_queue_draw();
}

static void
mainwin_apply_scale_factor(void)
{
    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;

    if (mainwin_drawing_area) {
        gtk_drawing_area_set_content_width(
            GTK_DRAWING_AREA(mainwin_drawing_area), MAINWIN_WIDTH * scale);
        gtk_drawing_area_set_content_height(
            GTK_DRAWING_AREA(mainwin_drawing_area),
            mainwin_current_height() * scale);
    }
    playlistwin_set_shaded(playlistwin_is_shaded());
    equalizerwin_set_shaded(equalizerwin_is_shaded());
    mainwin_update_attached_size();
    mainwin_queue_draw();
}

static void
mainwin_reload_skin(void)
{
    if (cfg.skin)
        skin_load(cfg.skin);
    mainwin_queue_draw();
    playlistwin_update();
}

static void
mainwin_set_doublesize(gboolean enabled)
{
    cfg.doublesize = enabled;
    cfg.scale_factor = enabled ? 2 : 1;
    mainwin_apply_scale_factor();
}

static void
mainwin_set_always_on_top(gboolean enabled)
{
    cfg.always_on_top = enabled;
    if (enabled && mainwin)
        gtk_window_present(GTK_WINDOW(mainwin));
}

static void
mainwin_set_sticky(gboolean enabled)
{
    cfg.sticky = enabled;
    if (enabled && mainwin)
        gtk_window_present(GTK_WINDOW(mainwin));
}

static void
mainwin_set_easy_move(gboolean enabled)
{
    cfg.easy_move = enabled;
}

void
draw_main_window(cairo_t *cr)
{
    if (mainwin_shaded) {
        draw_mainwin_titlebar(cr, TRUE);
        if (mainwin_menubtn)
            ((Widget *)mainwin_menubtn)->draw((Widget *)mainwin_menubtn, cr);
        if (mainwin_minimize)
            ((Widget *)mainwin_minimize)->draw((Widget *)mainwin_minimize, cr);
        if (mainwin_shade)
            ((Widget *)mainwin_shade)->draw((Widget *)mainwin_shade, cr);
        if (mainwin_close)
            ((Widget *)mainwin_close)->draw((Widget *)mainwin_close, cr);
        return;
    }

    /* Draw main background */
    skin_draw_pixmap(cr, SKIN_MAIN,
                     0, 0, 0, 0, MAINWIN_WIDTH, MAINWIN_HEIGHT);

    /* Draw titlebar */
    gboolean focused = TRUE; /* TODO: track focus */
    draw_mainwin_titlebar(cr, focused);

    /* Draw all widgets */
    widget_list_draw(mainwin_wlist, cr);
}

static void
mainwin_draw_func(GtkDrawingArea *area, cairo_t *cr,
                  int width, int height, gpointer data)
{
    (void)area; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;

    cairo_scale(cr, (double)width / MAINWIN_WIDTH,
                    (double)height / mainwin_current_height());

    draw_main_window(cr);
}

/* ---- Event handling ---- */

static void
mainwin_click_pressed(GtkGestureClick *gesture, int n_press,
                      double x, double y, gpointer data)
{
    (void)data; (void)n_press;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;

    /* Convert to skin coordinates */
    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);

    gint button = gtk_gesture_single_get_current_button(
        GTK_GESTURE_SINGLE(gesture));

    if (mainwin_shaded && sy >= 14)
        return;

    if (button == 1 && n_press == 2 && sy < 14) {
        mainwin_set_shaded(!mainwin_shaded);
        return;
    }

    pressed_widget = widget_list_find(mainwin_wlist, sx, sy);

    if (pressed_widget && pressed_widget->button_press) {
        pressed_widget->button_press(pressed_widget, sx, sy, button);
    } else if (button == 1 && (sy < 14 || cfg.easy_move)) {
        /* Titlebar drag */
        GdkSurface *surface = gtk_native_get_surface(GTK_NATIVE(mainwin));
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
mainwin_click_released(GtkGestureClick *gesture, int n_press,
                       double x, double y, gpointer data)
{
    (void)data; (void)n_press;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;

    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);

    gint button = gtk_gesture_single_get_current_button(
        GTK_GESTURE_SINGLE(gesture));

    if (pressed_widget && pressed_widget->button_release)
        pressed_widget->button_release(pressed_widget, sx, sy, button);

    pressed_widget = NULL;
}

static void
mainwin_motion(GtkEventControllerMotion *controller,
               double x, double y, gpointer data)
{
    (void)controller; (void)data;

    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 1;

    gint sx = (gint)(x / scale);
    gint sy = (gint)(y / scale);

    if (pressed_widget && pressed_widget->motion)
        pressed_widget->motion(pressed_widget, sx, sy);
}

static gboolean
mainwin_key_pressed(GtkEventControllerKey *controller, guint keyval,
                    guint keycode, GdkModifierType state, gpointer data)
{
    (void)controller; (void)keycode; (void)data;

    guint key = gdk_keyval_to_lower(keyval);
    gboolean ctrl = (state & GDK_CONTROL_MASK) != 0;
    gboolean shift = (state & GDK_SHIFT_MASK) != 0;
    gboolean alt = (state & GDK_ALT_MASK) != 0;

    if (!ctrl && !shift && !alt) {
        switch (key) {
        case GDK_KEY_z:
            playlist_prev();
            return GDK_EVENT_STOP;
        case GDK_KEY_x:
            mainwin_play_pushed();
            return GDK_EVENT_STOP;
        case GDK_KEY_c:
            mainwin_pause_pushed();
            return GDK_EVENT_STOP;
        case GDK_KEY_v:
            mainwin_stop_pushed();
            return GDK_EVENT_STOP;
        case GDK_KEY_b:
            playlist_next();
            return GDK_EVENT_STOP;
        case GDK_KEY_l:
            mainwin_eject_pushed();
            return GDK_EVENT_STOP;
        case GDK_KEY_j:
            mainwin_show_prompt("Jump to File", "Title or filename",
                                MAINWIN_PROMPT_JUMP_FILE);
            return GDK_EVENT_STOP;
        case GDK_KEY_r:
            playlist_repeat_toggle();
            tbutton_set_toggled(mainwin_repeat, playlist_get_repeat());
            mainwin_queue_draw();
            return GDK_EVENT_STOP;
        case GDK_KEY_s:
            playlist_shuffle_toggle();
            tbutton_set_toggled(mainwin_shuffle, playlist_get_shuffle());
            mainwin_queue_draw();
            return GDK_EVENT_STOP;
        case GDK_KEY_F5:
            mainwin_reload_skin();
            return GDK_EVENT_STOP;
        default:
            break;
        }
    }

    if (shift && !ctrl && !alt && key == GDK_KEY_l) {
        mainwin_open_directory();
        return GDK_EVENT_STOP;
    }

    if (alt && !ctrl && !shift) {
        switch (key) {
        case GDK_KEY_s:
            skinwin_show();
            return GDK_EVENT_STOP;
        case GDK_KEY_w:
            gtk_window_present(GTK_WINDOW(mainwin));
            return GDK_EVENT_STOP;
        case GDK_KEY_e:
            playlistwin_show(!playlistwin_is_visible());
            return GDK_EVENT_STOP;
        case GDK_KEY_g:
            equalizerwin_show(!equalizerwin_is_visible());
            return GDK_EVENT_STOP;
        default:
            break;
        }
    }

    if (ctrl && !alt) {
        if (shift && key == GDK_KEY_w) {
            playlistwin_set_shaded(!playlistwin_is_shaded());
            return GDK_EVENT_STOP;
        }

        if (shift)
            return GDK_EVENT_PROPAGATE;

        switch (key) {
        case GDK_KEY_p:
            mainwin_show_message("Preferences",
                                 "Preferences are not implemented yet.");
            return GDK_EVENT_STOP;
        case GDK_KEY_l:
            mainwin_show_prompt("Play Location", "https://...",
                                MAINWIN_PROMPT_PLAY_LOCATION);
            return GDK_EVENT_STOP;
        case GDK_KEY_n:
            playlist_set_no_advance(!playlist_get_no_advance());
            return GDK_EVENT_STOP;
        case GDK_KEY_e:
            cfg.timer_mode = TIMER_ELAPSED;
            mainwin_update_time_display();
            playlistwin_update();
            mainwin_queue_draw();
            return GDK_EVENT_STOP;
        case GDK_KEY_r:
            cfg.timer_mode = TIMER_REMAINING;
            mainwin_update_time_display();
            playlistwin_update();
            mainwin_queue_draw();
            return GDK_EVENT_STOP;
        case GDK_KEY_a:
            mainwin_set_always_on_top(!cfg.always_on_top);
            return GDK_EVENT_STOP;
        case GDK_KEY_s:
            mainwin_set_sticky(!cfg.sticky);
            return GDK_EVENT_STOP;
        case GDK_KEY_w:
            mainwin_set_shaded(!mainwin_shaded);
            return GDK_EVENT_STOP;
        case GDK_KEY_d:
            mainwin_set_doublesize(!cfg.doublesize);
            return GDK_EVENT_STOP;
        case GDK_KEY_3:
            mainwin_show_file_info();
            return GDK_EVENT_STOP;
        case GDK_KEY_j:
            mainwin_show_prompt("Jump to Time", "seconds or mm:ss",
                                MAINWIN_PROMPT_JUMP_TIME);
            return GDK_EVENT_STOP;
        case GDK_KEY_z:
            if (playlist_get_length() > 0) {
                playlist_set_position(0);
                playlist_play();
            }
            return GDK_EVENT_STOP;
        case GDK_KEY_v:
            mainwin_show_message("Visualization Plugins",
                                 "Visualization plugins are not implemented yet.");
            return GDK_EVENT_STOP;
        default:
            break;
        }
    }

    if (ctrl && alt && !shift && key == GDK_KEY_w) {
        equalizerwin_set_shaded(!equalizerwin_is_shaded());
        return GDK_EVENT_STOP;
    }

    return GDK_EVENT_PROPAGATE;
}

/* ---- Update timer ---- */

static void
mainwin_update_time_display(void)
{
    gint64 time_ms;
    gint64 dur = player_get_duration();

    if (cfg.timer_mode == TIMER_REMAINING && dur > 0)
        time_ms = dur - player_get_position();
    else
        time_ms = player_get_position();

    gint secs = (gint)(time_ms / 1000);
    gint mins = secs / 60;
    secs %= 60;

    if (cfg.timer_mode == TIMER_REMAINING && dur > 0)
        number_set_value(mainwin_minus_num, 11); /* dash */
    else
        number_set_value(mainwin_minus_num, 10); /* blank */

    number_set_value(mainwin_10min_num, mins / 10);
    number_set_value(mainwin_min_num, mins % 10);
    number_set_value(mainwin_10sec_num, secs / 10);
    number_set_value(mainwin_sec_num, secs % 10);
}

static void
mainwin_update_position_slider(void)
{
    if (mainwin_position->pressed)
        return;

    gint64 dur = player_get_duration();
    gint64 pos = player_get_position();

    if (dur > 0) {
        gint slider_pos = (gint)((pos * 219.0) / dur);
        hslider_set_position(mainwin_position, slider_pos);
    }
}

static void
mainwin_update_song_info_display(PlayerState state)
{
    if (state == PLAYER_STOPPED) {
        textbox_set_text(mainwin_rate_text, "   ");
        textbox_set_text(mainwin_freq_text, "  ");
        monostereo_set_channels(mainwin_monostereo, 0);
        return;
    }

    gint bitrate = 0;
    gint frequency = 0;
    gint channels = 0;
    player_get_song_info(&bitrate, &frequency, &channels);

    gchar text[8];
    if (bitrate > 0) {
        gint kbps = bitrate / 1000;
        if (kbps < 1000)
            g_snprintf(text, sizeof(text), "%3d", kbps);
        else
            g_snprintf(text, sizeof(text), "%2dH", kbps / 100);
        textbox_set_text(mainwin_rate_text, text);
    } else if (bitrate < 0) {
        textbox_set_text(mainwin_rate_text, "VBR");
    } else {
        textbox_set_text(mainwin_rate_text, "   ");
    }

    if (frequency > 0) {
        gint khz = (frequency + 500) / 1000;
        g_snprintf(text, sizeof(text), "%2d", khz);
        textbox_set_text(mainwin_freq_text, text);
    } else {
        textbox_set_text(mainwin_freq_text, "  ");
    }

    monostereo_set_channels(mainwin_monostereo, channels);
}

static gboolean
mainwin_update_cb(gpointer data)
{
    (void)data;

    player_update();

    static PlayerState prev_state = PLAYER_STOPPED;
    static gint prev_playlist_pos = -1;
    PlayerState state = player_get_state();

    if (state == PLAYER_PLAYING || state == PLAYER_PAUSED) {
        mainwin_update_time_display();
        mainwin_update_position_slider();
        mainwin_update_song_info_display(state);

        /* Update title */
        gint pos = playlist_get_position();
        const gchar *title = playlist_get_title(pos);
        if (title) {
            textbox_set_text(mainwin_info, title);

            /* Update MPRIS metadata only when track changes */
            if (pos != prev_playlist_pos) {
                prev_playlist_pos = pos;
                PlaylistEntry *entry = playlist_get_entry(pos);
                gchar *uri = entry ? filename_to_uri(entry->filename) : NULL;
                gint64 len_us = entry && entry->length > 0 ?
                                (gint64)entry->length * 1000 : 0;
                mpris_update_metadata(title, uri, len_us);
                g_free(uri);
            }
        }

        /* Play status */
        if (state == PLAYER_PLAYING)
            playstatus_set_status(mainwin_playstatus, 2);
        else
            playstatus_set_status(mainwin_playstatus, 1);

        /* Visualization data */
        gfloat vis_data[75];
        if (++vis_update_counter >= vis_update_divisor) {
            vis_update_counter = 0;
            if (player_get_vis_data(vis_data, 75))
                vis_set_data(mainwin_vis, vis_data, 75);
        }

        /* Update playlist window */
        playlistwin_update();
    } else {
        playstatus_set_status(mainwin_playstatus, 0);
        mainwin_update_song_info_display(state);

        number_set_value(mainwin_minus_num, 10);
        number_set_value(mainwin_10min_num, 10);
        number_set_value(mainwin_min_num, 10);
        number_set_value(mainwin_10sec_num, 10);
        number_set_value(mainwin_sec_num, 10);
    }

    /* Emit MPRIS playback status change */
    if (state != prev_state) {
        mpris_update_playback_status();
        prev_state = state;
    }

    return G_SOURCE_CONTINUE;
}

/* ---- Window creation ---- */

static void
create_mainwin_widgets(void)
{
    /* Titlebar buttons - source coords in titlebar.bmp:
       x=0: menu, x=9: shade, x=18: close/minimize
       y=0: normal, y=9: pressed */
    mainwin_menubtn = pbutton_new(&mainwin_wlist, 6, 3, 9, 9,
                                  0, 0, 0, 9,
                                  mainwin_menubtn_pushed, SKIN_TITLEBAR);
    mainwin_minimize = pbutton_new(&mainwin_wlist, 244, 3, 9, 9,
                                   9, 0, 9, 9,
                                   mainwin_minimize_pushed, SKIN_TITLEBAR);
    mainwin_shade = pbutton_new(&mainwin_wlist, 254, 3, 9, 9,
                                0, 18, 9, 18,
                                mainwin_shade_pushed, SKIN_TITLEBAR);
    mainwin_close = pbutton_new(&mainwin_wlist, 264, 3, 9, 9,
                                 18, 0, 18, 9,
                                 mainwin_close_pushed, SKIN_TITLEBAR);

    /* Transport controls - from cbuttons.bmp */
    mainwin_rew = pbutton_new(&mainwin_wlist, 16, 88, 23, 18,
                               0, 0, 0, 18,
                               mainwin_rew_pushed, SKIN_CBUTTONS);
    mainwin_play = pbutton_new(&mainwin_wlist, 39, 88, 23, 18,
                                23, 0, 23, 18,
                                mainwin_play_pushed, SKIN_CBUTTONS);
    mainwin_pause = pbutton_new(&mainwin_wlist, 62, 88, 23, 18,
                                 46, 0, 46, 18,
                                 mainwin_pause_pushed, SKIN_CBUTTONS);
    mainwin_stop = pbutton_new(&mainwin_wlist, 85, 88, 23, 18,
                                69, 0, 69, 18,
                                mainwin_stop_pushed, SKIN_CBUTTONS);
    mainwin_fwd = pbutton_new(&mainwin_wlist, 108, 88, 22, 18,
                               92, 0, 92, 18,
                               mainwin_fwd_pushed, SKIN_CBUTTONS);
    mainwin_eject = pbutton_new(&mainwin_wlist, 136, 89, 22, 16,
                                 114, 0, 114, 16,
                                 mainwin_eject_pushed, SKIN_CBUTTONS);

    /* Toggle buttons - from shufrep.bmp */
    mainwin_shuffle = tbutton_new(&mainwin_wlist, 164, 89, 46, 15,
                                   28, 0, 28, 15, 28, 30, 28, 45,
                                   mainwin_shuffle_pushed, SKIN_SHUFREP);
    mainwin_repeat = tbutton_new(&mainwin_wlist, 210, 89, 28, 15,
                                  0, 0, 0, 15, 0, 30, 0, 45,
                                  mainwin_repeat_pushed, SKIN_SHUFREP);
    mainwin_eq = tbutton_new(&mainwin_wlist, 219, 58, 23, 12,
                              0, 61, 46, 61, 0, 73, 46, 73,
                              mainwin_eq_pushed, SKIN_SHUFREP);
    mainwin_pl = tbutton_new(&mainwin_wlist, 242, 58, 23, 12,
                              23, 61, 69, 61, 23, 73, 69, 73,
                              mainwin_pl_pushed, SKIN_SHUFREP);

    /* Song info text */
    mainwin_info = textbox_new(&mainwin_wlist, 111, 27, 153,
                                TRUE, SKIN_TEXT);
    textbox_set_text(mainwin_info, "XMMS Resuscitated");

    /* Bitrate / frequency text */
    mainwin_rate_text = textbox_new(&mainwin_wlist, 111, 43, 15,
                                    FALSE, SKIN_TEXT);
    mainwin_freq_text = textbox_new(&mainwin_wlist, 156, 43, 10,
                                    FALSE, SKIN_TEXT);

    /* Volume slider */
    mainwin_volume = hslider_new(&mainwin_wlist, 107, 57, 68, 13,
                                  15, 422, 0, 422,
                                  14, 11,
                                  15, 0,
                                  0, 51,
                                  mainwin_volume_framecb,
                                  mainwin_volume_motioncb,
                                  mainwin_volume_releasecb,
                                  SKIN_VOLUME);
    hslider_set_position(mainwin_volume, 51); /* Max volume */

    /* Balance slider */
    mainwin_balance = hslider_new(&mainwin_wlist, 177, 57, 38, 13,
                                   15, 422, 0, 422,
                                   14, 11,
                                   15, 0,
                                   0, 24,
                                   mainwin_balance_framecb,
                                   mainwin_balance_motioncb,
                                   mainwin_balance_releasecb,
                                   SKIN_BALANCE);
    hslider_set_position(mainwin_balance, 12); /* Center */

    /* Position slider */
    mainwin_position = hslider_new(&mainwin_wlist, 16, 72, 248, 10,
                                    248, 0, 278, 0,
                                    29, 10,
                                    1, 0,
                                    0, 219,
                                    NULL,
                                    mainwin_position_motioncb,
                                    mainwin_position_releasecb,
                                    SKIN_POSBAR);

    /* Time display numbers */
    mainwin_minus_num = number_new(&mainwin_wlist, 36, 26, SKIN_NUMBERS);
    mainwin_10min_num = number_new(&mainwin_wlist, 48, 26, SKIN_NUMBERS);
    mainwin_min_num   = number_new(&mainwin_wlist, 60, 26, SKIN_NUMBERS);
    mainwin_10sec_num = number_new(&mainwin_wlist, 78, 26, SKIN_NUMBERS);
    mainwin_sec_num   = number_new(&mainwin_wlist, 90, 26, SKIN_NUMBERS);

    /* Mono/Stereo indicator */
    mainwin_monostereo = monostereo_new(&mainwin_wlist, 212, 41,
                                         SKIN_MONOSTEREO);

    /* Play status */
    mainwin_playstatus = playstatus_new(&mainwin_wlist, 24, 28,
                                         SKIN_PLAYPAUSE);

    /* Visualization */
    mainwin_vis = vis_new(&mainwin_wlist, 24, 43, 76);
}

static void
load_config(void)
{
    memset(&cfg, 0, sizeof(Config));
    cfg.player_x = 100;
    cfg.player_y = 100;
    cfg.scale_factor = 2;
    cfg.timer_mode = TIMER_ELAPSED;
    cfg.volume = 100;
    cfg.balance = 0;
    cfg.no_playlist_advance = FALSE;
    cfg.always_on_top = FALSE;
    cfg.sticky = FALSE;
    cfg.doublesize = TRUE;
    cfg.easy_move = FALSE;
    cfg.playlist_visible = FALSE;
    cfg.playlist_detached = FALSE;
    cfg.shuffle = FALSE;
    cfg.repeat = FALSE;
    cfg.playlist_position = -1;
    cfg.equalizer_visible = FALSE;
    cfg.equalizer_detached = FALSE;
    cfg.equalizer_active = TRUE;
    cfg.equalizer_auto = FALSE;
    cfg.equalizer_preamp_pos = 50;
    for (gint i = 0; i < 10; i++)
        cfg.equalizer_band_pos[i] = 50;

    if (startup_reset) {
        session_debug("reset requested; skipping saved config");
        return;
    }

    /* Try loading config file */
    gchar *config_dir = xmms_get_config_dir();
    gchar *config_file = g_build_filename(config_dir, "config", NULL);

    GKeyFile *kf = g_key_file_new();
    if (g_key_file_load_from_file(kf, config_file, 0, NULL)) {
        cfg.player_x = g_key_file_get_integer(kf, "xmms", "player_x", NULL);
        cfg.player_y = g_key_file_get_integer(kf, "xmms", "player_y", NULL);
        cfg.scale_factor = g_key_file_get_integer(kf, "xmms", "scale_factor", NULL);
        if (cfg.scale_factor < 1) cfg.scale_factor = 2;

        gchar *skin = g_key_file_get_string(kf, "xmms", "skin", NULL);
        if (skin && skin[0]) cfg.skin = skin;
        else g_free(skin);

        gchar *output = g_key_file_get_string(kf, "xmms", "output_device", NULL);
        if (output && output[0]) cfg.output_device = output;
        else g_free(output);

        if (g_key_file_has_key(kf, "xmms", "timer_mode", NULL))
            cfg.timer_mode = g_key_file_get_integer(kf, "xmms", "timer_mode", NULL);
        if (g_key_file_has_key(kf, "xmms", "volume", NULL))
            cfg.volume = CLAMP(g_key_file_get_integer(kf, "xmms", "volume", NULL), 0, 100);
        if (g_key_file_has_key(kf, "xmms", "balance", NULL))
            cfg.balance = CLAMP(g_key_file_get_integer(kf, "xmms", "balance", NULL), -100, 100);
        if (g_key_file_has_key(kf, "xmms", "no_playlist_advance", NULL))
            cfg.no_playlist_advance =
                g_key_file_get_boolean(kf, "xmms", "no_playlist_advance", NULL);
        if (g_key_file_has_key(kf, "xmms", "always_on_top", NULL))
            cfg.always_on_top =
                g_key_file_get_boolean(kf, "xmms", "always_on_top", NULL);
        if (g_key_file_has_key(kf, "xmms", "sticky", NULL))
            cfg.sticky =
                g_key_file_get_boolean(kf, "xmms", "sticky", NULL);
        if (g_key_file_has_key(kf, "xmms", "doublesize", NULL))
            cfg.doublesize =
                g_key_file_get_boolean(kf, "xmms", "doublesize", NULL);
        else
            cfg.doublesize = cfg.scale_factor > 1;
        if (g_key_file_has_key(kf, "xmms", "easy_move", NULL))
            cfg.easy_move =
                g_key_file_get_boolean(kf, "xmms", "easy_move", NULL);
        if (g_key_file_has_key(kf, "xmms", "playlist_visible", NULL))
            cfg.playlist_visible =
                g_key_file_get_boolean(kf, "xmms", "playlist_visible", NULL);
        if (g_key_file_has_key(kf, "xmms", "playlist_detached", NULL))
            cfg.playlist_detached =
                g_key_file_get_boolean(kf, "xmms", "playlist_detached", NULL);
        if (g_key_file_has_key(kf, "xmms", "shuffle", NULL))
            cfg.shuffle =
                g_key_file_get_boolean(kf, "xmms", "shuffle", NULL);
        if (g_key_file_has_key(kf, "xmms", "repeat", NULL))
            cfg.repeat =
                g_key_file_get_boolean(kf, "xmms", "repeat", NULL);
        if (g_key_file_has_key(kf, "xmms", "playlist_position", NULL))
            cfg.playlist_position =
                g_key_file_get_integer(kf, "xmms", "playlist_position", NULL);
        if (g_key_file_has_key(kf, "xmms", "equalizer_visible", NULL))
            cfg.equalizer_visible =
                g_key_file_get_boolean(kf, "xmms", "equalizer_visible", NULL);
        if (g_key_file_has_key(kf, "xmms", "equalizer_detached", NULL))
            cfg.equalizer_detached =
                g_key_file_get_boolean(kf, "xmms", "equalizer_detached", NULL);
        if (g_key_file_has_key(kf, "xmms", "equalizer_active", NULL))
            cfg.equalizer_active =
                g_key_file_get_boolean(kf, "xmms", "equalizer_active", NULL);
        if (g_key_file_has_key(kf, "xmms", "equalizer_auto", NULL))
            cfg.equalizer_auto =
                g_key_file_get_boolean(kf, "xmms", "equalizer_auto", NULL);
        if (g_key_file_has_key(kf, "xmms", "equalizer_preamp_pos", NULL))
            cfg.equalizer_preamp_pos = CLAMP(
                g_key_file_get_integer(kf, "xmms", "equalizer_preamp_pos", NULL),
                0, 100);
        for (gint i = 0; i < 10; i++) {
            gchar *key = g_strdup_printf("equalizer_band_%d_pos", i);
            if (g_key_file_has_key(kf, "xmms", key, NULL))
                cfg.equalizer_band_pos[i] = CLAMP(
                    g_key_file_get_integer(kf, "xmms", key, NULL), 0, 100);
            g_free(key);
        }

        session_debug("loaded config %s: player=(%d,%d) scale=%d playlist_visible=%d playlist_detached=%d equalizer_visible=%d equalizer_detached=%d",
                      config_file, cfg.player_x, cfg.player_y,
                      cfg.scale_factor, cfg.playlist_visible,
                      cfg.playlist_detached, cfg.equalizer_visible,
                      cfg.equalizer_detached);
    } else {
        session_debug("no config at %s; using defaults: player=(%d,%d) scale=%d",
                      config_file, cfg.player_x, cfg.player_y,
                      cfg.scale_factor);
    }
    cfg.scale_factor = cfg.doublesize ? 2 : 1;
    g_key_file_free(kf);
    g_free(config_file);
    g_free(config_dir);
}

void
save_config(void)
{
    gchar *config_dir = xmms_get_config_dir();
    g_mkdir_with_parents(config_dir, 0755);

    gchar *config_file = g_build_filename(config_dir, "config", NULL);

    GKeyFile *kf = g_key_file_new();
    g_key_file_set_integer(kf, "xmms", "player_x", cfg.player_x);
    g_key_file_set_integer(kf, "xmms", "player_y", cfg.player_y);
    g_key_file_set_integer(kf, "xmms", "scale_factor", cfg.scale_factor);
    g_key_file_set_integer(kf, "xmms", "timer_mode", cfg.timer_mode);
    g_key_file_set_integer(kf, "xmms", "volume", player_get_volume());
    g_key_file_set_integer(kf, "xmms", "balance", player_get_balance());
    g_key_file_set_boolean(kf, "xmms", "no_playlist_advance",
                           playlist_get_no_advance());
    g_key_file_set_boolean(kf, "xmms", "always_on_top",
                           cfg.always_on_top);
    g_key_file_set_boolean(kf, "xmms", "sticky", cfg.sticky);
    g_key_file_set_boolean(kf, "xmms", "doublesize", cfg.doublesize);
    g_key_file_set_boolean(kf, "xmms", "easy_move", cfg.easy_move);
    g_key_file_set_boolean(kf, "xmms", "playlist_visible",
                           playlistwin_is_visible());
    g_key_file_set_boolean(kf, "xmms", "playlist_detached",
                           cfg.playlist_detached);
    g_key_file_set_boolean(kf, "xmms", "shuffle", playlist_get_shuffle());
    g_key_file_set_boolean(kf, "xmms", "repeat", playlist_get_repeat());
    g_key_file_set_integer(kf, "xmms", "playlist_position",
                           playlist_get_position());
    g_key_file_set_boolean(kf, "xmms", "equalizer_visible",
                           equalizerwin_is_visible());
    g_key_file_set_boolean(kf, "xmms", "equalizer_detached",
                           cfg.equalizer_detached);
    gboolean eq_active, eq_auto;
    gint eq_preamp_pos, eq_band_pos[10];
    equalizerwin_get_state(&eq_active, &eq_auto, &eq_preamp_pos, eq_band_pos);
    g_key_file_set_boolean(kf, "xmms", "equalizer_active", eq_active);
    g_key_file_set_boolean(kf, "xmms", "equalizer_auto", eq_auto);
    g_key_file_set_integer(kf, "xmms", "equalizer_preamp_pos",
                           eq_preamp_pos);
    for (gint i = 0; i < 10; i++) {
        gchar *key = g_strdup_printf("equalizer_band_%d_pos", i);
        g_key_file_set_integer(kf, "xmms", key, eq_band_pos[i]);
        g_free(key);
    }
    if (cfg.skin)
        g_key_file_set_string(kf, "xmms", "skin", cfg.skin);

    const gchar *output_dev = player_get_output_device();
    if (output_dev)
        g_key_file_set_string(kf, "xmms", "output_device", output_dev);

    g_key_file_save_to_file(kf, config_file, NULL);
    gchar *playlist_file = playlist_state_file();
    playlist_save(playlist_file);
    session_debug("saved config %s: player=(%d,%d) scale=%d playlist_visible=%d playlist_detached=%d equalizer_visible=%d equalizer_detached=%d",
                  config_file, cfg.player_x, cfg.player_y,
                  cfg.scale_factor, playlistwin_is_visible(),
                  cfg.playlist_detached, equalizerwin_is_visible(),
                  cfg.equalizer_detached);
    g_key_file_free(kf);
    g_free(playlist_file);
    g_free(config_file);
    g_free(config_dir);
}

/* ---- File open callback (non-nested) ---- */

static void
open_files_cb(GObject *source, GAsyncResult *result, gpointer data)
{
    (void)data;
    GtkFileDialog *dlg = GTK_FILE_DIALOG(source);
    GListModel *files = gtk_file_dialog_open_multiple_finish(dlg, result, NULL);
    if (!files)
        return;

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
    playlist_play();
}

static void
open_directory_cb(GObject *source, GAsyncResult *result, gpointer data)
{
    (void)data;
    GtkFileDialog *dlg = GTK_FILE_DIALOG(source);
    GFile *folder = gtk_file_dialog_select_folder_finish(dlg, result, NULL);
    if (!folder)
        return;

    gchar *path = g_file_get_path(folder);
    if (path) {
        playlist_clear();
        playlist_add_dir(path);
        if (playlist_get_length() > 0)
            playlist_play();
        g_free(path);
    }
    g_object_unref(folder);
}

static gboolean
mainwin_save_state_cb(GtkApplicationWindow *window, GVariantDict *dict,
                      gpointer data)
{
    (void)window; (void)data;
    g_variant_dict_insert(dict, "window-kind", "s", "player");
    g_variant_dict_insert(dict, "playlist-visible", "b",
                          playlistwin_is_visible());
    g_variant_dict_insert(dict, "playlist-detached", "b",
                          cfg.playlist_detached);
    g_variant_dict_insert(dict, "equalizer-visible", "b",
                          equalizerwin_is_visible());
    g_variant_dict_insert(dict, "equalizer-detached", "b",
                          cfg.equalizer_detached);
    return FALSE;
}

/* ---- Drag and drop ---- */

static gboolean
mainwin_drop_cb(GtkDropTarget *target, const GValue *value,
                double x, double y, gpointer data)
{
    (void)target; (void)x; (void)y; (void)data;

    if (G_VALUE_HOLDS(value, GDK_TYPE_FILE_LIST)) {
        GSList *files = g_value_get_boxed(value);
        playlist_clear();
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
        playlist_play();
        return TRUE;
    }

    return FALSE;
}

/* ---- Application ---- */

static void
activate(GtkApplication *app, gpointer data)
{
    (void)data;

    if (app_initialized) {
        /* Already activated - just present the window */
        if (mainwin)
            gtk_window_present(GTK_WINDOW(mainwin));
        return;
    }
    app_initialized = TRUE;
    g_application_hold(G_APPLICATION(app));

    load_config();
    playlist_init();
    if (!startup_reset) {
        gchar *playlist_file = playlist_state_file();
        if (g_file_test(playlist_file, G_FILE_TEST_EXISTS))
            playlist_load(playlist_file);
        g_free(playlist_file);
    }
    playlist_set_position(cfg.playlist_position);
    playlist_set_shuffle(cfg.shuffle);
    playlist_set_repeat(cfg.repeat);
    playlist_set_no_advance(cfg.no_playlist_advance);
    skin_init();
    player_init();
    player_set_volume(cfg.volume);
    player_set_balance(cfg.balance);
    spotify_init();

    /* Apply saved output device */
    if (cfg.output_device)
        player_set_output_device(cfg.output_device);

    /* Load custom skin if configured */
    if (cfg.skin)
        skin_load(cfg.skin);

    /* Create main window */
    mainwin = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(mainwin), "XMMS Resuscitated");
    gtk_window_set_resizable(GTK_WINDOW(mainwin), FALSE);
    gtk_window_set_decorated(GTK_WINDOW(mainwin), FALSE);
    if (g_signal_lookup("save-state", GTK_TYPE_APPLICATION_WINDOW))
        g_signal_connect(mainwin, "save-state",
                         G_CALLBACK(mainwin_save_state_cb), NULL);

    /* Menu actions */
    static const GActionEntry win_actions[] = {
        { "preferences", mainwin_menu_prefs_cb, NULL, NULL, NULL },
        { "skin-browser", mainwin_menu_skin_cb, NULL, NULL, NULL },
        { "reload-skin", mainwin_menu_reload_skin_cb, NULL, NULL, NULL },
        { "repeat", mainwin_menu_repeat_cb, NULL, NULL, NULL },
        { "shuffle", mainwin_menu_shuffle_cb, NULL, NULL, NULL },
        { "no-advance", mainwin_menu_no_advance_cb, NULL, NULL, NULL },
        { "time-elapsed", mainwin_menu_time_elapsed_cb, NULL, NULL, NULL },
        { "time-remaining", mainwin_menu_time_remaining_cb, NULL, NULL, NULL },
        { "always-on-top", mainwin_menu_always_cb, NULL, NULL, NULL },
        { "sticky", mainwin_menu_sticky_cb, NULL, NULL, NULL },
        { "doublesize", mainwin_menu_doublesize_cb, NULL, NULL, NULL },
        { "easy-move", mainwin_menu_easy_move_cb, NULL, NULL, NULL },
        { "spotify", mainwin_menu_spotify_cb, NULL, NULL, NULL },
        { "output", mainwin_menu_output_cb, NULL, NULL, NULL },
        { "vis-analyzer", mainwin_menu_vis_analyzer_cb, NULL, NULL, NULL },
        { "vis-scope", mainwin_menu_vis_scope_cb, NULL, NULL, NULL },
        { "vis-off", mainwin_menu_vis_off_cb, NULL, NULL, NULL },
        { "vis-bars", mainwin_menu_vis_bars_cb, NULL, NULL, NULL },
        { "vis-lines", mainwin_menu_vis_lines_cb, NULL, NULL, NULL },
        { "vis-peaks", mainwin_menu_vis_peaks_cb, NULL, NULL, NULL },
        { "vis-falloff-slow", mainwin_menu_vis_falloff_slow_cb, NULL, NULL, NULL },
        { "vis-falloff-fast", mainwin_menu_vis_falloff_fast_cb, NULL, NULL, NULL },
        { "vis-refresh-full", mainwin_menu_vis_refresh_full_cb, NULL, NULL, NULL },
        { "vis-refresh-half", mainwin_menu_vis_refresh_half_cb, NULL, NULL, NULL },
        { "vis-refresh-quarter", mainwin_menu_vis_refresh_quarter_cb, NULL, NULL, NULL },
        { "windowshade", mainwin_menu_windowshade_cb, NULL, NULL, NULL },
        { "playlist-shade", mainwin_menu_playlist_shade_cb, NULL, NULL, NULL },
        { "equalizer-shade", mainwin_menu_equalizer_shade_cb, NULL, NULL, NULL },
    };
    g_action_map_add_action_entries(G_ACTION_MAP(mainwin), win_actions,
                                    G_N_ELEMENTS(win_actions), NULL);

    /* Drawing area */
    mainwin_drawing_area = gtk_drawing_area_new();
    gint scale = cfg.scale_factor;
    if (scale < 1) scale = 2;
    gtk_drawing_area_set_content_width(
        GTK_DRAWING_AREA(mainwin_drawing_area), MAINWIN_WIDTH * scale);
    gtk_drawing_area_set_content_height(
        GTK_DRAWING_AREA(mainwin_drawing_area), MAINWIN_HEIGHT * scale);
    gtk_drawing_area_set_draw_func(
        GTK_DRAWING_AREA(mainwin_drawing_area),
        mainwin_draw_func, NULL, NULL);

    /* Event controllers */
    GtkGesture *click = gtk_gesture_click_new();
    gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(click), 0);
    g_signal_connect(click, "pressed", G_CALLBACK(mainwin_click_pressed), NULL);
    g_signal_connect(click, "released", G_CALLBACK(mainwin_click_released), NULL);
    gtk_widget_add_controller(mainwin_drawing_area,
                              GTK_EVENT_CONTROLLER(click));

    GtkEventController *motion = gtk_event_controller_motion_new();
    g_signal_connect(motion, "motion", G_CALLBACK(mainwin_motion), NULL);
    gtk_widget_add_controller(mainwin_drawing_area, motion);

    GtkEventController *key = gtk_event_controller_key_new();
    g_signal_connect(key, "key-pressed", G_CALLBACK(mainwin_key_pressed), NULL);
    gtk_widget_add_controller(mainwin, key);

    /* Drag and drop */
    GtkDropTarget *drop = gtk_drop_target_new(GDK_TYPE_FILE_LIST,
                                               GDK_ACTION_COPY);
    g_signal_connect(drop, "drop", G_CALLBACK(mainwin_drop_cb), NULL);
    gtk_widget_add_controller(mainwin_drawing_area,
                              GTK_EVENT_CONTROLLER(drop));

    /* Create widgets */
    create_mainwin_widgets();
    hslider_set_position(mainwin_volume,
                         CLAMP((cfg.volume * 51 + 50) / 100, 0, 51));
    hslider_set_position(mainwin_balance,
                         CLAMP(12 + (cfg.balance * 12) / 100, 0, 24));
    tbutton_set_toggled(mainwin_shuffle, cfg.shuffle);
    tbutton_set_toggled(mainwin_repeat, cfg.repeat);

    /* Attach auxiliary panels below the player in the same toplevel. */
    equalizerwin_create(app);
    equalizerwin_set_state(cfg.equalizer_active, cfg.equalizer_auto,
                           cfg.equalizer_preamp_pos,
                           cfg.equalizer_band_pos);
    playlistwin_create(app);
    mainwin_container = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_widget_set_halign(mainwin_drawing_area, GTK_ALIGN_START);
    gtk_widget_set_halign(equalizerwin_get_widget(), GTK_ALIGN_START);
    gtk_widget_set_halign(playlistwin_get_widget(), GTK_ALIGN_START);
    gtk_box_append(GTK_BOX(mainwin_container), mainwin_drawing_area);
    gtk_box_append(GTK_BOX(mainwin_container), equalizerwin_get_widget());
    gtk_box_append(GTK_BOX(mainwin_container), playlistwin_get_widget());
    gtk_window_set_child(GTK_WINDOW(mainwin), mainwin_container);

    /* Create additional windows */

    /* Initialize MPRIS D-Bus interface */
    mpris_init();

    /* Set window position */
    /* Note: GTK4 on Wayland doesn't support setting window position */

    /* Update timer */
    update_timeout_tag = g_timeout_add(100, mainwin_update_cb, NULL);

    mainwin_set_always_on_top(cfg.always_on_top);
    mainwin_set_sticky(cfg.sticky);
    mainwin_set_easy_move(cfg.easy_move);
    gtk_window_present(GTK_WINDOW(mainwin));
    if (cfg.equalizer_visible)
        equalizerwin_show(TRUE);
    if (cfg.playlist_visible)
        playlistwin_show(TRUE);
    mainwin_update_panel_toggles();
}

static void
shutdown_cb(GtkApplication *app, gpointer data)
{
    (void)app; (void)data;

    session_debug("shutdown: saving fallback config");
    if (update_timeout_tag) {
        g_source_remove(update_timeout_tag);
        update_timeout_tag = 0;
    }

    playlistwin_shutdown();
    save_config();
    mpris_free();
    spotify_free();
    player_stop();
    player_free();
    playlist_free();
    skin_free();
}

static gint
handle_command_line(GApplication *app, GApplicationCommandLine *cmdline,
                    gpointer data)
{
    (void)data;

    gchar **argv;
    gint argc;
    argv = g_application_command_line_get_arguments(cmdline, &argc);
    GVariantDict *options =
        g_application_command_line_get_options_dict(cmdline);
    gboolean show_playlist = g_variant_dict_contains(options, "playlist");
    gboolean show_equalizer = g_variant_dict_contains(options, "equalizer");
    const gchar *playlist_menu = playlist_menu_option(options);
    gboolean files_added = FALSE;

    startup_reset = g_variant_dict_contains(options, "reset");
    g_application_activate(app);

    if (show_equalizer)
        equalizerwin_show(TRUE);
    if (show_playlist || playlist_menu)
        playlistwin_show(TRUE);
    if (playlist_menu)
        g_idle_add(open_playlist_menu_idle, g_strdup(playlist_menu));

    /* Add any files from command line */
    for (gint i = 1; i < argc; i++) {
        const gchar *arg = argv[i];
        if (g_strcmp0(arg, "--playlist") == 0 ||
            g_strcmp0(arg, "--equalizer") == 0 ||
            g_strcmp0(arg, "--reset") == 0 ||
            g_strcmp0(arg, "--playlist-menu-add") == 0 ||
            g_strcmp0(arg, "--playlist-menu-remove") == 0 ||
            g_strcmp0(arg, "--playlist-menu-select") == 0 ||
            g_strcmp0(arg, "--playlist-menu-misc") == 0 ||
            g_strcmp0(arg, "--playlist-menu-list") == 0)
            continue;
        if (arg[0] == '-')
            continue;

        if (g_file_test(arg, G_FILE_TEST_IS_DIR))
            playlist_add_dir(arg);
        else
            playlist_add(arg);
        files_added = TRUE;
    }

    if (files_added && playlist_get_length() > 0)
        playlist_play();

    g_strfreev(argv);
    return 0;
}

static void
session_state_save(GVariantDict *dict)
{
    cfg.playlist_visible = playlistwin_is_visible();
    cfg.equalizer_visible = equalizerwin_is_visible();
    g_variant_dict_insert(dict, "playlist-visible", "b", cfg.playlist_visible);
    g_variant_dict_insert(dict, "playlist-detached", "b", cfg.playlist_detached);
    g_variant_dict_insert(dict, "equalizer-visible", "b", cfg.equalizer_visible);
    g_variant_dict_insert(dict, "equalizer-detached", "b", cfg.equalizer_detached);
    session_debug("save-state: playlist_visible=%d playlist_detached=%d equalizer_visible=%d equalizer_detached=%d",
                  cfg.playlist_visible, cfg.playlist_detached,
                  cfg.equalizer_visible, cfg.equalizer_detached);
}

static void
session_state_restore(GVariant *state)
{
    gboolean playlist_visible;
    gboolean playlist_detached;
    gboolean equalizer_visible;
    gboolean equalizer_detached;

    if (!state)
        return;
    if (startup_reset)
        return;

    if (g_variant_lookup(state, "playlist-visible", "b", &playlist_visible))
        cfg.playlist_visible = playlist_visible;
    if (g_variant_lookup(state, "playlist-detached", "b", &playlist_detached))
        cfg.playlist_detached = playlist_detached;
    if (g_variant_lookup(state, "equalizer-visible", "b", &equalizer_visible))
        cfg.equalizer_visible = equalizer_visible;
    if (g_variant_lookup(state, "equalizer-detached", "b", &equalizer_detached))
        cfg.equalizer_detached = equalizer_detached;
    session_debug("restore-state: playlist_visible=%d playlist_detached=%d equalizer_visible=%d equalizer_detached=%d",
                  cfg.playlist_visible, cfg.playlist_detached,
                  cfg.equalizer_visible, cfg.equalizer_detached);
}

static void
query_end_cb(GtkApplication *app, gpointer data)
{
    (void)app; (void)data;
    session_debug("query-end: saving fallback config");
    save_config();
}

static gboolean
save_state_cb(GtkApplication *app, GVariantDict *dict, gpointer data)
{
    (void)app; (void)data;
    session_debug("application save-state signal");
    session_state_save(dict);
    save_config();
    return FALSE;
}

static gboolean
restore_state_cb(GtkApplication *app, gint reason, GVariant *state,
                 gpointer data)
{
    (void)app; (void)reason; (void)data;
    session_debug("application restore-state signal: reason=%d", reason);
    session_state_restore(state);
    return FALSE;
}

static void
restore_window_cb(GtkApplication *app, gint reason, GVariant *state,
                  gpointer data)
{
    (void)reason; (void)data;
    const gchar *window_kind = NULL;

    session_debug("application restore-window signal: reason=%d", reason);
    if (!startup_reset)
        session_state_restore(state);
    if (!app_initialized)
        activate(app, NULL);

    if (state && !startup_reset)
        g_variant_lookup(state, "window-kind", "&s", &window_kind);
    session_debug("restore-window: window-kind=%s playlist_visible=%d",
                  window_kind ? window_kind : "(none)", cfg.playlist_visible);

    if (cfg.equalizer_visible)
        equalizerwin_show(TRUE);
    if (g_strcmp0(window_kind, "playlist") == 0 || cfg.playlist_visible)
        playlistwin_show(TRUE);
    else if (mainwin)
        gtk_window_present(GTK_WINDOW(mainwin));
}

static void
setup_session_management(GtkApplication *app)
{
    GObjectClass *klass = G_OBJECT_GET_CLASS(app);
    gboolean has_register_session =
        g_object_class_find_property(klass, "register-session") != NULL;
    gboolean has_support_save =
        g_object_class_find_property(klass, "support-save") != NULL;
    gboolean has_query_end =
        g_signal_lookup("query-end", GTK_TYPE_APPLICATION) != 0;
    gboolean has_save_state =
        g_signal_lookup("save-state", GTK_TYPE_APPLICATION) != 0;
    gboolean has_restore_state =
        g_signal_lookup("restore-state", GTK_TYPE_APPLICATION) != 0;
    gboolean has_restore_window =
        g_signal_lookup("restore-window", GTK_TYPE_APPLICATION) != 0;

    session_debug("gtk session support: register-session=%d support-save=%d query-end=%d save-state=%d restore-state=%d restore-window=%d",
                  has_register_session, has_support_save, has_query_end,
                  has_save_state, has_restore_state, has_restore_window);
    session_debug("window positions are restored only by a session manager; normal close/reopen cannot read moved GTK4/Wayland coordinates");

    if (has_register_session)
        g_object_set(app, "register-session", TRUE, NULL);
    if (has_support_save)
        g_object_set(app, "support-save", TRUE, NULL);

    if (has_query_end)
        g_signal_connect(app, "query-end", G_CALLBACK(query_end_cb), NULL);
    if (has_save_state)
        g_signal_connect(app, "save-state", G_CALLBACK(save_state_cb), NULL);
    if (has_restore_state)
        g_signal_connect(app, "restore-state", G_CALLBACK(restore_state_cb), NULL);
    if (has_restore_window)
        g_signal_connect(app, "restore-window", G_CALLBACK(restore_window_cb), NULL);
}

int
main(int argc, char *argv[])
{
    /* gst_init is called later in player_init via gst_is_initialized check */

    GApplicationFlags flags = G_APPLICATION_HANDLES_COMMAND_LINE;
    if (g_getenv("XMMS_NON_UNIQUE"))
        flags |= G_APPLICATION_NON_UNIQUE;
    GtkApplication *app = gtk_application_new("org.xmms.Resuscitated", flags);
    g_application_add_main_option_entries(G_APPLICATION(app),
                                          app_option_entries);
    (void)app_option_entries;
    setup_session_management(app);

    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    g_signal_connect(app, "shutdown", G_CALLBACK(shutdown_cb), NULL);
    g_signal_connect(app, "command-line", G_CALLBACK(handle_command_line), NULL);

    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
