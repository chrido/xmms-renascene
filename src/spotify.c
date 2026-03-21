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

static gchar *client_id = NULL;
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

    if (g_key_file_load_from_file(kf, path, 0, NULL)) {
        client_id = g_key_file_get_string(kf, "spotify", "client_id", NULL);
        refresh_token = g_key_file_get_string(kf, "spotify", "refresh_token", NULL);
    }

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
    if (client_id)
        g_key_file_set_string(kf, "spotify", "client_id", client_id);
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
    if (!refresh_token || !client_id)
        return FALSE;

    gchar *body = g_strdup_printf(
        "grant_type=refresh_token&refresh_token=%s&client_id=%s",
        refresh_token, client_id);

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
        code, REDIRECT_URI, client_id, code_verifier);

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
        ? "<html><body><h2>XMMS: Spotify connected!</h2>"
          "<p>You can close this tab.</p></body></html>"
        : "<html><body><h2>XMMS: Authentication failed</h2></body></html>";

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
    g_free(client_id);
    g_free(access_token);
    g_free(refresh_token);
    g_free(code_verifier);
    client_id = NULL;
    access_token = NULL;
    refresh_token = NULL;
    code_verifier = NULL;
}

gboolean
spotify_is_authenticated(void)
{
    return ensure_token();
}

/* ---- Setup wizard ---- */

static void
on_setup_open_dashboard(GtkButton *button, gpointer data)
{
    (void)button;
    GtkWindow *parent = GTK_WINDOW(data);
    GtkUriLauncher *launcher = gtk_uri_launcher_new(
        "https://developer.spotify.com/dashboard");
    gtk_uri_launcher_launch(launcher, parent, NULL, NULL, NULL);
    g_object_unref(launcher);
}

static void
on_setup_copy_uri(GtkButton *button, gpointer data)
{
    (void)data;
    GdkClipboard *clipboard = gtk_widget_get_clipboard(GTK_WIDGET(button));
    gdk_clipboard_set_text(clipboard, REDIRECT_URI);
}

typedef struct {
    GtkWidget *dialog;
    GtkWidget *entry;
    GtkWindow *parent;
    gboolean   accepted;
} SetupContext;

static void
on_setup_connect(GtkButton *button, gpointer data)
{
    (void)button;
    SetupContext *ctx = data;

    GtkEntryBuffer *buf = gtk_entry_get_buffer(GTK_ENTRY(ctx->entry));
    const gchar *text = gtk_entry_buffer_get_text(buf);

    if (!text || !text[0])
        return;

    /* Save client_id */
    g_free(client_id);
    client_id = g_strdup(text);
    save_spotify_config();

    ctx->accepted = TRUE;
    gtk_window_destroy(GTK_WINDOW(ctx->dialog));
}

