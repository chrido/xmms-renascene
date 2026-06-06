use std::path::{Path, PathBuf};

use gtk::glib::{self, ChecksumType};

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

pub fn cache_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("podcast-cache")
}

pub fn cache_path_for_url(config_dir: &Path, url: &str) -> PathBuf {
    let hash = glib::compute_checksum_for_string(ChecksumType::Sha256, url)
        .expect("GLib must support SHA-256 checksums");
    cache_dir(config_dir).join(hash.as_str())
}

pub fn file_uri_for_cache(path: &Path) -> String {
    format!("file://{}", path.display())
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
}
