#include "xmms.h"

static GtkWidget *prefswin = NULL;
static GtkWidget *prefs_notebook = NULL;
static GtkWidget *output_combo = NULL;
static GtkWidget *volume_spin = NULL;
static GtkWidget *balance_spin = NULL;
static GtkWidget *repeat_check = NULL;
static GtkWidget *shuffle_check = NULL;
static GtkWidget *no_advance_check = NULL;
static GtkWidget *timer_remaining_check = NULL;
static GtkWidget *sticky_check = NULL;
static GtkWidget *doublesize_check = NULL;
static GtkWidget *easy_move_check = NULL;
static GtkWidget *playlist_visible_check = NULL;
static GtkWidget *playlist_detached_check = NULL;
static GtkWidget *equalizer_visible_check = NULL;
static GtkWidget *equalizer_detached_check = NULL;
static GtkWidget *convert_underscore_check = NULL;
static GtkWidget *convert_twenty_check = NULL;
static GtkWidget *show_numbers_check = NULL;
static GtkWidget *playlist_font_entry = NULL;
static GtkWidget *mainwin_font_entry = NULL;
static GtkWidget *title_format_entry = NULL;
static GtkWidget *vis_mode_combo = NULL;
static GtkWidget *vis_analyzer_mode_combo = NULL;
static GtkWidget *vis_style_combo = NULL;
static GtkWidget *vis_scope_mode_combo = NULL;
static GtkWidget *vis_peaks_check = NULL;
static GtkWidget *vis_falloff_combo = NULL;
static GtkWidget *vis_peaks_falloff_combo = NULL;
static GtkWidget *vis_vu_mode_combo = NULL;
static GtkWidget *vis_refresh_combo = NULL;
static gboolean prefs_loading_controls = FALSE;

static GtkWidget *
label_new_left(const gchar *text)
{
    GtkWidget *label = gtk_label_new(text);
    gtk_label_set_xalign(GTK_LABEL(label), 0.0);
    return label;
}

static GtkWidget *
frame_box_new(const gchar *title, GtkWidget *parent)
{
    GtkWidget *frame = gtk_frame_new(title);
    gtk_widget_set_margin_top(frame, 6);
    gtk_widget_set_margin_bottom(frame, 6);
    gtk_widget_set_margin_start(frame, 6);
    gtk_widget_set_margin_end(frame, 6);
    gtk_box_append(GTK_BOX(parent), frame);

    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
    gtk_widget_set_margin_top(box, 8);
    gtk_widget_set_margin_bottom(box, 8);
    gtk_widget_set_margin_start(box, 8);
    gtk_widget_set_margin_end(box, 8);
    gtk_frame_set_child(GTK_FRAME(frame), box);
    return box;
}

static GtkWidget *
grid_new(void)
{
    GtkWidget *grid = gtk_grid_new();
    gtk_grid_set_row_spacing(GTK_GRID(grid), 6);
    gtk_grid_set_column_spacing(GTK_GRID(grid), 12);
    return grid;
}

static void
grid_attach_label(GtkWidget *grid, const gchar *label, GtkWidget *child,
                  gint row)
{
    gtk_grid_attach(GTK_GRID(grid), label_new_left(label), 0, row, 1, 1);
    gtk_grid_attach(GTK_GRID(grid), child, 1, row, 1, 1);
    gtk_widget_set_hexpand(child, TRUE);
}

static GtkWidget *
check_new(const gchar *label)
{
    GtkWidget *check = gtk_check_button_new_with_label(label);
    gtk_widget_set_halign(check, GTK_ALIGN_START);
    return check;
}

static void
combo_set_active_id(GtkWidget *combo, const gchar *id)
{
    if (!gtk_combo_box_set_active_id(GTK_COMBO_BOX(combo), id))
        gtk_combo_box_set_active(GTK_COMBO_BOX(combo), 0);
}

static const gchar *
falloff_id(gint speed)
{
    switch (speed) {
    case VIS_FALLOFF_SLOWEST:
        return "slowest";
    case VIS_FALLOFF_SLOW:
        return "slow";
    case VIS_FALLOFF_FAST:
        return "fast";
    case VIS_FALLOFF_FASTEST:
        return "fastest";
    case VIS_FALLOFF_MEDIUM:
    default:
        return "medium";
    }
}

