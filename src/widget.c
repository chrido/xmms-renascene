#include "xmms.h"

/* ---- Widget list management ---- */

void
widget_list_add(GList **list, Widget *w)
{
    *list = g_list_append(*list, w);
}

void
widget_list_draw(GList *list, cairo_t *cr)
{
    for (GList *l = list; l; l = l->next) {
        Widget *w = l->data;
        if (w->visible && w->draw)
            w->draw(w, cr);
    }
}

Widget *
widget_list_find(GList *list, gint x, gint y)
{
    /* Search in reverse order (top widgets first) */
    for (GList *l = g_list_last(list); l; l = l->prev) {
        Widget *w = l->data;
        if (w->visible && widget_inside(w, x, y))
            return w;
    }
    return NULL;
}

gboolean
widget_inside(Widget *w, gint x, gint y)
{
    return (x >= w->x && x < w->x + w->width &&
            y >= w->y && y < w->y + w->height);
}

void
widget_queue_draw(Widget *w)
{
    w->redraw = TRUE;
    mainwin_queue_draw();
}

/* ---- PButton (Push Button) ---- */

static void
pbutton_draw(Widget *w, cairo_t *cr)
{
    PButton *pb = (PButton *)w;
    gint sx, sy;

    if (pb->pressed && pb->inside) {
        sx = pb->px;
        sy = pb->py;
    } else {
        sx = pb->nx;
        sy = pb->ny;
    }

    skin_draw_pixmap(cr, pb->skin_index,
                     sx, sy, w->x, w->y, w->width, w->height);
}

static void
pbutton_press(Widget *w, gint x, gint y, gint button)
{
    (void)x; (void)y;
    if (button != 1) return;
    PButton *pb = (PButton *)w;
    pb->pressed = TRUE;
    pb->inside = TRUE;
    widget_queue_draw(w);
}

static void
pbutton_release(Widget *w, gint x, gint y, gint button)
{
    (void)x; (void)y;
    if (button != 1) return;
    PButton *pb = (PButton *)w;
    if (pb->pressed && pb->inside && pb->push_cb)
        pb->push_cb();
    pb->pressed = FALSE;
    widget_queue_draw(w);
}

static void
pbutton_motion(Widget *w, gint x, gint y)
{
    PButton *pb = (PButton *)w;
    if (!pb->pressed) return;
    gboolean was_inside = pb->inside;
    pb->inside = widget_inside(w, x, y);
    if (was_inside != pb->inside)
        widget_queue_draw(w);
}

PButton *
pbutton_new(GList **list, gint x, gint y, gint w, gint h,
            gint nx, gint ny, gint px, gint py,
            void (*cb)(void), gint skin_index)
{
    PButton *pb = g_new0(PButton, 1);
    pb->w.x = x; pb->w.y = y;
    pb->w.width = w; pb->w.height = h;
    pb->w.visible = TRUE;
    pb->w.draw = pbutton_draw;
    pb->w.button_press = pbutton_press;
    pb->w.button_release = pbutton_release;
    pb->w.motion = pbutton_motion;
    pb->nx = nx; pb->ny = ny;
    pb->px = px; pb->py = py;
    pb->push_cb = cb;
    pb->skin_index = skin_index;
    widget_list_add(list, (Widget *)pb);
    return pb;
}

/* ---- TButton (Toggle Button) ---- */

static void
tbutton_draw(Widget *w, cairo_t *cr)
{
    TButton *tb = (TButton *)w;
    gint sx, sy;

    if (tb->selected) {
        if (tb->pressed && tb->inside) {
            sx = tb->psx; sy = tb->psy;
        } else {
            sx = tb->nsx; sy = tb->nsy;
        }
    } else {
        if (tb->pressed && tb->inside) {
            sx = tb->pux; sy = tb->puy;
        } else {
            sx = tb->nux; sy = tb->nuy;
        }
    }

    skin_draw_pixmap(cr, tb->skin_index,
                     sx, sy, w->x, w->y, w->width, w->height);
}

static void
tbutton_press(Widget *w, gint x, gint y, gint button)
{
    (void)x; (void)y;
    if (button != 1) return;
    TButton *tb = (TButton *)w;
    tb->pressed = TRUE;
    tb->inside = TRUE;
    widget_queue_draw(w);
}

static void
tbutton_release(Widget *w, gint x, gint y, gint button)
{
    (void)x; (void)y;
    if (button != 1) return;
    TButton *tb = (TButton *)w;
    if (tb->pressed && tb->inside) {
        tb->selected = !tb->selected;
        if (tb->push_cb)
            tb->push_cb(tb->selected);
    }
    tb->pressed = FALSE;
    widget_queue_draw(w);
}

