use std::borrow::Cow;
use std::cell::RefCell;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::prelude::*;
use id3::frame::{Comment, Content};
use id3::{Tag, TagLike, Version};

use super::{append_css_rule, append_css_rule_groups, CssColors, MainWindowUiState};
use crate::playlist::{file_uri_to_path, PlaylistEntry};
use crate::skin::PlaylistColors;

const FILE_INFO_SURFACE_SELECTORS: &[&str] = &[
    ".xmms-file-info",
    ".xmms-file-info box",
    ".xmms-file-info frame",
    "window.xmms-file-info contents",
];
const FILE_INFO_DECORATION_SELECTORS: &[&str] = &[
    "window.xmms-file-info decoration",
    "window.xmms-file-info decoration:backdrop",
    "window.xmms-file-info .titlebar",
    "window.xmms-file-info .titlebar:backdrop",
    "window.xmms-file-info .default-decoration",
    "window.xmms-file-info .default-decoration:backdrop",
    "window.xmms-file-info titlebar",
    "window.xmms-file-info titlebar:backdrop",
    "window.xmms-file-info headerbar",
    "window.xmms-file-info headerbar:backdrop",
    "window.xmms-file-info windowhandle",
    "window.xmms-file-info windowhandle:backdrop",
];
const FILE_INFO_CONTENT_SELECTORS: &[&str] = &[
    "window.xmms-file-info contents",
    "window.xmms-file-info contents:backdrop",
];
const FILE_INFO_OUTLINE_SELECTORS: &[&str] = &[
    "window.xmms-file-info .titlebar",
    "window.xmms-file-info .titlebar:backdrop",
    "window.xmms-file-info .default-decoration",
    "window.xmms-file-info .default-decoration:backdrop",
    "window.xmms-file-info titlebar",
    "window.xmms-file-info titlebar:backdrop",
    "window.xmms-file-info headerbar",
    "window.xmms-file-info headerbar:backdrop",
    "window.xmms-file-info windowhandle",
    "window.xmms-file-info windowhandle:backdrop",
];
const FILE_INFO_SEPARATOR_SELECTORS: &[&str] = &[
    "window.xmms-file-info .titlebar separator",
    "window.xmms-file-info .titlebar separator:backdrop",
    "window.xmms-file-info headerbar separator",
    "window.xmms-file-info headerbar separator:backdrop",
];
const FILE_INFO_CONTROL_SELECTORS: &[&str] = &[".xmms-file-info entry", ".xmms-file-info button"];
const FILE_INFO_NORMAL_TEXT_SELECTORS: &[&str] =
    &[".xmms-file-info label", ".xmms-file-info button"];
const FILE_INFO_CURRENT_TEXT_SELECTORS: &[&str] = &[
    ".xmms-file-info entry",
    ".xmms-file-info entry selection",
    ".xmms-file-info button:hover",
    ".xmms-file-info button:active",
];
const FILE_INFO_ACTIVE_SELECTORS: &[&str] = &[
    ".xmms-file-info entry selection",
    ".xmms-file-info button:hover",
    ".xmms-file-info button:active",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditableFileInfo {
    title: String,
    artist: String,
    album: String,
    comment: String,
    year: String,
    track_number: String,
    genre: String,
}

pub(crate) struct FileInfoDetails {
    pub(super) uri: String,
    path: Option<PathBuf>,
    editable: bool,
    has_tag: bool,
    pub(super) filename: String,
    pub(super) basename: String,
    pub(super) title: String,
    pub(super) artist: String,
    pub(super) album: String,
    pub(super) comment: String,
    pub(super) date_label: &'static str,
    pub(super) date: String,
    pub(super) track_number: String,
    pub(super) genre: String,
    pub(super) tag_frame: &'static str,
    pub(super) info_frame: &'static str,
    pub(super) format: String,
    pub(super) duration: String,
    pub(super) file_size: String,
}

pub(super) fn show_file_info_dialog(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| {
        show_file_info_dialog_inner(parent, Rc::clone(&main_state));
    })) {
        if let Ok(mut state) = main_state.try_borrow_mut() {
            state.set_file_info_dialog_visible(false);
        }
        eprintln!(
            "xmms-rs: file info dialog failed to open: {}",
            panic_payload_message(payload.as_ref())
        );
    }
}

