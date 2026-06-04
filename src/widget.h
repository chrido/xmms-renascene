#ifndef WIDGET_H
#define WIDGET_H

#include <gtk/gtk.h>
#include <cairo.h>

typedef struct _Widget Widget;

typedef void (*WidgetDrawFunc)(Widget *w, cairo_t *cr);
typedef void (*WidgetButtonFunc)(Widget *w, gint x, gint y, gint button);
typedef void (*WidgetMotionFunc)(Widget *w, gint x, gint y);

struct _Widget {
    gint x, y, width, height;
    gboolean visible;
    gboolean redraw;
    WidgetDrawFunc draw;
    WidgetButtonFunc button_press;
    WidgetButtonFunc button_release;
    WidgetMotionFunc motion;
};

/* Push Button - momentary action */
typedef struct {
    Widget w;
    gint nx, ny;    /* normal state skin coords */
    gint px, py;    /* pressed state skin coords */
    gboolean pressed, inside;
    void (*push_cb)(void);
    gint skin_index;
} PButton;

/* Toggle Button - stays on/off */
typedef struct {
    Widget w;
    gint nux, nuy;  /* normal unselected */
    gint pux, puy;  /* pressed unselected */
    gint nsx, nsy;  /* normal selected */
    gint psx, psy;  /* pressed selected */
    gboolean pressed, inside, selected;
    void (*push_cb)(gboolean selected);
    gint skin_index;
} TButton;

/* Text display with scrolling */
typedef struct {
    Widget w;
    cairo_surface_t *rendered;
    gchar *text;
    gchar *original_text;
    gchar *rendered_text;
    gint rendered_width;
    gint offset;
    gboolean scroll_enabled;
    gboolean is_scrollable;
    gboolean is_dragging;
    gint drag_x;
    guint scroll_tag;
    gint skin_index;
    gint skin_id;
} TextBox;

/* Horizontal Slider */
typedef struct {
    Widget w;
    gint frame_height, frame_offset;
    gint knob_nx, knob_ny;
    gint knob_px, knob_py;
    gint knob_width, knob_height;
    gint position;
    gint min, max;
    gboolean pressed;
    gint press_offset;
    gint (*frame_cb)(gint pos);
    void (*motion_cb)(gint pos);
    void (*release_cb)(gint pos);
    gint skin_index;
} HSlider;

/* Numeric time display */
typedef struct {
    Widget w;
    gint value;
    gint skin_index;
} Number;

/* Visualization display */
typedef enum {
    VIS_MODE_ANALYZER,
    VIS_MODE_SCOPE,
    VIS_MODE_OFF
} VisMode;

typedef enum {
    VIS_ANALYZER_BARS,
    VIS_ANALYZER_LINES
} VisAnalyzerStyle;

typedef struct {
    Widget w;
    gfloat data[75];
    gfloat peak[75];
    gfloat peak_speed[75];
    VisMode mode;
    VisAnalyzerStyle analyzer_style;
    gboolean peaks_enabled;
    gfloat falloff;
} Vis;

/* Mono/Stereo indicator */
typedef struct {
    Widget w;
    gint nchannels;
    gint skin_index;
} MonoStereo;

/* Play status indicator */
typedef struct {
    Widget w;
    gint status; /* 0=stop, 1=pause, 2=play */
    gint skin_index;
} PlayStatus;

/* Simple/invisible button (hit area only) */
typedef struct {
    Widget w;
    gboolean pressed, inside;
    void (*push_cb)(void);
} SButton;

/* Widget list management */
void widget_list_add(GList **list, Widget *w);
void widget_list_draw(GList *list, cairo_t *cr);
Widget *widget_list_find(GList *list, gint x, gint y);

gboolean widget_inside(Widget *w, gint x, gint y);
void widget_queue_draw(Widget *w);

/* Widget constructors */
PButton *pbutton_new(GList **list, gint x, gint y, gint w, gint h,
                     gint nx, gint ny, gint px, gint py,
                     void (*cb)(void), gint skin_index);

TButton *tbutton_new(GList **list, gint x, gint y, gint w, gint h,
                     gint nux, gint nuy, gint pux, gint puy,
                     gint nsx, gint nsy, gint psx, gint psy,
                     void (*cb)(gboolean), gint skin_index);

TextBox *textbox_new(GList **list, gint x, gint y, gint w,
                     gboolean scroll, gint skin_index);

HSlider *hslider_new(GList **list, gint x, gint y, gint w, gint h,
                     gint knob_nx, gint knob_ny, gint knob_px, gint knob_py,
                     gint knob_w, gint knob_h,
                     gint frame_height, gint frame_offset,
                     gint min, gint max,
                     gint (*frame_cb)(gint),
                     void (*motion_cb)(gint),
                     void (*release_cb)(gint),
                     gint skin_index);

Number *number_new(GList **list, gint x, gint y, gint skin_index);

Vis *vis_new(GList **list, gint x, gint y, gint w);

MonoStereo *monostereo_new(GList **list, gint x, gint y, gint skin_index);

PlayStatus *playstatus_new(GList **list, gint x, gint y, gint skin_index);

SButton *sbutton_new(GList **list, gint x, gint y, gint w, gint h,
                     void (*cb)(void));

void textbox_set_text(TextBox *tb, const gchar *text);
void number_set_value(Number *n, gint value);
void monostereo_set_channels(MonoStereo *ms, gint nch);
void vis_set_data(Vis *vis, gfloat *data, gint num);
void vis_set_mode(Vis *vis, VisMode mode);
void vis_set_analyzer_style(Vis *vis, VisAnalyzerStyle style);
void vis_set_peaks_enabled(Vis *vis, gboolean enabled);
void vis_set_falloff(Vis *vis, gfloat falloff);
void playstatus_set_status(PlayStatus *ps, gint status);
void hslider_set_position(HSlider *hs, gint pos);
void tbutton_set_toggled(TButton *tb, gboolean toggled);

#endif /* WIDGET_H */