static void
tbutton_motion(Widget *w, gint x, gint y)
{
    TButton *tb = (TButton *)w;
    if (!tb->pressed) return;
    gboolean was_inside = tb->inside;
    tb->inside = widget_inside(w, x, y);
    if (was_inside != tb->inside)
        widget_queue_draw(w);
}

TButton *
tbutton_new(GList **list, gint x, gint y, gint w, gint h,
            gint nux, gint nuy, gint pux, gint puy,
            gint nsx, gint nsy, gint psx, gint psy,
            void (*cb)(gboolean), gint skin_index)
{
    TButton *tb = g_new0(TButton, 1);
    tb->w.x = x; tb->w.y = y;
    tb->w.width = w; tb->w.height = h;
    tb->w.visible = TRUE;
    tb->w.draw = tbutton_draw;
    tb->w.button_press = tbutton_press;
    tb->w.button_release = tbutton_release;
    tb->w.motion = tbutton_motion;
    tb->nux = nux; tb->nuy = nuy;
    tb->pux = pux; tb->puy = puy;
    tb->nsx = nsx; tb->nsy = nsy;
    tb->psx = psx; tb->psy = psy;
    tb->push_cb = cb;
    tb->skin_index = skin_index;
    widget_list_add(list, (Widget *)tb);
    return tb;
}

void
tbutton_set_toggled(TButton *tb, gboolean toggled)
{
    tb->selected = toggled;
    widget_queue_draw((Widget *)tb);
}

/* ---- TextBox ---- */

static void
textbox_render(TextBox *tb)
{
    if (!tb->text)
        return;

    gint cur_id = skin_get_id();
    if (tb->rendered_text && g_strcmp0(tb->rendered_text, tb->text) == 0 &&
        tb->skin_id == cur_id)
        return;

    g_free(tb->rendered_text);
    tb->rendered_text = g_strdup(tb->text);
    tb->skin_id = cur_id;

    if (tb->rendered) {
        cairo_surface_destroy(tb->rendered);
        tb->rendered = NULL;
    }

    cairo_surface_t *font_surface = skin_get_surface(tb->skin_index);
    if (!font_surface)
        return;

    gint text_len = strlen(tb->text);
    gint char_w = 5, char_h = 6;
    tb->rendered_width = text_len * char_w;

    if (tb->rendered_width <= 0)
        tb->rendered_width = char_w;

    tb->rendered = cairo_image_surface_create(
        CAIRO_FORMAT_ARGB32, tb->rendered_width, char_h);
    cairo_t *cr = cairo_create(tb->rendered);

    for (gint i = 0; i < text_len; i++) {
        guchar c = (guchar)tb->text[i];
        gint sx, sy;

        if (c >= 'A' && c <= 'Z') {
            sx = (c - 'A') * char_w;
            sy = 0;
        } else if (c >= 'a' && c <= 'z') {
            sx = (c - 'a') * char_w;
            sy = 0;
        } else if (c >= '0' && c <= '9') {
            sx = (c - '0') * char_w;
            sy = 6;
        } else if (c == ' ') {
            /* Space - just skip (transparent) */
            continue;
        } else {
            /* Map special chars */
            switch (c) {
            case '"':  sx = 130; sy = 0; break;
            case '@':  sx = 135; sy = 0; break;
            case '.':  sx = 55;  sy = 6; break;
            case ':':  sx = 60;  sy = 6; break;
            case '(':  sx = 65;  sy = 6; break;
            case ')':  sx = 70;  sy = 6; break;
            case '-':  sx = 75;  sy = 6; break;
            case '\'': sx = 80;  sy = 6; break;
            case '!':  sx = 85;  sy = 6; break;
            case '_':  sx = 90;  sy = 6; break;
            case '+':  sx = 95;  sy = 6; break;
            case '\\': sx = 100; sy = 6; break;
            case '/':  sx = 105; sy = 6; break;
            case '[':  sx = 110; sy = 6; break;
            case ']':  sx = 115; sy = 6; break;
            case '^':  sx = 120; sy = 6; break;
            case '&':  sx = 125; sy = 6; break;
            case '%':  sx = 130; sy = 6; break;
            case ',':  sx = 135; sy = 6; break;
            case '=':  sx = 140; sy = 6; break;
            case '$':  sx = 145; sy = 6; break;
            case '#':  sx = 150; sy = 6; break;
            case '?':  sx = 50;  sy = 12; break;
            case '*':  sx = 55;  sy = 12; break;
            default:   continue; /* Unknown char, skip */
            }
        }

        cairo_save(cr);
        cairo_rectangle(cr, i * char_w, 0, char_w, char_h);
        cairo_clip(cr);
        cairo_set_source_surface(cr, font_surface,
                                 i * char_w - sx, -sy);
        cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_NEAREST);
        cairo_paint(cr);
        cairo_restore(cr);
    }

    cairo_destroy(cr);

    tb->is_scrollable = (tb->rendered_width > tb->w.width);
}

