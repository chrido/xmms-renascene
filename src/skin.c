#include "xmms.h"

#ifdef HAVE_LIBARCHIVE
#include <archive.h>
#include <archive_entry.h>
#endif

Skin *skin = NULL;

static const guchar default_viscolor[24][3] = {
    {9,34,53}, {10,18,26}, {0,54,108}, {0,58,116},
    {0,62,124}, {0,66,132}, {0,70,140}, {0,74,148},
    {0,78,156}, {0,82,164}, {0,86,172}, {0,92,184},
    {0,98,196}, {0,104,208}, {0,110,220}, {0,116,232},
    {0,122,244}, {0,128,255}, {0,128,255}, {0,104,208},
    {0,80,160}, {0,56,112}, {0,32,64}, {200,200,200}
};

static const struct {
    const gchar *name;
    gint width, height;
} skin_pixmap_info[SKIN_PIXMAP_COUNT] = {
    [SKIN_MAIN]       = { "main",     275, 116 },
    [SKIN_CBUTTONS]   = { "cbuttons", 136,  36 },
    [SKIN_TITLEBAR]   = { "titlebar", 275, 116 },
    [SKIN_SHUFREP]    = { "shufrep",  28,   60 },
    [SKIN_TEXT]        = { "text",     155,  18 },
    [SKIN_VOLUME]     = { "volume",   68,  421 },
    [SKIN_BALANCE]    = { "balance",  38,  421 },
    [SKIN_MONOSTEREO] = { "monoster", 56,   12 },
    [SKIN_PLAYPAUSE]  = { "playpaus", 11,    9 },
    [SKIN_NUMBERS]    = { "nums_ex",  108,  13 },
    [SKIN_POSBAR]     = { "posbar",   248,  10 },
    [SKIN_PLEDIT]     = { "pledit",   150,  18 },
    [SKIN_EQMAIN]     = { "eqmain",   275, 116 },
    [SKIN_EQ_EX]      = { "eq_ex",    275,  50 },
};

static cairo_surface_t *
surface_from_resource(const gchar *resource_path)
{
    GError *error = NULL;
    GBytes *bytes = g_resources_lookup_data(resource_path, 0, &error);
    if (!bytes) {
        g_warning("Failed to load resource %s: %s", resource_path,
                  error ? error->message : "unknown");
        g_clear_error(&error);
        return NULL;
    }

    GInputStream *stream = g_memory_input_stream_new_from_bytes(bytes);
    GdkPixbuf *pb = gdk_pixbuf_new_from_stream(stream, NULL, &error);
    g_object_unref(stream);
    g_bytes_unref(bytes);

    if (!pb) {
        g_warning("Failed to decode pixbuf from %s: %s", resource_path,
                  error ? error->message : "unknown");
        g_clear_error(&error);
        return NULL;
    }

    cairo_surface_t *s = pixbuf_to_surface(pb);
    g_object_unref(pb);
    return s;
}

static void
load_default_pixmaps(void)
{
    static const char *resource_names[SKIN_PIXMAP_COUNT] = {
        [SKIN_MAIN]       = "/org/xmms/defskin/defskin/main.png",
        [SKIN_CBUTTONS]   = "/org/xmms/defskin/defskin/cbuttons.png",
        [SKIN_TITLEBAR]   = "/org/xmms/defskin/defskin/titlebar.png",
        [SKIN_SHUFREP]    = "/org/xmms/defskin/defskin/shufrep.png",
        [SKIN_TEXT]        = "/org/xmms/defskin/defskin/text.png",
        [SKIN_VOLUME]     = "/org/xmms/defskin/defskin/volume.png",
        [SKIN_MONOSTEREO] = "/org/xmms/defskin/defskin/monoster.png",
        [SKIN_PLAYPAUSE]  = "/org/xmms/defskin/defskin/playpaus.png",
        [SKIN_NUMBERS]    = "/org/xmms/defskin/defskin/nums_ex.png",
        [SKIN_POSBAR]     = "/org/xmms/defskin/defskin/posbar.png",
        [SKIN_PLEDIT]     = "/org/xmms/defskin/defskin/pledit.png",
        [SKIN_EQMAIN]     = "/org/xmms/defskin/defskin/eqmain.png",
        [SKIN_EQ_EX]      = "/org/xmms/defskin/defskin/eq_ex.png",
    };

    for (int i = 0; i < SKIN_PIXMAP_COUNT; i++) {
        SkinPixmap *sp = &skin->pixmaps[i];
        sp->width = skin_pixmap_info[i].width;
        sp->height = skin_pixmap_info[i].height;
        sp->current_width = sp->width;
        sp->current_height = sp->height;

        if (resource_names[i]) {
            sp->def_surface = surface_from_resource(resource_names[i]);
        }
        sp->surface = NULL;
    }

    /* Balance defaults to volume pixmap */
    if (!skin->pixmaps[SKIN_BALANCE].def_surface) {
        cairo_surface_t *vol = skin->pixmaps[SKIN_VOLUME].def_surface;
        if (vol) {
            gint w = cairo_image_surface_get_width(vol);
            gint h = cairo_image_surface_get_height(vol);
            skin->pixmaps[SKIN_BALANCE].def_surface =
                cairo_surface_reference(vol);
            skin->pixmaps[SKIN_BALANCE].width = w;
            skin->pixmaps[SKIN_BALANCE].height = h;
            skin->pixmaps[SKIN_BALANCE].current_width = w;
            skin->pixmaps[SKIN_BALANCE].current_height = h;
        }
    }
}

