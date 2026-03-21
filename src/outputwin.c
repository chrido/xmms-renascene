#include "xmms.h"
#include "outputwin.h"

static GtkWidget *outputwin = NULL;
static GtkWidget *local_listbox = NULL;
static GtkWidget *network_listbox = NULL;
static GtkWidget *network_section = NULL;
static GtkWidget *spotify_listbox = NULL;
static GtkWidget *spotify_section = NULL;

static GList *system_devices = NULL;
static GList *spotify_devices = NULL;

/* Filtered sublists (do not free elements — owned by system_devices) */
static GList *local_devices = NULL;
static GList *network_devices = NULL;

static void
on_outputwin_destroy(GtkWidget *widget, gpointer data)
{
    (void)widget; (void)data;
    outputwin = NULL;
    local_listbox = NULL;
    network_listbox = NULL;
    network_section = NULL;
    spotify_listbox = NULL;
    spotify_section = NULL;
    output_device_list_free(system_devices);
    system_devices = NULL;
    spotify_device_list_free(spotify_devices);
    spotify_devices = NULL;
    g_list_free(local_devices);
    local_devices = NULL;
    g_list_free(network_devices);
    network_devices = NULL;
}

static void
listbox_clear(GtkWidget *listbox)
{
    GtkWidget *child;
    while ((child = gtk_widget_get_first_child(GTK_WIDGET(listbox))))
        gtk_list_box_remove(GTK_LIST_BOX(listbox), child);
}

static void
add_device_row(GtkWidget *listbox, const gchar *name, gboolean selected)
{
    gchar *text = g_strdup_printf("%s  %s",
                                   selected ? "\u2713" : "   ",
                                   name);
    GtkWidget *label = gtk_label_new(text);
    gtk_label_set_xalign(GTK_LABEL(label), 0.0);
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    gtk_list_box_append(GTK_LIST_BOX(listbox), label);
    g_free(text);
}

static void
populate_system_devices(void)
{
    listbox_clear(local_listbox);
    listbox_clear(network_listbox);

    output_device_list_free(system_devices);
    system_devices = player_get_output_devices();

    g_list_free(local_devices);
    local_devices = NULL;
    g_list_free(network_devices);
    network_devices = NULL;

    for (GList *l = system_devices; l; l = l->next) {
        OutputDevice *dev = l->data;
        if (dev->is_network)
            network_devices = g_list_prepend(network_devices, dev);
        else
            local_devices = g_list_prepend(local_devices, dev);
    }
    local_devices = g_list_reverse(local_devices);
    network_devices = g_list_reverse(network_devices);

    const gchar *current = player_get_output_device();

    /* "Automatic" entry in local section */
    GtkWidget *auto_label = gtk_label_new(
        current == NULL ? "\u2713  Automatic (System Default)"
                        : "    Automatic (System Default)");
    gtk_label_set_xalign(GTK_LABEL(auto_label), 0.0);
    gtk_list_box_append(GTK_LIST_BOX(local_listbox), auto_label);

    for (GList *l = local_devices; l; l = l->next) {
        OutputDevice *dev = l->data;
        gboolean selected = current && g_strcmp0(current, dev->id) == 0;
        add_device_row(local_listbox, dev->display_name, selected);
    }

    for (GList *l = network_devices; l; l = l->next) {
        OutputDevice *dev = l->data;
        gboolean selected = current && g_strcmp0(current, dev->id) == 0;
        add_device_row(network_listbox, dev->display_name, selected);
    }

    /* Hide network section if no network devices */
    gtk_widget_set_visible(network_section, network_devices != NULL);
}

static void
populate_spotify_devices(void)
{
    if (!spotify_section)
        return;

    if (!spotify_is_authenticated()) {
        gtk_widget_set_visible(spotify_section, FALSE);
        return;
    }

    gtk_widget_set_visible(spotify_section, TRUE);

    listbox_clear(spotify_listbox);

    spotify_device_list_free(spotify_devices);
    spotify_devices = spotify_get_devices();

    if (!spotify_devices) {
        GtkWidget *label = gtk_label_new("  No Spotify devices found. Open Spotify on a device.");
        gtk_label_set_xalign(GTK_LABEL(label), 0.0);
        gtk_widget_add_css_class(label, "dim-label");
        gtk_list_box_append(GTK_LIST_BOX(spotify_listbox), label);
        return;
    }

    for (GList *l = spotify_devices; l; l = l->next) {
        SpotifyDevice *dev = l->data;
        gchar *name = g_strdup_printf("%s  (%s)",
                                       dev->name,
                                       dev->type ? dev->type : "Unknown");
        add_device_row(spotify_listbox, name, dev->is_active);
        g_free(name);
    }
}

static void
refresh_all_devices(void)
{
    populate_system_devices();
    populate_spotify_devices();
}

static void
on_refresh_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    refresh_all_devices();
}