static void
textbox_draw(Widget *w, cairo_t *cr)
{
    TextBox *tb = (TextBox *)w;
    textbox_render(tb);

    if (!tb->rendered)
        return;

    gint src_x = tb->offset;
    gint draw_w = MIN(tb->w.width, tb->rendered_width - src_x);

    if (draw_w > 0) {
        cairo_save(cr);
        cairo_rectangle(cr, w->x, w->y, tb->w.width, tb->w.height);
        cairo_clip(cr);
        cairo_set_source_surface(cr, tb->rendered,
                                 w->x - src_x, w->y);
        cairo_pattern_set_filter(cairo_get_source(cr), CAIRO_FILTER_NEAREST);
        cairo_paint(cr);
        cairo_restore(cr);
    }
}

static gboolean
textbox_scroll_cb(gpointer data)
{
    TextBox *tb = data;
    if (!tb->is_scrollable || !tb->scroll_enabled)
        return G_SOURCE_CONTINUE;

    tb->offset += 1;
    if (tb->offset >= tb->rendered_width)
        tb->offset = 0;

    widget_queue_draw((Widget *)tb);
    return G_SOURCE_CONTINUE;
}

TextBox *
textbox_new(GList **list, gint x, gint y, gint w,
            gboolean scroll, gint skin_index)
{
    TextBox *tb = g_new0(TextBox, 1);
    tb->w.x = x; tb->w.y = y;
    tb->w.width = w; tb->w.height = 6;
    tb->w.visible = TRUE;
    tb->w.draw = textbox_draw;
    tb->scroll_enabled = scroll;
    tb->skin_index = skin_index;

    if (scroll)
        tb->scroll_tag = g_timeout_add(200, textbox_scroll_cb, tb);

    widget_list_add(list, (Widget *)tb);
    return tb;
}

void
textbox_set_text(TextBox *tb, const gchar *text)
{
    if (g_strcmp0(tb->original_text, text) == 0)
        return;
    g_free(tb->original_text);
    g_free(tb->text);
    tb->original_text = g_strdup(text);

    if (tb->scroll_enabled && text) {
        /* Add scroll separator for scrollable text */
        gint text_w = (gint)strlen(text) * 5;
        if (text_w > tb->w.width)
            tb->text = g_strdup_printf("%s  ***  ", text);
        else
            tb->text = g_strdup(text);
    } else {
        tb->text = g_strdup(text);
    }

    tb->offset = 0;
    if (tb->rendered) {
        cairo_surface_destroy(tb->rendered);
        tb->rendered = NULL;
    }
    widget_queue_draw((Widget *)tb);
}

/* ---- HSlider ---- */

static void
hslider_draw(Widget *w, cairo_t *cr)
{
    HSlider *hs = (HSlider *)w;

    /* Draw frame/background */
    gint frame = hs->frame_cb ? hs->frame_cb(hs->position) : 0;
    skin_draw_pixmap(cr, hs->skin_index,
                     hs->frame_offset, frame * hs->frame_height,
                     w->x, w->y, w->width, w->height);

    /* Draw knob */
    gint kx, ky;
    if (hs->pressed) {
        kx = hs->knob_px;
        ky = hs->knob_py;
    } else {
        kx = hs->knob_nx;
        ky = hs->knob_ny;
    }

    skin_draw_pixmap(cr, hs->skin_index,
                     kx, ky,
                     w->x + hs->position, w->y,
                     hs->knob_width, hs->knob_height);
}

static void
hslider_press(Widget *w, gint x, gint y, gint button)
{
    (void)y;
    if (button != 1) return;
    HSlider *hs = (HSlider *)w;
    gint knob_x = w->x + hs->position;

    if (x >= knob_x && x < knob_x + hs->knob_width) {
        hs->pressed = TRUE;
        hs->press_offset = x - knob_x;
    } else {
        hs->pressed = TRUE;
        hs->press_offset = hs->knob_width / 2;
        hs->position = CLAMP(x - w->x - hs->press_offset, hs->min, hs->max);
        if (hs->motion_cb)
            hs->motion_cb(hs->position);
    }
    widget_queue_draw(w);
}

