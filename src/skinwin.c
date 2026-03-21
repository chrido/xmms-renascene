#include "xmms.h"
#include "skinwin.h"

typedef struct {
    gchar *name;
    gchar *path;
} SkinNode;

static GtkWidget *skinwin = NULL;
static GtkWidget *skin_listbox = NULL;
static GList *skinlist = NULL;

static void
skin_node_free(gpointer data)
{
    SkinNode *node = data;
    g_free(node->name);
    g_free(node->path);
    g_free(node);
}

static void
skinlist_clear(void)
{
    g_list_free_full(skinlist, skin_node_free);
    skinlist = NULL;
}

static void
add_skin(const gchar *filepath)
{
    SkinNode *node = g_new0(SkinNode, 1);
    node->path = g_strdup(filepath);

    /* Extract display name from filename, strip extension */
    gchar *base = g_path_get_basename(filepath);
    gchar *dot = strrchr(base, '.');
    if (dot) {
        const gchar *ext = dot;
        /* Strip known archive extensions */
        if (g_ascii_strcasecmp(ext, ".zip") == 0 ||
            g_ascii_strcasecmp(ext, ".wsz") == 0 ||
            g_ascii_strcasecmp(ext, ".tgz") == 0 ||
            g_ascii_strcasecmp(ext, ".gz") == 0 ||
            g_ascii_strcasecmp(ext, ".bz2") == 0) {
            *dot = '\0';
            /* Check for .tar prefix */
            dot = strrchr(base, '.');
            if (dot && g_ascii_strcasecmp(dot, ".tar") == 0)
                *dot = '\0';
        }
    }
    node->name = base;
    skinlist = g_list_prepend(skinlist, node);
}

static void
scan_skindir(const gchar *path)
{
    GDir *dir = g_dir_open(path, 0, NULL);
    if (!dir)
        return;

    const gchar *entry;
    while ((entry = g_dir_read_name(dir)) != NULL) {
        if (g_str_has_prefix(entry, "."))
            continue;

        gchar *filepath = g_build_filename(path, entry, NULL);

        if (g_file_test(filepath, G_FILE_TEST_IS_DIR)) {
            /* Directories are skin folders */
            add_skin(filepath);
        } else if (g_file_test(filepath, G_FILE_TEST_IS_REGULAR)) {
            /* Check for known skin archive extensions */
            const gchar *dot = strrchr(entry, '.');
            if (dot && (g_ascii_strcasecmp(dot, ".zip") == 0 ||
                        g_ascii_strcasecmp(dot, ".wsz") == 0 ||
                        g_ascii_strcasecmp(dot, ".tgz") == 0 ||
                        g_ascii_strcasecmp(dot, ".gz") == 0 ||
                        g_ascii_strcasecmp(dot, ".bz2") == 0)) {
                add_skin(filepath);
            } else {
                g_free(filepath);
            }
        } else {
            g_free(filepath);
        }
    }
    g_dir_close(dir);
}

static gint
skinlist_compare(gconstpointer a, gconstpointer b)
{
    return g_ascii_strcasecmp(((const SkinNode *)a)->name,
                              ((const SkinNode *)b)->name);
}

static void
scan_skins(void)
{
    skinlist_clear();

    /* User skin directory: ~/.config/xmms/Skins */
    gchar *user_dir = g_build_filename(g_get_user_config_dir(), "xmms", "Skins", NULL);
    scan_skindir(user_dir);
    g_free(user_dir);

    /* Legacy user skin directory: ~/.xmms/Skins */
    gchar *legacy_dir = g_build_filename(g_get_home_dir(), ".xmms", "Skins", NULL);
    scan_skindir(legacy_dir);
    g_free(legacy_dir);

    /* System skin directory */
    gchar *sys_dir = xmms_get_skin_dir();
    scan_skindir(sys_dir);
    g_free(sys_dir);

    /* SKINSDIR environment variable */
    const gchar *skinsdir = g_getenv("SKINSDIR");
    if (skinsdir) {
        gchar **dirs = g_strsplit(skinsdir, ":", -1);
        for (int i = 0; dirs[i]; i++)
            scan_skindir(dirs[i]);
        g_strfreev(dirs);
    }

    skinlist = g_list_sort(skinlist, skinlist_compare);
}

