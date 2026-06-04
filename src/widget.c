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

static void
vis_draw(Widget *w, cairo_t *cr)
{
    Vis *vis = (Vis *)w;

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

    if (vis->mode == VIS_MODE_OFF) {
        cairo_restore(cr);
        return;
    }

    if (vis->mode == VIS_MODE_SCOPE) {
        cairo_set_source_rgb(cr,
            skin->vis_color[18][0] / 255.0,
            skin->vis_color[18][1] / 255.0,
            skin->vis_color[18][2] / 255.0);
        cairo_set_line_width(cr, 1.0);
        for (gint i = 0; i < 75 && i < w->width; i++) {
            gdouble y = w->y + 8.0 - (vis->data[i] - 0.5) * 14.0;
            if (i == 0)
                cairo_move_to(cr, w->x + i, y);
            else
                cairo_line_to(cr, w->x + i, y);
        }
        cairo_stroke(cr);
        cairo_restore(cr);
        return;
    }

    /* Draw analyzer bars */
    for (gint i = 0; i < 75 && i < w->width; i++) {
        gint h = (gint)(vis->data[i] * 16.0);
        if (h <= 0) continue;
        if (h > 16) h = 16;

        gint y_step = vis->analyzer_style == VIS_ANALYZER_LINES ? 2 : 1;
        for (gint y = 16 - h; y < 16; y += y_step) {
            gint color_idx = (16 - y) + 2;
            if (color_idx >= 24) color_idx = 23;
            if (color_idx < 2) color_idx = 2;

            cairo_set_source_rgb(cr,
                skin->vis_color[color_idx][0] / 255.0,
                skin->vis_color[color_idx][1] / 255.0,
                skin->vis_color[color_idx][2] / 255.0);
            cairo_rectangle(cr, w->x + i, w->y + y, 1, 1);
            cairo_fill(cr);
        }

        /* Draw peak */
        if (vis->peaks_enabled && vis->peak[i] > 0) {
            gint peak_y = 16 - (gint)(vis->peak[i] * 16.0);
            if (peak_y >= 0 && peak_y < 16) {
                cairo_set_source_rgb(cr,
                    skin->vis_color[23][0] / 255.0,
                    skin->vis_color[23][1] / 255.0,
                    skin->vis_color[23][2] / 255.0);
                cairo_rectangle(cr, w->x + i, w->y + peak_y, 1, 1);
                cairo_fill(cr);
            }
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
    vis->peaks_enabled = TRUE;
    vis->falloff = 0.03f;
    widget_list_add(list, (Widget *)vis);
    return vis;
}

void
vis_set_data(Vis *vis, gfloat *data, gint num)
{
    gint count = MIN(num, 75);
    for (gint i = 0; i < count; i++) {
        vis->data[i] = data[i];
        if (data[i] > vis->peak[i])
            vis->peak[i] = data[i];
        else
            vis->peak[i] = MAX(0.0f, vis->peak[i] - vis->falloff);
    }
}

void
vis_set_mode(Vis *vis, VisMode mode)
{
    if (!vis)
        return;
    vis->mode = mode;
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
vis_set_falloff(Vis *vis, gfloat falloff)
{
    if (!vis)
        return;
    vis->falloff = CLAMP(falloff, 0.005f, 0.2f);
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