static void
hslider_release(Widget *w, gint x, gint y, gint button)
{
    (void)x; (void)y;
    if (button != 1) return;
    HSlider *hs = (HSlider *)w;
    hs->pressed = FALSE;
    if (hs->release_cb)
        hs->release_cb(hs->position);
    widget_queue_draw(w);
}

static void
hslider_motion(Widget *w, gint x, gint y)
{
    (void)y;
    HSlider *hs = (HSlider *)w;
    if (!hs->pressed) return;

    hs->position = CLAMP(x - w->x - hs->press_offset, hs->min, hs->max);
    if (hs->motion_cb)
        hs->motion_cb(hs->position);
    widget_queue_draw(w);
}

HSlider *
hslider_new(GList **list, gint x, gint y, gint w, gint h,
            gint knob_nx, gint knob_ny, gint knob_px, gint knob_py,
            gint knob_w, gint knob_h,
            gint frame_height, gint frame_offset,
            gint min, gint max,
            gint (*frame_cb)(gint),
            void (*motion_cb)(gint),
            void (*release_cb)(gint),
            gint skin_index)
{
    HSlider *hs = g_new0(HSlider, 1);
    hs->w.x = x; hs->w.y = y;
    hs->w.width = w; hs->w.height = h;
    hs->w.visible = TRUE;
    hs->w.draw = hslider_draw;
    hs->w.button_press = hslider_press;
    hs->w.button_release = hslider_release;
    hs->w.motion = hslider_motion;
    hs->knob_nx = knob_nx; hs->knob_ny = knob_ny;
    hs->knob_px = knob_px; hs->knob_py = knob_py;
    hs->knob_width = knob_w; hs->knob_height = knob_h;
    hs->frame_height = frame_height;
    hs->frame_offset = frame_offset;
    hs->min = min; hs->max = max;
    hs->frame_cb = frame_cb;
    hs->motion_cb = motion_cb;
    hs->release_cb = release_cb;
    hs->skin_index = skin_index;
    widget_list_add(list, (Widget *)hs);
    return hs;
}

void
hslider_set_position(HSlider *hs, gint pos)
{
    hs->position = CLAMP(pos, hs->min, hs->max);
    widget_queue_draw((Widget *)hs);
}

/* ---- Number ---- */

static void
number_draw(Widget *w, cairo_t *cr)
{
    Number *n = (Number *)w;
    gint digit = n->value;

    if (digit < 0 || digit > 11)
        digit = 11; /* blank/dash */

    /* nums_ex.xpm: each digit is 9x13, laid out horizontally
       0-9 are digits, 10 is blank, 11 is dash */
    skin_draw_pixmap(cr, n->skin_index,
                     digit * 9, 0, w->x, w->y, 9, 13);
}

Number *
number_new(GList **list, gint x, gint y, gint skin_index)
{
    Number *n = g_new0(Number, 1);
    n->w.x = x; n->w.y = y;
    n->w.width = 9; n->w.height = 13;
    n->w.visible = TRUE;
    n->w.draw = number_draw;
    n->value = 10; /* blank */
    n->skin_index = skin_index;
    widget_list_add(list, (Widget *)n);
    return n;
}

void
number_set_value(Number *n, gint value)
{
    n->value = value;
    widget_queue_draw((Widget *)n);
}

/* ---- Visualization ---- */

static const gfloat vis_afalloff_speeds[] = {
    0.34f / 16.0f, 0.5f / 16.0f, 1.0f / 16.0f,
    1.3f / 16.0f, 1.6f / 16.0f
};
static const gfloat vis_pfalloff_speeds[] = {
    1.2f, 1.3f, 1.4f, 1.5f, 1.6f
};
static const guint8 vis_scope_colors[] = {
    21, 21, 20, 20, 19, 19, 18, 19, 19, 20, 20, 21, 21
};
static const guint8 vis_svis_scope_colors[] = { 20, 19, 18, 19, 20 };
static const guint8 vis_svis_vu_normal_colors[] = { 17, 17, 17, 12, 12, 12, 2, 2 };

static gint
vis_level(gfloat value)
{
    return CLAMP((gint)(value * 16.0f + 0.5f), 0, 16);
}

static void
vis_set_skin_color(cairo_t *cr, gint color_idx)
{
    color_idx = CLAMP(color_idx, 0, 23);
    cairo_set_source_rgb(cr,
        skin->vis_color[color_idx][0] / 255.0,
        skin->vis_color[color_idx][1] / 255.0,
        skin->vis_color[color_idx][2] / 255.0);
}

