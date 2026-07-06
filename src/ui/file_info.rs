use std::borrow::Cow;
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

use super::skinned_window;
use super::style::install_file_info_css;
use super::MainWindowUiState;
use crate::app::file_info::{
    fallback_title_from_basename, remove_id3_metadata, write_id3_metadata, EditableFileInfo,
};
pub(super) use crate::app::file_info::{file_info_details_for_entry, FileInfoDetails};
use gtk::prelude::*;

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

    let title_text = format!("File Info - {}", details.basename);
    let title = gtk_safe_text(&title_text);
    let window = skinned_window(title.as_ref(), 620, 320, &["xmms-file-info"]);
    window.set_transient_for(Some(parent));
    window.set_modal(false);

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
                match remove_id3_metadata(&path) {
                    Ok(()) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gtk_safe_text_replaces_interior_nuls() {
        assert_eq!(gtk_safe_text("abc"), Cow::Borrowed("abc"));
        assert_eq!(gtk_safe_text("a\0b").as_ref(), "a�b");
    }
}
