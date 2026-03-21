#ifndef XMMS_H
#define XMMS_H

#include <gtk/gtk.h>
#include <cairo.h>
#include <gst/gst.h>
#include <string.h>
#include <stdlib.h>
#include <ctype.h>

#include "config.h"
#include "skin.h"
#include "widget.h"
#include "player.h"
#include "playlist.h"
#include "util.h"
#include "playlistwin.h"
#include "equalizerwin.h"
#include "mpris.h"

typedef enum {
    TIMER_ELAPSED,
    TIMER_REMAINING
} TimerMode;

typedef enum {
    VIS_ANALYZER,
    VIS_SCOPE,
    VIS_OFF
} VisType;

typedef enum {
    ANALYZER_NORMAL,
    ANALYZER_FIRE,
    ANALYZER_VLINES
} AnalyzerMode;

typedef struct {
    gint player_x, player_y;
    gint scale_factor;
    gboolean shuffle, repeat;
    gboolean autoscroll;
    gboolean analyzer_peaks;
    gboolean player_shaded;
    gboolean save_window_position;
    gboolean smooth_title_scroll;
    gboolean show_numbers_in_pl;
    gboolean dim_titlebar;
    gboolean always_on_top;
    gfloat equalizer_preamp;
    gfloat equalizer_bands[10];
    gchar *skin;
    gchar *filesel_path;
    gint timer_mode;
    gint vis_type;
    gint analyzer_mode;
    gint vis_refresh;
    gint analyzer_falloff;
    gint peaks_falloff;
    gint playlist_position;
} Config;

extern Config cfg;

extern GtkWidget *mainwin;
extern GtkWidget *mainwin_drawing_area;

#define MAINWIN_WIDTH   275
#define MAINWIN_HEIGHT  116
#define MAINWIN_SHADED_HEIGHT 14

#define PLAYER_HEIGHT (cfg.player_shaded ? MAINWIN_SHADED_HEIGHT : MAINWIN_HEIGHT)
#define PLAYER_WIDTH  MAINWIN_WIDTH

void mainwin_queue_draw(void);
void draw_main_window(cairo_t *cr);

#endif /* XMMS_H */