fn show_file_info_dialog_inner(
    parent: &gtk::ApplicationWindow,
    main_state: Rc<RefCell<MainWindowUiState>>,
) {
    let (details, playlist_colors) = {
        let mut state = main_state.borrow_mut();
        let playlist_colors = state.active_skin().playlist_colors();
        let Some(details) = state.selected_or_current_file_info_details() else {
            return;
        };
        (details, playlist_colors)
    };
    install_file_info_css(playlist_colors);

    let window = gtk::Window::builder()
        .title(gtk_safe_text(&format!("File Info - {}", details.basename)))
        .transient_for(parent)
        .default_width(620)
        .default_height(320)
        .build();
    window.set_modal(false);
    window.add_css_class("xmms-skinned-window");
    window.add_css_class("xmms-file-info");

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.add_css_class("xmms-skinned-window");
    root.add_css_class("xmms-file-info");
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);
    window.set_child(Some(&root));

    let filename_row = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    filename_row.append(&gtk::Label::new(Some("Filename:")));
    let filename = gtk::Entry::new();
    filename.set_editable(false);
    filename.set_hexpand(true);
    set_entry_text(&filename, &details.filename);
    filename_row.append(&filename);
    root.append(&filename_row);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    root.append(&content);

    let left = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.append(&left);

    let tag_frame = gtk::Frame::new(Some(details.tag_frame));
    left.append(&tag_frame);
    let tag_grid = gtk::Grid::new();
    tag_grid.set_margin_top(5);
    tag_grid.set_margin_bottom(5);
    tag_grid.set_margin_start(5);
    tag_grid.set_margin_end(5);
    tag_grid.set_row_spacing(5);
    tag_grid.set_column_spacing(5);
    tag_frame.set_child(Some(&tag_grid));

    let title_entry = append_file_info_entry(
        &tag_grid,
        0,
        "Title:",
        &details.title,
        true,
        details.editable,
    );
    let artist_entry = append_file_info_entry(
        &tag_grid,
        1,
        "Artist:",
        &details.artist,
        true,
        details.editable,
    );
    let album_entry = append_file_info_entry(
        &tag_grid,
        2,
        "Album:",
        &details.album,
        true,
        details.editable,
    );
    let comment_entry = append_file_info_entry(
        &tag_grid,
        3,
        "Comment:",
        &details.comment,
        true,
        details.editable,
    );
    let date_entry = append_file_info_entry(
        &tag_grid,
        4,
        details.date_label,
        &details.date,
        false,
        details.editable,
    );
    let track_entry = append_file_info_entry(
        &tag_grid,
        5,
        "Track number:",
        &details.track_number,
        false,
        details.editable,
    );
    let genre_entry = append_file_info_entry(
        &tag_grid,
        6,
        "Genre:",
        &details.genre,
        true,
        details.editable,
    );

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    buttons.set_halign(gtk::Align::End);
    let save = gtk::Button::with_label("Save");
    save.set_sensitive(details.editable);
    if !details.editable {
        save.set_tooltip_text(Some(
            "Tag editing is currently supported for local MP3 files",
        ));
    }
    let remove = gtk::Button::with_label(if details.tag_frame == "ID3 Tag:" {
        "Remove ID3"
    } else {
        "Remove Tag"
    });
    remove.set_sensitive(details.editable && details.has_tag);
    if !details.editable {
        remove.set_tooltip_text(Some(
            "Tag editing is currently supported for local MP3 files",
        ));
    }
    let close = gtk::Button::with_label("Close");
    buttons.append(&save);
    buttons.append(&remove);
    buttons.append(&close);
    left.append(&buttons);

    let info_frame = gtk::Frame::new(Some(details.info_frame));
    info_frame.set_hexpand(true);
    content.append(&info_frame);
    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 5);
    info_box.set_margin_top(10);
    info_box.set_margin_bottom(10);
    info_box.set_margin_start(10);
    info_box.set_margin_end(10);
    info_frame.set_child(Some(&info_box));
    append_file_info_label(&info_box, &format!("Format: {}", details.format));
    append_file_info_label(&info_box, &format!("Duration: {}", details.duration));
    append_file_info_label(&info_box, &format!("File size: {}", details.file_size));
    append_file_info_label(&info_box, &format!("URI: {}", details.uri));

    if let Some(path) = details.path.clone() {
        let main_state_for_save = Rc::clone(&main_state);
        let window_for_save = window.clone();
        let uri = details.uri.clone();
        let save_path = path.clone();
        save.connect_clicked(move |_| {
            let values = EditableFileInfo {
                title: title_entry.text().to_string(),
                artist: artist_entry.text().to_string(),
                album: album_entry.text().to_string(),
                comment: comment_entry.text().to_string(),
                year: date_entry.text().to_string(),
                track_number: track_entry.text().to_string(),
                genre: genre_entry.text().to_string(),
            };
            if let Err(payload) =
                panic::catch_unwind(AssertUnwindSafe(|| {
                    match write_id3_metadata(&save_path, &values) {
                        Ok(()) => {
                            if let Ok(mut state) = main_state_for_save.try_borrow_mut() {
                                state.update_playlist_title_for_uri(&uri, &values.title);
                            }
                            window_for_save.close();
                        }
                        Err(err) => eprintln!("xmms-rs: failed to write ID3 tag: {err}"),
                    }
                }))
            {
                eprintln!(
                    "xmms-rs: failed to save file info metadata: {}",
                    panic_payload_message(payload.as_ref())
                );
            }
        });

        let main_state = Rc::clone(&main_state);
        let window_for_remove = window.clone();
        let uri = details.uri.clone();
        let fallback_title = fallback_title_from_basename(&details.basename);
        remove.connect_clicked(move |_| {
            if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| {
                match id3::v1v2::remove_from_path(&path) {
                    Ok(_) => {
                        if let Ok(mut state) = main_state.try_borrow_mut() {
                            state.update_playlist_title_for_uri(&uri, &fallback_title);
                        }
                        window_for_remove.close();
                    }
                    Err(err) => eprintln!("xmms-rs: failed to remove ID3 tag: {err}"),
                }
            })) {
                eprintln!(
                    "xmms-rs: failed to remove file info metadata: {}",
                    panic_payload_message(payload.as_ref())
                );
            }
        });
    }

    {
        let main_state = Rc::clone(&main_state);
        window.connect_close_request(move |_| {
            if let Ok(mut state) = main_state.try_borrow_mut() {
                state.set_file_info_dialog_visible(false);
            }
            gtk::glib::Propagation::Proceed
        });
    }
    {
        let window = window.clone();
        close.connect_clicked(move |_| {
            window.close();
        });
    }

    window.present();
}

