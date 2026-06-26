//! egui playlist panel/window.

use crate::app::command::{PlayerCommand, PlaylistCommand};
use crate::app::view_model::{playlist_view_model, PlaylistViewModel};

use super::app::EguiFrontendState;

pub fn playlist_row_count(view_model: &PlaylistViewModel) -> usize {
    view_model.rows.len()
}

pub fn show_playlist(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = playlist_view_model(app.controller().state());
    ui.heading("Playlist");
    ui.horizontal(|ui| {
        if ui.button("Select all").clicked() {
            app.dispatch(PlaylistCommand::SelectAll);
        }
        if ui.button("Invert").clicked() {
            app.dispatch(PlaylistCommand::InvertSelection);
        }
        if ui.button("Remove").clicked() {
            app.dispatch(PlaylistCommand::RemoveSelectedOrCurrent);
        }
        if ui.button("Reverse").clicked() {
            app.dispatch(PlaylistCommand::Reverse);
        }
        if ui.button("Randomize").clicked() {
            app.dispatch(PlaylistCommand::Randomize);
        }
    });

    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        for row in view_model.rows {
            let label = match row.duration_text.as_deref() {
                Some(duration) => format!("{:>3}. {} ({duration})", row.index + 1, row.title),
                None => format!("{:>3}. {}", row.index + 1, row.title),
            };
            let response = ui.selectable_label(row.selected || row.current, label);
            if response.clicked() {
                if let Some(entry) = app.controller_mut().state_mut().playlist.entries_mut().get_mut(row.index) {
                    entry.selected = !entry.selected;
                }
            }
            if response.double_clicked() {
                app.controller_mut().state_mut().playlist.set_position(row.index);
                app.dispatch(PlayerCommand::Play);
            }
        }
    });
}

pub fn playlist_menu_command(kind: crate::playlist::PlaylistMenuKind, index: usize) -> PlaylistCommand {
    PlaylistCommand::ExecuteMenu { kind, index }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::PlaylistMenuKind;

    #[test]
    fn playlist_menu_translation_uses_playlist_command_domain() {
        assert_eq!(
            playlist_menu_command(PlaylistMenuKind::Add, 2),
            PlaylistCommand::ExecuteMenu {
                kind: PlaylistMenuKind::Add,
                index: 2,
            }
        );
    }
}
