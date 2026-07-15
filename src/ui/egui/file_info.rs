//! Lightweight egui File Info dialog.

#[cfg(not(target_os = "android"))]
use std::sync::Arc;

use crate::app::command::{PlaylistCommand, UiCommand};
use crate::app::file_info::{
    fallback_title_from_basename, file_info_details_for_entry, remove_id3_metadata,
    write_id3_metadata, EditableFileInfo, FileInfoDetails,
};
use crate::playlist::PlaylistEntry;

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
        self.values = EditableFileInfo::from_details(details);
    }

    fn clear(&mut self) {
        self.uri = None;
        self.values = EditableFileInfo::default();
    }
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

    #[cfg(target_os = "android")]
    {
        let back_enabled = super::app::android_back_target_for(app)
            == Some(super::app::AndroidBackTarget::FileInfo);
        let mut state = app
            .file_info_viewport
            .lock()
            .expect("file info viewport state poisoned");
        show_android_file_info(ctx, &mut state, back_enabled);
    }

    #[cfg(not(target_os = "android"))]
    {
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
    }

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

#[cfg(target_os = "android")]
fn show_android_file_info(
    ctx: &egui::Context,
    state: &mut FileInfoViewportState,
    back_enabled: bool,
) {
    if android_file_info_back_key_pressed(ctx, back_enabled) {
        state.open = false;
        state.close_requested = true;
        return;
    }

    let pixels_per_point = ctx.pixels_per_point().max(f32::EPSILON);
    let Some(layout) = super::android_file_picker::window_layout_snapshot_pixels()
        .filter(|layout| layout.has_current_insets())
    else {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        return;
    };
    let screen = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            layout.width as f32 / pixels_per_point,
            layout.height as f32 / pixels_per_point,
        ),
    );
    let insets = layout.insets;
    let left_inset = insets.left as f32 / pixels_per_point;
    let top_inset = insets.top as f32 / pixels_per_point;
    let right_inset = insets.right as f32 / pixels_per_point;
    let bottom_inset = insets.bottom as f32 / pixels_per_point;
    let horizontal_margin = 16.0;
    let vertical_margin = 12.0;
    let content_width =
        (screen.width() - left_inset - right_inset - horizontal_margin * 2.0).max(1.0);
    let content_height =
        (screen.height() - top_inset - bottom_inset - vertical_margin * 2.0).max(1.0);

    egui::Area::new(egui::Id::new("xmms-android-file-info"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.set_min_size(screen.size());
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, egui::Color32::from_gray(46));
            ui.add_space(top_inset + vertical_margin);
            ui.horizontal(|ui| {
                ui.add_space(left_inset + horizontal_margin);
                ui.vertical(|ui| {
                    ui.set_width(content_width);
                    ui.set_min_height(content_height);
                    super::preferences::apply_android_preferences_style(ui);
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized([88.0, 48.0], egui::Button::new("Close"))
                            .clicked()
                        {
                            state.open = false;
                            state.close_requested = true;
                        }
                        ui.heading(file_info_title(state.details.as_ref()));
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| render_android_file_info_contents(ui, state));
                });
            });
        });
}

#[cfg(any(target_os = "android", test))]
fn android_file_info_back_key_pressed(ctx: &egui::Context, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    ctx.input_mut(|input| {
        [egui::Key::Escape, egui::Key::BrowserBack]
            .into_iter()
            .any(|key| input.consume_key(egui::Modifiers::NONE, key))
    })
}

#[cfg(target_os = "android")]
fn render_android_file_info_contents(ui: &mut egui::Ui, state: &mut FileInfoViewportState) {
    let Some(details) = state.details.clone() else {
        ui.label("No current or selected playlist entry.");
        return;
    };

    ui.label("Filename");
    let mut filename = details.filename.clone();
    ui.add_sized(
        [ui.available_width(), 48.0],
        egui::TextEdit::singleline(&mut filename).interactive(false),
    );
    ui.separator();
    ui.heading(details.tag_frame);
    android_tag_field(
        ui,
        "Title",
        &mut state.editor.values.title,
        details.editable,
    );
    android_tag_field(
        ui,
        "Artist",
        &mut state.editor.values.artist,
        details.editable,
    );
    android_tag_field(
        ui,
        "Album",
        &mut state.editor.values.album,
        details.editable,
    );
    android_tag_field(
        ui,
        "Comment",
        &mut state.editor.values.comment,
        details.editable,
    );
    android_tag_field(
        ui,
        details.date_label.trim_end_matches(':'),
        &mut state.editor.values.year,
        details.editable,
    );
    android_tag_field(
        ui,
        "Track number",
        &mut state.editor.values.track_number,
        details.editable,
    );
    android_tag_field(
        ui,
        "Genre",
        &mut state.editor.values.genre,
        details.editable,
    );

    ui.horizontal(|ui| {
        let save = ui.add_enabled_ui(details.editable, |ui| {
            ui.add_sized([120.0, 52.0], egui::Button::new("Save"))
        });
        if save.inner.clicked() {
            state.save_requested = true;
        }
        let remove_label = if details.tag_frame == "ID3 Tag:" {
            "Remove ID3"
        } else {
            "Remove Tag"
        };
        let remove = ui.add_enabled_ui(details.editable && details.has_tag, |ui| {
            ui.add_sized([140.0, 52.0], egui::Button::new(remove_label))
        });
        if remove.inner.clicked() {
            state.remove_requested = true;
        }
    });

    ui.separator();
    ui.heading(details.info_frame);
    android_labelled_value(ui, "Format", &details.format);
    android_labelled_value(ui, "Duration", &details.duration);
    android_labelled_value(ui, "File size", &details.file_size);
    android_labelled_value(ui, "URI", &details.uri);
}

#[cfg(target_os = "android")]
fn android_tag_field(ui: &mut egui::Ui, label: &str, value: &mut String, editable: bool) {
    ui.label(label);
    ui.add_enabled_ui(editable, |ui| {
        ui.add_sized(
            [ui.available_width(), 48.0],
            egui::TextEdit::singleline(value),
        );
    });
}

#[cfg(target_os = "android")]
fn android_labelled_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.monospace(value);
}

#[cfg(not(target_os = "android"))]
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

#[cfg(not(target_os = "android"))]
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

#[cfg(not(target_os = "android"))]
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

#[cfg(not(target_os = "android"))]
fn labelled_value(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        ui.monospace(value);
    });
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
    match remove_id3_metadata(path) {
        Ok(()) => {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn context_with_key(key: egui::Key) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            events: vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        });
        ctx
    }

    #[test]
    fn android_file_info_consumes_escape_and_system_back() {
        for key in [egui::Key::Escape, egui::Key::BrowserBack] {
            let ctx = context_with_key(key);

            assert!(android_file_info_back_key_pressed(&ctx, true));
            assert!(!ctx.input(|input| input.key_pressed(key)));
            let _ = ctx.end_pass();
        }
    }

    #[test]
    fn android_system_back_is_untouched_when_file_info_lacks_priority() {
        let ctx = context_with_key(egui::Key::BrowserBack);

        assert!(!android_file_info_back_key_pressed(&ctx, false));
        assert!(ctx.input(|input| input.key_pressed(egui::Key::BrowserBack)));
        let _ = ctx.end_pass();
    }
}