static gint
falloff_from_id(const gchar *id)
{
    if (g_strcmp0(id, "slowest") == 0)
        return VIS_FALLOFF_SLOWEST;
    if (g_strcmp0(id, "slow") == 0)
        return VIS_FALLOFF_SLOW;
    if (g_strcmp0(id, "fast") == 0)
        return VIS_FALLOFF_FAST;
    if (g_strcmp0(id, "fastest") == 0)
        return VIS_FALLOFF_FASTEST;
    return VIS_FALLOFF_MEDIUM;
}

static void
update_visualization_control_sensitivity(void)
{
    const gchar *mode = gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_mode_combo));
    gboolean analyzer = g_strcmp0(mode, "analyzer") == 0;
    gboolean scope = g_strcmp0(mode, "scope") == 0;
    gboolean enabled = g_strcmp0(mode, "off") != 0;
    gboolean peaks = gtk_check_button_get_active(GTK_CHECK_BUTTON(vis_peaks_check));

    gtk_widget_set_sensitive(vis_analyzer_mode_combo, analyzer);
    gtk_widget_set_sensitive(vis_style_combo, analyzer);
    gtk_widget_set_sensitive(vis_peaks_check, analyzer);
    gtk_widget_set_sensitive(vis_falloff_combo, analyzer);
    gtk_widget_set_sensitive(vis_peaks_falloff_combo, analyzer && peaks);
    gtk_widget_set_sensitive(vis_scope_mode_combo, scope);
    gtk_widget_set_sensitive(vis_vu_mode_combo, analyzer);
    gtk_widget_set_sensitive(vis_refresh_combo, enabled);
}

static void
populate_output_combo(void)
{
    gtk_combo_box_text_remove_all(GTK_COMBO_BOX_TEXT(output_combo));
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(output_combo), "auto",
                              "Automatic (System Default)");

    GList *devices = player_get_output_devices();
    for (GList *l = devices; l; l = l->next) {
        OutputDevice *dev = l->data;
        gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(output_combo), dev->id,
                                  dev->display_name);
    }

    const gchar *current = player_get_output_device();
    combo_set_active_id(output_combo, current ? current : "auto");
    output_device_list_free(devices);
}

