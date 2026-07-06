//! Lightweight egui File Info dialog.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use id3::frame::{Comment, Content};
use id3::{Tag, TagLike, Version};

use crate::app::command::{PlaylistCommand, UiCommand};
use crate::playlist::{file_uri_to_path, PlaylistEntry};

use super::app::EguiFrontendState;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInfoEditorState {
    uri: Option<String>,
    values: EditableFileInfo,
}

impl FileInfoEditorState {
    fn sync_for_details(&mut self, details: &FileInfoDetails) {
        if self.uri.as_deref() == Some(details.uri.as_str()) {
            return;
        }
        self.uri = Some(details.uri.clone());
        self.values = EditableFileInfo {
            title: details.title.clone(),
            artist: details.artist.clone(),
            album: details.album.clone(),
            comment: details.comment.clone(),
            year: details.date.clone(),
            track_number: details.track_number.clone(),
            genre: details.genre.clone(),
        };
    }

    fn clear(&mut self) {
        self.uri = None;
        self.values = EditableFileInfo::default();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EditableFileInfo {
    title: String,
    artist: String,
    album: String,
    comment: String,
    year: String,
    track_number: String,
    genre: String,
}

/// Self-contained state for the File Info dialog. It lives behind an
/// `Arc<Mutex<..>>` so the dialog can be rendered in its own OS-level egui
/// viewport (a real, freely draggable window like GTK) whose closure cannot
/// borrow `EguiFrontendState`.
#[derive(Debug, Default)]
pub struct FileInfoViewportState {
    open: bool,
    details: Option<FileInfoDetails>,
    editor: FileInfoEditorState,
    save_requested: bool,
    remove_requested: bool,
    close_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileInfoDetails {
    uri: String,
    path: Option<PathBuf>,
    editable: bool,
    has_tag: bool,
    filename: String,
    basename: String,
    title: String,
    artist: String,
    album: String,
    comment: String,
    date_label: &'static str,
    date: String,
    track_number: String,
    genre: String,
    tag_frame: &'static str,
    info_frame: &'static str,
    format: String,
    duration: String,
    file_size: String,
}

pub fn show_file_info_dialog(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if !app.file_info_open {
        let mut state = app
            .file_info_viewport
            .lock()
            .expect("file info viewport state poisoned");
        state.open = false;
        state.details = None;
        state.editor.clear();
        return;
    }

    let details = selected_or_current_entry(app).map(|entry| file_info_details_for_entry(&entry));

    // Seed the shared state that the (detached, `'static`) viewport closure
    // renders from. The editor only re-syncs when the URI changes so in-progress
    // edits are preserved across frames.
    {
        let mut state = app
            .file_info_viewport
            .lock()
            .expect("file info viewport state poisoned");
        state.open = true;
        if let Some(details) = details.as_ref() {
            state.editor.sync_for_details(details);
        }
        state.details = details.clone();
    }

    let shared = Arc::clone(&app.file_info_viewport);
    let builder = egui::ViewportBuilder::default()
        .with_title(file_info_title(details.as_ref()))
        .with_inner_size(egui::vec2(640.0, 340.0))
        .with_min_inner_size(egui::vec2(480.0, 260.0))
        .with_resizable(true)
        .with_decorations(true);

    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of("xmms-egui-file-info"),
        builder,
        move |ctx, class| {
            let mut state = shared.lock().expect("file info viewport state poisoned");
            if ctx.input(|input| input.viewport().close_requested())
                || ctx.input(|input| input.key_pressed(egui::Key::Escape))
            {
                state.open = false;
                state.close_requested = true;
                return;
            }
            match class {
                egui::ViewportClass::EmbeddedWindow | egui::ViewportClass::Root => {
                    egui::Window::new("File Info")
                        .resizable(true)
                        .show(ctx, |ui| render_file_info_viewport(ui, &mut state));
                }
                egui::ViewportClass::Deferred | egui::ViewportClass::Immediate => {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| render_file_info_viewport(ui, &mut state));
                }
            }
        },
    );

    let (save_requested, remove_requested, close_requested, still_open, values) = {
        let mut state = app
            .file_info_viewport
            .lock()
            .expect("file info viewport state poisoned");
        let save = std::mem::take(&mut state.save_requested);
        let remove = std::mem::take(&mut state.remove_requested);
        let close = std::mem::take(&mut state.close_requested);
        (save, remove, close, state.open, state.editor.values.clone())
    };

    let mut close = close_requested || !still_open;
    if let Some(details) = details.as_ref() {
        if save_requested && save_file_info(app, details, &values) {
            close = true;
        }
        if remove_requested && remove_file_info_tag(app, details) {
            close = true;
        }
    }

    if close {
        let mut state = app
            .file_info_viewport
            .lock()
            .expect("file info viewport state poisoned");
        state.open = false;
        state.editor.clear();
    }
    app.dispatch(UiCommand::SetFileInfoVisible(!close));
}

fn render_file_info_viewport(ui: &mut egui::Ui, state: &mut FileInfoViewportState) {
    if let Some(details) = state.details.clone() {
        let mut save_requested = false;
        let mut remove_requested = false;
        let mut close_requested = false;
        show_file_info_contents(
            ui,
            &details,
            &mut state.editor,
            &mut save_requested,
            &mut remove_requested,
            &mut close_requested,
        );
        state.save_requested |= save_requested;
        state.remove_requested |= remove_requested;
        if close_requested {
            state.close_requested = true;
            state.open = false;
        }
    } else {
        ui.label("No current or selected playlist entry.");
        if ui.button("Close").clicked() {
            state.close_requested = true;
            state.open = false;
        }
    }
}

fn show_file_info_contents(
    ui: &mut egui::Ui,
    details: &FileInfoDetails,
    editor: &mut FileInfoEditorState,
    save_requested: &mut bool,
    remove_requested: &mut bool,
    close_requested: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Filename:");
        let mut filename = details.filename.clone();
        ui.add_enabled(
            false,
            egui::TextEdit::singleline(&mut filename).desired_width(f32::INFINITY),
        );
    });
    ui.separator();
    ui.columns(2, |columns| {
        columns[0].group(|ui| {
            ui.heading(details.tag_frame);
            tag_field(
                ui,
                "Title:",
                &mut editor.values.title,
                details.editable,
                true,
            );
            tag_field(
                ui,
                "Artist:",
                &mut editor.values.artist,
                details.editable,
                true,
            );
            tag_field(
                ui,
                "Album:",
                &mut editor.values.album,
                details.editable,
                true,
            );
            tag_field(
                ui,
                "Comment:",
                &mut editor.values.comment,
                details.editable,
                true,
            );
            tag_field(
                ui,
                details.date_label,
                &mut editor.values.year,
                details.editable,
                false,
            );
            tag_field(
                ui,
                "Track number:",
                &mut editor.values.track_number,
                details.editable,
                false,
            );
            tag_field(
                ui,
                "Genre:",
                &mut editor.values.genre,
                details.editable,
                true,
            );

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        *close_requested = true;
                    }
                    let remove_label = if details.tag_frame == "ID3 Tag:" {
                        "Remove ID3"
                    } else {
                        "Remove Tag"
                    };
                    let remove = ui.add_enabled(
                        details.editable && details.has_tag,
                        egui::Button::new(remove_label),
                    );
                    if remove.clicked() {
                        *remove_requested = true;
                    }
                    if !details.editable {
                        remove.on_disabled_hover_text(
                            "Tag editing is currently supported for local MP3 files",
                        );
                    }
                    let save = ui.add_enabled(details.editable, egui::Button::new("Save"));
                    if save.clicked() {
                        *save_requested = true;
                    }
                    if !details.editable {
                        save.on_disabled_hover_text(
                            "Tag editing is currently supported for local MP3 files",
                        );
                    }
                });
            });
        });
        columns[1].group(|ui| {
            ui.heading(details.info_frame);
            labelled_value(ui, "Format:", &details.format);
            labelled_value(ui, "Duration:", &details.duration);
            labelled_value(ui, "File size:", &details.file_size);
            labelled_value(ui, "URI:", &details.uri);
        });
    });
}

