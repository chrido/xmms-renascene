use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use gtk::prelude::*;

use super::MainWindowUiState;
use crate::playlist::{file_uri_to_path, PlaylistEntry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileInfoDetails {
    pub(super) uri: String,
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
    let Some(details) = main_state
        .borrow_mut()
        .selected_or_current_file_info_details()
    else {
        return;
    };

    let window = gtk::Window::builder()
        .title(format!("File Info - {}", details.basename))
        .transient_for(parent)
        .default_width(620)
        .default_height(320)
        .build();
    window.set_modal(false);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
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
    filename.set_text(&details.filename);
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

    append_file_info_entry(&tag_grid, 0, "Title:", &details.title, true);
    append_file_info_entry(&tag_grid, 1, "Artist:", &details.artist, true);
    append_file_info_entry(&tag_grid, 2, "Album:", &details.album, true);
    append_file_info_entry(&tag_grid, 3, "Comment:", &details.comment, true);
    append_file_info_entry(&tag_grid, 4, details.date_label, &details.date, false);
    append_file_info_entry(&tag_grid, 5, "Track number:", &details.track_number, false);
    append_file_info_entry(&tag_grid, 6, "Genre:", &details.genre, true);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    buttons.set_halign(gtk::Align::End);
    let save = gtk::Button::with_label("Save");
    save.set_sensitive(false);
    save.set_tooltip_text(Some("Tag editing is not implemented yet"));
    let remove = gtk::Button::with_label(if details.tag_frame == "ID3 Tag:" {
        "Remove ID3"
    } else {
        "Remove Tag"
    });
    remove.set_sensitive(false);
    remove.set_tooltip_text(Some("Tag editing is not implemented yet"));
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

    {
        let main_state = Rc::clone(&main_state);
        window.connect_close_request(move |_| {
            main_state.borrow_mut().set_file_info_dialog_visible(false);
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

fn append_file_info_entry(grid: &gtk::Grid, row: i32, label: &str, value: &str, wide: bool) {
    let label_widget = gtk::Label::new(Some(label));
    label_widget.set_halign(gtk::Align::End);
    grid.attach(&label_widget, 0, row, 1, 1);
    let entry = gtk::Entry::new();
    entry.set_text(value);
    entry.set_editable(false);
    entry.set_hexpand(wide);
    grid.attach(&entry, 1, row, if wide { 3 } else { 1 }, 1);
}

fn append_file_info_label(box_: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
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
        filename,
        basename,
        title: entry.title.clone(),
        artist: String::new(),
        album: String::new(),
        comment: String::new(),
        date_label: if extension == "mp3" { "Year:" } else { "Date:" },
        date: String::new(),
        track_number: String::new(),
        genre: String::new(),
        tag_frame,
        info_frame,
        format: format.to_string(),
        duration: format_duration_ms(entry.length_ms),
        file_size,
    }
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