static void
vis_draw_pixel(cairo_t *cr, Widget *w, gint x, gint y, gint color_idx)
{
    if (x < 0 || x >= w->width || y < 0 || y >= 16)
        return;
    vis_set_skin_color(cr, color_idx);
    cairo_rectangle(cr, w->x + x, w->y + y, 1, 1);
    cairo_fill(cr);
}

static gint
vis_analyzer_color(Vis *vis, gint row, gint height)
{
    switch (vis->analyzer_mode) {
    case VIS_ANALYZER_FIRE:
        return (16 - height + row) + 2;
    case VIS_ANALYZER_VLINES:
        return 18 - height;
    case VIS_ANALYZER_NORMAL:
    default:
        return row + 2;
    }
}

static void
vis_decay(Vis *vis)
{
    VisFalloffSpeed af = CLAMP(vis->analyzer_falloff,
                               VIS_FALLOFF_SLOWEST, VIS_FALLOFF_FASTEST);
    VisFalloffSpeed pf = CLAMP(vis->peaks_falloff,
                               VIS_FALLOFF_SLOWEST, VIS_FALLOFF_FASTEST);

    for (gint i = 0; i < 75; i++) {
        if (vis->data[i] > 0.0f)
            vis->data[i] = MAX(0.0f, vis->data[i] - vis_afalloff_speeds[af]);
        if (vis->peak[i] > 0.0f) {
            vis->peak[i] = MAX(0.0f, vis->peak[i] - vis->peak_speed[i]);
            vis->peak_speed[i] *= vis_pfalloff_speeds[pf];
            if (vis->peak[i] < vis->data[i])
                vis->peak[i] = vis->data[i];
        }
    }
}

static void
vis_draw_milkdrop(Vis *vis, cairo_t *cr)
{
    Widget *w = (Widget *)vis;
    const gfloat cx = (w->width - 1) * 0.5f;
    const gfloat cy = 7.5f;
    gfloat phase = vis->milkdrop_phase;
    gfloat energy = CLAMP(vis->milkdrop_energy, 0.0f, 1.0f);
    gfloat rot = phase * (0.35f + energy * 0.25f);
    gfloat s = sinf(rot);
    gfloat c = cosf(rot);

    for (gint y = 0; y < 16; y++) {
        for (gint x = 0; x < w->width && x < 76; x++) {
            gfloat nx = (x - cx) / MAX(cx, 1.0f);
            gfloat ny = (y - cy) / 8.0f;
            gfloat rx = nx * c - ny * s;
            gfloat ry = nx * s + ny * c;
            gfloat r = sqrtf(rx * rx + ry * ry);
            gfloat angle = atan2f(ry, rx);
            gfloat warp = sinf(angle * 5.0f + phase * 1.7f) * 0.18f +
                sinf(r * 12.0f - phase * 2.4f) * 0.16f;
            gfloat tunnel = sinf((r + warp) * 18.0f - phase * 3.0f);
            gfloat plasma = sinf((rx - ry) * 7.0f + phase) +
                cosf((rx + ry) * 5.0f - phase * 1.3f);
            gint color = 3 + (gint)((tunnel + plasma + 4.0f + energy * 2.0f) * 2.6f);
            color = CLAMP(color, 2, 22);
            vis_draw_pixel(cr, w, x, y, color);
        }
    }

    for (gint x = 0; x < w->width && x < 75; x++) {
        gfloat sample = vis->data[x];
        gint y = CLAMP((gint)(7.5f + sinf(x * 0.19f + phase * 2.0f) * 3.0f -
                              sample * 6.0f), 0, 15);
        vis_draw_pixel(cr, w, x, y, 23);
        if (y + 1 < 16)
            vis_draw_pixel(cr, w, x, y + 1, 18);
    }

    for (gint i = 0; i < 28; i++) {
        gfloat a = phase * 0.9f + i * (G_PI * 2.0f / 28.0f);
        gfloat radius = 2.0f + energy * 7.0f +
            sinf(phase * 1.4f + i * 0.7f) * 1.4f;
        gint x = CLAMP((gint)(cx + cosf(a) * radius * 3.4f), 0, w->width - 1);
        gint y = CLAMP((gint)(cy + sinf(a) * radius), 0, 15);
        vis_draw_pixel(cr, w, x, y, 21 + (i % 3));
    }
}

