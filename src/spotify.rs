use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use gtk::glib::{Checksum, ChecksumType};
use serde_json::Value;

pub const AUTH_URL: &str = "https://accounts.spotify.com/authorize";
pub const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
pub const API_BASE: &str = "https://api.spotify.com/v1";
pub const REDIRECT_PORT: u16 = 8391;
pub const REDIRECT_URI: &str = "http://127.0.0.1:8391/callback";
pub const SCOPES: &str = "user-read-playback-state user-modify-playback-state playlist-read-private playlist-read-collaborative";
pub const CLIENT_ID: &str = "60687ec3a8e1407cb86dc18f14030fff";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpotifyAuthConfig {
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpotifyAuthState {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub token_expiry_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyPlaylist {
    pub id: String,
    pub name: String,
    pub total_tracks: i32,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyTrack {
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub uri: String,
    pub duration_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyDevice {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpotifyPlaybackState {
    pub is_playing: bool,
    pub progress_ms: i64,
    pub duration_ms: i64,
    pub track_name: Option<String>,
    pub artist_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyPkcePair {
    pub verifier: String,
    pub challenge: String,
}

#[derive(Debug)]
pub enum SpotifyHttpError {
    Transport(String),
    HttpStatus(u16, String),
    Read(io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpotifyPlaybackRequest {
    Play,
    Pause,
    Next,
    Previous,
    TransferDevice {
        device_id: String,
    },
    PlayTrack {
        track_uri: Option<String>,
        context_uri: Option<String>,
        offset: i32,
        device_id: Option<String>,
    },
}

impl SpotifyPlaybackRequest {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Next | Self::Previous => "POST",
            Self::Play | Self::Pause | Self::TransferDevice { .. } | Self::PlayTrack { .. } => {
                "PUT"
            }
        }
    }

    pub fn endpoint(&self) -> String {
        match self {
            Self::Play => "/me/player/play".to_string(),
            Self::Pause => "/me/player/pause".to_string(),
            Self::Next => "/me/player/next".to_string(),
            Self::Previous => "/me/player/previous".to_string(),
            Self::TransferDevice { .. } => "/me/player".to_string(),
            Self::PlayTrack { device_id, .. } => {
                if let Some(device_id) = device_id {
                    format!("/me/player/play?device_id={device_id}")
                } else {
                    "/me/player/play".to_string()
                }
            }
        }
    }

    pub fn body(&self) -> Option<String> {
        match self {
            Self::Play | Self::Pause | Self::Next | Self::Previous => None,
            Self::TransferDevice { device_id } => Some(format!(
                "{{\"device_ids\":[\"{}\"],\"play\":false}}",
                json_escape(device_id)
            )),
            Self::PlayTrack {
                track_uri,
                context_uri,
                offset,
                ..
            } => Some(play_track_body(
                track_uri.as_deref(),
                context_uri.as_deref(),
                *offset,
            )),
        }
    }
}

pub fn playlists_endpoint(offset: usize) -> String {
    format!("/me/playlists?limit=50&offset={offset}")
}

pub fn playlist_tracks_endpoint(playlist_id: &str, offset: usize) -> String {
    format!("/playlists/{playlist_id}/items?limit=100&offset={offset}")
}

pub fn devices_endpoint() -> &'static str {
    "/me/player/devices"
}

pub fn playback_state_endpoint() -> &'static str {
    "/me/player"
}

pub fn parse_playlists_response(body: &str) -> Result<(Vec<SpotifyPlaylist>, i64), String> {
    let root: Value = serde_json::from_str(body).map_err(|err| err.to_string())?;
    let total = root.get("total").and_then(Value::as_i64).unwrap_or(0);
    let mut playlists = Vec::new();
    for item in root
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(uri) = item.get("uri").and_then(Value::as_str) else {
            continue;
        };
        let total_tracks = item
            .get("tracks")
            .or_else(|| item.get("items"))
            .and_then(|tracks| tracks.get("total"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        playlists.push(SpotifyPlaylist {
            id: id.to_string(),
            name: name.to_string(),
            total_tracks,
            uri: uri.to_string(),
        });
    }
    Ok((playlists, total))
}

pub fn parse_playlist_tracks_response(body: &str) -> Result<(Vec<SpotifyTrack>, i64), String> {
    let root: Value = serde_json::from_str(body).map_err(|err| err.to_string())?;
    let total = root.get("total").and_then(Value::as_i64).unwrap_or(0);
    let mut tracks = Vec::new();
    for item in root
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(track) = item.get("track").or_else(|| item.get("item")) else {
            continue;
        };
        if track.get("id").is_none_or(Value::is_null) {
            continue;
        }
        let Some(id) = track.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = track.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(uri) = track.get("uri").and_then(Value::as_str) else {
            continue;
        };
        let artist = track
            .get("artists")
            .and_then(Value::as_array)
            .and_then(|artists| artists.first())
            .and_then(|artist| artist.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let album = track
            .get("album")
            .and_then(|album| album.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        tracks.push(SpotifyTrack {
            id: id.to_string(),
            name: name.to_string(),
            artist,
            album,
            uri: uri.to_string(),
            duration_ms: track
                .get("duration_ms")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
        });
    }
    Ok((tracks, total))
}

pub fn parse_devices_response(body: &str) -> Result<Vec<SpotifyDevice>, String> {
    let root: Value = serde_json::from_str(body).map_err(|err| err.to_string())?;
    let mut devices = Vec::new();
    for device in root
        .get("devices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = device.get("id").and_then(Value::as_str) else {
            continue;
        };
        devices.push(SpotifyDevice {
            id: id.to_string(),
            name: device
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            device_type: device
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            is_active: device
                .get("is_active")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(devices)
}

pub fn preferred_device_id(devices: &[SpotifyDevice]) -> Option<&str> {
    devices
        .iter()
        .find(|device| device.is_active)
        .or_else(|| devices.first())
        .map(|device| device.id.as_str())
}

pub fn parse_playback_state_response(body: &str) -> Result<SpotifyPlaybackState, String> {
    let root: Value = serde_json::from_str(body).map_err(|err| err.to_string())?;
    let item = root.get("item").or_else(|| root.get("track"));
    let artist_name = item
        .and_then(|item| item.get("artists"))
        .and_then(Value::as_array)
        .and_then(|artists| artists.first())
        .and_then(|artist| artist.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(SpotifyPlaybackState {
        is_playing: root
            .get("is_playing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        progress_ms: root.get("progress_ms").and_then(Value::as_i64).unwrap_or(0),
        duration_ms: item
            .and_then(|item| item.get("duration_ms"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
        track_name: item
            .and_then(|item| item.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        artist_name,
    })
}

pub fn play_track_body(track_uri: Option<&str>, context_uri: Option<&str>, offset: i32) -> String {
    match (track_uri, context_uri) {
        (Some(track_uri), Some(context_uri)) => format!(
            "{{\"context_uri\":\"{}\",\"offset\":{{\"uri\":\"{}\"}}}}",
            json_escape(context_uri),
            json_escape(track_uri)
        ),
        (Some(track_uri), None) => {
            format!("{{\"uris\":[\"{}\"]}}", json_escape(track_uri))
        }
        (None, Some(context_uri)) => format!(
            "{{\"context_uri\":\"{}\",\"offset\":{{\"position\":{offset}}}}}",
            json_escape(context_uri)
        ),
        (None, None) => "{}".to_string(),
    }
}

pub fn pkce_pair_from_random() -> io::Result<SpotifyPkcePair> {
    let mut bytes = [0u8; 64];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(pkce_pair_from_bytes(&bytes))
}

pub fn pkce_pair_from_bytes(bytes: &[u8]) -> SpotifyPkcePair {
    let verifier = base64_url_no_pad(bytes);
    let challenge = code_challenge_for_verifier(&verifier);
    SpotifyPkcePair {
        verifier,
        challenge,
    }
}

pub fn code_challenge_for_verifier(verifier: &str) -> String {
    let mut checksum =
        Checksum::new(ChecksumType::Sha256).expect("GLib must support SHA-256 checksums");
    checksum.update(verifier.as_bytes());
    base64_url_no_pad(&checksum.digest())
}

pub fn auth_code_request_body(code: &str, verifier: &str) -> String {
    format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={CLIENT_ID}&code_verifier={}",
        form_encode(code),
        form_encode(REDIRECT_URI),
        form_encode(verifier)
    )
}

pub fn exchange_code_for_token_with_url(
    state: &mut SpotifyAuthState,
    token_url: &str,
    code: &str,
    verifier: &str,
    now_unix: i64,
) -> Result<bool, SpotifyHttpError> {
    let body = auth_code_request_body(code, verifier);
    let response = post_form(token_url, &body)?;
    Ok(state.apply_token_response(&response, now_unix))
}

pub fn refresh_access_token_with_url(
    state: &mut SpotifyAuthState,
    token_url: &str,
    now_unix: i64,
) -> Result<bool, SpotifyHttpError> {
    let Some(body) = state.refresh_request_body() else {
        return Ok(false);
    };
    let response = post_form(token_url, &body)?;
    Ok(state.apply_token_response(&response, now_unix))
}

pub fn exchange_code_for_token(
    state: &mut SpotifyAuthState,
    code: &str,
    verifier: &str,
    now_unix: i64,
) -> Result<bool, SpotifyHttpError> {
    exchange_code_for_token_with_url(state, TOKEN_URL, code, verifier, now_unix)
}

pub fn refresh_access_token(
    state: &mut SpotifyAuthState,
    now_unix: i64,
) -> Result<bool, SpotifyHttpError> {
    refresh_access_token_with_url(state, TOKEN_URL, now_unix)
}

impl SpotifyAuthState {
    pub fn from_config(config: SpotifyAuthConfig) -> Self {
        Self {
            refresh_token: config.refresh_token,
            ..Self::default()
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.refresh_token
            .as_deref()
            .is_some_and(|token| !token.is_empty())
    }

    pub fn access_token_valid(&self, now_unix: i64) -> bool {
        self.access_token
            .as_deref()
            .is_some_and(|token| !token.is_empty() && now_unix < self.token_expiry_unix)
    }

    pub fn refresh_request_body(&self) -> Option<String> {
        self.refresh_token.as_deref().map(|refresh_token| {
            format!("grant_type=refresh_token&refresh_token={refresh_token}&client_id={CLIENT_ID}")
        })
    }

    pub fn apply_token_response(&mut self, body: &str, now_unix: i64) -> bool {
        let Some(access_token) = json_string_member(body, "access_token") else {
            return false;
        };
        let Some(expires_in) = json_i64_member(body, "expires_in") else {
            return false;
        };

        self.access_token = Some(access_token);
        self.token_expiry_unix = now_unix + expires_in - 60;
        if let Some(refresh_token) = json_string_member(body, "refresh_token") {
            self.refresh_token = Some(refresh_token);
        }
        true
    }
}

impl SpotifyAuthConfig {
    pub fn is_authenticated(&self) -> bool {
        self.refresh_token
            .as_deref()
            .is_some_and(|token| !token.is_empty())
    }

    pub fn load_from_file(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(contents) => Ok(Self {
                refresh_token: parse_refresh_token(&contents),
            }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err),
        }
    }

    pub fn save_to_file(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut contents = String::from("[spotify]\n");
        if let Some(refresh_token) = self.refresh_token.as_deref() {
            contents.push_str("refresh_token=");
            contents.push_str(refresh_token);
            contents.push('\n');
        }
        fs::write(path, contents)
    }
}

pub fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join("xmms").join("spotify.conf")
}

pub fn authorization_url(code_challenge: &str) -> String {
    escape_auth_url(&format!(
        "{AUTH_URL}?response_type=code&client_id={CLIENT_ID}&scope={SCOPES}&redirect_uri={REDIRECT_URI}&code_challenge_method=S256&code_challenge={code_challenge}"
    ))
}

fn parse_refresh_token(contents: &str) -> Option<String> {
    let mut in_spotify_section = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_spotify_section = &line[1..line.len() - 1] == "spotify";
            continue;
        }
        if in_spotify_section {
            if let Some(value) = line.strip_prefix("refresh_token=") {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn json_string_member(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let mut rest = body.split_once(&marker)?.1.trim_start();
    rest = rest.strip_prefix(':')?.trim_start();
    rest = rest.strip_prefix('"')?;

    let mut value = String::new();
    let mut escaped = false;
    for ch in rest.chars() {
        if escaped {
            value.push(match ch {
                '"' | '\\' | '/' => ch,
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => ch,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(value);
        } else {
            value.push(ch);
        }
    }
    None
}

fn json_i64_member(body: &str, key: &str) -> Option<i64> {
    let marker = format!("\"{key}\"");
    let mut rest = body.split_once(&marker)?.1.trim_start();
    rest = rest.strip_prefix(':')?.trim_start();
    let digits: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect();
    digits.parse().ok()
}

fn post_form(url: &str, body: &str) -> Result<String, SpotifyHttpError> {
    let result = ureq::post(url)
        .set("User-Agent", "XMMS Renascene")
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(body);
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Status(status, response)) => {
            let mut body = String::new();
            response
                .into_reader()
                .read_to_string(&mut body)
                .map_err(SpotifyHttpError::Read)?;
            return Err(SpotifyHttpError::HttpStatus(status, body));
        }
        Err(err) => return Err(SpotifyHttpError::Transport(err.to_string())),
    };
    let mut body = String::new();
    response
        .into_reader()
        .read_to_string(&mut body)
        .map_err(SpotifyHttpError::Read)?;
    Ok(body)
}

fn escape_auth_url(input: &str) -> String {
    const ALLOWED: &str = ":/?#[]@!$&'()*+,;=-._~";
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        let ch = byte as char;
        if ch.is_ascii_alphanumeric() || ALLOWED.contains(ch) {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn form_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        let ch = byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn base64_url_no_pad(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        }
    }
    out
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_config_loads_and_saves_refresh_token_like_c_keyfile() {
        let dir = std::env::temp_dir().join(format!(
            "xmms-rs-spotify-{}-{}",
            std::process::id(),
            "auth-config"
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = config_path(&dir);

        let missing = SpotifyAuthConfig::load_from_file(&path).unwrap();
        assert!(!missing.is_authenticated());

        SpotifyAuthConfig {
            refresh_token: Some("refresh-token".to_string()),
        }
        .save_to_file(&path)
        .unwrap();

        let loaded = SpotifyAuthConfig::load_from_file(&path).unwrap();
        assert_eq!(loaded.refresh_token.as_deref(), Some("refresh-token"));
        assert!(loaded.is_authenticated());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn authorization_url_preserves_c_spotify_contract() {
        let url = authorization_url("challenge-value");

        assert!(url.starts_with(AUTH_URL));
        assert!(url.contains(&format!("client_id={CLIENT_ID}")));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("scope=user-read-playback-state%20user-modify-playback-state"));
        assert!(url.contains(&format!("redirect_uri={REDIRECT_URI}")));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge=challenge-value"));
    }

    #[test]
    fn pkce_helpers_match_rfc7636_challenge_example_and_request_body() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            code_challenge_for_verifier(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(pkce_pair_from_bytes(b"abc").verifier, "YWJj");
        let body = auth_code_request_body("code value", verifier);
        assert!(body.contains("grant_type=authorization_code"));
        assert!(body.contains("code=code%20value"));
        assert!(body.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8391%2Fcallback"));
        assert!(body.contains("code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
    }

    #[test]
    fn token_response_updates_access_expiry_and_optional_refresh_token() {
        let mut state = SpotifyAuthState::from_config(SpotifyAuthConfig {
            refresh_token: Some("old-refresh".to_string()),
        });

        assert_eq!(
            state.refresh_request_body().as_deref(),
            Some(
                "grant_type=refresh_token&refresh_token=old-refresh&client_id=60687ec3a8e1407cb86dc18f14030fff"
            )
        );

        assert!(state.apply_token_response(
            r#"{"access_token":"access","expires_in":3600,"refresh_token":"new-refresh"}"#,
            1000,
        ));
        assert_eq!(state.access_token.as_deref(), Some("access"));
        assert_eq!(state.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(state.token_expiry_unix, 4540);
        assert!(state.access_token_valid(4539));
        assert!(!state.access_token_valid(4540));

        assert!(!state.apply_token_response(r#"{"expires_in":3600}"#, 1000));
    }

    #[test]
    fn playlist_and_track_parsers_accept_old_and_new_spotify_shapes() {
        let (playlists, total) = parse_playlists_response(
            r#"{"total":2,"items":[
                {"id":"old","name":"Old","uri":"spotify:playlist:old","tracks":{"total":3}},
                {"id":"new","name":"New","uri":"spotify:playlist:new","items":{"total":4}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(total, 2);
        assert_eq!(playlists[0].total_tracks, 3);
        assert_eq!(playlists[1].total_tracks, 4);

        let (tracks, total) = parse_playlist_tracks_response(
            r#"{"total":3,"items":[
                {"track":{"id":"one","name":"One","uri":"spotify:track:one","duration_ms":1000,"artists":[{"name":"Artist"}],"album":{"name":"Album"}}},
                {"item":{"id":"two","name":"Two","uri":"spotify:track:two","duration_ms":2000,"artists":[]}},
                {"track":{"id":null}}
            ]}"#,
        )
        .unwrap();
        assert_eq!(total, 3);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].artist.as_deref(), Some("Artist"));
        assert_eq!(tracks[0].album.as_deref(), Some("Album"));
        assert_eq!(tracks[1].id, "two");
    }

    #[test]
    fn device_playback_and_request_helpers_match_c_web_api_contract() {
        let devices = parse_devices_response(
            r#"{"devices":[
                {"id":"inactive","name":"Laptop","type":"Computer","is_active":false},
                {"id":"active","name":"Phone","type":"Smartphone","is_active":true}
            ]}"#,
        )
        .unwrap();
        assert_eq!(preferred_device_id(&devices), Some("active"));
        assert_eq!(
            SpotifyPlaybackRequest::TransferDevice {
                device_id: "active".to_string(),
            }
            .body()
            .as_deref(),
            Some(r#"{"device_ids":["active"],"play":false}"#)
        );

        let state = parse_playback_state_response(
            r#"{"is_playing":true,"progress_ms":42,"item":{"name":"Track","duration_ms":123,"artists":[{"name":"Artist"}]}}"#,
        )
        .unwrap();
        assert!(state.is_playing);
        assert_eq!(state.track_name.as_deref(), Some("Track"));
        assert_eq!(state.artist_name.as_deref(), Some("Artist"));

        assert_eq!(
            play_track_body(Some("spotify:track:one"), None, 0),
            r#"{"uris":["spotify:track:one"]}"#
        );
        assert_eq!(SpotifyPlaybackRequest::Next.method(), "POST");
        assert_eq!(SpotifyPlaybackRequest::Play.endpoint(), "/me/player/play");
    }
}