static void
on_skin_selected(GtkListBox *listbox, GtkListBoxRow *row, gpointer data)
{
    (void)listbox; (void)data;

    if (!row)
        return;

    gint idx = gtk_list_box_row_get_index(row);
    if (idx == 0) {
        /* "(Default)" selected — reload default skin */
        for (int i = 0; i < SKIN_PIXMAP_COUNT; i++) {
            if (skin->pixmaps[i].surface) {
                cairo_surface_destroy(skin->pixmaps[i].surface);
                skin->pixmaps[i].surface = NULL;
            }
        }
        g_free(skin->path);
        skin->path = NULL;
        skin->id++;
        g_free(cfg.skin);
        cfg.skin = NULL;
    } else {
        GList *nth = g_list_nth(skinlist, idx - 1);
        if (!nth)
            return;
        SkinNode *node = nth->data;
        skin_load(node->path);
        g_free(cfg.skin);
        cfg.skin = g_strdup(node->path);
    }

    save_config();

    /* Redraw all windows */
    mainwin_queue_draw();
    playlistwin_update();
    if (equalizerwin_is_visible())
        gtk_widget_queue_draw(equalizerwin);
}

static void
populate_listbox(void)
{
    /* Remove existing rows */
    GtkWidget *child;
    while ((child = gtk_widget_get_first_child(GTK_WIDGET(skin_listbox))))
        gtk_list_box_remove(GTK_LIST_BOX(skin_listbox), child);

    /* Add default entry */
    GtkWidget *label = gtk_label_new("(Default)");
    gtk_label_set_xalign(GTK_LABEL(label), 0.0);
    gtk_list_box_append(GTK_LIST_BOX(skin_listbox), label);

    /* Add skins */
    gint select_idx = 0;
    gint i = 1;
    for (GList *l = skinlist; l; l = l->next, i++) {
        SkinNode *node = l->data;
        label = gtk_label_new(node->name);
        gtk_label_set_xalign(GTK_LABEL(label), 0.0);
        gtk_list_box_append(GTK_LIST_BOX(skin_listbox), label);

        if (skin->path && g_strcmp0(node->path, skin->path) == 0)
            select_idx = i;
    }

    /* Select current skin */
    GtkListBoxRow *row = gtk_list_box_get_row_at_index(
        GTK_LIST_BOX(skin_listbox), select_idx);
    if (row)
        gtk_list_box_select_row(GTK_LIST_BOX(skin_listbox), row);
}

void
skinwin_create(GtkWindow *parent)
{
    if (skinwin) {
        gtk_window_present(GTK_WINDOW(skinwin));
        return;
    }

    skinwin = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(skinwin), "Skin Selector");
    gtk_window_set_default_size(GTK_WINDOW(skinwin), 300, 400);
    gtk_window_set_transient_for(GTK_WINDOW(skinwin), parent);
    gtk_window_set_destroy_with_parent(GTK_WINDOW(skinwin), TRUE);
    g_signal_connect(skinwin, "destroy",
                     G_CALLBACK(gtk_widget_unparent), NULL);
    g_signal_connect_swapped(skinwin, "destroy",
                             G_CALLBACK(g_nullify_pointer), &skinwin);

    GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 6);
    gtk_widget_set_margin_start(vbox, 10);
    gtk_widget_set_margin_end(vbox, 10);
    gtk_widget_set_margin_top(vbox, 10);
    gtk_widget_set_margin_bottom(vbox, 10);
    gtk_window_set_child(GTK_WINDOW(skinwin), vbox);

    /* Scrolled list */
    GtkWidget *scrolled = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scrolled),
                                   GTK_POLICY_AUTOMATIC, GTK_POLICY_AUTOMATIC);
    gtk_widget_set_vexpand(scrolled, TRUE);
    gtk_box_append(GTK_BOX(vbox), scrolled);

    skin_listbox = gtk_list_box_new();
    gtk_list_box_set_selection_mode(GTK_LIST_BOX(skin_listbox),
                                    GTK_SELECTION_SINGLE);
    g_signal_connect(skin_listbox, "row-selected",
                     G_CALLBACK(on_skin_selected), NULL);
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scrolled), skin_listbox);

    /* Close button */
    GtkWidget *btn_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    gtk_widget_set_halign(btn_box, GTK_ALIGN_END);
    gtk_box_append(GTK_BOX(vbox), btn_box);

    GtkWidget *close_btn = gtk_button_new_with_label("Close");
    g_signal_connect_swapped(close_btn, "clicked",
                             G_CALLBACK(gtk_window_destroy),
                             skinwin);
    gtk_box_append(GTK_BOX(btn_box), close_btn);
}

void
skinwin_show(void)
{
    if (!skinwin)
        skinwin_create(GTK_WINDOW(mainwin));

    scan_skins();
    populate_listbox();
    gtk_window_present(GTK_WINDOW(skinwin));
}
