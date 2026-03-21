#include "xmms.h"
#include "spotify.h"

#include <libsoup/soup.h>
#include <json-glib/json-glib.h>

#define SPOTIFY_AUTH_URL    "https://accounts.spotify.com/authorize"
#define SPOTIFY_TOKEN_URL   "https://accounts.spotify.com/api/token"
#define SPOTIFY_API_BASE    "https://api.spotify.com/v1"
#define REDIRECT_PORT       8391
#define REDIRECT_URI        "http://127.0.0.1:8391/callback"
#define SCOPES              "user-read-playback-state user-modify-playback-state playlist-read-private playlist-read-collaborative"
#define CLIENT_ID           "60687ec3a8e1407cb86dc18f14030fff"

static gchar *access_token = NULL;
static gchar *refresh_token = NULL;
static gint64  token_expiry = 0;

static gchar *code_verifier = NULL;

static SoupSession *session = NULL;

/* ---- Config file ---- */

static gchar *
get_spotify_config_path(void)
{
    return g_build_filename(g_get_user_config_dir(), "xmms", "spotify.conf", NULL);
}

static void
load_spotify_config(void)
{
    gchar *path = get_spotify_config_path();
    GKeyFile *kf = g_key_file_new();

    if (g_key_file_load_from_file(kf, path, 0, NULL))
        refresh_token = g_key_file_get_string(kf, "spotify", "refresh_token", NULL);

    g_key_file_free(kf);
    g_free(path);
}

static void
save_spotify_config(void)
{
    gchar *path = get_spotify_config_path();
    gchar *dir = g_path_get_dirname(path);
    g_mkdir_with_parents(dir, 0755);
    g_free(dir);

    GKeyFile *kf = g_key_file_new();
    if (refresh_token)
        g_key_file_set_string(kf, "spotify", "refresh_token", refresh_token);

    gchar *data = g_key_file_to_data(kf, NULL, NULL);
    g_file_set_contents(path, data, -1, NULL);
    g_free(data);
    g_key_file_free(kf);
    g_free(path);
}

/* ---- PKCE helpers ---- */