static void
vis_draw(Widget *w, cairo_t *cr)
{
    Vis *vis = (Vis *)w;
    gint levels[75] = { 0 };

    /* Draw visualization background from skin */
    cairo_save(cr);
    cairo_rectangle(cr, w->x, w->y, w->width, 16);
    cairo_clip(cr);

    /* Dark background */
    gdk_cairo_set_source_rgba(cr, &(GdkRGBA){
        skin->vis_color[0][0] / 255.0,
        skin->vis_color[0][1] / 255.0,
        skin->vis_color[0][2] / 255.0, 1.0
    });
    cairo_paint(cr);

    for (gint y = 1; y < 16; y += 2) {
        for (gint x = 0; x < w->width && x < 76; x += 2)
            vis_draw_pixel(cr, w, x, y, 1);
    }

    if (vis->mode == VIS_MODE_OFF) {
        cairo_restore(cr);
        return;
    }

    for (gint i = 0; i < 75; i++)
        levels[i] = vis_level(vis->data[i]);

    if (vis->mode == VIS_MODE_SCOPE) {
        for (gint x = 0; x < 75 && x < w->width; x++) {
            gint h = CLAMP(levels[x], 0, 15);
            switch (vis->scope_mode) {
            case VIS_SCOPE_DOT:
                vis_draw_pixel(cr, w, x, 15 - h,
                               vis_scope_colors[CLAMP(h, 0, 12)]);
                break;
            case VIS_SCOPE_LINE:
                if (x < 74 && x + 1 < w->width) {
                    gint y1 = 15 - h;
                    gint y2 = 15 - CLAMP(levels[x + 1], 0, 15);
                    gint start = MIN(y1, y2);
                    gint end = MAX(y1, y2);
                    for (gint y = start; y <= end; y++)
                        vis_draw_pixel(cr, w, x, y,
                                       vis_scope_colors[CLAMP(y - 3, 0, 12)]);
                } else {
                    gint y = 15 - h;
                    vis_draw_pixel(cr, w, x, y,
                                   vis_scope_colors[CLAMP(y, 0, 12)]);
                }
                break;
            case VIS_SCOPE_SOLID: {
                gint y1 = 15 - h;
                gint y2 = 9;
                gint color = vis_scope_colors[CLAMP(h, 0, 12)];
                for (gint y = MIN(y1, y2); y <= MAX(y1, y2); y++)
                    vis_draw_pixel(cr, w, x, y, color);
                break;
            }
            }
        }
        cairo_restore(cr);
        return;
    }

    if (vis->mode == VIS_MODE_MILKDROP) {
        vis_draw_milkdrop(vis, cr);
        cairo_restore(cr);
        return;
    }

    for (gint x = 0; x < 75 && x < w->width; x++) {
        gint h = 0;
        if (vis->analyzer_style == VIS_ANALYZER_BARS) {
            if (x % 4 == 3)
                continue;
            h = levels[x >> 2];
        } else {
            h = levels[x];
        }

        if (h <= 0)
            continue;

        h = CLAMP(h, 0, 16);
        for (gint y = 16 - h; y < 16; y++)
            vis_draw_pixel(cr, w, x, y, vis_analyzer_color(vis, y, h));

        if (vis->peaks_enabled) {
            gint peak_idx = vis->analyzer_style == VIS_ANALYZER_BARS ?
                x >> 2 : x;
            gint peak_y = 16 - vis_level(vis->peak[peak_idx]);
            if (peak_y >= 0 && peak_y < 16)
                vis_draw_pixel(cr, w, x, peak_y, 23);
        }
    }

    cairo_restore(cr);
}

Vis *
vis_new(GList **list, gint x, gint y, gint w)
{
    Vis *vis = g_new0(Vis, 1);
    vis->w.x = x; vis->w.y = y;
    vis->w.width = w; vis->w.height = 16;
    vis->w.visible = TRUE;
    vis->w.draw = vis_draw;
    vis->mode = VIS_MODE_ANALYZER;
    vis->analyzer_style = VIS_ANALYZER_BARS;
    vis->analyzer_mode = VIS_ANALYZER_NORMAL;
    vis->scope_mode = VIS_SCOPE_LINE;
    vis->peaks_enabled = TRUE;
    vis->analyzer_falloff = VIS_FALLOFF_MEDIUM;
    vis->peaks_falloff = VIS_FALLOFF_SLOW;
    widget_list_add(list, (Widget *)vis);
    return vis;
}

void
vis_set_data(Vis *vis, gfloat *data, gint num)
{
    if (!vis)
        return;

    gint count = MIN(num, 75);
    for (gint i = 0; i < count; i++) {
        data[i] = CLAMP(data[i], 0.0f, 1.0f);
        if (data[i] > vis->data[i])
            vis->data[i] = data[i];
        if (data[i] > vis->peak[i]) {
            vis->peak[i] = data[i];
            vis->peak_speed[i] = 0.01f / 16.0f;
        }
    }
}