static void
set_controls_from_config(void)
{
    prefs_loading_controls = TRUE;

    populate_output_combo();
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(volume_spin), player_get_volume());
    gtk_spin_button_set_value(GTK_SPIN_BUTTON(balance_spin), player_get_balance());

    gtk_check_button_set_active(GTK_CHECK_BUTTON(repeat_check),
                                playlist_get_repeat());
    gtk_check_button_set_active(GTK_CHECK_BUTTON(shuffle_check),
                                playlist_get_shuffle());
    gtk_check_button_set_active(GTK_CHECK_BUTTON(no_advance_check),
                                playlist_get_no_advance());
    gtk_check_button_set_active(GTK_CHECK_BUTTON(timer_remaining_check),
                                cfg.timer_mode == TIMER_REMAINING);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(sticky_check), cfg.sticky);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(doublesize_check),
                                cfg.doublesize);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(easy_move_check),
                                cfg.easy_move);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(playlist_visible_check),
                                playlistwin_is_visible());
    gtk_check_button_set_active(GTK_CHECK_BUTTON(playlist_detached_check),
                                cfg.playlist_detached);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(equalizer_visible_check),
                                equalizerwin_is_visible());
    gtk_check_button_set_active(GTK_CHECK_BUTTON(equalizer_detached_check),
                                cfg.equalizer_detached);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(convert_underscore_check),
                                cfg.convert_underscore);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(convert_twenty_check),
                                cfg.convert_twenty);
    gtk_check_button_set_active(GTK_CHECK_BUTTON(show_numbers_check),
                                cfg.show_numbers_in_pl);

    gtk_editable_set_text(GTK_EDITABLE(playlist_font_entry),
                          cfg.playlist_font ? cfg.playlist_font : "Helvetica");
    gtk_editable_set_text(GTK_EDITABLE(mainwin_font_entry),
                          cfg.mainwin_font ? cfg.mainwin_font : "Skin bitmap font");
    gtk_editable_set_text(GTK_EDITABLE(title_format_entry),
                          cfg.title_format ? cfg.title_format : "%p - %t");

    combo_set_active_id(vis_mode_combo,
                        cfg.vis_mode == VIS_MODE_SCOPE ? "scope" :
                        cfg.vis_mode == VIS_MODE_MILKDROP ? "milkdrop" :
                        cfg.vis_mode == VIS_MODE_OFF ? "off" : "analyzer");
    combo_set_active_id(vis_analyzer_mode_combo,
                        cfg.vis_analyzer_mode == VIS_ANALYZER_FIRE ? "fire" :
                        cfg.vis_analyzer_mode == VIS_ANALYZER_VLINES ?
                        "vlines" : "normal");
    combo_set_active_id(vis_style_combo,
                        cfg.vis_analyzer_style == VIS_ANALYZER_LINES ?
                        "lines" : "bars");
    combo_set_active_id(vis_scope_mode_combo,
                        cfg.vis_scope_mode == VIS_SCOPE_DOT ? "dot" :
                        cfg.vis_scope_mode == VIS_SCOPE_SOLID ?
                        "solid" : "line");
    gtk_check_button_set_active(GTK_CHECK_BUTTON(vis_peaks_check),
                                cfg.vis_peaks_enabled);
    combo_set_active_id(vis_falloff_combo, falloff_id(cfg.vis_analyzer_falloff));
    combo_set_active_id(vis_peaks_falloff_combo,
                        falloff_id(cfg.vis_peaks_falloff));
    combo_set_active_id(vis_vu_mode_combo,
                        cfg.vis_vu_mode == VIS_VU_SMOOTH ?
                        "smooth" : "normal");
    combo_set_active_id(vis_refresh_combo,
                        cfg.vis_refresh_divisor >= 8 ? "eighth" :
                        cfg.vis_refresh_divisor >= 4 ? "quarter" :
                        cfg.vis_refresh_divisor >= 2 ? "half" : "full");
    update_visualization_control_sensitivity();

    prefs_loading_controls = FALSE;
}

static void
apply_visualization_controls(void)
{
    const gchar *mode = gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_mode_combo));
    cfg.vis_mode = g_strcmp0(mode, "scope") == 0 ? VIS_MODE_SCOPE :
        g_strcmp0(mode, "milkdrop") == 0 ? VIS_MODE_MILKDROP :
        g_strcmp0(mode, "off") == 0 ? VIS_MODE_OFF : VIS_MODE_ANALYZER;
    const gchar *analyzer_mode =
        gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_analyzer_mode_combo));
    cfg.vis_analyzer_mode = g_strcmp0(analyzer_mode, "fire") == 0 ?
        VIS_ANALYZER_FIRE : g_strcmp0(analyzer_mode, "vlines") == 0 ?
        VIS_ANALYZER_VLINES : VIS_ANALYZER_NORMAL;
    const gchar *style = gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_style_combo));
    cfg.vis_analyzer_style = g_strcmp0(style, "lines") == 0 ?
        VIS_ANALYZER_LINES : VIS_ANALYZER_BARS;
    const gchar *scope_mode =
        gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_scope_mode_combo));
    cfg.vis_scope_mode = g_strcmp0(scope_mode, "dot") == 0 ?
        VIS_SCOPE_DOT : g_strcmp0(scope_mode, "solid") == 0 ?
        VIS_SCOPE_SOLID : VIS_SCOPE_LINE;
    cfg.vis_peaks_enabled =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(vis_peaks_check));
    const gchar *falloff =
        gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_falloff_combo));
    cfg.vis_analyzer_falloff = falloff_from_id(falloff);
    const gchar *peaks_falloff =
        gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_peaks_falloff_combo));
    cfg.vis_peaks_falloff = falloff_from_id(peaks_falloff);
    const gchar *vu_mode =
        gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_vu_mode_combo));
    cfg.vis_vu_mode = g_strcmp0(vu_mode, "smooth") == 0 ?
        VIS_VU_SMOOTH : VIS_VU_NORMAL;
    const gchar *refresh =
        gtk_combo_box_get_active_id(GTK_COMBO_BOX(vis_refresh_combo));
    cfg.vis_refresh_divisor = g_strcmp0(refresh, "eighth") == 0 ? 8 :
        g_strcmp0(refresh, "quarter") == 0 ? 4 :
        g_strcmp0(refresh, "half") == 0 ? 2 : 1;
}

