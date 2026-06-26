//! Data-only view models and pure presentation helpers consumed by UI frontends.
//!
//! View models should expose presentation-ready state without tying callers to
//! GTK or any other concrete UI toolkit.

use std::path::Path;

use crate::config::Config;
use crate::playlist::PlaylistMenuKind;
use crate::skin::layout::{
    playlist_menu_button_at, playlist_menu_popup_rect, PlaylistMenuButton,
};

pub fn playlist_menu_at(x: i32, y: i32, width: i32, height: i32) -> Option<PlaylistMenuKind> {
    playlist_menu_button_at(x, y, width, height).map(playlist_menu_from_button)
}

pub fn playlist_menu_from_button(button: PlaylistMenuButton) -> PlaylistMenuKind {
    match button {
        PlaylistMenuButton::Add => PlaylistMenuKind::Add,
        PlaylistMenuButton::Remove => PlaylistMenuKind::Remove,
        PlaylistMenuButton::Select => PlaylistMenuKind::Select,
        PlaylistMenuButton::Misc => PlaylistMenuKind::Misc,
        PlaylistMenuButton::List => PlaylistMenuKind::List,
    }
}

pub fn playlist_menu_button_from_kind(menu: PlaylistMenuKind) -> PlaylistMenuButton {
    match menu {
        PlaylistMenuKind::Add => PlaylistMenuButton::Add,
        PlaylistMenuKind::Remove => PlaylistMenuButton::Remove,
        PlaylistMenuKind::Select => PlaylistMenuButton::Select,
        PlaylistMenuKind::Misc => PlaylistMenuButton::Misc,
        PlaylistMenuKind::List => PlaylistMenuButton::List,
    }
}

pub fn playlist_menu_rect(menu: PlaylistMenuKind, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let rect = playlist_menu_popup_rect(playlist_menu_button_from_kind(menu), width, height);
    (rect.x, rect.y, rect.width, rect.height)
}

pub fn volume_to_position(volume: i32) -> i32 {
    ((volume.clamp(0, 100) * 51 + 50) / 100).clamp(0, 51)
}

pub fn position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 51) * 100) as f64 / 51.0) as i32
}

pub fn volume_to_eq_shaded_position(volume: i32) -> i32 {
    ((volume.clamp(0, 100) * 94 + 50) / 100).clamp(0, 94)
}

pub fn balance_to_position(balance: i32) -> i32 {
    (12 + (balance.clamp(-100, 100) * 12) / 100).clamp(0, 24)
}

pub fn position_to_balance(position: i32) -> i32 {
    (((position.clamp(0, 24) - 12) * 100) as f64 / 12.0) as i32
}

pub fn balance_to_eq_shaded_position(balance: i32) -> i32 {
    (19 + (balance.clamp(-100, 100) * 19) / 100).clamp(0, 39)
}

pub fn format_duration(milliseconds: i64) -> String {
    let seconds = (milliseconds.max(0) / 1000) as i32;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn format_playlist_footer_duration(milliseconds: i64, more: bool) -> String {
    if milliseconds <= 0 && more {
        return "?".to_string();
    }

    let seconds = milliseconds.max(0) / 1000;
    if seconds > 3600 {
        format!(
            "{}:{:02}:{:02}{}",
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60,
            if more { "+" } else { "" }
        )
    } else {
        format!(
            "{}:{:02}{}",
            seconds / 60,
            seconds % 60,
            if more { "+" } else { "" }
        )
    }
}

pub fn format_title_for_preferences(
    format: &str,
    filename: &str,
    title: &str,
    config: &Config,
) -> String {
    let title = title.trim();
    let fallback_title =
        if title.is_empty() || title == crate::playlist::format_title(filename, None) {
            filename_title(filename, config)
        } else {
            normalize_title_text(title, config)
        };
    let (artist, track_title) = split_artist_title(&fallback_title);
    let file_title = filename_title(filename, config);
    let format = if format.trim().is_empty() {
        "%p - %t"
    } else {
        format.trim()
    };

    let mut output = String::new();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('p') => output.push_str(artist.unwrap_or("")),
            Some('t') => output.push_str(track_title),
            Some('f') => output.push_str(&file_title),
            Some('a') | Some('g') => {}
            Some('%') => output.push('%'),
            Some(other) => {
                output.push('%');
                output.push(other);
            }
            None => output.push('%'),
        }
    }

    cleanup_formatted_title(&output).unwrap_or(fallback_title)
}

