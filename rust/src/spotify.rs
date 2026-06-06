use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
}
