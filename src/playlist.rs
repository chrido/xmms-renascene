use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MEDIA_EXTENSIONS: &[&str] = &[
    "mp3", "ogg", "flac", "wav", "m4a", "aac", "opus", "wma", "mp4", "webm",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistSortKey {
    Title,
    Filename,
    Path,
    Date,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurationIndexItem {
    pub index: usize,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurationIndexResult {
    pub index: usize,
    pub uri: String,
    pub length_ms: i64,
    pub title: Option<String>,
}

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
    shuffle_order: Vec<usize>,
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

    pub fn entries_mut(&mut self) -> &mut [PlaylistEntry] {
        &mut self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add_uri(&mut self, uri: impl Into<String>) {
        self.entries.push(PlaylistEntry::new_uri(uri));
        self.invalidate_shuffle_order();
    }

    pub fn add_path(&mut self, path: impl AsRef<Path>) {
        self.add_uri(path_to_file_uri(path.as_ref()));
    }

    pub fn add_location(&mut self, location: impl AsRef<str>) -> io::Result<usize> {
        let location = location.as_ref();
        if location.is_empty() {
            return Ok(0);
        }

        if let Some(path) = file_uri_to_path(location) {
            if path.exists() {
                return self.add_path_or_directory(&path);
            }
            self.add_uri(location);
            return Ok(1);
        }

        let path = Path::new(location);
        if path.exists() {
            return self.add_path_or_directory(path);
        }

        self.add_uri(location);
        Ok(1)
    }

    pub fn add_path_or_directory(&mut self, path: &Path) -> io::Result<usize> {
        if path.is_dir() {
            self.add_directory(path)
        } else if path.is_file() {
            self.add_path(path);
            Ok(1)
        } else {
            Ok(0)
        }
    }

    pub fn add_directory(&mut self, path: &Path) -> io::Result<usize> {
        let mut added = 0;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                added += self.add_directory(&path)?;
            } else if file_type.is_file() && is_media_file(&path) {
                self.add_path(&path);
                added += 1;
            }
        }
        Ok(added)
    }

    pub fn add_podcast_entry(
        &mut self,
        uri: impl Into<String>,
        title: Option<String>,
        feed: Option<String>,
        guid: Option<String>,
    ) {
        let uri = uri.into();
        if uri.is_empty() {
            return;
        }

        if let Some(entry) = self.entries.iter_mut().find(|entry| {
            if !entry.is_podcast {
                return false;
            }
            let same_guid = guid
                .as_ref()
                .filter(|guid| !guid.is_empty())
                .is_some_and(|guid| {
                    entry.podcast_guid.as_deref() == Some(guid.as_str())
                        && entry.podcast_feed.as_deref() == feed.as_deref()
                });
            let same_url = entry.filename == uri;
            same_guid || same_url
        }) {
            if let Some(title) = title.as_ref().filter(|title| !title.is_empty()) {
                entry.title = title.clone();
            }
            if let Some(feed) = feed.as_ref().filter(|feed| !feed.is_empty()) {
                entry.podcast_feed = Some(feed.clone());
            }
            if let Some(guid) = guid.as_ref().filter(|guid| !guid.is_empty()) {
                entry.podcast_guid = Some(guid.clone());
            }
            return;
        }

        self.entries
            .push(PlaylistEntry::podcast(uri, title, feed, guid));
        self.invalidate_shuffle_order();
    }

    pub fn add_timed_uri(
        &mut self,
        uri: impl Into<String>,
        title: impl Into<String>,
        duration_ms: i64,
    ) {
        let mut entry = PlaylistEntry::new_uri(uri);
        entry.title = title.into();
        entry.length_ms = duration_ms;
        self.entries.push(entry);
        self.invalidate_shuffle_order();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.position = None;
        self.invalidate_shuffle_order();
    }

    pub fn select_all(&mut self, selected: bool) {
        for entry in &mut self.entries {
            entry.selected = selected;
        }
    }

    pub fn invert_selection(&mut self) {
        for entry in &mut self.entries {
            entry.selected = !entry.selected;
        }
    }

    pub fn remove_selected_or_current(&mut self) -> bool {
        let old_position = self.position;
        let current = old_position.and_then(|position| self.entries.get(position).cloned());
        let has_selected = self.entries.iter().any(|entry| entry.selected);
        let mut removed = false;
        let mut index = 0;
        self.entries.retain(|entry| {
            let remove = if has_selected {
                entry.selected
            } else {
                Some(index) == old_position
            };
            index += 1;
            removed |= remove;
            !remove
        });

        if !removed {
            return false;
        }
        self.update_position_after_reorder_or_remove(current.as_ref(), old_position);
        true
    }

    pub fn remove_selected(&mut self) -> bool {
        let old_position = self.position;
        let current = old_position.and_then(|position| self.entries.get(position).cloned());
        let old_len = self.entries.len();
        self.entries.retain(|entry| !entry.selected);
        if self.entries.len() == old_len {
            return false;
        }
        self.update_position_after_reorder_or_remove(current.as_ref(), old_position);
        true
    }

    pub fn crop_to_selected_or_current(&mut self) -> bool {
        let old_position = self.position;
        let current = old_position.and_then(|position| self.entries.get(position).cloned());
        let old_len = self.entries.len();
        let has_selected = self.entries.iter().any(|entry| entry.selected);
        let mut index = 0;
        self.entries.retain(|entry| {
            let keep = if has_selected {
                entry.selected
            } else {
                Some(index) == old_position
            };
            index += 1;
            keep
        });

        if self.entries.len() == old_len {
            return false;
        }
        self.update_position_after_reorder_or_remove(current.as_ref(), old_position);
        true
    }

    pub fn remove_dead_files(&mut self) -> bool {
        let old_position = self.position;
        let current = old_position.and_then(|position| self.entries.get(position).cloned());
        let old_len = self.entries.len();
        self.entries.retain(|entry| {
            !entry_local_path(entry)
                .as_ref()
                .is_some_and(|path| !path.exists())
        });

        if self.entries.len() == old_len {
            return false;
        }
        self.update_position_after_reorder_or_remove(current.as_ref(), old_position);
        true
    }

    pub fn physically_delete_selected(&mut self) -> io::Result<usize> {
        let selected: Vec<(usize, PathBuf)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.selected)
            .filter_map(|(index, entry)| entry_local_path(entry).map(|path| (index, path)))
            .collect();
        if selected.is_empty() {
            return Ok(0);
        }

        let old_position = self.position;
        let current = old_position.and_then(|position| self.entries.get(position).cloned());
        let mut deleted = Vec::with_capacity(selected.len());
        for (index, path) in selected {
            fs::remove_file(&path)?;
            deleted.push(index);
        }
        for index in deleted.iter().rev() {
            self.entries.remove(*index);
        }
        self.update_position_after_reorder_or_remove(current.as_ref(), old_position);
        Ok(deleted.len())
    }

    pub fn position(&self) -> Option<usize> {
        self.position
    }

    pub fn set_position(&mut self, pos: usize) {
        if pos < self.entries.len() {
            self.position = Some(pos);
        }
    }

    pub fn next(&mut self) -> bool {
        if let Some(next) = self.next_position() {
            self.position = Some(next);
            true
        } else {
            false
        }
    }

    pub fn previous(&mut self) -> bool {
        if let Some(prev) = self.previous_position() {
            self.position = Some(prev);
            true
        } else {
            false
        }
    }

    pub fn eof_reached(&mut self) -> bool {
        if self.no_advance {
            return false;
        }
        self.next()
    }

    pub fn skip_failed_current(&mut self) -> bool {
        let current = self.position;
        if !current.is_some_and(|pos| {
            self.entries
                .get(pos)
                .is_some_and(|entry| entry.title.starts_with("failed: "))
        }) {
            return false;
        }

        if let Some(next) = self.next_position().filter(|next| Some(*next) != current) {
            self.position = Some(next);
            true
        } else {
            false
        }
    }

    pub fn sort_by(&mut self, key: PlaylistSortKey) {
        let current = self
            .position
            .and_then(|position| self.entries.get(position).cloned());
        self.entries
            .sort_by(|left, right| compare_entries(left, right, key));
        self.refresh_position(current.as_ref());
    }

    pub fn sort_selected_by(&mut self, key: PlaylistSortKey) {
        let current = self
            .position
            .and_then(|position| self.entries.get(position).cloned());
        let indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| entry.selected.then_some(index))
            .collect();
        let mut selected: Vec<PlaylistEntry> = indices
            .iter()
            .filter_map(|index| self.entries.get(*index).cloned())
            .collect();

        selected.sort_by(|left, right| compare_entries(left, right, key));
        for (index, entry) in indices.into_iter().zip(selected) {
            self.entries[index] = entry;
        }
        self.refresh_position(current.as_ref());
    }

    pub fn reverse(&mut self) {
        let current = self
            .position
            .and_then(|position| self.entries.get(position).cloned());
        self.entries.reverse();
        self.refresh_position(current.as_ref());
    }

    pub fn randomize(&mut self) {
        let current = self
            .position
            .and_then(|position| self.entries.get(position).cloned());
        shuffle_slice(&mut self.entries);
        self.refresh_position(current.as_ref());
    }

    pub fn move_entry(&mut self, from: usize, to: usize) -> bool {
        if from >= self.entries.len() || to >= self.entries.len() || from == to {
            return false;
        }

        let current = self
            .position
            .and_then(|position| self.entries.get(position).cloned());
        let entry = self.entries.remove(from);
        self.entries.insert(to, entry);
        self.refresh_position(current.as_ref());
        true
    }

    pub fn missing_duration_items(&self) -> Vec<DurationIndexItem> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.length_ms < 0 && !entry.filename.is_empty() && !entry.is_podcast
            })
            .map(|(index, entry)| DurationIndexItem {
                index,
                uri: entry.filename.clone(),
            })
            .collect()
    }

    pub fn apply_duration_index_result(&mut self, result: DurationIndexResult) -> bool {
        let entry = if self
            .entries
            .get(result.index)
            .is_some_and(|entry| entry.filename == result.uri)
        {
            self.entries.get_mut(result.index)
        } else {
            self.entries
                .iter_mut()
                .find(|entry| entry.filename == result.uri)
        };
        let Some(entry) = entry else {
            return false;
        };

        let mut changed = false;
        if result.length_ms > 0 && entry.length_ms != result.length_ms {
            entry.length_ms = result.length_ms;
            changed = true;
        }
        if let Some(title) = result.title.filter(|title| !title.is_empty()) {
            if entry.title != title {
                entry.title = title;
                changed = true;
            }
        }
        changed
    }

    pub fn index_missing_durations_with<F, E>(&mut self, mut discover: F) -> Result<usize, E>
    where
        F: FnMut(&DurationIndexItem) -> Result<Option<DurationIndexResult>, E>,
    {
        let items = self.missing_duration_items();
        let mut changed = 0;
        for item in items {
            if let Some(result) = discover(&item)? {
                if self.apply_duration_index_result(result) {
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }

    pub fn index_missing_durations_with_gstreamer(&mut self) -> Result<usize, String> {
        gstreamer::init().map_err(|err| format!("failed to initialize GStreamer: {err}"))?;
        let discoverer = gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5))
            .map_err(|err| format!("failed to create GStreamer discoverer: {err}"))?;

        self.index_missing_durations_with(|item| {
            let info = match discoverer.discover_uri(&item.uri) {
                Ok(info) => info,
                Err(err) => {
                    eprintln!(
                        "xmms-rs: failed to discover playlist item {}: {err}",
                        item.uri
                    );
                    return Ok(None);
                }
            };
            let length_ms = info
                .duration()
                .map(|duration| duration.mseconds() as i64)
                .unwrap_or(-1);
            let title = info.tags().and_then(|tags| title_from_tags(&tags));
            Ok(Some(DurationIndexResult {
                index: item.index,
                uri: item.uri.clone(),
                length_ms,
                title,
            }))
        })
    }

    pub fn set_shuffle(&mut self, enabled: bool) {
        self.shuffle = enabled;
        if enabled {
            self.generate_shuffle_order();
        } else {
            self.invalidate_shuffle_order();
        }
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

    fn next_position(&mut self) -> Option<usize> {
        let len = self.entries.len();
        if len == 0 {
            return None;
        }

        if self.shuffle {
            if self.shuffle_order.len() != len {
                self.generate_shuffle_order();
            }
            if self.shuffle_order.is_empty() {
                return None;
            }
            let current = self.position;
            if let Some(index) = current.and_then(|current| {
                self.shuffle_order
                    .iter()
                    .position(|candidate| *candidate == current)
            }) {
                if let Some(next) = self.shuffle_order.get(index + 1) {
                    return Some(*next);
                }
                if self.repeat {
                    self.generate_shuffle_order();
                    return self.shuffle_order.first().copied();
                }
                return None;
            }
            return self.shuffle_order.first().copied();
        }

        let next = self.position.map_or(0, |pos| pos + 1);
        if next >= len {
            self.repeat.then_some(0)
        } else {
            Some(next)
        }
    }

    fn previous_position(&self) -> Option<usize> {
        let len = self.entries.len();
        if len == 0 {
            return None;
        }

        match self.position {
            Some(pos) if pos > 0 => Some(pos - 1),
            Some(_) | None if self.repeat => Some(len - 1),
            Some(_) | None => Some(0),
        }
    }

    fn invalidate_shuffle_order(&mut self) {
        self.shuffle_order.clear();
    }

    fn generate_shuffle_order(&mut self) {
        self.shuffle_order = (0..self.entries.len()).collect();
        shuffle_slice(&mut self.shuffle_order);
    }

    fn refresh_position(&mut self, current: Option<&PlaylistEntry>) {
        self.invalidate_shuffle_order();
        self.position = if let Some(current) = current {
            self.entries.iter().position(|entry| entry == current)
        } else if self.entries.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    fn update_position_after_reorder_or_remove(
        &mut self,
        current: Option<&PlaylistEntry>,
        old_position: Option<usize>,
    ) {
        self.refresh_position(current);
        if self.position.is_none() {
            self.position = old_position
                .filter(|_| !self.entries.is_empty())
                .map(|position| position.min(self.entries.len() - 1));
        }
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
        || Path::new(line).is_absolute()
    {
        return line.to_string();
    }

    let path: PathBuf = base_dir.join(line);
    path.to_string_lossy().into_owned()
}

fn compare_entries(left: &PlaylistEntry, right: &PlaylistEntry, key: PlaylistSortKey) -> Ordering {
    match key {
        PlaylistSortKey::Title => compare_ascii_case_insensitive(&left.title, &right.title),
        PlaylistSortKey::Filename => {
            let left = entry_path_for_compare(left);
            let right = entry_path_for_compare(right);
            compare_ascii_case_insensitive(path_basename(&left), path_basename(&right))
        }
        PlaylistSortKey::Path => compare_ascii_case_insensitive(
            &entry_path_for_compare(left),
            &entry_path_for_compare(right),
        ),
        PlaylistSortKey::Date => compare_entries_by_date(left, right),
    }
}

fn compare_entries_by_date(left: &PlaylistEntry, right: &PlaylistEntry) -> Ordering {
    let left_path = entry_path_for_compare(left);
    let right_path = entry_path_for_compare(right);
    let left_modified = fs::metadata(&left_path).and_then(|metadata| metadata.modified());
    let right_modified = fs::metadata(&right_path).and_then(|metadata| metadata.modified());

    match (left_modified, right_modified) {
        (Ok(left_time), Ok(right_time)) => match left_time.cmp(&right_time) {
            Ordering::Equal => Ordering::Equal,
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
        },
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => compare_entries(left, right, PlaylistSortKey::Filename),
    }
}

fn entry_path_for_compare(entry: &PlaylistEntry) -> String {
    file_uri_to_path(&entry.filename)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| entry.filename.clone())
}

fn entry_local_path(entry: &PlaylistEntry) -> Option<PathBuf> {
    file_uri_to_path(&entry.filename).or_else(|| {
        let path = Path::new(&entry.filename);
        path.is_absolute().then(|| path.to_path_buf())
    })
}

fn path_basename(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(_, basename)| basename)
        .unwrap_or(path)
}

fn compare_ascii_case_insensitive(left: &str, right: &str) -> Ordering {
    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
}

fn title_from_tags(tags: &gstreamer::TagList) -> Option<String> {
    let artist = tags
        .get::<gstreamer::tags::Artist>()
        .map(|value| value.get().to_string())
        .filter(|value| !value.is_empty());
    let title = tags
        .get::<gstreamer::tags::Title>()
        .map(|value| value.get().to_string())
        .filter(|value| !value.is_empty());

    match (artist, title) {
        (Some(artist), Some(title)) => Some(format!("{artist} - {title}")),
        (None, Some(title)) => Some(title),
        (Some(artist), None) => Some(artist),
        (None, None) => None,
    }
}

fn shuffle_slice<T>(items: &mut [T]) {
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x584d_4d53);
    for i in (1..items.len()).rev() {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (seed as usize) % (i + 1);
        items.swap(i, j);
    }
}