static gchar *
find_skin_file(const gchar *dir, const gchar *name)
{
    /* Try common extensions and case variations */
    const gchar *exts[] = { ".bmp", ".BMP", ".png", ".PNG", NULL };
    gchar *lower = g_ascii_strdown(name, -1);
    gchar *upper = g_ascii_strup(name, -1);
    /* Title case: first letter uppercase, rest lowercase */
    gchar *title = g_strdup(lower);
    if (title[0])
        title[0] = g_ascii_toupper(title[0]);

    const gchar *cases[] = { name, lower, upper, title, NULL };

    for (const gchar **ext = exts; *ext; ext++) {
        for (const gchar **c = cases; *c; c++) {
            gchar *fname = g_strconcat(*c, *ext, NULL);
            gchar *path = g_build_filename(dir, fname, NULL);
            g_free(fname);
            if (g_file_test(path, G_FILE_TEST_EXISTS)) {
                g_free(lower);
                g_free(upper);
                g_free(title);
                return path;
            }
            g_free(path);
        }
    }

    g_free(lower);
    g_free(upper);
    g_free(title);
    return NULL;
}

static gboolean
extract_skin_archive(const gchar *archive_path, const gchar *dest_dir)
{
#ifdef HAVE_LIBARCHIVE
    struct archive *a = archive_read_new();
    archive_read_support_format_all(a);
    archive_read_support_filter_all(a);

    if (archive_read_open_filename(a, archive_path, 10240) != ARCHIVE_OK) {
        g_warning("Failed to open skin archive: %s", archive_error_string(a));
        archive_read_free(a);
        return FALSE;
    }

    struct archive_entry *entry;
    while (archive_read_next_header(a, &entry) == ARCHIVE_OK) {
        const char *name = archive_entry_pathname(entry);
        gchar *path = g_build_filename(dest_dir, name, NULL);
        archive_entry_set_pathname(entry, path);
        archive_read_extract(a, entry, ARCHIVE_EXTRACT_TIME);
        g_free(path);
    }

    archive_read_free(a);
    return TRUE;
#else
    /* Fallback: use system unzip for .wsz/.zip files */
    gchar *cmd = g_strdup_printf("unzip -o -q '%s' -d '%s'",
                                  archive_path, dest_dir);
    gboolean ok = (system(cmd) == 0);
    g_free(cmd);
    if (!ok) {
        /* Try tar for .tar.gz/.tar.bz2 */
        cmd = g_strdup_printf("tar xf '%s' -C '%s'",
                               archive_path, dest_dir);
        ok = (system(cmd) == 0);
        g_free(cmd);
    }
    return ok;
#endif
}