static void
apply_preferences(void)
{
    const gchar *output_id =
        gtk_combo_box_get_active_id(GTK_COMBO_BOX(output_combo));
    const gchar *new_output_id =
        g_strcmp0(output_id, "auto") == 0 ? NULL : output_id;
    if (g_strcmp0(player_get_output_device(), new_output_id) != 0)
        player_set_output_device(new_output_id);

    cfg.volume = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(volume_spin));
    cfg.balance = gtk_spin_button_get_value_as_int(GTK_SPIN_BUTTON(balance_spin));
    cfg.repeat = gtk_check_button_get_active(GTK_CHECK_BUTTON(repeat_check));
    cfg.shuffle = gtk_check_button_get_active(GTK_CHECK_BUTTON(shuffle_check));
    cfg.no_playlist_advance =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(no_advance_check));
    cfg.timer_mode = gtk_check_button_get_active(GTK_CHECK_BUTTON(timer_remaining_check)) ?
        TIMER_REMAINING : TIMER_ELAPSED;
    cfg.sticky = gtk_check_button_get_active(GTK_CHECK_BUTTON(sticky_check));
    cfg.doublesize =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(doublesize_check));
    cfg.easy_move =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(easy_move_check));
    cfg.playlist_visible =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(playlist_visible_check));
    cfg.playlist_detached =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(playlist_detached_check));
    cfg.equalizer_visible =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(equalizer_visible_check));
    cfg.equalizer_detached =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(equalizer_detached_check));
    cfg.convert_underscore =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(convert_underscore_check));
    cfg.convert_twenty =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(convert_twenty_check));
    cfg.show_numbers_in_pl =
        gtk_check_button_get_active(GTK_CHECK_BUTTON(show_numbers_check));

    g_free(cfg.playlist_font);
    cfg.playlist_font = g_strdup(gtk_editable_get_text(
        GTK_EDITABLE(playlist_font_entry)));
    g_strstrip(cfg.playlist_font);
    if (!cfg.playlist_font[0]) {
        g_free(cfg.playlist_font);
        cfg.playlist_font = g_strdup("Helvetica");
    }

    g_free(cfg.mainwin_font);
    cfg.mainwin_font = g_strdup(gtk_editable_get_text(
        GTK_EDITABLE(mainwin_font_entry)));
    g_strstrip(cfg.mainwin_font);
    if (!cfg.mainwin_font[0]) {
        g_free(cfg.mainwin_font);
        cfg.mainwin_font = g_strdup("Skin bitmap font");
    }

    g_free(cfg.title_format);
    cfg.title_format = g_strdup(gtk_editable_get_text(
        GTK_EDITABLE(title_format_entry)));
    g_strstrip(cfg.title_format);
    if (!cfg.title_format[0]) {
        g_free(cfg.title_format);
        cfg.title_format = g_strdup("%p - %t");
    }

    apply_visualization_controls();

    playlistwin_set_detached(cfg.playlist_detached);
    equalizerwin_set_detached(cfg.equalizer_detached);
    playlistwin_show(cfg.playlist_visible);
    equalizerwin_show(cfg.equalizer_visible);
    mainwin_apply_preferences();
    save_config();
}

static void
apply_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    apply_preferences();
}

static void
visualization_control_changed(GtkWidget *widget, gpointer data)
{
    (void)widget; (void)data;
    if (prefs_loading_controls)
        return;

    update_visualization_control_sensitivity();
    apply_visualization_controls();
    mainwin_apply_visualization_preferences();
    save_config();
}

static void
ok_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    apply_preferences();
    gtk_window_destroy(GTK_WINDOW(prefswin));
}

static void
cancel_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    gtk_window_destroy(GTK_WINDOW(prefswin));
}

