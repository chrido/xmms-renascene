#include "xmms.h"

Player *player = NULL;

static GstBus *bus = NULL;

static gboolean
bus_callback(GstBus *b, GstMessage *msg, gpointer data)
{
    (void)b; (void)data;

    switch (GST_MESSAGE_TYPE(msg)) {
    case GST_MESSAGE_EOS:
        player->state = PLAYER_STOPPED;
        playlist_eof_reached();
        break;

    case GST_MESSAGE_ERROR: {
        GError *err = NULL;
        gchar *debug = NULL;
        gst_message_parse_error(msg, &err, &debug);
        g_warning("GStreamer error: %s\n%s", err->message,
                  debug ? debug : "");
        g_error_free(err);
        g_free(debug);
        player->state = PLAYER_STOPPED;
        break;
    }

    case GST_MESSAGE_DURATION_CHANGED:
        player->has_duration = FALSE;
        break;

    case GST_MESSAGE_TAG: {
        GstTagList *tags = NULL;
        gst_message_parse_tag(msg, &tags);
        /* Could extract title/artist here for display */
        gst_tag_list_unref(tags);
        break;
    }

    case GST_MESSAGE_ELEMENT: {
        const GstStructure *s = gst_message_get_structure(msg);
        if (g_strcmp0(gst_structure_get_name(s), "spectrum") == 0) {
            const GValue *magnitudes = gst_structure_get_value(s, "magnitude");
            if (magnitudes && GST_VALUE_HOLDS_LIST(magnitudes)) {
                gint num = gst_value_list_get_size(magnitudes);
                if (num > 75) num = 75;
                for (gint i = 0; i < num; i++) {
                    const GValue *mag = gst_value_list_get_value(magnitudes, i);
                    gfloat val = g_value_get_float(mag);
                    /* Normalize from dB (-80..0) to 0..1 */
                    val = (val + 80.0f) / 80.0f;
                    if (val < 0.0f) val = 0.0f;
                    if (val > 1.0f) val = 1.0f;
                    player->vis_data[i] = val;
                }
                player->vis_data_valid = TRUE;
            }
        }
        break;
    }

    default:
        break;
    }

    return TRUE;
}

void
player_init(void)
{
    if (!gst_is_initialized())
        gst_init(NULL, NULL);

    player = g_new0(Player, 1);
    player->state = PLAYER_STOPPED;
    player->volume = 100;
    player->balance = 0;

    player->pipeline = gst_element_factory_make("playbin", "player");
    if (!player->pipeline) {
        g_error("Failed to create GStreamer playbin element");
        return;
    }

    /* Add 10-band equalizer and spectrum analyzer */
    player->equalizer = gst_element_factory_make("equalizer-10bands", "eq");
    player->spectrum = gst_element_factory_make("spectrum", "spectrum");
    {
        GstElement *sink = gst_element_factory_make("autoaudiosink", "sink");
        GstElement *bin = gst_bin_new("audio-sink-bin");
        GstElement *convert = gst_element_factory_make("audioconvert", "convert");

        if (player->spectrum) {
            g_object_set(player->spectrum,
                         "bands", 75,
                         "threshold", -80,
                         "post-messages", TRUE,
                         "interval", (guint64)(50 * GST_MSECOND),
                         "message-magnitude", TRUE,
                         NULL);
        }

        if (player->equalizer && player->spectrum) {
            gst_bin_add_many(GST_BIN(bin), convert, player->equalizer,
                             player->spectrum, sink, NULL);
            gst_element_link_many(convert, player->equalizer,
                                  player->spectrum, sink, NULL);
        } else if (player->equalizer) {
            gst_bin_add_many(GST_BIN(bin), convert, player->equalizer,
                             sink, NULL);
            gst_element_link_many(convert, player->equalizer, sink, NULL);
        } else {
            gst_bin_add_many(GST_BIN(bin), convert, sink, NULL);
            gst_element_link_many(convert, sink, NULL);
        }

        GstPad *pad = gst_element_get_static_pad(convert, "sink");
        gst_element_add_pad(bin, gst_ghost_pad_new("sink", pad));
        gst_object_unref(pad);

        g_object_set(player->pipeline, "audio-sink", bin, NULL);
    }

    /* Disable video */
    g_object_set(player->pipeline, "video-sink",
                 gst_element_factory_make("fakesink", "fakevideo"), NULL);

    bus = gst_element_get_bus(player->pipeline);
    gst_bus_add_watch(bus, bus_callback, NULL);
    gst_object_unref(bus);
}

void
player_free(void)
{
    if (!player)
        return;

    if (player->pipeline) {
        gst_element_set_state(player->pipeline, GST_STATE_NULL);
        gst_object_unref(player->pipeline);
    }

    g_free(player);
    player = NULL;
}

