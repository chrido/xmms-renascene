//! Frontend-neutral File Info model and tag helpers.
//!
//! Concrete UI frontends own dialog/window construction. This module owns the
//! shared data extraction and editable tag persistence so GTK and egui cannot
//! drift in displayed metadata or ID3 write behavior.

use std::fs;
use std::path::{Path, PathBuf};

use id3::frame::{Comment, Content};
use id3::{Tag, TagLike, Version};

use crate::playlist::{file_uri_to_path, PlaylistEntry};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditableFileInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub comment: String,
    pub year: String,
    pub track_number: String,
    pub genre: String,
}

impl EditableFileInfo {
    pub fn from_details(details: &FileInfoDetails) -> Self {
        Self {
            title: details.title.clone(),
            artist: details.artist.clone(),
            album: details.album.clone(),
            comment: details.comment.clone(),
            year: details.date.clone(),
            track_number: details.track_number.clone(),
            genre: details.genre.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfoDetails {
    pub uri: String,
    pub path: Option<PathBuf>,
    pub editable: bool,
    pub has_tag: bool,
    pub filename: String,
    pub basename: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub comment: String,
    pub date_label: &'static str,
    pub date: String,
    pub track_number: String,
    pub genre: String,
    pub tag_frame: &'static str,
    pub info_frame: &'static str,
    pub format: String,
    pub duration: String,
    pub file_size: String,
}

pub fn file_info_details_for_entry(entry: &PlaylistEntry) -> FileInfoDetails {
    let local_path = file_uri_to_path(&entry.filename).or_else(|| {
        let path = Path::new(&entry.filename);
        path.is_absolute().then(|| path.to_path_buf())
    });
    let basename = local_path
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| basename_from_uri(&entry.filename));
    let extension = local_path
        .as_deref()
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (tag_frame, info_frame, format) = file_info_labels_for_extension(&extension);
    let editable = extension == "mp3" && local_path.as_deref().is_some_and(Path::exists);
    let id3_tag = local_path
        .as_deref()
        .filter(|_| editable)
        .and_then(read_id3_tag);
    let has_tag = id3_tag.is_some();
    let file_size = local_path
        .as_deref()
        .and_then(|path| fs::metadata(path).ok())
        .map(|metadata| format_file_size(metadata.len()))
        .unwrap_or_else(|| "Unknown".to_string());
    let filename = local_path
        .as_deref()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| entry.filename.clone());

    FileInfoDetails {
        uri: entry.filename.clone(),
        path: local_path,
        editable,
        has_tag,
        filename,
        basename,
        title: id3_tag
            .as_ref()
            .and_then(TagLike::title)
            .unwrap_or(&entry.title)
            .to_string(),
        artist: id3_tag
            .as_ref()
            .and_then(TagLike::artist)
            .unwrap_or_default()
            .to_string(),
        album: id3_tag
            .as_ref()
            .and_then(TagLike::album)
            .unwrap_or_default()
            .to_string(),
        comment: id3_tag
            .as_ref()
            .and_then(first_id3_comment)
            .unwrap_or_default(),
        date_label: if extension == "mp3" { "Year:" } else { "Date:" },
        date: id3_tag
            .as_ref()
            .and_then(TagLike::year)
            .map(|year| format!("{year:04}"))
            .unwrap_or_default(),
        track_number: id3_tag
            .as_ref()
            .and_then(TagLike::track)
            .map(|track| track.to_string())
            .unwrap_or_default(),
        genre: id3_tag
            .as_ref()
            .and_then(TagLike::genre)
            .unwrap_or_default()
            .to_string(),
        tag_frame,
        info_frame,
        format: format.to_string(),
        duration: format_duration_ms(entry.length_ms),
        file_size,
    }
}

fn basename_from_uri(uri: &str) -> String {
    file_uri_to_path(uri)
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| uri.rsplit('/').next().map(str::to_string))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| uri.to_string())
}

fn read_id3_tag(path: &Path) -> Option<Tag> {
    match id3::no_tag_ok(id3::v1v2::read_from_path(path)) {
        Ok(tag) => tag,
        Err(err) => {
            eprintln!(
                "xmms-rs: failed to read ID3 tag from {}: {err}",
                path.display()
            );
            None
        }
    }
}

fn first_id3_comment(tag: &Tag) -> Option<String> {
    tag.frames().find_map(|frame| match frame.content() {
        Content::Comment(comment) => Some(comment.text.clone()),
        _ => None,
    })
}

