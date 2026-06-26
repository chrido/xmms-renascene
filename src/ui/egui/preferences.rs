//! egui preferences dialog/window.

use crate::app::command::{PanelCommand, PlaylistCommand};
use crate::app::view_model::format_title_for_preferences;

use super::app::EguiFrontendState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesPage {
    #[default]
    Options,
    Audio,
    Playlist,
    Visualization,
    Titles,
}

pub fn show_preferences(ctx: &egui::Context, app: &mut EguiFrontendState) {
    let mut open = app.preferences_open;
    egui::Window::new("Preferences")
        .open(&mut open)
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for page in [
                    PreferencesPage::Options,
                    PreferencesPage::Audio,
                    PreferencesPage::Playlist,
                    PreferencesPage::Visualization,
                    PreferencesPage::Titles,
                ] {
                    if ui
                        .selectable_label(app.selected_preferences_page == page, page.label())
                        .clicked()
                    {
                        app.selected_preferences_page = page;
                    }
                }
            });
            ui.separator();
            match app.selected_preferences_page {
                PreferencesPage::Options => show_options_page(ui, app),
                PreferencesPage::Audio => show_audio_page(ui, app),
                PreferencesPage::Playlist => show_playlist_page(ui, app),
                PreferencesPage::Visualization => show_visualization_page(ui, app),
                PreferencesPage::Titles => show_titles_page(ui, app),
            }
        });
    app.preferences_open = open;
}

impl PreferencesPage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Options => "Options",
            Self::Audio => "Audio",
            Self::Playlist => "Playlist",
            Self::Visualization => "Visualization",
            Self::Titles => "Titles",
        }
    }
}

fn show_options_page(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let config = &app.controller().state().config;
    let mut main_shaded = config.main_shaded;
    let mut playlist_visible = config.playlist_visible;
    let mut equalizer_visible = config.equalizer_visible;
    if ui.checkbox(&mut main_shaded, "Main window shaded").changed() {
        app.dispatch(PanelCommand::SetMainShade(main_shaded));
    }
    if ui.checkbox(&mut playlist_visible, "Show playlist").changed() {
        app.dispatch(PanelCommand::SetPlaylistVisibility(playlist_visible));
    }
    if ui.checkbox(&mut equalizer_visible, "Show equalizer").changed() {
        app.dispatch(PanelCommand::SetEqualizerVisibility(equalizer_visible));
    }
}

fn show_audio_page(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let mut volume = app.controller().state().player.volume();
    let mut balance = app.controller().state().player.balance();
    if ui.add(egui::Slider::new(&mut volume, 0..=100).text("Startup volume")).changed() {
        app.controller_mut().state_mut().config.volume = volume;
    }
    if ui
        .add(egui::Slider::new(&mut balance, -100..=100).text("Startup balance"))
        .changed()
    {
        app.controller_mut().state_mut().config.balance = balance;
    }
}

fn show_playlist_page(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let state = app.controller().state();
    let mut shuffle = state.playlist.shuffle();
    let mut repeat = state.playlist.repeat();
    let mut no_advance = state.playlist.no_advance();
    if ui.checkbox(&mut shuffle, "Shuffle").changed() {
        app.dispatch(PlaylistCommand::ToggleShuffle);
    }
    if ui.checkbox(&mut repeat, "Repeat").changed() {
        app.dispatch(PlaylistCommand::ToggleRepeat);
    }
    if ui.checkbox(&mut no_advance, "No playlist advance").changed() {
        app.dispatch(PlaylistCommand::ToggleNoAdvance);
    }
}

fn show_visualization_page(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let config = &mut app.controller_mut().state_mut().config;
    let mut peaks = config.vis_peaks_enabled;
    if ui.checkbox(&mut peaks, "Analyzer peaks").changed() {
        config.vis_peaks_enabled = peaks;
    }
    let mut falloff = config.vis_falloff;
    if ui
        .add(egui::Slider::new(&mut falloff, 0.001..=0.25).text("Visualization falloff"))
        .changed()
    {
        config.vis_falloff = falloff;
    }
}

fn show_titles_page(ui: &mut egui::Ui, app: &mut EguiFrontendState) {
    let config = &mut app.controller_mut().state_mut().config;
    ui.label("Title format");
    ui.text_edit_singleline(&mut config.title_format);
    let preview = format_title_for_preferences(
        &config.title_format,
        "file:///tmp/Example_Artist%20-%20Example_Title.mp3",
        "Example Artist - Example Title",
        config,
    );
    ui.label(format!("Preview: {preview}"));
}

pub fn page_labels() -> Vec<&'static str> {
    [
        PreferencesPage::Options,
        PreferencesPage::Audio,
        PreferencesPage::Playlist,
        PreferencesPage::Visualization,
        PreferencesPage::Titles,
    ]
    .into_iter()
    .map(PreferencesPage::label)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_pages_have_stable_labels() {
        assert_eq!(page_labels(), vec!["Options", "Audio", "Playlist", "Visualization", "Titles"]);
    }
}