pub fn split_artist_title(title: &str) -> (Option<&str>, &str) {
    title
        .split_once(" - ")
        .map(|(artist, track)| (Some(artist.trim()), track.trim()))
        .unwrap_or((None, title.trim()))
}

pub fn filename_title(filename: &str, config: &Config) -> String {
    let without_query = filename.split(['?', '#']).next().unwrap_or(filename);
    let normalized = normalize_title_text(without_query, config);
    let path = normalized
        .strip_prefix("file://")
        .unwrap_or(normalized.as_str())
        .trim_end_matches('/');
    let basename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let stem = basename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(basename);
    stem.to_string()
}

pub fn normalize_title_text(text: &str, config: &Config) -> String {
    let mut normalized = text.to_string();
    if config.convert_twenty {
        normalized = normalized.replace("%20", " ");
    }
    if config.convert_underscore {
        normalized = normalized.replace('_', " ");
    }
    normalized
}

pub fn cleanup_formatted_title(text: &str) -> Option<String> {
    let mut cleaned = text.trim().to_string();
    for prefix in ["- ", ":", "/", "|"] {
        cleaned = cleaned.trim_start_matches(prefix).trim_start().to_string();
    }
    for suffix in [" -", ":", "/", "|"] {
        cleaned = cleaned.trim_end_matches(suffix).trim_end().to_string();
    }
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

pub fn ellipsize_chars(text: &str, max_len: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_len {
        return text.to_string();
    }
    if max_len > 3 {
        let mut truncated: String = text.chars().take(max_len - 3).collect();
        truncated.push_str("...");
        truncated
    } else {
        text.chars().take(max_len).collect()
    }
}

pub fn eq_shaded_position_to_volume(position: i32) -> i32 {
    ((position.clamp(0, 94) * 100 + 47) / 94).clamp(0, 100)
}

pub fn eq_shaded_position_to_balance(position: i32) -> i32 {
    let position = position.clamp(0, 38);
    (((position - 19) * 100 + if position >= 19 { 9 } else { -9 }) / 19).clamp(-100, 100)
}

pub fn eq_slider_position_to_pixel(position: i32) -> i32 {
    let pixel = position.clamp(0, 100) / 2;
    if (24..=26).contains(&pixel) {
        25
    } else {
        pixel
    }
}

pub fn eq_slider_pixel_to_position(pixel: i32) -> i32 {
    let pixel = pixel.clamp(0, 50);
    if (24..=26).contains(&pixel) {
        50
    } else {
        pixel * 2
    }
}

pub fn scale_event_coords(
    width: f64,
    height: f64,
    base_width: i32,
    base_height: i32,
    x: f64,
    y: f64,
) -> (i32, i32) {
    (
        (x / (width / f64::from(base_width))) as i32,
        (y / (height / f64::from(base_height))) as i32,
    )
}

pub fn parse_time_ms(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    if let Some((minutes, seconds)) = text.split_once(':') {
        if seconds.contains(':') {
            return None;
        }
        let minutes = minutes.parse::<i64>().ok()?;
        let seconds = seconds.parse::<i64>().ok()?;
        return Some((minutes * 60 + seconds) * 1000);
    }
    Some(text.parse::<i64>().ok()? * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_accepts_seconds_and_minutes_seconds() {
        assert_eq!(parse_time_ms("42"), Some(42_000));
        assert_eq!(parse_time_ms("1:23"), Some(83_000));
        assert_eq!(parse_time_ms(""), None);
        assert_eq!(parse_time_ms("1:2:3"), None);
        assert_eq!(parse_time_ms("not-time"), None);
    }

    #[test]
    fn duration_formatting_matches_xmms_style() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(83_000), "1:23");
        assert_eq!(format_playlist_footer_duration(0, true), "?");
        assert_eq!(format_playlist_footer_duration(83_000, false), "1:23");
        assert_eq!(format_playlist_footer_duration(3_661_000, true), "1:01:01+");
    }

    #[test]
    fn slider_conversions_clamp_to_skin_ranges() {
        assert_eq!(volume_to_position(-1), 0);
        assert_eq!(volume_to_position(100), 51);
        assert_eq!(position_to_volume(51), 100);
        assert_eq!(balance_to_position(-100), 0);
        assert_eq!(balance_to_position(100), 24);
        assert_eq!(eq_slider_position_to_pixel(50), 25);
        assert_eq!(eq_slider_pixel_to_position(25), 50);
    }
}
