#include "xmms.h"
#include "mpris.h"
#include <gio/gio.h>

static GDBusConnection *dbus_conn = NULL;
static guint bus_name_id = 0;
static guint root_reg_id = 0;
static guint player_reg_id = 0;

static const gchar introspection_xml[] =
    "<node>"
    "  <interface name='org.mpris.MediaPlayer2'>"
    "    <method name='Raise'/>"
    "    <method name='Quit'/>"
    "    <property name='CanQuit' type='b' access='read'/>"
    "    <property name='CanRaise' type='b' access='read'/>"
    "    <property name='HasTrackList' type='b' access='read'/>"
    "    <property name='Identity' type='s' access='read'/>"
    "    <property name='DesktopEntry' type='s' access='read'/>"
    "    <property name='SupportedUriSchemes' type='as' access='read'/>"
    "    <property name='SupportedMimeTypes' type='as' access='read'/>"
    "  </interface>"
    "  <interface name='org.mpris.MediaPlayer2.Player'>"
    "    <method name='Next'/>"
    "    <method name='Previous'/>"
    "    <method name='Pause'/>"
    "    <method name='PlayPause'/>"
    "    <method name='Stop'/>"
    "    <method name='Play'/>"
    "    <method name='Seek'>"
    "      <arg direction='in' name='Offset' type='x'/>"
    "    </method>"
    "    <method name='SetPosition'>"
    "      <arg direction='in' name='TrackId' type='o'/>"
    "      <arg direction='in' name='Position' type='x'/>"
    "    </method>"
    "    <method name='OpenUri'>"
    "      <arg direction='in' name='Uri' type='s'/>"
    "    </method>"
    "    <property name='PlaybackStatus' type='s' access='read'/>"
    "    <property name='Rate' type='d' access='readwrite'/>"
    "    <property name='Metadata' type='a{sv}' access='read'/>"
    "    <property name='Volume' type='d' access='readwrite'/>"
    "    <property name='Position' type='x' access='read'/>"
    "    <property name='CanGoNext' type='b' access='read'/>"
    "    <property name='CanGoPrevious' type='b' access='read'/>"
    "    <property name='CanPlay' type='b' access='read'/>"
    "    <property name='CanPause' type='b' access='read'/>"
    "    <property name='CanSeek' type='b' access='read'/>"
    "    <property name='CanControl' type='b' access='read'/>"
    "    <signal name='Seeked'>"
    "      <arg name='Position' type='x'/>"
    "    </signal>"
    "  </interface>"
    "</node>";

static GDBusNodeInfo *introspection_data = NULL;

/* Cached metadata for property emission */
static gchar *current_title = NULL;
static gchar *current_uri = NULL;
static gint64 current_length_us = 0;

static const gchar *
get_playback_status(void)
{
    switch (player_get_state()) {
    case PLAYER_PLAYING: return "Playing";
    case PLAYER_PAUSED:  return "Paused";
    default:             return "Stopped";
    }
}

static GVariant *
get_metadata(void)
{
    GVariantBuilder builder;
    g_variant_builder_init(&builder, G_VARIANT_TYPE("a{sv}"));

    gint pos = playlist_get_position();
    if (pos < 0) pos = 0;
    gchar *track_id = g_strdup_printf("/org/xmms/Track/%d", pos);
    g_variant_builder_add(&builder, "{sv}", "mpris:trackid",
                          g_variant_new_object_path(track_id));
    g_free(track_id);

    if (current_title)
        g_variant_builder_add(&builder, "{sv}", "xesam:title",
                              g_variant_new_string(current_title));

    if (current_uri)
        g_variant_builder_add(&builder, "{sv}", "xesam:url",
                              g_variant_new_string(current_uri));

    if (current_length_us > 0)
        g_variant_builder_add(&builder, "{sv}", "mpris:length",
                              g_variant_new_int64(current_length_us));

    return g_variant_builder_end(&builder);
}