static void
output_config_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    outputwin_show(GTK_WINDOW(prefswin));
}

static void
skin_browser_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    skinwin_show();
}

static GtkWidget *
create_audio_page(void)
{
    GtkWidget *page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *input = frame_box_new("Input Plugins", page);
    gtk_box_append(GTK_BOX(input),
                   label_new_left("GStreamer input support (built in)"));
    gtk_box_append(GTK_BOX(input),
                   label_new_left("File, URI, and stream decoding are provided by installed GStreamer plugins."));

    GtkWidget *output = frame_box_new("Output Plugin", page);
    GtkWidget *grid = grid_new();
    gtk_box_append(GTK_BOX(output), grid);
    output_combo = gtk_combo_box_text_new();
    grid_attach_label(grid, "Output device:", output_combo, 0);
    GtkWidget *buttons = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    GtkWidget *configure = gtk_button_new_with_label("Configure");
    g_signal_connect(configure, "clicked", G_CALLBACK(output_config_clicked), NULL);
    gtk_box_append(GTK_BOX(buttons), configure);
    gtk_grid_attach(GTK_GRID(grid), buttons, 1, 1, 1, 1);
    return page;
}

static GtkWidget *
create_plugins_page(void)
{
    GtkWidget *page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *effects = frame_box_new("Effect Plugins", page);
    gtk_box_append(GTK_BOX(effects),
                   label_new_left("GStreamer equalizer (built in, controlled by the Equalizer window)"));

    GtkWidget *general = frame_box_new("General Plugins", page);
    gtk_box_append(GTK_BOX(general),
                   label_new_left("MPRIS desktop integration (built in)"));
    gtk_box_append(GTK_BOX(general),
                   label_new_left("Spotify support (built in, configured from the Spotify window)"));
    return page;
}

static GtkWidget *
create_visualization_page(void)
{
    GtkWidget *page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *box = frame_box_new("Visualization", page);
    GtkWidget *grid = grid_new();
    gtk_box_append(GTK_BOX(box), grid);
    gtk_box_append(GTK_BOX(box),
                   label_new_left("Controls that do not affect the selected visualization mode are disabled."));

    vis_mode_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_mode_combo), "analyzer", "Analyzer");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_mode_combo), "scope", "Scope");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_mode_combo), "milkdrop", "MilkDrop-inspired");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_mode_combo), "off", "Off");
    grid_attach_label(grid, "Visualization mode:", vis_mode_combo, 0);

    vis_analyzer_mode_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_analyzer_mode_combo), "normal", "Analyzer normal");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_analyzer_mode_combo), "fire", "Analyzer fire");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_analyzer_mode_combo), "vlines", "Analyzer vertical lines");
    grid_attach_label(grid, "Analyzer mode:", vis_analyzer_mode_combo, 1);

    vis_style_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_style_combo), "bars", "Analyzer bars");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_style_combo), "lines", "Analyzer lines");
    grid_attach_label(grid, "Analyzer style:", vis_style_combo, 2);

    vis_scope_mode_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_scope_mode_combo), "dot", "Dot scope");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_scope_mode_combo), "line", "Line scope");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_scope_mode_combo), "solid", "Solid scope");
    grid_attach_label(grid, "Scope mode:", vis_scope_mode_combo, 3);

    vis_peaks_check = check_new("Show analyzer peaks");
    gtk_grid_attach(GTK_GRID(grid), vis_peaks_check, 1, 4, 1, 1);

    vis_falloff_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_falloff_combo), "slowest", "Slowest");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_falloff_combo), "slow", "Slow");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_falloff_combo), "medium", "Medium");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_falloff_combo), "fast", "Fast");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_falloff_combo), "fastest", "Fastest");
    grid_attach_label(grid, "Analyzer falloff:", vis_falloff_combo, 5);

    vis_peaks_falloff_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_peaks_falloff_combo), "slowest", "Slowest");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_peaks_falloff_combo), "slow", "Slow");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_peaks_falloff_combo), "medium", "Medium");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_peaks_falloff_combo), "fast", "Fast");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_peaks_falloff_combo), "fastest", "Fastest");
    grid_attach_label(grid, "Peaks falloff:", vis_peaks_falloff_combo, 6);

    vis_vu_mode_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_vu_mode_combo), "normal", "Normal");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_vu_mode_combo), "smooth", "Smooth");
    grid_attach_label(grid, "WindowShade VU mode:", vis_vu_mode_combo, 7);

    vis_refresh_combo = gtk_combo_box_text_new();
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_refresh_combo), "full", "Full");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_refresh_combo), "half", "Half");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_refresh_combo), "quarter", "Quarter");
    gtk_combo_box_text_append(GTK_COMBO_BOX_TEXT(vis_refresh_combo), "eighth", "Eighth");
    grid_attach_label(grid, "Refresh rate:", vis_refresh_combo, 8);

    GtkWidget *vis_controls[] = {
        vis_mode_combo, vis_analyzer_mode_combo, vis_style_combo,
        vis_scope_mode_combo, vis_falloff_combo, vis_peaks_falloff_combo,
        vis_vu_mode_combo, vis_refresh_combo
    };
    for (guint i = 0; i < G_N_ELEMENTS(vis_controls); i++) {
        g_signal_connect(vis_controls[i], "changed",
                         G_CALLBACK(visualization_control_changed), NULL);
    }
    g_signal_connect(vis_peaks_check, "toggled",
                     G_CALLBACK(visualization_control_changed), NULL);

    return page;
}