fn install_file_info_css(colors: PlaylistColors) {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&file_info_css(colors));
    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn file_info_css(colors: PlaylistColors) -> String {
    let colors = CssColors::from_playlist_colors(colors);
    let mut css = String::new();

    append_css_rule(
        &mut css,
        FILE_INFO_SURFACE_SELECTORS,
        &[
            ("background", colors.normal_bg.as_str()),
            ("color", colors.normal.as_str()),
        ],
    );
    append_css_rule_groups(
        &mut css,
        &[
            FILE_INFO_CONTENT_SELECTORS,
            FILE_INFO_DECORATION_SELECTORS,
            FILE_INFO_CONTROL_SELECTORS,
        ],
        &[("box-shadow", "none")],
    );
    append_css_rule_groups(
        &mut css,
        &[FILE_INFO_DECORATION_SELECTORS, FILE_INFO_CONTROL_SELECTORS],
        &[("border", colors.selected_border.as_str())],
    );
    append_css_rule(&mut css, FILE_INFO_OUTLINE_SELECTORS, &[("outline", "0")]);
    append_css_rule_groups(
        &mut css,
        &[FILE_INFO_CONTENT_SELECTORS, FILE_INFO_SEPARATOR_SELECTORS],
        &[("border", "0")],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_SEPARATOR_SELECTORS,
        &[("background", "transparent"), ("min-height", "0")],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_CONTROL_SELECTORS,
        &[
            ("background", colors.normal_bg.as_str()),
            ("background-image", "none"),
            ("border-radius", "0"),
            ("text-shadow", "none"),
        ],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_NORMAL_TEXT_SELECTORS,
        &[("color", colors.normal.as_str())],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_CURRENT_TEXT_SELECTORS,
        &[("color", colors.current.as_str())],
    );
    append_css_rule(
        &mut css,
        &[".xmms-file-info entry"],
        &[("caret-color", colors.current.as_str())],
    );
    append_css_rule(
        &mut css,
        FILE_INFO_ACTIVE_SELECTORS,
        &[("background", colors.selected_bg.as_str())],
    );
    append_css_rule(
        &mut css,
        &[".xmms-file-info button:disabled"],
        &[("opacity", "0.45")],
    );
    css
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&'static str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic")
}

