//! Lightweight egui File Info dialog.

use crate::playlist::{file_uri_to_path, PlaylistEntry};

use super::app::EguiFrontendState;

pub fn show_file_info_dialog(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if !app.file_info_open {
        return;
    }
    let mut open = app.file_info_open;
    egui::Window::new(file_info_title(app))
        .open(&mut open)
        .resizable(true)
        .default_width(620.0)
        .default_height(320.0)
        .show(ctx, |ui| {
            if let Some(entry) = selected_or_current_entry(app) {
                ui.horizontal(|ui| {
                    ui.label("Filename:");
                    let mut filename = entry.filename.clone();
                    ui.add_enabled(
                        false,
                        egui::TextEdit::singleline(&mut filename).desired_width(f32::INFINITY),
                    );
                });
                ui.separator();
                ui.columns(2, |columns| {
                    columns[0].group(|ui| {
                        ui.heading("Tag:");
                        labelled_value(ui, "Title:", &entry.title);
                        labelled_value(ui, "Artist:", "");
                        labelled_value(ui, "Album:", "");
                        labelled_value(ui, "Comment:", "");
                        labelled_value(ui, "Year:", "");
                        labelled_value(ui, "Track number:", "");
                        labelled_value(ui, "Genre:", "");
                        ui.horizontal(|ui| {
                            ui.add_enabled(false, egui::Button::new("Save"));
                            ui.add_enabled(false, egui::Button::new("Remove Tag"));
                            if ui.button("Close").clicked() {
                                app.file_info_open = false;
                            }
                        });
                    });
                    columns[1].group(|ui| {
                        ui.heading("Info:");
                        labelled_value(ui, "Format:", file_format(&entry));
                        labelled_value(ui, "Duration:", &duration_text(&entry));
                        labelled_value(ui, "File size:", &file_size_text(&entry));
                        labelled_value(ui, "URI:", &entry.filename);
                    });
                });
            } else {
                ui.label("No current or selected playlist entry.");
                if ui.button("Close").clicked() {
                    app.file_info_open = false;
                }
            }
        });
    app.file_info_open = open;
}

fn file_info_title(app: &EguiFrontendState) -> String {
    selected_or_current_entry(app)
        .map(|entry| format!("File Info - {}", basename(&entry.filename)))
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

fn labelled_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        ui.monospace(value);
    });
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

fn file_format(entry: &PlaylistEntry) -> &'static str {
    if entry.is_podcast {
        "Podcast/stream"
    } else if entry.filename.starts_with("http://") || entry.filename.starts_with("https://") {
        "Stream"
    } else {
        "Audio file"
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

fn file_size_text(entry: &PlaylistEntry) -> String {
    let Some(path) = file_uri_to_path(&entry.filename) else {
        return "Unknown".to_string();
    };
    match std::fs::metadata(path) {
        Ok(metadata) => format!("{} bytes", metadata.len()),
        Err(_) => "Unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_info_helpers_format_basename_and_duration() {
        let mut entry = PlaylistEntry::new_uri("file:///tmp/song.mp3");
        entry.length_ms = 83_000;
        assert_eq!(basename(&entry.filename), "song.mp3");
        assert_eq!(duration_text(&entry), "1:23");
        assert_eq!(file_format(&entry), "Audio file");
    }
}