static GtkWidget *
create_options_page(void)
{
    GtkWidget *page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *box = frame_box_new("Options", page);
    GtkWidget *grid = grid_new();
    gtk_box_append(GTK_BOX(box), grid);

    volume_spin = gtk_spin_button_new_with_range(0, 100, 1);
    balance_spin = gtk_spin_button_new_with_range(-100, 100, 1);
    grid_attach_label(grid, "Volume:", volume_spin, 0);
    grid_attach_label(grid, "Balance:", balance_spin, 1);

    repeat_check = check_new("Repeat");
    shuffle_check = check_new("Shuffle");
    no_advance_check = check_new("No playlist advance");
    timer_remaining_check = check_new("Time remaining");
    sticky_check = check_new("Sticky");
    doublesize_check = check_new("Double size");
    easy_move_check = check_new("Easy move");
    playlist_visible_check = check_new("Show playlist");
    playlist_detached_check = check_new("Detach playlist");
    equalizer_visible_check = check_new("Show equalizer");
    equalizer_detached_check = check_new("Detach equalizer");
    convert_twenty_check = check_new("Convert %20 to space");
    convert_underscore_check = check_new("Convert underscore to space");
    show_numbers_check = check_new("Show numbers in playlist");

    GtkWidget *checks[] = {
        repeat_check, shuffle_check, no_advance_check, timer_remaining_check,
        sticky_check, doublesize_check, easy_move_check,
        playlist_visible_check, playlist_detached_check,
        equalizer_visible_check, equalizer_detached_check,
        convert_twenty_check, convert_underscore_check, show_numbers_check
    };
    for (guint i = 0; i < G_N_ELEMENTS(checks); i++)
        gtk_grid_attach(GTK_GRID(grid), checks[i], i % 2, 2 + (i / 2), 1, 1);
    return page;
}

static GtkWidget *
create_fonts_page(void)
{
    GtkWidget *page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *playlist = frame_box_new("Playlist", page);
    GtkWidget *grid = grid_new();
    gtk_box_append(GTK_BOX(playlist), grid);
    playlist_font_entry = gtk_entry_new();
    grid_attach_label(grid, "Playlist font family:", playlist_font_entry, 0);
    gtk_box_append(GTK_BOX(playlist),
                   label_new_left("XMMS used a Helvetica bold 10px playlist font. This port keeps the original fixed row height, so only the family is configurable."));

    GtkWidget *main = frame_box_new("Main Window", page);
    mainwin_font_entry = gtk_entry_new();
    gtk_editable_set_editable(GTK_EDITABLE(mainwin_font_entry), FALSE);
    gtk_box_append(GTK_BOX(main), mainwin_font_entry);
    gtk_box_append(GTK_BOX(main),
                   label_new_left("The main window uses the skin bitmap font, matching XMMS skins."));
    GtkWidget *skin_button = gtk_button_new_with_label("Open Skin Browser");
    g_signal_connect(skin_button, "clicked", G_CALLBACK(skin_browser_clicked), NULL);
    gtk_box_append(GTK_BOX(main), skin_button);
    return page;
}