fn is_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| MEDIA_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn path_to_file_uri(path: &Path) -> String {
    format!("file://{}", percent_encode_path(&path.to_string_lossy()))
}

pub(crate) fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://")
        .map(percent_decode)
        .map(PathBuf::from)
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

fn percent_encode_path(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
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
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn add_directory_recursively_imports_supported_media_files() {
        let root = unique_temp_dir();
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("ignore.txt"), b"not audio").unwrap();
        fs::write(nested.join("Track_One.OGG"), b"audio").unwrap();

        let mut playlist = Playlist::new();
        let added = playlist.add_directory(&root).unwrap();

        assert_eq!(added, 1);
        assert_eq!(playlist.len(), 1);
        assert_eq!(
            playlist.entries()[0].filename,
            path_to_file_uri(&nested.join("Track_One.OGG"))
        );
        assert_eq!(playlist.entries()[0].title, "Track One");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn add_podcast_entry_updates_existing_by_guid() {
        let mut playlist = Playlist::new();
        playlist.add_podcast_entry(
            "https://example.test/old.mp3",
            Some("Old".to_string()),
            Some("https://example.test/feed.xml".to_string()),
            Some("episode-1".to_string()),
        );
        playlist.add_podcast_entry(
            "https://example.test/new.mp3",
            Some("New".to_string()),
            Some("https://example.test/feed.xml".to_string()),
            Some("episode-1".to_string()),
        );

        assert_eq!(playlist.len(), 1);
        assert_eq!(playlist.entries()[0].title, "New");
        assert_eq!(
            playlist.entries()[0].filename,
            "https://example.test/old.mp3"
        );
    }

    #[test]
    fn next_previous_and_repeat_match_playlist_boundaries() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///tmp/one.ogg");
        playlist.add_uri("file:///tmp/two.ogg");

        assert!(playlist.next());
        assert_eq!(playlist.position(), Some(0));
        assert!(playlist.next());
        assert_eq!(playlist.position(), Some(1));
        assert!(!playlist.next());
        assert_eq!(playlist.position(), Some(1));
        assert!(playlist.previous());
        assert_eq!(playlist.position(), Some(0));
        assert!(playlist.previous());
        assert_eq!(playlist.position(), Some(0));

        playlist.set_repeat(true);
        assert!(playlist.previous());
        assert_eq!(playlist.position(), Some(1));
        assert!(playlist.next());
        assert_eq!(playlist.position(), Some(0));
    }

    #[test]
    fn eof_respects_no_advance_and_repeat() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///tmp/one.ogg");
        playlist.add_uri("file:///tmp/two.ogg");
        playlist.set_position(0);

        playlist.set_no_advance(true);
        assert!(!playlist.eof_reached());
        assert_eq!(playlist.position(), Some(0));

        playlist.set_no_advance(false);
        assert!(playlist.eof_reached());
        assert_eq!(playlist.position(), Some(1));
        assert!(!playlist.eof_reached());
        assert_eq!(playlist.position(), Some(1));

        playlist.set_repeat(true);
        assert!(playlist.eof_reached());
        assert_eq!(playlist.position(), Some(0));
    }

    #[test]
    fn failed_current_skip_advances_only_from_failed_entries() {
        let mut playlist = Playlist::new();
        playlist.add_podcast_entry(
            "https://example.test/failed.mp3",
            Some("failed: Episode".to_string()),
            Some("https://example.test/feed.xml".to_string()),
            Some("episode-1".to_string()),
        );
        playlist.add_uri("file:///tmp/next.ogg");
        playlist.set_position(0);

        assert!(playlist.skip_failed_current());
        assert_eq!(playlist.position(), Some(1));
        assert!(!playlist.skip_failed_current());
        assert_eq!(playlist.position(), Some(1));
    }

    #[test]
    fn sort_by_title_filename_and_path_preserves_current_entry() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///music/Beta/b_song.ogg");
        playlist.add_uri("file:///music/Alpha/c_song.ogg");
        playlist.add_uri("file:///music/Gamma/a_song.ogg");
        playlist.entries[0].title = "Zulu".to_string();
        playlist.entries[1].title = "alpha".to_string();
        playlist.entries[2].title = "Echo".to_string();
        playlist.set_position(0);

        playlist.sort_by(PlaylistSortKey::Title);
        assert_eq!(playlist.entries()[0].title, "alpha");
        assert_eq!(playlist.entries()[1].title, "Echo");
        assert_eq!(playlist.entries()[2].title, "Zulu");
        assert_eq!(playlist.position(), Some(2));

        playlist.sort_by(PlaylistSortKey::Filename);
        assert_eq!(
            playlist.entries()[0].filename,
            "file:///music/Gamma/a_song.ogg"
        );
        assert_eq!(
            playlist.entries()[1].filename,
            "file:///music/Beta/b_song.ogg"
        );
        assert_eq!(
            playlist.entries()[2].filename,
            "file:///music/Alpha/c_song.ogg"
        );
        assert_eq!(playlist.position(), Some(1));

        playlist.sort_by(PlaylistSortKey::Path);
        assert_eq!(
            playlist.entries()[0].filename,
            "file:///music/Alpha/c_song.ogg"
        );
        assert_eq!(
            playlist.entries()[1].filename,
            "file:///music/Beta/b_song.ogg"
        );
        assert_eq!(
            playlist.entries()[2].filename,
            "file:///music/Gamma/a_song.ogg"
        );
        assert_eq!(playlist.position(), Some(1));
    }

    #[test]
    fn sort_by_date_uses_file_mtime_then_filename_fallback() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).unwrap();
        let older = root.join("older.ogg");
        let newer = root.join("newer.ogg");
        fs::write(&older, b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&newer, b"new").unwrap();

        let mut playlist = Playlist::new();
        playlist.add_path(&newer);
        playlist.add_path(&older);
        playlist.sort_by(PlaylistSortKey::Date);

        assert_eq!(playlist.entries()[0].filename, path_to_file_uri(&older));
        assert_eq!(playlist.entries()[1].filename, path_to_file_uri(&newer));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sort_selected_only_reorders_selected_entries_at_selected_indices() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///music/4-zulu.ogg");
        playlist.add_uri("file:///music/3-charlie.ogg");
        playlist.add_uri("file:///music/2-bravo.ogg");
        playlist.add_uri("file:///music/1-alpha.ogg");
        playlist.entries[0].selected = true;
        playlist.entries[2].selected = true;
        playlist.entries[3].selected = true;
        playlist.set_position(0);

        playlist.sort_selected_by(PlaylistSortKey::Filename);

        assert_eq!(playlist.entries()[0].filename, "file:///music/1-alpha.ogg");
        assert_eq!(
            playlist.entries()[1].filename,
            "file:///music/3-charlie.ogg"
        );
        assert_eq!(playlist.entries()[2].filename, "file:///music/2-bravo.ogg");
        assert_eq!(playlist.entries()[3].filename, "file:///music/4-zulu.ogg");
        assert!(playlist.entries()[0].selected);
        assert!(!playlist.entries()[1].selected);
        assert!(playlist.entries()[2].selected);
        assert!(playlist.entries()[3].selected);
        assert_eq!(playlist.position(), Some(3));
    }

    #[test]
    fn reverse_preserves_current_entry_position() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///music/one.ogg");
        playlist.add_uri("file:///music/two.ogg");
        playlist.add_uri("file:///music/three.ogg");
        playlist.set_position(0);

        playlist.reverse();

        assert_eq!(playlist.entries()[0].filename, "file:///music/three.ogg");
        assert_eq!(playlist.entries()[1].filename, "file:///music/two.ogg");
        assert_eq!(playlist.entries()[2].filename, "file:///music/one.ogg");
        assert_eq!(playlist.position(), Some(2));
    }

    #[test]
    fn randomize_preserves_all_entries_and_current_entry() {
        let mut playlist = Playlist::new();
        for index in 0..8 {
            playlist.add_uri(format!("file:///music/{index}.ogg"));
        }
        playlist.set_position(3);
        let current = playlist.entries()[3].clone();

        playlist.randomize();

        let mut sorted: Vec<_> = playlist
            .entries()
            .iter()
            .map(|entry| entry.filename.as_str())
            .collect();
        sorted.sort();
        assert_eq!(
            sorted,
            vec![
                "file:///music/0.ogg",
                "file:///music/1.ogg",
                "file:///music/2.ogg",
                "file:///music/3.ogg",
                "file:///music/4.ogg",
                "file:///music/5.ogg",
                "file:///music/6.ogg",
                "file:///music/7.ogg",
            ]
        );
        assert_eq!(
            playlist
                .position()
                .map(|position| &playlist.entries()[position]),
            Some(&current)
        );
    }

    #[test]
    fn duration_indexing_skips_known_and_podcast_entries() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///music/missing.ogg");
        playlist.add_uri("file:///music/known.ogg");
        playlist.entries[1].length_ms = 42_000;
        playlist.add_podcast_entry(
            "https://example.test/episode.mp3",
            Some("Episode".to_string()),
            Some("https://example.test/feed.xml".to_string()),
            Some("episode-1".to_string()),
        );

        let items = playlist.missing_duration_items();

        assert_eq!(
            items,
            vec![DurationIndexItem {
                index: 0,
                uri: "file:///music/missing.ogg".to_string()
            }]
        );
    }

    #[test]
    fn duration_index_result_updates_only_matching_entry() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///music/song.ogg");

        assert!(!playlist.apply_duration_index_result(DurationIndexResult {
            index: 0,
            uri: "file:///music/replaced.ogg".to_string(),
            length_ms: 12_000,
            title: Some("Wrong".to_string()),
        }));
        assert_eq!(playlist.entries()[0].length_ms, -1);
        assert_ne!(playlist.entries()[0].title, "Wrong");

        assert!(playlist.apply_duration_index_result(DurationIndexResult {
            index: 0,
            uri: "file:///music/song.ogg".to_string(),
            length_ms: 12_000,
            title: Some("Artist - Title".to_string()),
        }));
        assert_eq!(playlist.entries()[0].length_ms, 12_000);
        assert_eq!(playlist.entries()[0].title, "Artist - Title");

        playlist.add_uri("file:///music/second.ogg");
        playlist.move_entry(0, 1);
        assert!(playlist.apply_duration_index_result(DurationIndexResult {
            index: 0,
            uri: "file:///music/song.ogg".to_string(),
            length_ms: 24_000,
            title: Some("Moved".to_string()),
        }));
        assert_eq!(playlist.entries()[1].length_ms, 24_000);
        assert_eq!(playlist.entries()[1].title, "Moved");
    }

    #[test]
    fn duration_indexing_with_mock_discoverer_updates_missing_entries() {
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///music/a.ogg");
        playlist.add_uri("file:///music/b.ogg");

        let changed = playlist
            .index_missing_durations_with(|item| {
                Ok::<_, std::convert::Infallible>(Some(DurationIndexResult {
                    index: item.index,
                    uri: item.uri.clone(),
                    length_ms: if item.uri.ends_with("a.ogg") {
                        1_000
                    } else {
                        2_000
                    },
                    title: Some(format!("indexed {}", item.index)),
                }))
            })
            .unwrap();

        assert_eq!(changed, 2);
        assert_eq!(playlist.entries()[0].length_ms, 1_000);
        assert_eq!(playlist.entries()[0].title, "indexed 0");
        assert_eq!(playlist.entries()[1].length_ms, 2_000);
        assert_eq!(playlist.entries()[1].title, "indexed 1");
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("xmms-rs-playlist-test-{nanos}"))
    }
}