static void
handle_root_method(GDBusConnection *conn, const gchar *sender,
                   const gchar *path, const gchar *iface,
                   const gchar *method, GVariant *params,
                   GDBusMethodInvocation *invocation, gpointer data)
{
    (void)conn; (void)sender; (void)path; (void)iface;
    (void)params; (void)data;

    if (g_strcmp0(method, "Raise") == 0) {
        if (mainwin)
            gtk_window_present(GTK_WINDOW(mainwin));
        g_dbus_method_invocation_return_value(invocation, NULL);
    } else if (g_strcmp0(method, "Quit") == 0) {
        g_dbus_method_invocation_return_value(invocation, NULL);
        exit(0);
    }
}

static GVariant *
handle_root_get_property(GDBusConnection *conn, const gchar *sender,
                         const gchar *path, const gchar *iface,
                         const gchar *property, GError **error,
                         gpointer data)
{
    (void)conn; (void)sender; (void)path; (void)iface;
    (void)error; (void)data;

    if (g_strcmp0(property, "CanQuit") == 0)
        return g_variant_new_boolean(TRUE);
    if (g_strcmp0(property, "CanRaise") == 0)
        return g_variant_new_boolean(TRUE);
    if (g_strcmp0(property, "HasTrackList") == 0)
        return g_variant_new_boolean(FALSE);
    if (g_strcmp0(property, "Identity") == 0)
        return g_variant_new_string("XMMS");
    if (g_strcmp0(property, "DesktopEntry") == 0)
        return g_variant_new_string("org.xmms.XMMS");
    if (g_strcmp0(property, "SupportedUriSchemes") == 0) {
        const gchar *schemes[] = { "file", "http", "https", NULL };
        return g_variant_new_strv(schemes, -1);
    }
    if (g_strcmp0(property, "SupportedMimeTypes") == 0) {
        const gchar *types[] = { "audio/mpeg", "audio/ogg", "audio/flac",
                                  "audio/x-wav", "audio/mp4", NULL };
        return g_variant_new_strv(types, -1);
    }
    return NULL;
}

static void
handle_player_method(GDBusConnection *conn, const gchar *sender,
                     const gchar *path, const gchar *iface,
                     const gchar *method, GVariant *params,
                     GDBusMethodInvocation *invocation, gpointer data)
{
    (void)conn; (void)sender; (void)path; (void)iface; (void)data;

    if (g_strcmp0(method, "Next") == 0)
        playlist_next();
    else if (g_strcmp0(method, "Previous") == 0)
        playlist_prev();
    else if (g_strcmp0(method, "Pause") == 0)
        player_pause();
    else if (g_strcmp0(method, "PlayPause") == 0)
        player_toggle_pause();
    else if (g_strcmp0(method, "Stop") == 0)
        player_stop();
    else if (g_strcmp0(method, "Play") == 0) {
        if (player_get_state() == PLAYER_PAUSED)
            player_unpause();
        else
            playlist_play();
    } else if (g_strcmp0(method, "Seek") == 0) {
        gint64 offset;
        g_variant_get(params, "(x)", &offset);
        gint64 pos = player_get_position() * 1000 + offset; /* us */
        player_seek(pos / 1000);
    } else if (g_strcmp0(method, "SetPosition") == 0) {
        const gchar *track_id;
        gint64 position;
        g_variant_get(params, "(&ox)", &track_id, &position);
        player_seek(position / 1000);
    } else if (g_strcmp0(method, "OpenUri") == 0) {
        const gchar *uri;
        g_variant_get(params, "(&s)", &uri);
        playlist_clear();
        playlist_add_uri(uri);
        playlist_play();
    }

    g_dbus_method_invocation_return_value(invocation, NULL);
}

static GVariant *
handle_player_get_property(GDBusConnection *conn, const gchar *sender,
                           const gchar *path, const gchar *iface,
                           const gchar *property, GError **error,
                           gpointer data)
{
    (void)conn; (void)sender; (void)path; (void)iface;
    (void)error; (void)data;

    if (g_strcmp0(property, "PlaybackStatus") == 0)
        return g_variant_new_string(get_playback_status());
    if (g_strcmp0(property, "Rate") == 0)
        return g_variant_new_double(1.0);
    if (g_strcmp0(property, "Metadata") == 0)
        return get_metadata();
    if (g_strcmp0(property, "Volume") == 0)
        return g_variant_new_double(player_get_volume() / 100.0);
    if (g_strcmp0(property, "Position") == 0)
        return g_variant_new_int64(player_get_position() * 1000);
    if (g_strcmp0(property, "CanGoNext") == 0)
        return g_variant_new_boolean(TRUE);
    if (g_strcmp0(property, "CanGoPrevious") == 0)
        return g_variant_new_boolean(TRUE);
    if (g_strcmp0(property, "CanPlay") == 0)
        return g_variant_new_boolean(TRUE);
    if (g_strcmp0(property, "CanPause") == 0)
        return g_variant_new_boolean(TRUE);
    if (g_strcmp0(property, "CanSeek") == 0)
        return g_variant_new_boolean(TRUE);
    if (g_strcmp0(property, "CanControl") == 0)
        return g_variant_new_boolean(TRUE);
    return NULL;
}

