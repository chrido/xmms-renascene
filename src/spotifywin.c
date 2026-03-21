#include "xmms.h"
#include "spotify.h"
#include "spotifywin.h"

static GtkWidget *spotifywin = NULL;
static GtkWidget *playlist_listbox = NULL;
static GtkWidget *track_listbox = NULL;
static GtkWidget *status_label = NULL;
static GtkWidget *stack = NULL;

static GList *current_playlists = NULL;
static GList *current_tracks = NULL;
static gchar *current_playlist_uri = NULL;

/* ---- Track list view ---- */

static void
show_playlists_page(void);

static void
load_tracks_into_xmms_playlist(void)
{
    if (!current_tracks)
        return;

    playlist_clear();

    for (GList *l = current_tracks; l; l = l->next) {
        SpotifyTrack *t = l->data;
        gchar *title = g_strdup_printf("%s - %s",
                                        t->artist ? t->artist : "Unknown",
                                        t->name ? t->name : "Unknown");
        /* Store Spotify URI as the "filename" and formatted title */
        playlist_add_spotify(t->uri, title, t->duration_ms);
        g_free(title);
    }

    playlistwin_update();

    if (spotifywin)
        gtk_window_destroy(GTK_WINDOW(spotifywin));
}

static void
on_tracks_received(GList *tracks, gpointer data)
{
    (void)data;

    spotify_track_list_free(current_tracks);
    current_tracks = tracks;

    /* Clear track listbox */
    GtkWidget *child;
    while ((child = gtk_widget_get_first_child(GTK_WIDGET(track_listbox))))
        gtk_list_box_remove(GTK_LIST_BOX(track_listbox), child);

    gint i = 1;
    for (GList *l = tracks; l; l = l->next, i++) {
        SpotifyTrack *t = l->data;
        gchar *dur = time_to_string(t->duration_ms);
        gchar *text = g_strdup_printf("%d. %s - %s  [%s]",
                                       i,
                                       t->artist ? t->artist : "Unknown",
                                       t->name ? t->name : "Unknown",
                                       dur);
        GtkWidget *label = gtk_label_new(text);
        gtk_label_set_xalign(GTK_LABEL(label), 0.0);
        gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
        gtk_list_box_append(GTK_LIST_BOX(track_listbox), label);
        g_free(text);
        g_free(dur);
    }

    gtk_label_set_text(GTK_LABEL(status_label),
                       g_strdup_printf("%d tracks", g_list_length(tracks)));
    gtk_stack_set_visible_child_name(GTK_STACK(stack), "tracks");
}

static void
on_playlist_selected(GtkListBox *listbox, GtkListBoxRow *row, gpointer data)
{
    (void)listbox; (void)data;

    if (!row)
        return;

    gint idx = gtk_list_box_row_get_index(row);
    GList *nth = g_list_nth(current_playlists, idx);
    if (!nth)
        return;

    SpotifyPlaylist *pl = nth->data;

    g_free(current_playlist_uri);
    current_playlist_uri = g_strdup(pl->uri);

    gtk_label_set_text(GTK_LABEL(status_label), "Loading tracks...");

    spotify_get_playlist_tracks(pl->id, on_tracks_received, NULL);
}

/* ---- Playlist list view ---- */

static void
on_playlists_received(GList *playlists, gpointer data)
{
    (void)data;

    spotify_playlist_list_free(current_playlists);
    current_playlists = playlists;

    /* Clear playlist listbox */
    GtkWidget *child;
    while ((child = gtk_widget_get_first_child(GTK_WIDGET(playlist_listbox))))
        gtk_list_box_remove(GTK_LIST_BOX(playlist_listbox), child);

    for (GList *l = playlists; l; l = l->next) {
        SpotifyPlaylist *pl = l->data;
        gchar *text = g_strdup_printf("%s  (%d tracks)",
                                       pl->name, pl->total_tracks);
        GtkWidget *label = gtk_label_new(text);
        gtk_label_set_xalign(GTK_LABEL(label), 0.0);
        gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
        gtk_list_box_append(GTK_LIST_BOX(playlist_listbox), label);
        g_free(text);
    }

    gtk_label_set_text(GTK_LABEL(status_label),
                       g_strdup_printf("%d playlists", g_list_length(playlists)));
    gtk_stack_set_visible_child_name(GTK_STACK(stack), "playlists");
}

