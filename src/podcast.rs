use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};

use gtk::glib::{self, ChecksumType};

use crate::playlist::Playlist;

pub const FETCH_LIMIT: usize = 5 * 1024 * 1024;
pub const BUFFER_SIZE: usize = 8192;
pub const DOWNLOAD_RETRIES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodcastUrlKind {
    Feed,
    DirectStream,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodcastEpisode {
    pub url: String,
    pub title: String,
    pub feed: String,
    pub guid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodcastCacheEntry {
    pub name: String,
    pub modified_unix: i64,
    pub size: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PodcastRefreshScheduler {
    feeds: BTreeSet<String>,
    next_refresh_unix: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodcastHttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub has_icy_name: bool,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodcastResponseAction {
    AddedFeedEpisodes(usize),
    AddedDirectStream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodcastResponseError {
    HttpStatus(u16),
}

#[derive(Debug)]
pub enum PodcastFetchError {
    Transport(String),
    Response(PodcastResponseError),
    BodyTooLarge { limit: usize },
    Read(io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodcastDownloadAttempt {
    pub status: u16,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodcastDownloadOutcome {
    pub cache_path: PathBuf,
    pub attempts: u32,
    pub retry_delays: Vec<u32>,
}

#[derive(Debug)]
pub enum PodcastDownloadError {
    Request(io::Error),
    HttpStatus {
        status: u16,
        attempts: u32,
        retry_delays: Vec<u32>,
    },
    CacheWrite(io::Error),
}

pub fn content_type_is_audio(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|content_type| {
        content_type.starts_with("audio/")
            || content_type.starts_with("video/")
            || content_type.contains("ogg")
            || content_type.contains("mpegurl")
            || content_type.contains("pls")
    })
}

pub fn content_type_is_xml(content_type: Option<&str>) -> bool {
    content_type.is_some_and(|content_type| {
        content_type.contains("xml")
            || content_type.contains("rss")
            || content_type.contains("atom")
    })
}

pub fn bytes_look_like_feed(bytes: &[u8]) -> bool {
    let prefix_len = bytes.len().min(4096);
    let prefix = String::from_utf8_lossy(&bytes[..prefix_len]).to_lowercase();
    prefix.contains("<rss") || prefix.contains("<feed")
}

pub fn classify_url_response(
    content_type: Option<&str>,
    has_icy_name: bool,
    body_prefix: &[u8],
) -> PodcastUrlKind {
    if content_type_is_audio(content_type) || has_icy_name {
        PodcastUrlKind::DirectStream
    } else if content_type_is_xml(content_type) || bytes_look_like_feed(body_prefix) {
        PodcastUrlKind::Feed
    } else {
        PodcastUrlKind::DirectStream
    }
}

pub fn parse_feed(feed_xml: &str, feed_url: &str) -> Vec<PodcastEpisode> {
    let mut episodes = Vec::new();
    let mut rest = feed_xml;
    while let Some((tag, _, after_start)) = next_episode_start(rest) {
        let Some((_, body_start)) = after_start.split_once('>') else {
            break;
        };
        let Some((body, after_end)) = split_once_case_insensitive(body_start, &format!("</{tag}>"))
        else {
            break;
        };
        if let Some(episode) = parse_episode_block(body, feed_url) {
            episodes.push(episode);
        }
        rest = after_end;
    }
    episodes
}

pub fn add_feed_to_playlist(playlist: &mut Playlist, feed_xml: &str, feed_url: &str) -> usize {
    let episodes = parse_feed(feed_xml, feed_url);
    for episode in &episodes {
        playlist.add_podcast_entry(
            &episode.url,
            Some(episode.title.clone()),
            Some(episode.feed.clone()),
            episode.guid.clone(),
        );
    }
    episodes.len()
}

pub fn handle_url_response(
    playlist: &mut Playlist,
    url: &str,
    response: &PodcastHttpResponse,
) -> Result<PodcastResponseAction, PodcastResponseError> {
    if response.status < 200 || response.status >= 300 {
        return Err(PodcastResponseError::HttpStatus(response.status));
    }

    match classify_url_response(
        response.content_type.as_deref(),
        response.has_icy_name,
        &response.body,
    ) {
        PodcastUrlKind::Feed => {
            let feed = String::from_utf8_lossy(&response.body);
            Ok(PodcastResponseAction::AddedFeedEpisodes(
                add_feed_to_playlist(playlist, &feed, url),
            ))
        }
        PodcastUrlKind::DirectStream | PodcastUrlKind::Unknown => {
            playlist.add_podcast_entry(url, None, None, None);
            Ok(PodcastResponseAction::AddedDirectStream)
        }
    }
}

pub fn fetch_url(url: &str) -> Result<PodcastHttpResponse, PodcastFetchError> {
    let result = ureq::get(url).set("User-Agent", "XMMS Resuscitated").call();
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(err) => return Err(PodcastFetchError::Transport(err.to_string())),
    };

    let status = response.status();
    let content_type = response.header("Content-Type").map(ToOwned::to_owned);
    let has_icy_name = response.header("icy-name").is_some();
    let retry_after = response.header("Retry-After").map(ToOwned::to_owned);
    let mut reader = response.into_reader().take((FETCH_LIMIT + 1) as u64);
    let mut body = Vec::new();
    reader
        .read_to_end(&mut body)
        .map_err(PodcastFetchError::Read)?;
    if body.len() > FETCH_LIMIT {
        return Err(PodcastFetchError::BodyTooLarge { limit: FETCH_LIMIT });
    }

    Ok(PodcastHttpResponse {
        status,
        content_type,
        has_icy_name,
        retry_after,
        body,
    })
}

pub fn fetch_url_into_playlist(
    playlist: &mut Playlist,
    url: &str,
) -> Result<PodcastResponseAction, PodcastFetchError> {
    let response = fetch_url(url)?;
    handle_url_response(playlist, url, &response).map_err(PodcastFetchError::Response)
}

pub fn cache_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("podcast-cache")
}

pub fn cache_path_for_url(config_dir: &Path, url: &str) -> PathBuf {
    let hash = glib::compute_checksum_for_string(ChecksumType::Sha256, url)
        .expect("GLib must support SHA-256 checksums");
    cache_dir(config_dir).join(hash.as_str())
}

pub fn ensure_cache_dir(config_dir: &Path) -> io::Result<PathBuf> {
    let dir = cache_dir(config_dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn write_cache_file(config_dir: &Path, url: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    ensure_cache_dir(config_dir)?;
    let path = cache_path_for_url(config_dir, url);
    let tmp = path.with_extension("part");
    if let Err(err) = fs::write(&tmp, bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(path)
}

pub fn download_with_retries<F>(
    config_dir: &Path,
    url: &str,
    mut fetch_attempt: F,
) -> Result<PodcastDownloadOutcome, PodcastDownloadError>
where
    F: FnMut(u32) -> io::Result<PodcastDownloadAttempt>,
{
    let mut retry_delays = Vec::new();
    for attempt in 0..=DOWNLOAD_RETRIES {
        let response = fetch_attempt(attempt).map_err(PodcastDownloadError::Request)?;
        if (200..300).contains(&response.status) {
            let cache_path = write_cache_file(config_dir, url, &response.body)
                .map_err(PodcastDownloadError::CacheWrite)?;
            return Ok(PodcastDownloadOutcome {
                cache_path,
                attempts: attempt + 1,
                retry_delays,
            });
        }

        if !status_should_retry(response.status) || attempt >= DOWNLOAD_RETRIES {
            return Err(PodcastDownloadError::HttpStatus {
                status: response.status,
                attempts: attempt + 1,
                retry_delays,
            });
        }
        retry_delays.push(retry_delay_seconds(
            response.retry_after.as_deref(),
            attempt,
        ));
    }

    unreachable!("download retry loop always returns before exhausting bounded attempts")
}

pub fn download_url_with_retries(
    config_dir: &Path,
    url: &str,
) -> Result<PodcastDownloadOutcome, PodcastDownloadError> {
    download_with_retries(config_dir, url, |_| {
        let response = fetch_url(url)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{err:?}")))?;
        Ok(PodcastDownloadAttempt {
            status: response.status,
            retry_after: response.retry_after,
            body: response.body,
        })
    })
}

pub fn file_uri_for_cache(path: &Path) -> String {
    format!("file://{}", path.display())
}

pub fn cache_file_is_fresh(path: &Path, now_unix: i64, ttl_days: i32) -> io::Result<bool> {
    let metadata = fs::metadata(path)?;
    let modified_unix = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Ok(cache_is_fresh(
        metadata.len(),
        modified_unix,
        now_unix,
        ttl_days,
    ))
}

pub fn cache_is_fresh(size: u64, modified_unix: i64, now_unix: i64, ttl_days: i32) -> bool {
    if size == 0 {
        return false;
    }
    let ttl_days = if ttl_days > 0 { ttl_days } else { 60 };
    let max_age = i64::from(ttl_days) * 24 * 60 * 60;
    now_unix.saturating_sub(modified_unix) <= max_age
}

pub fn stale_cache_files(
    entries: &[PodcastCacheEntry],
    now_unix: i64,
    ttl_days: i32,
) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| !entry.name.ends_with(".part"))
        .filter(|entry| !cache_is_fresh(entry.size, entry.modified_unix, now_unix, ttl_days))
        .map(|entry| entry.name.clone())
        .collect()
}

pub fn cleanup_cache_dir(config_dir: &Path, now_unix: i64, ttl_days: i32) -> io::Result<usize> {
    let dir = cache_dir(config_dir);
    let mut removed = 0;
    if !dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.ends_with(".part") || !path.is_file() {
            continue;
        }
        if !cache_file_is_fresh(&path, now_unix, ttl_days)? {
            fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn status_should_retry(status: u16) -> bool {
    status == 429 || status == 503
}

pub fn retry_delay_seconds(retry_after: Option<&str>, attempt: u32) -> u32 {
    if let Some(seconds) = retry_after.and_then(|value| value.parse::<u32>().ok()) {
        if seconds > 0 {
            return seconds.min(60);
        }
    }
    1u32 << attempt.min(4)
}

pub fn refresh_interval_seconds(minutes: i32) -> u32 {
    let minutes = if minutes > 0 { minutes } else { 60 };
    (minutes as u32) * 60
}

impl PodcastRefreshScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feeds(&self) -> Vec<&str> {
        self.feeds.iter().map(String::as_str).collect()
    }

    pub fn next_refresh_unix(&self) -> Option<i64> {
        self.next_refresh_unix
    }

    pub fn add_feed(&mut self, feed: impl Into<String>) {
        let feed = feed.into();
        if !feed.is_empty() {
            self.feeds.insert(feed);
        }
    }

    pub fn remove_feed(&mut self, feed: &str) {
        self.feeds.remove(feed);
        if self.feeds.is_empty() {
            self.next_refresh_unix = None;
        }
    }

    pub fn schedule_from(&mut self, now_unix: i64, interval_minutes: i32) {
        if self.feeds.is_empty() {
            self.next_refresh_unix = None;
        } else {
            self.next_refresh_unix =
                Some(now_unix + i64::from(refresh_interval_seconds(interval_minutes)));
        }
    }

    pub fn due_feeds(&self, now_unix: i64) -> Vec<&str> {
        if self
            .next_refresh_unix
            .is_some_and(|next| now_unix >= next && !self.feeds.is_empty())
        {
            self.feeds()
        } else {
            Vec::new()
        }
    }

    pub fn mark_refreshed(&mut self, now_unix: i64, interval_minutes: i32) {
        self.schedule_from(now_unix, interval_minutes);
    }
}

pub fn discover_cached_duration_ms(cache_path: &Path) -> Result<Option<i64>, String> {
    gstreamer::init().map_err(|err| format!("failed to initialize GStreamer: {err}"))?;
    let uri = file_uri_for_cache(cache_path);
    let discoverer = gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5))
        .map_err(|err| format!("failed to create GStreamer discoverer: {err}"))?;
    let info = discoverer
        .discover_uri(&uri)
        .map_err(|err| format!("failed to discover podcast cache file {uri}: {err}"))?;
    Ok(info
        .duration()
        .map(|duration| duration.mseconds() as i64)
        .filter(|duration| *duration > 0))
}

pub fn mark_cache_ready(playlist: &mut Playlist, url: &str, length_ms: i64) -> usize {
    let mut updated = 0;
    for entry in playlist.entries_mut() {
        if entry.is_podcast && entry.filename == url {
            entry.podcast_downloading = false;
            if length_ms > 0 {
                entry.length_ms = length_ms;
            }
            updated += 1;
        }
    }
    updated
}

pub fn mark_cache_failed_and_skip_current(playlist: &mut Playlist, url: &str) -> bool {
    for entry in playlist.entries_mut() {
        if entry.is_podcast && entry.filename == url {
            entry.podcast_downloading = false;
            if !entry.title.starts_with("failed: ") {
                entry.title = format!("failed: {}", entry.title);
            }
        }
    }
    playlist.skip_failed_current()
}

pub fn prepare_playback_uri(
    is_podcast: bool,
    url: &str,
    cache_path: &Path,
    cache_fresh: bool,
) -> String {
    if is_podcast && cache_fresh {
        file_uri_for_cache(cache_path)
    } else {
        url.to_string()
    }
}

fn parse_episode_block(block: &str, feed_url: &str) -> Option<PodcastEpisode> {
    let url = find_enclosure_url(block, feed_url)?;
    let title = child_text(block, "title")
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| url.clone());
    let guid = child_text(block, "guid").or_else(|| child_text(block, "id"));
    Some(PodcastEpisode {
        url,
        title,
        feed: feed_url.to_string(),
        guid,
    })
}

fn next_episode_start<'a>(input: &'a str) -> Option<(&'static str, usize, &'a str)> {
    ["item", "entry"]
        .into_iter()
        .filter_map(|tag| {
            split_once_case_insensitive(input, &format!("<{tag}"))
                .map(|(before, after)| (tag, before.len(), after))
        })
        .min_by_key(|(_, index, _)| *index)
}

fn find_enclosure_url(block: &str, feed_url: &str) -> Option<String> {
    for tag in ["enclosure", "content", "media:content"] {
        if let Some(attrs) = first_start_tag_attrs(block, tag) {
            if let Some(url) = attr_value(attrs, "url") {
                return Some(resolve_url(feed_url, &url));
            }
        }
    }
    let mut rest = block;
    while let Some((_, after)) = split_once_case_insensitive(rest, "<link") {
        let Some((attrs, tail)) = after.split_once('>') else {
            break;
        };
        if attr_value(attrs, "rel").is_some_and(|rel| rel.eq_ignore_ascii_case("enclosure")) {
            if let Some(href) = attr_value(attrs, "href") {
                return Some(resolve_url(feed_url, &href));
            }
        }
        rest = tail;
    }
    None
}

fn first_start_tag_attrs<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    split_once_case_insensitive(block, &format!("<{tag}"))
        .and_then(|(_, after)| after.split_once('>').map(|(attrs, _)| attrs))
}

fn child_text(block: &str, name: &str) -> Option<String> {
    let (_, after_start) = split_once_case_insensitive(block, &format!("<{name}"))?;
    let (_, text_start) = after_start.split_once('>')?;
    let (text, _) = split_once_case_insensitive(text_start, &format!("</{name}>"))?;
    Some(xml_unescape(text.trim()))
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let (_, rest) = split_once_case_insensitive(attrs, name)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &rest[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(xml_unescape(&rest[..end]))
}

fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    if url.starts_with('/') {
        if let Some((scheme, host)) = base
            .split_once("://")
            .and_then(|(scheme, rest)| rest.split_once('/').map(|(host, _)| (scheme, host)))
        {
            return format!("{scheme}://{host}{url}");
        }
    }
    let base_dir = base.rsplit_once('/').map(|(dir, _)| dir).unwrap_or(base);
    format!("{base_dir}/{url}")
}

fn split_once_case_insensitive<'a>(haystack: &'a str, needle: &str) -> Option<(&'a str, &'a str)> {
    let haystack_lower = haystack.to_lowercase();
    let needle_lower = needle.to_lowercase();
    let index = haystack_lower.find(&needle_lower)?;
    Some((&haystack[..index], &haystack[index + needle.len()..]))
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_feed_and_direct_stream_responses_like_c() {
        assert_eq!(
            classify_url_response(Some("application/rss+xml"), false, b""),
            PodcastUrlKind::Feed
        );
        assert_eq!(
            classify_url_response(Some("audio/mpeg"), false, b""),
            PodcastUrlKind::DirectStream
        );
        assert_eq!(
            classify_url_response(None, false, b"<?xml version='1.0'?><feed></feed>"),
            PodcastUrlKind::Feed
        );
        assert_eq!(
            classify_url_response(None, true, b""),
            PodcastUrlKind::DirectStream
        );
    }

    #[test]
    fn parses_rss_atom_enclosures_and_metadata() {
        let episodes = parse_feed(
            r#"
            <rss><channel>
              <item><title>Episode &amp; One</title><guid>guid-1</guid><enclosure url="audio/one.mp3"/></item>
              <entry><title>Episode Two</title><id>id-2</id><link rel="enclosure" href="https://cdn.test/two.ogg"/></entry>
              <item><media:content url="/three.mp3"/></item>
            </channel></rss>
            "#,
            "https://example.test/feed/feed.xml",
        );

        assert_eq!(episodes.len(), 3);
        assert_eq!(episodes[0].url, "https://example.test/feed/audio/one.mp3");
        assert_eq!(episodes[0].title, "Episode & One");
        assert_eq!(episodes[0].guid.as_deref(), Some("guid-1"));
        assert_eq!(episodes[1].guid.as_deref(), Some("id-2"));
        assert_eq!(episodes[2].url, "https://example.test/three.mp3");
        assert_eq!(episodes[2].title, "https://example.test/three.mp3");
    }

    #[test]
    fn cache_retry_and_refresh_helpers_match_c_defaults() {
        let path = cache_path_for_url(Path::new("/tmp/xmms"), "https://example.test/a.mp3");
        assert!(path.starts_with("/tmp/xmms/podcast-cache"));
        assert_eq!(path.file_name().unwrap().to_string_lossy().len(), 64);
        assert!(cache_is_fresh(10, 1_000, 1_000 + 60, 60));
        assert!(!cache_is_fresh(0, 1_000, 1_000 + 60, 60));
        assert_eq!(
            stale_cache_files(
                &[
                    PodcastCacheEntry {
                        name: "old".to_string(),
                        modified_unix: 0,
                        size: 1,
                    },
                    PodcastCacheEntry {
                        name: "old.part".to_string(),
                        modified_unix: 0,
                        size: 1,
                    },
                ],
                90 * 24 * 60 * 60,
                60,
            ),
            vec!["old"]
        );
        assert!(status_should_retry(429));
        assert!(status_should_retry(503));
        assert!(!status_should_retry(500));
        assert_eq!(retry_delay_seconds(Some("120"), 0), 60);
        assert_eq!(retry_delay_seconds(None, 3), 8);
        assert_eq!(refresh_interval_seconds(0), 3600);
        assert_eq!(
            prepare_playback_uri(
                true,
                "https://example.test/a.mp3",
                Path::new("/tmp/cache"),
                true
            ),
            "file:///tmp/cache"
        );
    }

    #[test]
    fn importing_feed_adds_podcast_entries_to_playlist() {
        let mut playlist = Playlist::new();
        let added = add_feed_to_playlist(
            &mut playlist,
            r#"<rss><channel><item><title>Episode</title><guid>g1</guid><enclosure url="one.mp3"/></item></channel></rss>"#,
            "https://example.test/feed.xml",
        );

        assert_eq!(added, 1);
        assert_eq!(playlist.len(), 1);
        let entry = &playlist.entries()[0];
        assert!(entry.is_podcast);
        assert_eq!(entry.filename, "https://example.test/one.mp3");
        assert_eq!(entry.title, "Episode");
        assert_eq!(
            entry.podcast_feed.as_deref(),
            Some("https://example.test/feed.xml")
        );
        assert_eq!(entry.podcast_guid.as_deref(), Some("g1"));
    }

    #[test]
    fn cache_write_and_freshness_use_sha_path_and_part_file_cleanup() {
        let root =
            std::env::temp_dir().join(format!("xmms-rs-podcast-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path =
            write_cache_file(&root, "https://example.test/audio.wav", b"cached bytes").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"cached bytes");
        assert!(!path.with_extension("part").exists());
        assert!(cache_file_is_fresh(&path, i64::MAX, 0).is_ok());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cleanup_cache_dir_removes_stale_files_but_keeps_part_files() {
        let root =
            std::env::temp_dir().join(format!("xmms-rs-podcast-cleanup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dir = ensure_cache_dir(&root).unwrap();
        let stale = dir.join("stale");
        let part = dir.join("stale.part");
        fs::write(&stale, b"old").unwrap();
        fs::write(&part, b"old").unwrap();

        assert_eq!(cleanup_cache_dir(&root, i64::MAX, 0).unwrap(), 1);
        assert!(!stale.exists());
        assert!(part.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn response_handling_imports_feeds_or_direct_streams() {
        let mut playlist = Playlist::new();
        let action = handle_url_response(
            &mut playlist,
            "https://example.test/feed.xml",
            &PodcastHttpResponse {
                status: 200,
                content_type: Some("application/rss+xml".to_string()),
                has_icy_name: false,
                retry_after: None,
                body: br#"<rss><channel><item><title>E</title><enclosure url="e.mp3"/></item></channel></rss>"#.to_vec(),
            },
        )
        .unwrap();
        assert_eq!(action, PodcastResponseAction::AddedFeedEpisodes(1));
        assert_eq!(playlist.entries()[0].filename, "https://example.test/e.mp3");

        let action = handle_url_response(
            &mut playlist,
            "https://example.test/live.pls",
            &PodcastHttpResponse {
                status: 200,
                content_type: Some("audio/x-scpls".to_string()),
                has_icy_name: false,
                retry_after: None,
                body: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(action, PodcastResponseAction::AddedDirectStream);
        assert_eq!(
            playlist.entries()[1].filename,
            "https://example.test/live.pls"
        );
    }

    #[test]
    fn download_retry_loop_retries_retryable_statuses_and_writes_success() {
        let root =
            std::env::temp_dir().join(format!("xmms-rs-podcast-retry-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let outcome = download_with_retries(&root, "https://example.test/a.mp3", |attempt| {
            Ok(if attempt == 0 {
                PodcastDownloadAttempt {
                    status: 503,
                    retry_after: Some("2".to_string()),
                    body: Vec::new(),
                }
            } else {
                PodcastDownloadAttempt {
                    status: 200,
                    retry_after: None,
                    body: b"ok".to_vec(),
                }
            })
        })
        .unwrap();

        assert_eq!(outcome.attempts, 2);
        assert_eq!(outcome.retry_delays, vec![2]);
        assert_eq!(fs::read(&outcome.cache_path).unwrap(), b"ok");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refresh_scheduler_tracks_due_feeds_and_reschedules() {
        let mut scheduler = PodcastRefreshScheduler::new();
        scheduler.add_feed("https://example.test/b.xml");
        scheduler.add_feed("https://example.test/a.xml");
        scheduler.add_feed("");
        scheduler.schedule_from(1_000, 0);

        assert_eq!(scheduler.next_refresh_unix(), Some(4_600));
        assert!(scheduler.due_feeds(4_599).is_empty());
        assert_eq!(
            scheduler.due_feeds(4_600),
            vec!["https://example.test/a.xml", "https://example.test/b.xml"]
        );

        scheduler.mark_refreshed(4_600, 5);
        assert_eq!(scheduler.next_refresh_unix(), Some(4_900));
        scheduler.remove_feed("https://example.test/a.xml");
        scheduler.remove_feed("https://example.test/b.xml");
        assert_eq!(scheduler.next_refresh_unix(), None);
    }

    #[test]
    fn podcast_cache_failure_marks_current_failed_and_skips() {
        let mut playlist = Playlist::new();
        playlist.add_podcast_entry(
            "https://example.test/fail.mp3",
            Some("Fail".to_string()),
            None,
            None,
        );
        playlist.add_uri("file:///tmp/next.mp3");
        playlist.set_position(0);

        assert!(mark_cache_failed_and_skip_current(
            &mut playlist,
            "https://example.test/fail.mp3"
        ));
        assert_eq!(playlist.position(), Some(1));
        assert_eq!(playlist.entries()[0].title, "failed: Fail");
    }
}