void
vis_tick(Vis *vis, gfloat *data, gint num)
{
    if (!vis)
        return;
    if (data)
        vis_set_data(vis, data, num);
    vis_decay(vis);
    gfloat energy = 0.0f;
    for (gint i = 0; i < 32; i++)
        energy += vis->data[i];
    energy /= 32.0f;
    vis->milkdrop_energy = vis->milkdrop_energy * 0.88f + energy * 0.12f;
    vis->milkdrop_phase = fmodf(vis->milkdrop_phase +
                                0.08f + vis->milkdrop_energy * 0.08f,
                                (gfloat)(G_PI * 2.0));
    widget_queue_draw((Widget *)vis);
}

void
vis_set_mode(Vis *vis, VisMode mode)
{
    if (!vis)
        return;
    vis->mode = CLAMP(mode, VIS_MODE_ANALYZER, VIS_MODE_MILKDROP);
    widget_queue_draw((Widget *)vis);
}

void
vis_set_analyzer_style(Vis *vis, VisAnalyzerStyle style)
{
    if (!vis)
        return;
    vis->analyzer_style = style;
    widget_queue_draw((Widget *)vis);
}

void
vis_set_analyzer_mode(Vis *vis, VisAnalyzerMode mode)
{
    if (!vis)
        return;
    vis->analyzer_mode = CLAMP(mode, VIS_ANALYZER_NORMAL, VIS_ANALYZER_VLINES);
    widget_queue_draw((Widget *)vis);
}

void
vis_set_scope_mode(Vis *vis, VisScopeMode mode)
{
    if (!vis)
        return;
    vis->scope_mode = CLAMP(mode, VIS_SCOPE_DOT, VIS_SCOPE_SOLID);
    widget_queue_draw((Widget *)vis);
}

void
vis_set_peaks_enabled(Vis *vis, gboolean enabled)
{
    if (!vis)
        return;
    vis->peaks_enabled = enabled;
    if (!enabled)
        memset(vis->peak, 0, sizeof(vis->peak));
    widget_queue_draw((Widget *)vis);
}

void
vis_set_falloff(Vis *vis, VisFalloffSpeed analyzer_falloff,
                VisFalloffSpeed peaks_falloff)
{
    if (!vis)
        return;
    vis->analyzer_falloff = CLAMP(analyzer_falloff,
                                  VIS_FALLOFF_SLOWEST,
                                  VIS_FALLOFF_FASTEST);
    vis->peaks_falloff = CLAMP(peaks_falloff,
                               VIS_FALLOFF_SLOWEST,
                               VIS_FALLOFF_FASTEST);
}

void
vis_draw_windowshade(Vis *vis, cairo_t *cr, gint x, gint y, VisVUMode vu_mode)
{
    if (!vis)
        return;

    cairo_save(cr);
    cairo_rectangle(cr, x, y, 38, 5);
    cairo_clip(cr);
    vis_set_skin_color(cr, 0);
    cairo_paint(cr);

    if (vis->mode == VIS_MODE_OFF) {
        cairo_restore(cr);
        return;
    }

    if (vis->mode == VIS_MODE_SCOPE) {
        for (gint sx = 0; sx < 38; sx++) {
            gint h = CLAMP(vis_level(vis->data[sx * 2]) / 3, 0, 4);
            vis_set_skin_color(cr, vis_svis_scope_colors[h]);
            cairo_rectangle(cr, x + sx, y + 4 - h, 1, 1);
            cairo_fill(cr);
        }
        cairo_restore(cr);
        return;
    }

    for (gint row = 0; row < 2; row++) {
        gint level = CLAMP((gint)(vis->data[row] * 37.0f + 0.5f), 0, 37);
        if (vu_mode == VIS_VU_SMOOTH) {
            for (gint sx = 0; sx < level && sx < 38; sx++) {
                vis_set_skin_color(cr, 17 - ((sx * 15) / 37));
                cairo_rectangle(cr, x + sx, y + row * 3, 1, 1);
                cairo_rectangle(cr, x + sx, y + row * 3 + 1, 1, 1);
                cairo_fill(cr);
            }
        } else {
            gint bars = CLAMP((level * 7) / 37, 0, 7);
            for (gint sx = 0; sx < bars; sx++) {
                vis_set_skin_color(cr, vis_svis_vu_normal_colors[sx]);
                cairo_rectangle(cr, x + sx * 5, y + row * 3, 3, 2);
                cairo_fill(cr);
            }
        }
    }

    cairo_restore(cr);
}