static void
show_playlists_page(void)
{
    gtk_stack_set_visible_child_name(GTK_STACK(stack), "playlists");
}

/* ---- Window ---- */

static void
on_back_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    show_playlists_page();
}

static void
on_load_clicked(GtkButton *button, gpointer data)
{
    (void)button; (void)data;
    load_tracks_into_xmms_playlist();
}

static void
on_spotifywin_destroy(GtkWidget *widget, gpointer data)
{
    (void)widget; (void)data;
    spotifywin = NULL;
    playlist_listbox = NULL;
    track_listbox = NULL;
    status_label = NULL;
    stack = NULL;
}

void
spotifywin_show(GtkWindow *parent)
{
    if (spotifywin) {
        gtk_window_present(GTK_WINDOW(spotifywin));
        return;
    }

    /* Check authentication */
    if (!spotify_is_authenticated()) {
        spotify_authenticate(parent);
        if (!spotify_is_authenticated())
            return;
    }

    spotifywin = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(spotifywin), "Spotify Playlists");
    gtk_window_set_default_size(GTK_WINDOW(spotifywin), 450, 500);
    gtk_window_set_transient_for(GTK_WINDOW(spotifywin), parent);
    g_signal_connect(spotifywin, "destroy",
                     G_CALLBACK(on_spotifywin_destroy), NULL);

    GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
    gtk_widget_set_margin_start(vbox, 10);
    gtk_widget_set_margin_end(vbox, 10);
    gtk_widget_set_margin_top(vbox, 10);
    gtk_widget_set_margin_bottom(vbox, 10);
    gtk_window_set_child(GTK_WINDOW(spotifywin), vbox);

    /* Status label */
    status_label = gtk_label_new("Loading playlists...");
    gtk_label_set_xalign(GTK_LABEL(status_label), 0.0);
    gtk_box_append(GTK_BOX(vbox), status_label);

    /* Stack for playlists / tracks views */
    stack = gtk_stack_new();
    gtk_widget_set_vexpand(stack, TRUE);
    gtk_box_append(GTK_BOX(vbox), stack);

    /* Playlists page */
    GtkWidget *pl_scrolled = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(pl_scrolled),
                                   GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
    playlist_listbox = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(playlist_listbox),
                                    GTK_SELECTION_SINGLE);
    g_signal_connect(playlist_listbox, "row-activated",
                     G_CALLBACK(on_playlist_selected), NULL);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(pl_scrolled),
                                  playlist_listbox);
    gtk_stack_add_named(GTK_STACK(stack), pl_scrolled, "playlists");

    /* Tracks page */
    GtkWidget *tr_scrolled = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(tr_scrolled),
                                   GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
    track_listbox = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(track_listbox),
                                    GTK_SELECTION_NONE);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(tr_scrolled),
                                  track_listbox);
    gtk_stack_add_named(GTK_STACK(stack), tr_scrolled, "tracks");

    /* Button bar */
    GtkWidget *btn_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_box_append(GTK_BOX(vbox), btn_box);

    GtkWidget *back_btn = gtk_button_new_with_label("Back");
    g_signal_connect(back_btn, "clicked", G_CALLBACK(on_back_clicked), NULL);
    gtk_box_append(GTK_BOX(btn_box), back_btn);

    GtkWidget *load_btn = gtk_button_new_with_label("Load into Playlist");
    g_signal_connect(load_btn, "clicked", G_CALLBACK(on_load_clicked), NULL);
    gtk_box_append(GTK_BOX(btn_box), load_btn);

    GtkWidget *spacer = gtk_label_new("");
    gtk_widget_set_hexpand(spacer, TRUE);
    gtk_box_append(GTK_BOX(btn_box), spacer);

    GtkWidget *close_btn = gtk_button_new_with_label("Close");
    g_signal_connect_swapped(close_btn, "clicked",
                             G_CALLBACK(gtk_window_destroy), spotifywin);
    gtk_box_append(GTK_BOX(btn_box), close_btn);

    gtk_window_present(GTK_WINDOW(spotifywin));

    /* Fetch playlists */
    spotify_get_playlists(on_playlists_received, NULL);
}