static gboolean
handle_player_set_property(GDBusConnection *conn, const gchar *sender,
                           const gchar *path, const gchar *iface,
                           const gchar *property, GVariant *value,
                           GError **error, gpointer data)
{
    (void)conn; (void)sender; (void)path; (void)iface;
    (void)error; (void)data;

    if (g_strcmp0(property, "Volume") == 0) {
        gdouble vol = g_variant_get_double(value);
        player_set_volume((gint)(vol * 100));
        return TRUE;
    }
    return FALSE;
}

static const GDBusInterfaceVTable root_vtable = {
    handle_root_method, handle_root_get_property, NULL
};

static const GDBusInterfaceVTable player_vtable = {
    handle_player_method, handle_player_get_property,
    handle_player_set_property
};

static void
on_bus_acquired(GDBusConnection *conn, const gchar *name, gpointer data)
{
    (void)name; (void)data;
    dbus_conn = conn;

    root_reg_id = g_dbus_connection_register_object(
        conn, "/org/mpris/MediaPlayer2",
        introspection_data->interfaces[0],
        &root_vtable, NULL, NULL, NULL);

    player_reg_id = g_dbus_connection_register_object(
        conn, "/org/mpris/MediaPlayer2",
        introspection_data->interfaces[1],
        &player_vtable, NULL, NULL, NULL);
}

static void
emit_properties_changed(const gchar *iface, GVariantBuilder *changed)
{
    if (!dbus_conn)
        return;

    g_dbus_connection_emit_signal(
        dbus_conn, NULL, "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties", "PropertiesChanged",
        g_variant_new("(sa{sv}as)", iface,
                       changed, NULL),
        NULL);
}

void
mpris_init(void)
{
    introspection_data = g_dbus_node_info_new_for_xml(introspection_xml, NULL);

    bus_name_id = g_bus_own_name(
        G_BUS_TYPE_SESSION,
        "org.mpris.MediaPlayer2.xmms",
        G_BUS_NAME_OWNER_FLAGS_NONE,
        on_bus_acquired, NULL, NULL, NULL, NULL);
}

void
mpris_free(void)
{
    if (bus_name_id)
        g_bus_unown_name(bus_name_id);
    if (introspection_data)
        g_dbus_node_info_unref(introspection_data);
    g_free(current_title);
    g_free(current_uri);
    current_title = NULL;
    current_uri = NULL;
}

void
mpris_update_metadata(const gchar *title, const gchar *uri, gint64 length_us)
{
    g_free(current_title);
    g_free(current_uri);
    current_title = g_strdup(title);
    current_uri = g_strdup(uri);
    current_length_us = length_us;

    if (!dbus_conn)
        return;

    GVariantBuilder builder;
    g_variant_builder_init(&builder, G_VARIANT_TYPE("a{sv}"));
    g_variant_builder_add(&builder, "{sv}", "Metadata", get_metadata());

    emit_properties_changed("org.mpris.MediaPlayer2.Player", &builder);
}

void
mpris_update_playback_status(void)
{
    if (!dbus_conn)
        return;

    GVariantBuilder builder;
    g_variant_builder_init(&builder, G_VARIANT_TYPE("a{sv}"));
    g_variant_builder_add(&builder, "{sv}", "PlaybackStatus",
                          g_variant_new_string(get_playback_status()));

    emit_properties_changed("org.mpris.MediaPlayer2.Player", &builder);
}

void
mpris_update_position(gint64 position_us)
{
    (void)position_us;
    /* Position doesn't use PropertiesChanged, it uses Seeked signal
       We'll emit that when actual seeking happens */
}