static gboolean
show_setup_wizard(GtkWindow *parent)
{
    SetupContext ctx = { .dialog = NULL, .entry = NULL,
                         .parent = parent, .accepted = FALSE };

    ctx.dialog = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(ctx.dialog), "Spotify Setup");
    gtk_window_set_default_size(GTK_WINDOW(ctx.dialog), 480, -1);
    gtk_window_set_resizable(GTK_WINDOW(ctx.dialog), FALSE);
    gtk_window_set_modal(GTK_WINDOW(ctx.dialog), TRUE);
    gtk_window_set_transient_for(GTK_WINDOW(ctx.dialog), parent);

    GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 12);
    gtk_widget_set_margin_start(vbox, 20);
    gtk_widget_set_margin_end(vbox, 20);
    gtk_widget_set_margin_top(vbox, 20);
    gtk_widget_set_margin_bottom(vbox, 20);
    gtk_window_set_child(GTK_WINDOW(ctx.dialog), vbox);

    /* Title */
    GtkWidget *title = gtk_label_new(NULL);
    gtk_label_set_markup(GTK_LABEL(title),
        "<big><b>Connect XMMS to Spotify</b></big>");
    gtk_box_append(GTK_BOX(vbox), title);

    /* Step 1 */
    GtkWidget *step1_label = gtk_label_new(NULL);
    gtk_label_set_markup(GTK_LABEL(step1_label),
        "<b>Step 1:</b> Create a Spotify Developer app\n"
        "(select <i>Web API</i> when asked which API to use)");
    gtk_label_set_xalign(GTK_LABEL(step1_label), 0.0);
    gtk_box_append(GTK_BOX(vbox), step1_label);

    GtkWidget *dash_btn = gtk_button_new_with_label(
        "Open Spotify Developer Dashboard");
    g_signal_connect(dash_btn, "clicked",
                     G_CALLBACK(on_setup_open_dashboard), parent);
    gtk_box_append(GTK_BOX(vbox), dash_btn);

    /* Step 2 */
    GtkWidget *step2_label = gtk_label_new(NULL);
    gtk_label_set_markup(GTK_LABEL(step2_label),
        "<b>Step 2:</b> Add this Redirect URI to your app settings:");
    gtk_label_set_xalign(GTK_LABEL(step2_label), 0.0);
    gtk_box_append(GTK_BOX(vbox), step2_label);

    GtkWidget *uri_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_box_append(GTK_BOX(vbox), uri_box);

    GtkWidget *uri_label = gtk_label_new(REDIRECT_URI);
    gtk_label_set_selectable(GTK_LABEL(uri_label), TRUE);
    gtk_widget_set_hexpand(uri_label, TRUE);
    gtk_label_set_xalign(GTK_LABEL(uri_label), 0.0);
    gtk_box_append(GTK_BOX(uri_box), uri_label);

    GtkWidget *copy_btn = gtk_button_new_with_label("Copy");
    g_signal_connect(copy_btn, "clicked",
                     G_CALLBACK(on_setup_copy_uri), NULL);
    gtk_box_append(GTK_BOX(uri_box), copy_btn);

    /* Step 3 */
    GtkWidget *step3_label = gtk_label_new(NULL);
    gtk_label_set_markup(GTK_LABEL(step3_label),
        "<b>Step 3:</b> Paste your Client ID below:");
    gtk_label_set_xalign(GTK_LABEL(step3_label), 0.0);
    gtk_box_append(GTK_BOX(vbox), step3_label);

    ctx.entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(ctx.entry),
                                    "e.g. 1a2b3c4d5e6f7g8h9i0j...");
    gtk_box_append(GTK_BOX(vbox), ctx.entry);

    /* Buttons */
    GtkWidget *btn_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 6);
    gtk_widget_set_halign(btn_box, GTK_ALIGN_END);
    gtk_widget_set_margin_top(btn_box, 6);
    gtk_box_append(GTK_BOX(vbox), btn_box);

    GtkWidget *cancel_btn = gtk_button_new_with_label("Cancel");
    g_signal_connect_swapped(cancel_btn, "clicked",
                             G_CALLBACK(gtk_window_destroy), ctx.dialog);
    gtk_box_append(GTK_BOX(btn_box), cancel_btn);

    GtkWidget *connect_btn = gtk_button_new_with_label("Connect to Spotify");
    gtk_widget_add_css_class(connect_btn, "suggested-action");
    g_signal_connect(connect_btn, "clicked",
                     G_CALLBACK(on_setup_connect), &ctx);
    gtk_box_append(GTK_BOX(btn_box), connect_btn);

    gtk_window_present(GTK_WINDOW(ctx.dialog));

    /* Run a nested main loop until the dialog is closed */
    GMainLoop *loop = g_main_loop_new(NULL, FALSE);
    g_signal_connect_swapped(ctx.dialog, "destroy",
                             G_CALLBACK(g_main_loop_quit), loop);
    g_main_loop_run(loop);
    g_main_loop_unref(loop);

    return ctx.accepted;
}

void
spotify_authenticate(GtkWindow *parent)
{
    if (!client_id || !client_id[0]) {
        if (!show_setup_wizard(parent))
            return;
    }

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
        SPOTIFY_AUTH_URL, client_id, SCOPES, REDIRECT_URI, challenge);
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
        if (response) g_bytes_unref(response);
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

    if (response) g_bytes_unref(response);
    return (status >= 200 && status < 300);
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

/* ---- Playback control ---- */

void
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

    spotify_api_put("/me/player/play", body);
    g_free(body);
}

void spotify_play(void)  { spotify_api_put("/me/player/play", NULL); }
void spotify_pause(void) { spotify_api_put("/me/player/pause", NULL); }
void spotify_next(void)  { spotify_api_post("/me/player/next", NULL); }
void spotify_previous(void) { spotify_api_post("/me/player/previous", NULL); }