fn gtk_safe_text(text: &str) -> Cow<'_, str> {
    if text.contains('\0') {
        Cow::Owned(text.replace('\0', "�"))
    } else {
        Cow::Borrowed(text)
    }
}

fn set_entry_text(entry: &gtk::Entry, text: &str) {
    entry.set_text(&gtk_safe_text(text));
}

fn append_file_info_entry(
    grid: &gtk::Grid,
    row: i32,
    label: &str,
    value: &str,
    wide: bool,
    editable: bool,
) -> gtk::Entry {
    let label_widget = gtk::Label::new(Some(&gtk_safe_text(label)));
    label_widget.set_halign(gtk::Align::End);
    grid.attach(&label_widget, 0, row, 1, 1);
    let entry = gtk::Entry::new();
    set_entry_text(&entry, value);
    entry.set_editable(editable);
    entry.set_hexpand(wide);
    grid.attach(&entry, 1, row, if wide { 3 } else { 1 }, 1);
    entry
}

fn append_file_info_label(box_: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(&gtk_safe_text(text)));
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_selectable(true);
    box_.append(&label);
}

pub(super) fn file_info_details_for_entry(entry: &PlaylistEntry) -> FileInfoDetails {
    let local_path = file_uri_to_path(&entry.filename).or_else(|| {
        let path = Path::new(&entry.filename);
        path.is_absolute().then(|| path.to_path_buf())
    });
    let basename = local_path
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            entry
                .filename
                .rsplit('/')
                .next()
                .unwrap_or(&entry.filename)
                .to_string()
        });
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

fn format_duration_ms(length_ms: i64) -> String {
    if length_ms < 0 {
        return "Unknown".to_string();
    }
    let total_seconds = length_ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
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
    fn file_info_css_uses_playlist_skin_colors() {
        let css = file_info_css(PlaylistColors {
            normal: [1, 2, 3],
            current: [4, 5, 6],
            normal_bg: [7, 8, 9],
            selected_bg: [10, 11, 12],
        });
        assert!(css.contains("#010203"));
        assert!(css.contains("#040506"));
        assert!(css.contains("#070809"));
        assert!(css.contains("#0a0b0c"));
    }

    #[test]
    fn gtk_safe_text_replaces_interior_nuls() {
        assert_eq!(gtk_safe_text("abc"), Cow::Borrowed("abc"));
        assert_eq!(gtk_safe_text("a\0b").as_ref(), "a�b");
    }

    #[test]
    fn id3_metadata_round_trips_through_file_info_details() {
        let path = unique_temp_file("xmms-file-info-id3");
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