/* ---- MonoStereo ---- */

static void
monostereo_draw(Widget *w, cairo_t *cr)
{
    MonoStereo *ms = (MonoStereo *)w;

    /* monoster.bmp layout:
       0,0:  stereo active (29x12)
       0,12: stereo inactive
       29,0: mono active (27x12)
       29,12: mono inactive */

    if (ms->nchannels == 2) {
        /* Stereo active */
        skin_draw_pixmap(cr, ms->skin_index, 0, 0, w->x, w->y, 29, 12);
        /* Mono inactive */
        skin_draw_pixmap(cr, ms->skin_index, 29, 12, w->x + 29, w->y, 27, 12);
    } else if (ms->nchannels == 1) {
        /* Stereo inactive */
        skin_draw_pixmap(cr, ms->skin_index, 0, 12, w->x, w->y, 29, 12);
        /* Mono active */
        skin_draw_pixmap(cr, ms->skin_index, 29, 0, w->x + 29, w->y, 27, 12);
    } else {
        /* Both inactive */
        skin_draw_pixmap(cr, ms->skin_index, 0, 12, w->x, w->y, 29, 12);
        skin_draw_pixmap(cr, ms->skin_index, 29, 12, w->x + 29, w->y, 27, 12);
    }
}

MonoStereo *
monostereo_new(GList **list, gint x, gint y, gint skin_index)
{
    MonoStereo *ms = g_new0(MonoStereo, 1);
    ms->w.x = x; ms->w.y = y;
    ms->w.width = 56; ms->w.height = 12;
    ms->w.visible = TRUE;
    ms->w.draw = monostereo_draw;
    ms->skin_index = skin_index;
    widget_list_add(list, (Widget *)ms);
    return ms;
}

void
monostereo_set_channels(MonoStereo *ms, gint nch)
{
    ms->nchannels = nch;
    widget_queue_draw((Widget *)ms);
}

/* ---- PlayStatus ---- */

static void
playstatus_draw(Widget *w, cairo_t *cr)
{
    PlayStatus *ps = (PlayStatus *)w;

    /* playpaus.bmp: 3 states, each 11x9
       0,0: stopped (or paused combined)
       Winamp layout: play=1*9, pause=0, stop=2*9 from top */
    gint sy;
    switch (ps->status) {
    case 2: sy = 0;  break; /* play */
    case 1: sy = 9;  break; /* pause */
    default: sy = 18; break; /* stop */
    }

    skin_draw_pixmap(cr, ps->skin_index,
                     0, sy, w->x, w->y, 11, 9);
}

PlayStatus *
playstatus_new(GList **list, gint x, gint y, gint skin_index)
{
    PlayStatus *ps = g_new0(PlayStatus, 1);
    ps->w.x = x; ps->w.y = y;
    ps->w.width = 11; ps->w.height = 9;
    ps->w.visible = TRUE;
    ps->w.draw = playstatus_draw;
    ps->skin_index = skin_index;
    widget_list_add(list, (Widget *)ps);
    return ps;
}

void
playstatus_set_status(PlayStatus *ps, gint status)
{
    ps->status = status;
    widget_queue_draw((Widget *)ps);
}

/* ---- SButton (Simple/invisible button) ---- */

static void
sbutton_press(Widget *w, gint x, gint y, gint button)
{
    (void)x; (void)y;
    if (button != 1) return;
    SButton *sb = (SButton *)w;
    sb->pressed = TRUE;
    sb->inside = TRUE;
}

static void
sbutton_release(Widget *w, gint x, gint y, gint button)
{
    (void)x; (void)y;
    if (button != 1) return;
    SButton *sb = (SButton *)w;
    if (sb->pressed && sb->inside && sb->push_cb)
        sb->push_cb();
    sb->pressed = FALSE;
}

static void
sbutton_motion(Widget *w, gint x, gint y)
{
    SButton *sb = (SButton *)w;
    if (!sb->pressed) return;
    sb->inside = widget_inside(w, x, y);
}

SButton *
sbutton_new(GList **list, gint x, gint y, gint w, gint h,
            void (*cb)(void))
{
    SButton *sb = g_new0(SButton, 1);
    sb->w.x = x; sb->w.y = y;
    sb->w.width = w; sb->w.height = h;
    sb->w.visible = TRUE;
    sb->w.button_press = sbutton_press;
    sb->w.button_release = sbutton_release;
    sb->w.motion = sbutton_motion;
    sb->push_cb = cb;
    widget_list_add(list, (Widget *)sb);
    return sb;
}
