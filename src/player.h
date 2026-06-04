#ifndef PLAYER_H
#define PLAYER_H

#include <gst/gst.h>

typedef enum {
    PLAYER_STOPPED,
    PLAYER_PLAYING,
    PLAYER_PAUSED
} PlayerState;

typedef struct {
    GstElement *pipeline;
    GstElement *equalizer;
    GstElement *spectrum;
    GstElement *audio_convert;
    PlayerState state;
    gint64 duration;
    gboolean has_duration;
    gint bitrate;    /* bits per second, -1 for VBR/unknown */
    gint frequency;  /* Hz */
    gint channels;
    gint volume;     /* 0-100 */
    gint balance;    /* -100 to 100 */
    gfloat vis_data[75];
    gboolean vis_data_valid;
} Player;

extern Player *player;

void player_init(void);
void player_free(void);

void player_play(const gchar *uri);
void player_stop(void);
void player_pause(void);
void player_unpause(void);
void player_toggle_pause(void);

gboolean player_is_playing(void);
gboolean player_is_paused(void);
PlayerState player_get_state(void);
void player_get_song_info(gint *bitrate, gint *frequency, gint *channels);

gint64 player_get_position(void);   /* in milliseconds */
gint64 player_get_duration(void);   /* in milliseconds */
void player_seek(gint64 ms);

void player_set_volume(gint percent);
gint player_get_volume(void);
void player_set_balance(gint balance);
gint player_get_balance(void);

void player_set_equalizer(gfloat preamp, gfloat *bands);

/* Called from main loop to update UI */
void player_update(void);

/* Get visualization data (PCM) */
gboolean player_get_vis_data(gfloat *data, gint num_samples);

/* Output device selection */
typedef struct {
    gchar *id;          /* device path/name for GstDeviceMonitor */
    gchar *display_name;
    gchar *class_name;  /* e.g. "Audio/Sink" */
    gboolean is_network;
} OutputDevice;

GList *player_get_output_devices(void);
void   player_set_output_device(const gchar *device_id);
const gchar *player_get_output_device(void);
void   output_device_free(OutputDevice *dev);
void   output_device_list_free(GList *list);

#endif /* PLAYER_H */