static void
load_skin_pixmaps(const gchar *dir)
{
    for (int i = 0; i < SKIN_PIXMAP_COUNT; i++) {
        SkinPixmap *sp = &skin->pixmaps[i];

        if (sp->surface) {
            cairo_surface_destroy(sp->surface);
            sp->surface = NULL;
        }

        gchar *path = find_skin_file(dir, skin_pixmap_info[i].name);
        if (!path) {
            /* Try "numbers" as fallback for "nums_ex" */
            if (i == SKIN_NUMBERS)
                path = find_skin_file(dir, "numbers");
            if (!path) {
                continue;
            }
        }

        GdkPixbuf *pb = gdk_pixbuf_new_from_file(path, NULL);
        g_free(path);
        if (!pb)
            continue;

        sp->surface = pixbuf_to_surface(pb);
        sp->current_width = gdk_pixbuf_get_width(pb);
        sp->current_height = gdk_pixbuf_get_height(pb);
        g_object_unref(pb);
    }

    /* Balance falls back to volume */
    if (!skin->pixmaps[SKIN_BALANCE].surface &&
        skin->pixmaps[SKIN_VOLUME].surface) {
        SkinPixmap *bal = &skin->pixmaps[SKIN_BALANCE];
        SkinPixmap *vol = &skin->pixmaps[SKIN_VOLUME];
        bal->surface = cairo_surface_reference(vol->surface);
        bal->current_width = vol->current_width;
        bal->current_height = vol->current_height;
    }
}

static void
load_skin_viscolor(const gchar *dir)
{
    gchar *path = g_build_filename(dir, "viscolor.txt", NULL);
    gchar *contents = NULL;

    if (!g_file_get_contents(path, &contents, NULL, NULL)) {
        g_free(path);
        /* Use defaults */
        memcpy(skin->vis_color, default_viscolor, sizeof(default_viscolor));
        return;
    }
    g_free(path);

    gchar **lines = g_strsplit(contents, "\n", 25);
    g_free(contents);

    for (int i = 0; i < 24 && lines[i]; i++) {
        gint r, g, b;
        if (sscanf(lines[i], "%d,%d,%d", &r, &g, &b) == 3 ||
            sscanf(lines[i], "%d %d %d", &r, &g, &b) == 3) {
            skin->vis_color[i][0] = CLAMP(r, 0, 255);
            skin->vis_color[i][1] = CLAMP(g, 0, 255);
            skin->vis_color[i][2] = CLAMP(b, 0, 255);
        }
    }

    g_strfreev(lines);
}

static void
load_skin_pledit_colors(const gchar *dir)
{
    gchar *path = g_build_filename(dir, "pledit.txt", NULL);
    gchar *contents = NULL;

    /* Defaults */
    skin->pledit_normal    = (GdkRGBA){ 0.0, 1.0, 0.0, 1.0 };
    skin->pledit_current   = (GdkRGBA){ 1.0, 1.0, 1.0, 1.0 };
    skin->pledit_normalbg  = (GdkRGBA){ 0.0, 0.0, 0.0, 1.0 };
    skin->pledit_selectedbg = (GdkRGBA){ 0.0, 0.0, 0.4, 1.0 };

    if (!g_file_get_contents(path, &contents, NULL, NULL)) {
        g_free(path);
        return;
    }
    g_free(path);

    /* Parse INI-style pledit.txt */
    gchar **lines = g_strsplit(contents, "\n", -1);
    g_free(contents);

    for (int i = 0; lines[i]; i++) {
        gchar *line = g_strstrip(lines[i]);
        guint r, g, b;

        if (g_str_has_prefix(line, "Normal=") ||
            g_str_has_prefix(line, "normal=")) {
            if (sscanf(line + 7, "#%02x%02x%02x", &r, &g, &b) == 3)
                skin->pledit_normal = (GdkRGBA){ r/255.0, g/255.0, b/255.0, 1.0 };
        } else if (g_str_has_prefix(line, "Current=") ||
                   g_str_has_prefix(line, "current=")) {
            if (sscanf(line + 8, "#%02x%02x%02x", &r, &g, &b) == 3)
                skin->pledit_current = (GdkRGBA){ r/255.0, g/255.0, b/255.0, 1.0 };
        } else if (g_str_has_prefix(line, "NormalBG=") ||
                   g_str_has_prefix(line, "normalbg=")) {
            if (sscanf(line + 9, "#%02x%02x%02x", &r, &g, &b) == 3)
                skin->pledit_normalbg = (GdkRGBA){ r/255.0, g/255.0, b/255.0, 1.0 };
        } else if (g_str_has_prefix(line, "SelectedBG=") ||
                   g_str_has_prefix(line, "selectedbg=")) {
            if (sscanf(line + 11, "#%02x%02x%02x", &r, &g, &b) == 3)
                skin->pledit_selectedbg = (GdkRGBA){ r/255.0, g/255.0, b/255.0, 1.0 };
        }
    }

    g_strfreev(lines);
}