fn file_info_title(details: Option<&FileInfoDetails>) -> String {
    details
        .map(|details| format!("File Info - {}", details.basename))
        .unwrap_or_else(|| "File Info".to_string())
}

fn selected_or_current_entry(app: &EguiFrontendState) -> Option<PlaylistEntry> {
    let playlist = &app.controller().state().playlist;
    playlist
        .entries()
        .iter()
        .find(|entry| entry.selected)
        .or_else(|| {
            playlist
                .position()
                .and_then(|position| playlist.entries().get(position))
        })
        .cloned()
}

fn tag_field(ui: &mut egui::Ui, label: &str, value: &mut String, editable: bool, wide: bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        let width = if wide { f32::INFINITY } else { 80.0 };
        ui.add_enabled(
            editable,
            egui::TextEdit::singleline(value).desired_width(width),
        );
    });
}

fn labelled_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        ui.monospace(value);
    });
}

fn file_info_details_for_entry(entry: &PlaylistEntry) -> FileInfoDetails {
    let local_path = file_uri_to_path(&entry.filename).or_else(|| {
        let path = Path::new(&entry.filename);
        path.is_absolute().then(|| path.to_path_buf())
    });
    let basename = local_path
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| basename(&entry.filename));
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
        duration: duration_text(entry),
        file_size,
    }
}

