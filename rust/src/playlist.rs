use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistEntry {
    pub filename: String,
    pub title: String,
    pub length_ms: i64,
    pub selected: bool,
    pub is_podcast: bool,
    pub podcast_feed: Option<String>,
    pub podcast_guid: Option<String>,
    pub podcast_downloading: bool,
}

impl PlaylistEntry {
    pub fn new_uri(uri: impl Into<String>) -> Self {
        let filename = uri.into();
        let title = format_title(&filename, None);
        Self {
            filename,
            title,
            length_ms: -1,
            selected: false,
            is_podcast: false,
            podcast_feed: None,
            podcast_guid: None,
            podcast_downloading: false,
        }
    }

    pub fn podcast(
        uri: impl Into<String>,
        title: Option<String>,
        feed: Option<String>,
        guid: Option<String>,
    ) -> Self {
        let filename = uri.into();
        Self {
            title: title
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| filename.clone()),
            filename,
            length_ms: -1,
            selected: false,
            is_podcast: true,
            podcast_feed: feed.filter(|s| !s.is_empty()),
            podcast_guid: guid.filter(|s| !s.is_empty()),
            podcast_downloading: false,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Playlist {
    entries: Vec<PlaylistEntry>,
    position: Option<usize>,
    shuffle: bool,
    repeat: bool,
    no_advance: bool,
}

impl Playlist {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[PlaylistEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add_uri(&mut self, uri: impl Into<String>) {
        self.entries.push(PlaylistEntry::new_uri(uri));
    }

    pub fn add_spotify(
        &mut self,
        spotify_uri: impl Into<String>,
        title: impl Into<String>,
        duration_ms: i64,
    ) {
        let mut entry = PlaylistEntry::new_uri(spotify_uri);
        entry.title = title.into();
        entry.length_ms = duration_ms;
        self.entries.push(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.position = None;
    }

    pub fn position(&self) -> Option<usize> {
        self.position
    }

    pub fn set_position(&mut self, pos: usize) {
        if pos < self.entries.len() {
            self.position = Some(pos);
        }
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        self.shuffle = enabled;
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn set_repeat(&mut self, enabled: bool) {
        self.repeat = enabled;
    }

    pub fn repeat(&self) -> bool {
        self.repeat
    }

    pub fn set_no_advance(&mut self, enabled: bool) {
        self.no_advance = enabled;
    }

    pub fn no_advance(&self) -> bool {
        self.no_advance
    }

    pub fn load_m3u_file(path: &Path) -> io::Result<Self> {
        let contents = fs::read_to_string(path)?;
        Ok(Self::load_m3u(
            &contents,
            path.parent().unwrap_or_else(|| Path::new(".")),
        ))
    }

    pub fn load_m3u(contents: &str, base_dir: &Path) -> Self {
        let mut playlist = Self::new();
        let mut pending_length = -1_i64;
        let mut pending_title: Option<String> = None;
        let mut pending_feed: Option<String> = None;
        let mut pending_guid: Option<String> = None;
        let mut pending_podcast = false;

        for raw in contents.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }

            if line == "#XMMSPODCAST" {
                pending_podcast = true;
                continue;
            }

            if let Some(payload) = line.strip_prefix("#XMMSPODCAST:") {
                pending_podcast = true;
                pending_feed = None;
                pending_guid = None;
                for part in payload.split('\t') {
                    if let Some(value) = part.strip_prefix("feed=") {
                        pending_feed = Some(percent_decode(value));
                    } else if let Some(value) = part.strip_prefix("guid=") {
                        pending_guid = Some(percent_decode(value));
                    }
                }
                continue;
            }

            if let Some(rest) = line.strip_prefix("#EXTINF:") {
                if let Some((seconds, title)) = rest.split_once(',') {
                    let seconds = seconds.parse::<i64>().unwrap_or(-1);
                    pending_length = if seconds >= 0 { seconds * 1000 } else { -1 };
                    pending_title = Some(title.to_string());
                }
                continue;
            }

            if line.starts_with('#') {
                continue;
            }

            let filename = normalize_playlist_path(line, base_dir);
            let mut entry = if pending_podcast {
                PlaylistEntry::podcast(
                    filename,
                    pending_title.clone(),
                    pending_feed.clone(),
                    pending_guid.clone(),
                )
            } else {
                PlaylistEntry::new_uri(filename)
            };

            if pending_length >= 0 {
                entry.length_ms = pending_length;
            }
            if let Some(title) = pending_title.as_ref().filter(|s| !s.is_empty()) {
                entry.title = title.clone();
            }
            playlist.entries.push(entry);

            pending_length = -1;
            pending_title = None;
            pending_feed = None;
            pending_guid = None;
            pending_podcast = false;
        }

        playlist
    }

    pub fn save_m3u_file(&self, path: &Path) -> io::Result<()> {
        fs::write(path, self.to_m3u())
    }

    pub fn to_m3u(&self) -> String {
        let mut out = String::from("#EXTM3U\n");

        for entry in &self.entries {
            if entry.is_podcast {
                match (&entry.podcast_feed, &entry.podcast_guid) {
                    (Some(feed), Some(guid)) => {
                        out.push_str("#XMMSPODCAST:");
                        out.push_str("feed=");
                        out.push_str(&percent_encode(feed));
                        out.push('\t');
                        out.push_str("guid=");
                        out.push_str(&percent_encode(guid));
                        out.push('\n');
                    }
                    (Some(feed), None) => {
                        out.push_str("#XMMSPODCAST:feed=");
                        out.push_str(&percent_encode(feed));
                        out.push('\n');
                    }
                    (None, Some(guid)) => {
                        out.push_str("#XMMSPODCAST:guid=");
                        out.push_str(&percent_encode(guid));
                        out.push('\n');
                    }
                    (None, None) => out.push_str("#XMMSPODCAST\n"),
                }
            }

            if entry.length_ms >= 0 || (entry.is_podcast && !entry.title.is_empty()) {
                let seconds = if entry.length_ms >= 0 {
                    entry.length_ms / 1000
                } else {
                    -1
                };
                out.push_str(&format!("#EXTINF:{seconds},{}\n", entry.title));
            }
            out.push_str(&entry.filename);
            out.push('\n');
        }

        out
    }
}

pub fn format_title(filename: &str, title: Option<&str>) -> String {
    if let Some(title) = title.filter(|s| !s.is_empty()) {
        return title.to_string();
    }

    let mut base = filename
        .rsplit_once('/')
        .map(|(_, base)| base)
        .unwrap_or(filename)
        .to_string();
    if let Some((stem, _)) = base.rsplit_once('.') {
        base = stem.to_string();
    }
    base.replace('_', " ")
}

fn normalize_playlist_path(line: &str, base_dir: &Path) -> String {
    if line.starts_with("file://")
        || line.starts_with("http://")
        || line.starts_with("https://")
        || line.starts_with("spotify:")
        || Path::new(line).is_absolute()
    {
        return line.to_string();
    }

    let path: PathBuf = base_dir.join(line);
    path.to_string_lossy().into_owned()
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_title_uses_basename_without_extension_and_underscores() {
        assert_eq!(format_title("/music/Foo_Bar.mp3", None), "Foo Bar");
        assert_eq!(
            format_title("ignored.mp3", Some("Real Title")),
            "Real Title"
        );
    }

    #[test]
    fn m3u_preserves_podcast_metadata() {
        let playlist = Playlist::load_m3u(
            "#EXTM3U\n#XMMSPODCAST:feed=https%3A%2F%2Fexample.test%2Ffeed.xml\tguid=item%201\n#EXTINF:42,Episode\nhttps://example.test/audio.mp3\n",
            Path::new("/tmp"),
        );
        assert_eq!(playlist.len(), 1);
        let entry = &playlist.entries()[0];
        assert!(entry.is_podcast);
        assert_eq!(
            entry.podcast_feed.as_deref(),
            Some("https://example.test/feed.xml")
        );
        assert_eq!(entry.podcast_guid.as_deref(), Some("item 1"));
        assert_eq!(entry.length_ms, 42_000);
        assert!(playlist
            .to_m3u()
            .contains("#XMMSPODCAST:feed=https%3A%2F%2Fexample.test%2Ffeed.xml\tguid=item%201"));
    }
}