pub fn write_id3_metadata(path: &Path, values: &EditableFileInfo) -> id3::Result<()> {
    let mut tag = id3::v1v2::read_from_path(path).unwrap_or_else(|_| Tag::new());
    set_or_remove_text(&mut tag, "TIT2", &values.title);
    set_or_remove_text(&mut tag, "TPE1", &values.artist);
    set_or_remove_text(&mut tag, "TALB", &values.album);
    set_or_remove_text(&mut tag, "TCON", &values.genre);
    set_or_remove_text(&mut tag, "TYER", &values.year);
    set_or_remove_text(&mut tag, "TRCK", &values.track_number);
    tag.remove_comment(None, None);
    if !values.comment.trim().is_empty() {
        tag.add_frame(Comment {
            lang: "eng".to_string(),
            description: String::new(),
            text: values.comment.trim().to_string(),
        });
    }
    id3::v1v2::write_to_path(path, &tag, Version::Id3v24)
}

fn set_or_remove_text(tag: &mut Tag, frame: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        tag.remove(frame);
    } else {
        tag.set_text(frame, value);
    }
}

pub fn remove_id3_metadata(path: &Path) -> id3::Result<()> {
    id3::v1v2::remove_from_path(path).map(|_| ())
}

pub fn fallback_title_from_basename(basename: &str) -> String {
    Path::new(basename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(basename)
        .replace(['_', '-'], " ")
}

pub fn file_info_labels_for_extension(
    extension: &str,
) -> (&'static str, &'static str, &'static str) {
    match extension {
        "mp3" => ("ID3 Tag:", "MPEG Info:", "MPEG audio"),
        "ogg" | "oga" | "opus" => (
            "Ogg Vorbis Tag:",
            "Ogg Vorbis Info:",
            "Ogg Vorbis/Opus audio",
        ),
        "flac" => ("Vorbis Comment:", "FLAC Info:", "FLAC audio"),
        "wav" => ("RIFF Tags:", "WAV Info:", "WAV audio"),
        "m4a" | "mp4" | "aac" => ("MP4 Tags:", "AAC/MP4 Info:", "AAC/MP4 audio"),
        _ => ("Tags:", "File Info:", "Audio file"),
    }
}

pub fn format_duration_ms(length_ms: i64) -> String {
    if length_ms < 0 {
        return "Unknown".to_string();
    }
    let total_seconds = length_ms / 1_000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

pub fn format_file_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    if bytes as f64 >= MIB {
        format!("{:.1} MiB ({bytes} bytes)", bytes as f64 / MIB)
    } else if bytes as f64 >= KIB {
        format!("{:.1} KiB ({bytes} bytes)", bytes as f64 / KIB)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_file(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}.mp3", std::process::id()))
    }

    fn file_uri(path: &Path) -> String {
        format!("file://{}", path.display())
    }

    #[test]
    fn file_info_helpers_format_basename_and_duration() {
        let mut entry = PlaylistEntry::new_uri("file:///tmp/song.mp3");
        entry.length_ms = 83_000;
        let details = file_info_details_for_entry(&entry);
        assert_eq!(details.basename, "song.mp3");
        assert_eq!(details.duration, "1:23");
        assert_eq!(details.format, "MPEG audio");
        assert_eq!(details.tag_frame, "ID3 Tag:");
        assert_eq!(details.info_frame, "MPEG Info:");
    }

    #[test]
    fn file_info_labels_cover_common_audio_formats() {
        assert_eq!(
            file_info_labels_for_extension("flac"),
            ("Vorbis Comment:", "FLAC Info:", "FLAC audio")
        );
        assert_eq!(
            file_info_labels_for_extension("ogg"),
            (
                "Ogg Vorbis Tag:",
                "Ogg Vorbis Info:",
                "Ogg Vorbis/Opus audio",
            )
        );
        assert_eq!(
            file_info_labels_for_extension("wav"),
            ("RIFF Tags:", "WAV Info:", "WAV audio")
        );
    }

    #[test]
    fn id3_metadata_round_trips_through_file_info_details() {
        let path = unique_temp_file("xmms-shared-file-info-id3");
        fs::File::create(&path)
            .unwrap()
            .write_all(b"fake mp3 payload")
            .unwrap();

        write_id3_metadata(
            &path,
            &EditableFileInfo {
                title: "Title".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                comment: "Comment".to_string(),
                year: "1994".to_string(),
                track_number: "7".to_string(),
                genre: "Electronic".to_string(),
            },
        )
        .unwrap();

        let entry = PlaylistEntry::new_uri(file_uri(&path));
        let details = file_info_details_for_entry(&entry);
        assert_eq!(details.title, "Title");
        assert_eq!(details.artist, "Artist");
        assert_eq!(details.album, "Album");
        assert_eq!(details.comment, "Comment");
        assert_eq!(details.date, "1994");
        assert_eq!(details.track_number, "7");
        assert_eq!(details.genre, "Electronic");
        assert!(details.editable);
        assert!(details.has_tag);

        remove_id3_metadata(&path).unwrap();
        let removed = file_info_details_for_entry(&entry);
        assert!(!removed.has_tag);

        fs::remove_file(path).unwrap();
    }
}