static void
on_local_device_selected(GtkListBox *listbox, GtkListBoxRow *row,
                          gpointer data)
{
    (void)listbox; (void)data;
    if (!row)
        return;

    gint idx = gtk_list_box_row_get_index(row);

    if (idx == 0) {
        player_set_output_device(NULL);
    } else {
        GList *nth = g_list_nth(local_devices, idx - 1);
        if (nth) {
            OutputDevice *dev = nth->data;
            player_set_output_device(dev->id);
        }
    }

    populate_system_devices();
}

static void
on_network_device_selected(GtkListBox *listbox, GtkListBoxRow *row,
                            gpointer data)
{
    (void)listbox; (void)data;
    if (!row)
        return;

    gint idx = gtk_list_box_row_get_index(row);
    GList *nth = g_list_nth(network_devices, idx);
    if (nth) {
        OutputDevice *dev = nth->data;
        player_set_output_device(dev->id);
    }

    populate_system_devices();
}

static void
on_spotify_device_selected(GtkListBox *listbox, GtkListBoxRow *row,
                            gpointer data)
{
    (void)listbox; (void)data;
    if (!row)
        return;

    gint idx = gtk_list_box_row_get_index(row);
    GList *nth = g_list_nth(spotify_devices, idx);
    if (!nth)
        return;

    SpotifyDevice *dev = nth->data;
    spotify_set_device(dev->id);

    populate_spotify_devices();
}

static GtkWidget *
create_section(GtkWidget *vbox, const gchar *title, GtkWidget **listbox_out,
               GCallback row_activated_cb)
{
    GtkWidget *section = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
    gtk_box_append(GTK_BOX(vbox), section);

    GtkWidget *header = gtk_label_new(NULL);
    gchar *markup = g_strdup_printf("<b>%s</b>", title);
    gtk_label_set_markup(GTK_LABEL(header), markup);
    g_free(markup);
    gtk_label_set_xalign(GTK_LABEL(header), 0.0);
    gtk_box_append(GTK_BOX(section), header);

    GtkWidget *scrolled = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled),
                                   GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
    gtk_widget_set_vexpand(scrolled, TRUE);

    *listbox_out = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(*listbox_out),
                                    GTK_SELECTION_SINGLE);
    g_signal_connect(*listbox_out, "row-activated", row_activated_cb, NULL);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled), *listbox_out);
    gtk_box_append(GTK_BOX(section), scrolled);

    return section;
}

void
outputwin_show(GtkWindow *parent)
{
    if (outputwin) {
        gtk_window_present(GTK_WINDOW(outputwin));
        return;
    }

    outputwin = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(outputwin), "Output Device");
    gtk_window_set_default_size(GTK_WINDOW(outputwin), 400, 500);
    gtk_window_set_transient_for(GTK_WINDOW(outputwin), parent);
    g_signal_connect(outputwin, "destroy",
                     G_CALLBACK(on_outputwin_destroy), NULL);

    GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 8);
    gtk_widget_set_margin_start(vbox, 10);
    gtk_widget_set_margin_end(vbox, 10);
    gtk_widget_set_margin_top(vbox, 10);
    gtk_widget_set_margin_bottom(vbox, 10);
    gtk_window_set_child(GTK_WINDOW(outputwin), vbox);

    /* Local section */
    create_section(vbox, "Local", &local_listbox,
                   G_CALLBACK(on_local_device_selected));

    /* Network section */
    network_section = create_section(vbox, "Network Speakers", &network_listbox,
                                     G_CALLBACK(on_network_device_selected));

    /* Separator before Spotify */
    GtkWidget *sep = gtk_separator_new(GTK_ORIENTATION_HORIZONTAL);
    gtk_box_append(GTK_BOX(vbox), sep);

    /* Spotify Connect section */
    spotify_section = create_section(vbox, "Spotify Connect", &spotify_listbox,
                                     G_CALLBACK(on_spotify_device_selected));

    /* Button bar */
    GtkWidget *btn_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_box_append(GTK_BOX(vbox), btn_box);

    GtkWidget *refresh_btn = gtk_button_new_with_label("Refresh");
    g_signal_connect(refresh_btn, "clicked",
                     G_CALLBACK(on_refresh_clicked), NULL);
    gtk_box_append(GTK_BOX(btn_box), refresh_btn);

    GtkWidget *spacer = gtk_label_new("");
    gtk_widget_set_hexpand(spacer, TRUE);
    gtk_box_append(GTK_BOX(btn_box), spacer);

    GtkWidget *close_btn = gtk_button_new_with_label("Close");
    g_signal_connect_swapped(close_btn, "clicked",
                             G_CALLBACK(gtk_window_destroy), outputwin);
    gtk_box_append(GTK_BOX(btn_box), close_btn);

    gtk_window_present(GTK_WINDOW(outputwin));

    refresh_all_devices();
}
