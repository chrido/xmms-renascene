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
#include "skinwin.h"
#include "spotify.h"
#include "spotifywin.h"
#include "outputwin.h"
#include "prefswin.h"

typedef enum {
    TIMER_ELAPSED,
    TIMER_REMAINING
} TimerMode;

typedef struct {
    gint player_x, player_y;
    gint scale_factor;
    gchar *skin;
    gint timer_mode;
    gchar *output_device;
    gint volume;
    gint balance;
    gboolean no_playlist_advance;
    gboolean sticky;
    gboolean doublesize;
    gboolean easy_move;
    gboolean playlist_visible;
    gboolean playlist_detached;
    gboolean shuffle;
    gboolean repeat;
    gint playlist_position;
    gboolean equalizer_visible;
    gboolean equalizer_detached;
    gboolean equalizer_active;
    gboolean equalizer_auto;
    gint equalizer_preamp_pos;
    gint equalizer_band_pos[10];
    gboolean convert_underscore;
    gboolean convert_twenty;
    gboolean show_numbers_in_pl;
    gchar *playlist_font;
    gchar *mainwin_font;
    gchar *title_format;
    gint vis_mode;
    gint vis_analyzer_mode;
    gint vis_analyzer_style;
    gint vis_scope_mode;
    gboolean vis_peaks_enabled;
    gdouble vis_falloff;
    gint vis_analyzer_falloff;
    gint vis_peaks_falloff;
    gint vis_vu_mode;
    gint vis_refresh_divisor;
} Config;

extern Config cfg;

extern GtkWidget *mainwin;
extern GtkWidget *mainwin_drawing_area;
extern GtkWidget *mainwin_container;

#define MAINWIN_WIDTH   275
#define MAINWIN_HEIGHT  116
#define PLAYER_HEIGHT MAINWIN_HEIGHT
#define PLAYER_WIDTH  MAINWIN_WIDTH

void mainwin_queue_draw(void);
void mainwin_update_attached_size(void);
void mainwin_update_panel_toggles(void);
void mainwin_sync_volume_balance(void);
void mainwin_set_doublesize(gboolean enabled);
void mainwin_set_sticky(gboolean enabled);
void mainwin_set_easy_move(gboolean enabled);
void mainwin_apply_preferences(void);
void mainwin_apply_visualization_preferences(void);
void draw_main_window(cairo_t *cr);
void save_config(void);

#endif /* XMMS_H */