fn save_file_info(
    app: &mut EguiFrontendState,
    details: &FileInfoDetails,
    values: &EditableFileInfo,
) -> bool {
    let Some(path) = details.path.as_deref() else {
        return false;
    };
    match write_id3_metadata(path, values) {
        Ok(()) => {
            app.dispatch(PlaylistCommand::UpdateTitleForUri {
                uri: details.uri.clone(),
                title: values.title.clone(),
            });
            true
        }
        Err(err) => {
            app.runtime
                .pending_messages
                .push(format!("failed to write ID3 tag: {err}"));
            false
        }
    }
}

fn remove_file_info_tag(app: &mut EguiFrontendState, details: &FileInfoDetails) -> bool {
    let Some(path) = details.path.as_deref() else {
        return false;
    };
    match id3::v1v2::remove_from_path(path) {
        Ok(_) => {
            app.dispatch(PlaylistCommand::UpdateTitleForUri {
                uri: details.uri.clone(),
                title: fallback_title_from_basename(&details.basename),
            });
            true
        }
        Err(err) => {
            app.runtime
                .pending_messages
                .push(format!("failed to remove ID3 tag: {err}"));
            false
        }
    }
}

fn basename(uri: &str) -> String {
    file_uri_to_path(uri)
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .or_else(|| uri.rsplit('/').next().map(str::to_string))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| uri.to_string())
}

fn file_info_labels_for_extension(extension: &str) -> (&'static str, &'static str, &'static str) {
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

fn duration_text(entry: &PlaylistEntry) -> String {
    if entry.length_ms < 0 {
        "Unknown".to_string()
    } else {
        let total_seconds = entry.length_ms / 1_000;
        format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
    }
}

fn format_file_size(bytes: u64) -> String {
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

fn write_id3_metadata(path: &Path, values: &EditableFileInfo) -> id3::Result<()> {
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

fn fallback_title_from_basename(basename: &str) -> String {
    Path::new(basename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or(basename)
        .replace(['_', '-'], " ")
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
    fn id3_metadata_round_trips_through_egui_file_info_details() {
        let path = unique_temp_file("xmms-egui-file-info-id3");
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

        id3::v1v2::remove_from_path(&path).unwrap();
        let removed = file_info_details_for_entry(&entry);
        assert!(!removed.has_tag);

        fs::remove_file(path).unwrap();
    }
}