void
player_play(const gchar *uri)
{
    if (!player || !player->pipeline)
        return;

    /* Handle Spotify URIs via Spotify Web API */
    if (g_str_has_prefix(uri, "spotify:")) {
        gst_element_set_state(player->pipeline, GST_STATE_NULL);
        spotify_play_track(uri, NULL, 0);
        player->state = PLAYER_PLAYING;
        player->has_duration = FALSE;
        return;
    }

    gst_element_set_state(player->pipeline, GST_STATE_NULL);

    g_object_set(player->pipeline, "uri", uri, NULL);

    /* Apply current volume */
    gdouble vol = player->volume / 100.0;
    g_object_set(player->pipeline, "volume", vol, NULL);

    gst_element_set_state(player->pipeline, GST_STATE_PLAYING);
    player->state = PLAYER_PLAYING;
    player->has_duration = FALSE;
}

void
player_stop(void)
{
    if (!player || !player->pipeline)
        return;

    gst_element_set_state(player->pipeline, GST_STATE_NULL);
    player->state = PLAYER_STOPPED;
    player->has_duration = FALSE;
}

void
player_pause(void)
{
    if (!player || !player->pipeline)
        return;

    gst_element_set_state(player->pipeline, GST_STATE_PAUSED);
    player->state = PLAYER_PAUSED;
}

void
player_unpause(void)
{
    if (!player || !player->pipeline)
        return;

    gst_element_set_state(player->pipeline, GST_STATE_PLAYING);
    player->state = PLAYER_PLAYING;
}

void
player_toggle_pause(void)
{
    if (player->state == PLAYER_PAUSED)
        player_unpause();
    else if (player->state == PLAYER_PLAYING)
        player_pause();
}

gboolean
player_is_playing(void)
{
    return player && player->state == PLAYER_PLAYING;
}

gboolean
player_is_paused(void)
{
    return player && player->state == PLAYER_PAUSED;
}

PlayerState
player_get_state(void)
{
    return player ? player->state : PLAYER_STOPPED;
}

gint64
player_get_position(void)
{
    if (!player || !player->pipeline)
        return 0;

    gint64 pos = 0;
    if (gst_element_query_position(player->pipeline, GST_FORMAT_TIME, &pos))
        return pos / GST_MSECOND;
    return 0;
}

gint64
player_get_duration(void)
{
    if (!player || !player->pipeline)
        return 0;

    if (!player->has_duration) {
        if (gst_element_query_duration(player->pipeline,
                                       GST_FORMAT_TIME,
                                       &player->duration))
            player->has_duration = TRUE;
        else
            return 0;
    }
    return player->duration / GST_MSECOND;
}

void
player_seek(gint64 ms)
{
    if (!player || !player->pipeline)
        return;

    gst_element_seek_simple(player->pipeline, GST_FORMAT_TIME,
                            GST_SEEK_FLAG_FLUSH | GST_SEEK_FLAG_KEY_UNIT,
                            ms * GST_MSECOND);
}

void
player_set_volume(gint percent)
{
    if (!player || !player->pipeline)
        return;

    player->volume = CLAMP(percent, 0, 100);
    gdouble vol = player->volume / 100.0;
    g_object_set(player->pipeline, "volume", vol, NULL);
}

gint
player_get_volume(void)
{
    return player ? player->volume : 0;
}

void
player_set_balance(gint balance)
{
    if (!player)
        return;
    player->balance = CLAMP(balance, -100, 100);
    /* GStreamer playbin doesn't have a built-in balance property,
       but we can use a panning element if needed */
}

void
player_set_equalizer(gfloat preamp, gfloat *bands)
{
    if (!player || !player->equalizer)
        return;

    /* GStreamer equalizer-10bands has band0..band9 properties
       Values are in dB (-24 to +12) */
    for (int i = 0; i < 10; i++) {
        gchar *prop = g_strdup_printf("band%d", i);
        gdouble val = (gdouble)(bands[i] + preamp);
        val = CLAMP(val, -24.0, 12.0);
        g_object_set(player->equalizer, prop, val, NULL);
        g_free(prop);
    }
}

void
player_update(void)
{
    /* Called from main loop timeout to update duration cache */
    if (player && player->state != PLAYER_STOPPED && !player->has_duration) {
        player_get_duration();
    }
}

gboolean
player_get_vis_data(gfloat *data, gint num_samples)
{
    if (!player || !player->vis_data_valid)
        return FALSE;

    gint count = MIN(num_samples, 75);
    memcpy(data, player->vis_data, count * sizeof(gfloat));
    player->vis_data_valid = FALSE;
    return TRUE;
}