void
skin_init(void)
{
    skin = g_new0(Skin, 1);
    memcpy(skin->vis_color, default_viscolor, sizeof(default_viscolor));
    load_default_pixmaps();
    skin->id = 1;
}

void
skin_free(void)
{
    if (!skin)
        return;

    for (int i = 0; i < SKIN_PIXMAP_COUNT; i++) {
        SkinPixmap *sp = &skin->pixmaps[i];
        if (sp->surface)
            cairo_surface_destroy(sp->surface);
        if (sp->def_surface)
            cairo_surface_destroy(sp->def_surface);
    }

    g_free(skin->path);
    g_free(skin);
    skin = NULL;
}

gboolean
skin_load(const gchar *path)
{
    if (!path || !path[0])
        return FALSE;

    gchar *skin_dir = NULL;
    gchar *temp_dir = NULL;

    if (g_file_test(path, G_FILE_TEST_IS_DIR)) {
        skin_dir = g_strdup(path);
    } else if (g_file_test(path, G_FILE_TEST_IS_REGULAR)) {
        /* Archive - extract to temp dir */
        temp_dir = g_dir_make_tmp("xmms-skin-XXXXXX", NULL);
        if (!temp_dir)
            return FALSE;
        if (!extract_skin_archive(path, temp_dir)) {
            rmdir(temp_dir);
            g_free(temp_dir);
            return FALSE;
        }
        skin_dir = temp_dir;

        /* Check if files are in a subdirectory */
        GDir *d = g_dir_open(skin_dir, 0, NULL);
        if (d) {
            const gchar *entry;
            gchar *subdir = NULL;
            gboolean found_bmp = FALSE;
            while ((entry = g_dir_read_name(d)) != NULL) {
                if (g_str_has_suffix(entry, ".bmp") ||
                    g_str_has_suffix(entry, ".BMP")) {
                    found_bmp = TRUE;
                    break;
                }
                gchar *full = g_build_filename(skin_dir, entry, NULL);
                if (g_file_test(full, G_FILE_TEST_IS_DIR) && !subdir)
                    subdir = g_strdup(full);
                g_free(full);
            }
            g_dir_close(d);
            if (!found_bmp && subdir) {
                skin_dir = subdir;
            } else {
                g_free(subdir);
            }
        }
    } else {
        return FALSE;
    }

    /* Clear custom pixmaps */
    for (int i = 0; i < SKIN_PIXMAP_COUNT; i++) {
        if (skin->pixmaps[i].surface) {
            cairo_surface_destroy(skin->pixmaps[i].surface);
            skin->pixmaps[i].surface = NULL;
        }
    }

    load_skin_pixmaps(skin_dir);
    load_skin_viscolor(skin_dir);
    load_skin_pledit_colors(skin_dir);

    g_free(skin->path);
    skin->path = g_strdup(path);
    skin->id++;

    if (skin_dir != temp_dir)
        g_free(skin_dir);
    g_free(temp_dir);
    return TRUE;
}

void
skin_draw_pixmap(cairo_t *cr, SkinIndex index,
                 gint xsrc, gint ysrc,
                 gint xdest, gint ydest,
                 gint width, gint height)
{
    if (index < 0 || index >= SKIN_PIXMAP_COUNT)
        return;

    SkinPixmap *sp = &skin->pixmaps[index];
    cairo_surface_t *surface = sp->surface ? sp->surface : sp->def_surface;

    if (!surface)
        return;

    cairo_save(cr);
    cairo_rectangle(cr, xdest, ydest, width, height);
    cairo_clip(cr);
    cairo_set_source_surface(cr, surface, xdest - xsrc, ydest - ysrc);
    cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_NEAREST);
    cairo_paint(cr);
    cairo_restore(cr);
}

cairo_surface_t *
skin_get_surface(SkinIndex index)
{
    if (index < 0 || index >= SKIN_PIXMAP_COUNT)
        return NULL;
    SkinPixmap *sp = &skin->pixmaps[index];
    return sp->surface ? sp->surface : sp->def_surface;
}

gint
skin_get_id(void)
{
    return skin ? skin->id : 0;
}
