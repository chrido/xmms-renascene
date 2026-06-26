//! egui main player panel/window.

use crate::app::command::{AudioCommand, PanelCommand, PlayerCommand, PlaylistCommand};
use crate::app::view_model::{main_player_view_model, MainPlayerViewModel};

use super::app::EguiFrontendState;

pub fn main_player_title(view_model: &MainPlayerViewModel) -> &str {
    &view_model.title
}

pub fn show_main_player(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let view_model = main_player_view_model(app.controller().state());
    ui.heading("XMMS Renascene");
    ui.label(if view_model.title.is_empty() {
        "XMMS Renascene"
    } else {
        main_player_title(&view_model)
    });
    ui.horizontal(|ui| {
        if ui.button("⏮").clicked() {
            app.dispatch(PlayerCommand::PreviousTrack);
        }
        if ui.button("▶").clicked() {
            app.dispatch(PlayerCommand::Play);
        }
        if ui.button("⏸").clicked() {
            app.dispatch(PlayerCommand::TogglePause);
        }
        if ui.button("■").clicked() {
            app.dispatch(PlayerCommand::Stop);
        }
        if ui.button("⏭").clicked() {
            app.dispatch(PlayerCommand::NextTrack);
        }
    });

    ui.horizontal(|ui| {
        let mut volume = view_model.volume;
        if ui.add(egui::Slider::new(&mut volume, 0..=100).text("volume")).changed() {
            app.dispatch(AudioCommand::SetVolume(volume));
        }
        let mut balance = view_model.balance;
        if ui
            .add(egui::Slider::new(&mut balance, -100..=100).text("balance"))
            .changed()
        {
            app.dispatch(AudioCommand::SetBalance(balance));
        }
    });

    ui.horizontal(|ui| {
        if ui.selectable_label(view_model.shuffle, "Shuffle").clicked() {
            app.dispatch(PlaylistCommand::ToggleShuffle);
        }
        if ui.selectable_label(view_model.repeat, "Repeat").clicked() {
            app.dispatch(PlaylistCommand::ToggleRepeat);
        }
        if ui.selectable_label(view_model.shaded, "Shade").clicked() {
            app.dispatch(PanelCommand::ToggleMainShade);
        }
        if ui.button("Playlist").clicked() {
            app.dispatch(PanelCommand::TogglePlaylistVisibility);
        }
        if ui.button("Equalizer").clicked() {
            app.dispatch(PanelCommand::ToggleEqualizerVisibility);
        }
    });

    ui.label(format!(
        "state: {:?} | bitrate: {} | frequency: {} | channels: {}",
        view_model.player_state,
        view_model.bitrate_text,
        view_model.frequency_text,
        view_model.channels_text
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::AppCommand;

    #[test]
    fn main_player_commands_use_hierarchical_domains() {
        assert_eq!(AppCommand::from(PlayerCommand::Play), AppCommand::Player(PlayerCommand::Play));
        assert_eq!(
            AppCommand::from(AudioCommand::SetVolume(50)),
            AppCommand::Audio(AudioCommand::SetVolume(50))
        );
        assert_eq!(
            AppCommand::from(PanelCommand::TogglePlaylistVisibility),
            AppCommand::Panel(PanelCommand::TogglePlaylistVisibility)
        );
    }
}
