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
} Config;

extern Config cfg;

extern GtkWidget *mainwin;
extern GtkWidget *mainwin_drawing_area;

#define MAINWIN_WIDTH   275
#define MAINWIN_HEIGHT  116
#define PLAYER_HEIGHT MAINWIN_HEIGHT
#define PLAYER_WIDTH  MAINWIN_WIDTH

void mainwin_queue_draw(void);
void draw_main_window(cairo_t *cr);
void save_config(void);

#endif /* XMMS_H */
