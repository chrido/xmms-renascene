#include "xmms.h"

Config cfg;

GtkWidget *mainwin = NULL;
GtkWidget *mainwin_drawing_area = NULL;

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

/* Forward declarations */
static void open_files_cb(GObject *source, GAsyncResult *result, gpointer data);

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

    g_object_unref(filters);
    g_object_unref(filter);
    g_object_unref(all_filter);
    g_object_unref(dialog);
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

static void mainwin_close_pushed(void) { exit(0); }
static void mainwin_minimize_pushed(void) { gtk_window_minimize(GTK_WINDOW(mainwin)); }
static void mainwin_shade_pushed(void) { /* TODO */ }
static void mainwin_menubtn_pushed(void) { skinwin_show(); }

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

void
draw_main_window(cairo_t *cr)
{
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
                    (double)height / PLAYER_HEIGHT);

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

    pressed_widget = widget_list_find(mainwin_wlist, sx, sy);

    if (pressed_widget && pressed_widget->button_press) {
        pressed_widget->button_press(pressed_widget, sx, sy, button);
    } else if (sy < 14) {
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
        if (player_get_vis_data(vis_data, 75))
            vis_set_data(mainwin_vis, vis_data, 75);

        /* Update playlist window */
        playlistwin_update();
    } else {
        playstatus_set_status(mainwin_playstatus, 0);

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
    textbox_set_text(mainwin_info, "XMMS 2.0");

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
    cfg.vis_type = VIS_ANALYZER;
    cfg.vis_refresh = 30;
    cfg.analyzer_falloff = 3;
    cfg.peaks_falloff = 1;
    cfg.analyzer_peaks = TRUE;
    cfg.smooth_title_scroll = TRUE;
    cfg.save_window_position = TRUE;
    cfg.shuffle = FALSE;
    cfg.repeat = FALSE;

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
    }
    g_key_file_free(kf);
    g_free(config_file);
    g_free(config_dir);
}

static void
save_config(void)
{
    gchar *config_dir = xmms_get_config_dir();
    g_mkdir_with_parents(config_dir, 0755);

    gchar *config_file = g_build_filename(config_dir, "config", NULL);

    GKeyFile *kf = g_key_file_new();
    g_key_file_set_integer(kf, "xmms", "player_x", cfg.player_x);
    g_key_file_set_integer(kf, "xmms", "player_y", cfg.player_y);
    g_key_file_set_integer(kf, "xmms", "scale_factor", cfg.scale_factor);
    if (cfg.skin)
        g_key_file_set_string(kf, "xmms", "skin", cfg.skin);

    g_key_file_save_to_file(kf, config_file, NULL);
    g_key_file_free(kf);
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
        gchar *path = g_file_get_path(file);
        if (path) {
            playlist_add(path);
            g_free(path);
        }
        g_object_unref(file);
    }
    g_object_unref(files);
    playlist_play();
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

    load_config();
    playlist_init();
    skin_init();
    player_init();

    /* Load custom skin if configured */
    if (cfg.skin)
        skin_load(cfg.skin);

    /* Create main window */
    mainwin = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(mainwin), "XMMS");
    gtk_window_set_resizable(GTK_WINDOW(mainwin), FALSE);
    gtk_window_set_decorated(GTK_WINDOW(mainwin), FALSE);

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

    gtk_window_set_child(GTK_WINDOW(mainwin), mainwin_drawing_area);

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

    /* Drag and drop */
    GtkDropTarget *drop = gtk_drop_target_new(GDK_TYPE_FILE_LIST,
                                               GDK_ACTION_COPY);
    g_signal_connect(drop, "drop", G_CALLBACK(mainwin_drop_cb), NULL);
    gtk_widget_add_controller(mainwin_drawing_area,
                              GTK_EVENT_CONTROLLER(drop));

    /* Create widgets */
    create_mainwin_widgets();

    /* Create playlist and equalizer windows */
    playlistwin_create(app);
    equalizerwin_create(app);

    /* Initialize MPRIS D-Bus interface */
    mpris_init();

    /* Set window position */
    /* Note: GTK4 on Wayland doesn't support setting window position */

    /* Update timer */
    update_timeout_tag = g_timeout_add(100, mainwin_update_cb, NULL);

    gtk_window_present(GTK_WINDOW(mainwin));
}

static void
shutdown_cb(GtkApplication *app, gpointer data)
{
    (void)app; (void)data;

    if (update_timeout_tag) {
        g_source_remove(update_timeout_tag);
        update_timeout_tag = 0;
    }

    save_config();
    mpris_free();
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

    g_application_activate(app);

    /* Add any files from command line */
    for (gint i = 1; i < argc; i++) {
        const gchar *arg = argv[i];
        if (arg[0] == '-')
            continue;

        if (g_file_test(arg, G_FILE_TEST_IS_DIR))
            playlist_add_dir(arg);
        else
            playlist_add(arg);
    }

    if (argc > 1 && playlist_get_length() > 0)
        playlist_play();

    g_strfreev(argv);
    return 0;
}

int
main(int argc, char *argv[])
{
    /* gst_init is called later in player_init via gst_is_initialized check */

    GtkApplication *app = gtk_application_new("org.xmms.XMMS",
                                               G_APPLICATION_HANDLES_COMMAND_LINE);

    g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
    g_signal_connect(app, "shutdown", G_CALLBACK(shutdown_cb), NULL);
    g_signal_connect(app, "command-line", G_CALLBACK(handle_command_line), NULL);

    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