static GtkWidget *
create_title_page(void)
{
    GtkWidget *page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *box = frame_box_new("Title", page);
    GtkWidget *grid = grid_new();
    gtk_box_append(GTK_BOX(box), grid);
    title_format_entry = gtk_entry_new();
    grid_attach_label(grid, "Title format:", title_format_entry, 0);
    gtk_box_append(GTK_BOX(box),
                   label_new_left("Original XMMS tokens include %p artist, %a album, %g genre, %f filename, and %t title. The current decoder uses embedded titles when available and stores this format for compatibility."));
    return page;
}

static void
prefswin_destroyed(GtkWidget *widget, gpointer data)
{
    (void)widget; (void)data;
    prefswin = NULL;
    prefs_notebook = NULL;
}

static void
create_prefswin(GtkWindow *parent)
{
    prefswin = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(prefswin), "Preferences");
    gtk_window_set_default_size(GTK_WINDOW(prefswin), 560, 520);
    gtk_window_set_transient_for(GTK_WINDOW(prefswin), parent);
    gtk_window_set_destroy_with_parent(GTK_WINDOW(prefswin), TRUE);
    g_signal_connect(prefswin, "destroy", G_CALLBACK(prefswin_destroyed), NULL);

    GtkWidget *root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 10);
    gtk_widget_set_margin_top(root, 10);
    gtk_widget_set_margin_bottom(root, 10);
    gtk_widget_set_margin_start(root, 10);
    gtk_widget_set_margin_end(root, 10);
    gtk_window_set_child(GTK_WINDOW(prefswin), root);

    prefs_notebook = gtk_notebook_new();
    gtk_widget_set_vexpand(prefs_notebook, TRUE);
    gtk_box_append(GTK_BOX(root), prefs_notebook);
    gtk_notebook_append_page(GTK_NOTEBOOK(prefs_notebook), create_audio_page(),
                             gtk_label_new("Audio I/O Plugins"));
    gtk_notebook_append_page(GTK_NOTEBOOK(prefs_notebook), create_plugins_page(),
                             gtk_label_new("Effect/General Plugins"));
    gtk_notebook_append_page(GTK_NOTEBOOK(prefs_notebook),
                             create_visualization_page(),
                             gtk_label_new("Visualization Plugins"));
    gtk_notebook_append_page(GTK_NOTEBOOK(prefs_notebook), create_options_page(),
                             gtk_label_new("Options"));
    gtk_notebook_append_page(GTK_NOTEBOOK(prefs_notebook), create_fonts_page(),
                             gtk_label_new("Fonts"));
    gtk_notebook_append_page(GTK_NOTEBOOK(prefs_notebook), create_title_page(),
                             gtk_label_new("Title"));

    GtkWidget *buttons = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_widget_set_halign(buttons, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(root), buttons);
    GtkWidget *ok = gtk_button_new_with_label("Ok");
    GtkWidget *cancel = gtk_button_new_with_label("Cancel");
    GtkWidget *apply = gtk_button_new_with_label("Apply");
    g_signal_connect(ok, "clicked", G_CALLBACK(ok_clicked), NULL);
    g_signal_connect(cancel, "clicked", G_CALLBACK(cancel_clicked), NULL);
    g_signal_connect(apply, "clicked", G_CALLBACK(apply_clicked), NULL);
    gtk_box_append(GTK_BOX(buttons), ok);
    gtk_box_append(GTK_BOX(buttons), cancel);
    gtk_box_append(GTK_BOX(buttons), apply);
}

void
prefswin_show(GtkWindow *parent)
{
    if (!prefswin)
        create_prefswin(parent);
    set_controls_from_config();
    gtk_window_present(GTK_WINDOW(prefswin));
}

void
prefswin_show_visualization_page(GtkWindow *parent)
{
    prefswin_show(parent);
    gtk_notebook_set_current_page(GTK_NOTEBOOK(prefs_notebook), 2);
}