static gchar *
generate_random_string(gint length)
{
    static const char charset[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    gchar *str = g_malloc(length + 1);
    for (gint i = 0; i < length; i++)
        str[i] = charset[g_random_int_range(0, sizeof(charset) - 1)];
    str[length] = '\0';
    return str;
}

static gchar *
base64url_encode(const guchar *data, gsize len)
{
    gchar *b64 = g_base64_encode(data, len);
    /* Convert to base64url: replace + with -, / with _, strip = */
    for (gchar *p = b64; *p; p++) {
        if (*p == '+') *p = '-';
        else if (*p == '/') *p = '_';
    }
    /* Strip trailing = */
    gchar *eq = strchr(b64, '=');
    if (eq) *eq = '\0';
    return b64;
}

static gchar *
generate_code_challenge(const gchar *verifier)
{
    GChecksum *cs = g_checksum_new(G_CHECKSUM_SHA256);
    g_checksum_update(cs, (const guchar *)verifier, strlen(verifier));

    guchar digest[32];
    gsize digest_len = sizeof(digest);
    g_checksum_get_digest(cs, digest, &digest_len);
    g_checksum_free(cs);

    return base64url_encode(digest, digest_len);
}

/* ---- Token management ---- */

static gboolean
parse_token_response(const gchar *body)
{
    JsonParser *parser = json_parser_new();
    if (!json_parser_load_from_data(parser, body, -1, NULL)) {
        g_object_unref(parser);
        return FALSE;
    }

    JsonObject *obj = json_node_get_object(json_parser_get_root(parser));

    g_free(access_token);
    access_token = g_strdup(json_object_get_string_member(obj, "access_token"));

    gint64 expires_in = json_object_get_int_member(obj, "expires_in");
    token_expiry = g_get_real_time() / G_USEC_PER_SEC + expires_in - 60;

    if (json_object_has_member(obj, "refresh_token")) {
        g_free(refresh_token);
        refresh_token = g_strdup(json_object_get_string_member(obj, "refresh_token"));
        save_spotify_config();
    }

    g_object_unref(parser);
    return access_token != NULL;
}

static gboolean
refresh_access_token(void)
{
    if (!refresh_token)
        return FALSE;

    gchar *body = g_strdup_printf(
        "grant_type=refresh_token&refresh_token=%s&client_id=%s",
        refresh_token, CLIENT_ID);

    SoupMessage *msg = soup_message_new("POST", SPOTIFY_TOKEN_URL);
    GBytes *request_body = g_bytes_new_take(body, strlen(body));
    soup_message_set_request_body_from_bytes(msg, "application/x-www-form-urlencoded",
                                              request_body);

    GBytes *response = soup_session_send_and_read(session, msg, NULL, NULL);
    gboolean ok = FALSE;

    if (response) {
        gsize len;
        const gchar *data = g_bytes_get_data(response, &len);
        if (soup_message_get_status(msg) == 200)
            ok = parse_token_response(data);
        g_bytes_unref(response);
    }

    g_bytes_unref(request_body);
    g_object_unref(msg);
    return ok;
}

static gboolean
ensure_token(void)
{
    if (access_token && g_get_real_time() / G_USEC_PER_SEC < token_expiry)
        return TRUE;
    return refresh_access_token();
}

/* ---- OAuth callback server ---- */

typedef struct {
    GtkWindow *parent;
    GMainLoop *loop;
    gboolean   success;
} AuthContext;

static void
auth_callback_handler(SoupServer *server, SoupServerMessage *msg,
                      const char *path, GHashTable *query,
                      gpointer data)
{
    (void)server; (void)path;
    AuthContext *ctx = data;

    if (!query || !g_hash_table_contains(query, "code")) {
        soup_server_message_set_status(msg, 400, NULL);
        soup_server_message_set_response(msg, "text/plain",
                                          SOUP_MEMORY_STATIC,
                                          "Missing code parameter", 22);
        return;
    }

    const gchar *code = g_hash_table_lookup(query, "code");

    /* Exchange code for token */
    gchar *body = g_strdup_printf(
        "grant_type=authorization_code&code=%s&redirect_uri=%s"
        "&client_id=%s&code_verifier=%s",
        code, REDIRECT_URI, CLIENT_ID, code_verifier);

    SoupMessage *token_msg = soup_message_new("POST", SPOTIFY_TOKEN_URL);
    GBytes *request_body = g_bytes_new_take(body, strlen(body));
    soup_message_set_request_body_from_bytes(token_msg,
        "application/x-www-form-urlencoded", request_body);

    GBytes *response = soup_session_send_and_read(session, token_msg, NULL, NULL);

    if (response) {
        gsize len;
        const gchar *resp_data = g_bytes_get_data(response, &len);
        if (soup_message_get_status(token_msg) == 200)
            ctx->success = parse_token_response(resp_data);
        g_bytes_unref(response);
    }

    g_bytes_unref(request_body);
    g_object_unref(token_msg);

    const gchar *html = ctx->success
        ? "<html><body><h2>XMMS Resuscitated: Spotify connected!</h2>"
          "<p>You can close this tab.</p></body></html>"
        : "<html><body><h2>XMMS Resuscitated: Authentication failed</h2></body></html>";

    soup_server_message_set_status(msg, 200, NULL);
    soup_server_message_set_response(msg, "text/html",
                                      SOUP_MEMORY_STATIC,
                                      html, strlen(html));

    /* Quit the loop after responding */
    if (ctx->loop)
        g_main_loop_quit(ctx->loop);
}

/* ---- Public API ---- */

void
spotify_init(void)
{
    session = soup_session_new();
    load_spotify_config();
}

void
spotify_free(void)
{
    g_clear_object(&session);
    g_free(access_token);
    g_free(refresh_token);
    g_free(code_verifier);
    access_token = NULL;
    refresh_token = NULL;
    code_verifier = NULL;
}

gboolean
spotify_is_authenticated(void)
{
    return ensure_token();
}

void
spotify_authenticate(GtkWindow *parent)
{
    /* Generate PKCE verifier and challenge */
    g_free(code_verifier);
    code_verifier = generate_random_string(64);
    gchar *challenge = generate_code_challenge(code_verifier);

    /* Start local HTTP server for callback */
    GError *error = NULL;
    SoupServer *server = soup_server_new(NULL, NULL);
    AuthContext ctx = { .parent = parent, .loop = NULL, .success = FALSE };

    soup_server_add_handler(server, "/callback", auth_callback_handler,
                            &ctx, NULL);

    if (!soup_server_listen_local(server, REDIRECT_PORT, 0, &error)) {
        g_warning("Failed to start auth server: %s", error->message);
        g_error_free(error);
        g_object_unref(server);
        g_free(challenge);
        return;
    }

    /* Build authorization URL and open browser */
    gchar *auth_url = g_strdup_printf(
        "%s?response_type=code&client_id=%s&scope=%s"
        "&redirect_uri=%s&code_challenge_method=S256&code_challenge=%s",
        SPOTIFY_AUTH_URL, CLIENT_ID, SCOPES, REDIRECT_URI, challenge);
    g_free(challenge);

    /* URL-encode spaces in scope */
    gchar *encoded_url = g_uri_escape_string(auth_url, ":/?#[]@!$&'()*+,;=-.", FALSE);
    g_free(auth_url);

    /* Open in default browser */
    GtkUriLauncher *launcher = gtk_uri_launcher_new(encoded_url);
    gtk_uri_launcher_launch(launcher, parent, NULL, NULL, NULL);
    g_object_unref(launcher);
    g_free(encoded_url);

    /* Run a temporary main loop until callback is received (with timeout) */
    ctx.loop = g_main_loop_new(NULL, FALSE);

    /* 2-minute timeout */
    guint timeout_id = g_timeout_add_seconds(120, (GSourceFunc)g_main_loop_quit,
                                              ctx.loop);
    g_main_loop_run(ctx.loop);
    g_source_remove(timeout_id);
    g_main_loop_unref(ctx.loop);

    soup_server_disconnect(server);
    g_object_unref(server);

    if (ctx.success)
        g_message("Spotify authentication successful");
    else
        g_warning("Spotify authentication failed or timed out");
}

/* ---- API helpers ---- */

static GBytes *
spotify_api_get(const gchar *endpoint)
{
    if (!ensure_token())
        return NULL;

    gchar *url = g_strdup_printf("%s%s", SPOTIFY_API_BASE, endpoint);
    SoupMessage *msg = soup_message_new("GET", url);
    g_free(url);

    SoupMessageHeaders *headers = soup_message_get_request_headers(msg);
    gchar *auth = g_strdup_printf("Bearer %s", access_token);
    soup_message_headers_replace(headers, "Authorization", auth);
    g_free(auth);

    GBytes *response = soup_session_send_and_read(session, msg, NULL, NULL);
    guint status = soup_message_get_status(msg);
    g_object_unref(msg);

    if (status != 200) {
        if (response) {
            gsize elen;
            const gchar *ebody = g_bytes_get_data(response, &elen);
            g_warning("Spotify API %s returned %u: %.*s",
                      endpoint, status, (int)MIN(elen, 300), ebody);
            g_bytes_unref(response);
        } else {
            g_warning("Spotify API %s returned %u (no body)", endpoint, status);
        }
        return NULL;
    }

    return response;
}

static gboolean
spotify_api_put(const gchar *endpoint, const gchar *body)
{
    if (!ensure_token())
        return FALSE;

    gchar *url = g_strdup_printf("%s%s", SPOTIFY_API_BASE, endpoint);
    SoupMessage *msg = soup_message_new("PUT", url);
    g_free(url);

    SoupMessageHeaders *headers = soup_message_get_request_headers(msg);
    gchar *auth = g_strdup_printf("Bearer %s", access_token);
    soup_message_headers_replace(headers, "Authorization", auth);
    g_free(auth);

    if (body) {
        GBytes *request_body = g_bytes_new(body, strlen(body));
        soup_message_set_request_body_from_bytes(msg, "application/json",
                                                  request_body);
        g_bytes_unref(request_body);
    }

    GBytes *response = soup_session_send_and_read(session, msg, NULL, NULL);
    guint status = soup_message_get_status(msg);
    g_object_unref(msg);

    if (status < 200 || status >= 300) {
        if (response) {
            gsize elen;
            const gchar *ebody = g_bytes_get_data(response, &elen);
            g_warning("Spotify PUT %s returned %u: %.*s",
                      endpoint, status, (int)MIN(elen, 300), ebody);
            g_bytes_unref(response);
        }
        return FALSE;
    }

    if (response) g_bytes_unref(response);
    return TRUE;
}

static gboolean
spotify_api_post(const gchar *endpoint, const gchar *body)
{
    if (!ensure_token())
        return FALSE;

    gchar *url = g_strdup_printf("%s%s", SPOTIFY_API_BASE, endpoint);
    SoupMessage *msg = soup_message_new("POST", url);
    g_free(url);

    SoupMessageHeaders *headers = soup_message_get_request_headers(msg);
    gchar *auth = g_strdup_printf("Bearer %s", access_token);
    soup_message_headers_replace(headers, "Authorization", auth);
    g_free(auth);

    if (body) {
        GBytes *request_body = g_bytes_new(body, strlen(body));
        soup_message_set_request_body_from_bytes(msg, "application/json",
                                                  request_body);
        g_bytes_unref(request_body);
    }

    GBytes *response = soup_session_send_and_read(session, msg, NULL, NULL);
    guint status = soup_message_get_status(msg);
    g_object_unref(msg);

    if (response) g_bytes_unref(response);
    return (status >= 200 && status < 300);
}

/* ---- Playlists ---- */

void
spotify_playlist_free(SpotifyPlaylist *p)
{
    if (!p) return;
    g_free(p->id);
    g_free(p->name);
    g_free(p->uri);
    g_free(p);
}

void
spotify_playlist_list_free(GList *list)
{
    g_list_free_full(list, (GDestroyNotify)spotify_playlist_free);
}

void
spotify_track_free(SpotifyTrack *t)
{
    if (!t) return;
    g_free(t->id);
    g_free(t->name);
    g_free(t->artist);
    g_free(t->album);
    g_free(t->uri);
    g_free(t);
}

void
spotify_track_list_free(GList *list)
{
    g_list_free_full(list, (GDestroyNotify)spotify_track_free);
}

void
spotify_get_playlists(SpotifyPlaylistsCb cb, gpointer data)
{
    GList *result = NULL;
    gint offset = 0;

    while (TRUE) {
        gchar *endpoint = g_strdup_printf("/me/playlists?limit=50&offset=%d", offset);
        GBytes *response = spotify_api_get(endpoint);
        g_free(endpoint);

        if (!response)
            break;

        gsize len;
        const gchar *body = g_bytes_get_data(response, &len);

        JsonParser *parser = json_parser_new();

        if (!json_parser_load_from_data(parser, body, len, NULL)) {
            g_object_unref(parser);
            g_bytes_unref(response);
            break;
        }

        JsonObject *root = json_node_get_object(json_parser_get_root(parser));
        JsonArray *items = json_object_get_array_member(root, "items");
        guint count = json_array_get_length(items);

        for (guint i = 0; i < count; i++) {
            JsonObject *item = json_array_get_object_element(items, i);
            SpotifyPlaylist *pl = g_new0(SpotifyPlaylist, 1);
            pl->id = g_strdup(json_object_get_string_member(item, "id"));
            pl->name = g_strdup(json_object_get_string_member(item, "name"));
            pl->uri = g_strdup(json_object_get_string_member(item, "uri"));

            /* Track count: "tracks" (old API) or "items" (Feb 2026 API) */
            const gchar *count_key = json_object_has_member(item, "tracks")
                                     ? "tracks" : "items";
            if (json_object_has_member(item, count_key)) {
                JsonObject *count_obj = json_object_get_object_member(item, count_key);
                if (count_obj && json_object_has_member(count_obj, "total"))
                    pl->total_tracks = json_object_get_int_member(count_obj, "total");
            }

            result = g_list_prepend(result, pl);
        }

        gint64 total = json_object_get_int_member(root, "total");
        g_object_unref(parser);
        g_bytes_unref(response);

        offset += count;
        if (offset >= total || count == 0)
            break;
    }

    result = g_list_reverse(result);
    if (cb)
        cb(result, data);
}

void
spotify_get_playlist_tracks(const gchar *playlist_id,
                             SpotifyTracksCb cb, gpointer data)
{
    GList *result = NULL;
    gint offset = 0;

    while (TRUE) {
        gchar *endpoint = g_strdup_printf(
            "/playlists/%s/items?limit=100&offset=%d",
            playlist_id, offset);
        GBytes *response = spotify_api_get(endpoint);
        g_free(endpoint);

        if (!response)
            break;

        gsize len;
        const gchar *body = g_bytes_get_data(response, &len);

        JsonParser *parser = json_parser_new();

        if (!json_parser_load_from_data(parser, body, len, NULL)) {
            g_object_unref(parser);
            g_bytes_unref(response);
            break;
        }

        JsonObject *root = json_node_get_object(json_parser_get_root(parser));
        JsonArray *items = json_object_get_array_member(root, "items");
        if (!items) {
            g_object_unref(parser);
            g_bytes_unref(response);
            break;
        }
        guint count = json_array_get_length(items);

        for (guint i = 0; i < count; i++) {
            JsonObject *item = json_array_get_object_element(items, i);
            if (!item)
                continue;

            /* Feb 2026 API uses "item", older uses "track" */
            const gchar *track_key = json_object_has_member(item, "track")
                                     ? "track" : "item";
            if (!json_object_has_member(item, track_key))
                continue;
            JsonNode *track_node = json_object_get_member(item, track_key);
            if (!track_node || json_node_is_null(track_node))
                continue;
            JsonObject *track = json_node_get_object(track_node);
            if (!track)
                continue;

            /* Skip local files and null tracks */
            if (!json_object_has_member(track, "id") ||
                json_object_get_null_member(track, "id"))
                continue;

            SpotifyTrack *t = g_new0(SpotifyTrack, 1);
            t->id = g_strdup(json_object_get_string_member(track, "id"));
            t->name = g_strdup(json_object_get_string_member(track, "name"));
            t->uri = g_strdup(json_object_get_string_member(track, "uri"));
            t->duration_ms = json_object_get_int_member(track, "duration_ms");

            /* First artist name */
            JsonArray *artists = json_object_get_array_member(track, "artists");
            if (artists && json_array_get_length(artists) > 0) {
                JsonObject *artist = json_array_get_object_element(artists, 0);
                t->artist = g_strdup(json_object_get_string_member(artist, "name"));
            }

            /* Album name */
            JsonObject *album = json_object_get_object_member(track, "album");
            if (album)
                t->album = g_strdup(json_object_get_string_member(album, "name"));

            result = g_list_prepend(result, t);
        }

        gint64 total = json_object_get_int_member(root, "total");
        g_object_unref(parser);
        g_bytes_unref(response);

        offset += count;
        if (offset >= total || count == 0)
            break;
    }

    result = g_list_reverse(result);
    if (cb)
        cb(result, data);
}

/* ---- Device management ---- */

/* Get the first available Spotify device ID, or NULL */
static gchar *
spotify_get_device_id(void)
{
    GBytes *response = spotify_api_get("/me/player/devices");
    if (!response)
        return NULL;

    gsize len;
    const gchar *body = g_bytes_get_data(response, &len);
    JsonParser *parser = json_parser_new();

    if (!json_parser_load_from_data(parser, body, len, NULL)) {
        g_object_unref(parser);
        g_bytes_unref(response);
        return NULL;
    }

    JsonObject *root = json_node_get_object(json_parser_get_root(parser));
    JsonArray *devices = json_object_get_array_member(root, "devices");
    gchar *device_id = NULL;

    if (devices) {
        guint count = json_array_get_length(devices);
        /* Prefer active device, fall back to first available */
        for (guint i = 0; i < count; i++) {
            JsonObject *dev = json_array_get_object_element(devices, i);
            gboolean is_active = json_object_get_boolean_member(dev, "is_active");
            const gchar *id = json_object_get_string_member(dev, "id");
            if (is_active && id) {
                g_free(device_id);
                device_id = g_strdup(id);
                break;
            }
            if (!device_id && id)
                device_id = g_strdup(id);
        }
    }

    g_object_unref(parser);
    g_bytes_unref(response);
    return device_id;
}

/* Transfer playback to a device, activating it */
static gboolean
spotify_transfer_playback(const gchar *device_id)
{
    gchar *body = g_strdup_printf("{\"device_ids\":[\"%s\"],\"play\":false}",
                                  device_id);
    gboolean ok = spotify_api_put("/me/player", body);
    g_free(body);
    return ok;
}

/* ---- Playback control ---- */

gboolean
spotify_play_track(const gchar *track_uri, const gchar *context_uri,
                   gint offset)
{
    gchar *body;
    if (context_uri && track_uri) {
        body = g_strdup_printf(
            "{\"context_uri\":\"%s\",\"offset\":{\"uri\":\"%s\"}}",
            context_uri, track_uri);
    } else if (track_uri) {
        body = g_strdup_printf("{\"uris\":[\"%s\"]}", track_uri);
    } else if (context_uri) {
        body = g_strdup_printf(
            "{\"context_uri\":\"%s\",\"offset\":{\"position\":%d}}",
            context_uri, offset);
    } else {
        body = g_strdup("{}");
    }

    gboolean ok = spotify_api_put("/me/player/play", body);

    if (!ok) {
        /* No active device — try to find and activate one */
        gchar *device_id = spotify_get_device_id();
        if (device_id) {
            g_message("No active Spotify device, transferring to %s", device_id);
            if (spotify_transfer_playback(device_id)) {
                /* Wait for device activation */
                g_usleep(1500000);
                /* Retry with device_id parameter */
                gchar *endpoint = g_strdup_printf(
                    "/me/player/play?device_id=%s", device_id);
                ok = spotify_api_put(endpoint, body);
                g_free(endpoint);
                if (ok)
                    g_message("Spotify playback started after device transfer");
                else
                    g_warning("Spotify retry after transfer also failed");
            } else {
                g_warning("Failed to transfer playback to device %s", device_id);
            }
            g_free(device_id);
        }

        if (!ok)
            g_warning("Spotify play failed — open Spotify on a device first");
    }

    g_free(body);
    return ok;
}

void spotify_play(void)  { spotify_api_put("/me/player/play", NULL); }
void spotify_pause(void) { spotify_api_put("/me/player/pause", NULL); }
void spotify_next(void)  { spotify_api_post("/me/player/next", NULL); }
void spotify_previous(void) { spotify_api_post("/me/player/previous", NULL); }

/* ---- Playback state ---- */

void
spotify_playback_state_clear(SpotifyPlaybackState *state)
{
    g_free(state->track_name);
    g_free(state->artist_name);
    memset(state, 0, sizeof(*state));
}

gboolean
spotify_get_playback_state(SpotifyPlaybackState *state)
{
    memset(state, 0, sizeof(*state));

    GBytes *response = spotify_api_get("/me/player");
    if (!response)
        return FALSE;

    gsize len;
    const gchar *body = g_bytes_get_data(response, &len);
    JsonParser *parser = json_parser_new();

    if (!json_parser_load_from_data(parser, body, len, NULL)) {
        g_object_unref(parser);
        g_bytes_unref(response);
        return FALSE;
    }

    JsonObject *root = json_node_get_object(json_parser_get_root(parser));

    state->is_playing = json_object_get_boolean_member(root, "is_playing");
    state->progress_ms = json_object_get_int_member(root, "progress_ms");

    /* Track info from "item" */
    const gchar *item_key = json_object_has_member(root, "item")
                            ? "item" : "track";
    if (json_object_has_member(root, item_key)) {
        JsonNode *item_node = json_object_get_member(root, item_key);
        if (item_node && !json_node_is_null(item_node)) {
            JsonObject *item = json_node_get_object(item_node);
            if (item) {
                state->duration_ms = json_object_get_int_member(item, "duration_ms");
                if (json_object_has_member(item, "name"))
                    state->track_name = g_strdup(
                        json_object_get_string_member(item, "name"));

                if (json_object_has_member(item, "artists")) {
                    JsonArray *artists = json_object_get_array_member(item, "artists");
                    if (artists && json_array_get_length(artists) > 0) {
                        JsonObject *artist = json_array_get_object_element(artists, 0);
                        state->artist_name = g_strdup(
                            json_object_get_string_member(artist, "name"));
                    }
                }
            }
        }
    }

    g_object_unref(parser);
    g_bytes_unref(response);
    return TRUE;
}
