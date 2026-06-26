//! eframe application lifecycle for the egui frontend.

use std::path::Path;

use crate::app::command::{AppCommand, PlaylistCommand};
use crate::app::controller::AppController;
use crate::app::effect::{AppEffect, FileDialogRequest};
use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app_state::AppState;
#[cfg(feature = "gstreamer-backend")]
use crate::player::GStreamerBackend;
use crate::render::{
    main_window_height, MainPushButton, MainSlider, MainToggleButton, MAIN_WINDOW_WIDTH,
};
use crate::skin::DefaultSkin;

use super::preferences::{self, PreferencesPage};
use super::runtime::EguiRuntime;
use super::{equalizer, main_player, playlist};

#[derive(Debug, Default)]
pub struct EguiTextureCache {
    pub generation: u64,
}

pub struct EguiFrontendState {
    pub preferences_open: bool,
    pub selected_preferences_page: PreferencesPage,
    pub texture_cache: EguiTextureCache,
    pub scale_factor: f32,
    pub dock_panels: bool,
    pub runtime: EguiRuntime,
    pub active_skin: DefaultSkin,
    pub main_pressed_push: Option<MainPushButton>,
    pub main_pressed_toggle: Option<MainToggleButton>,
    pub main_pressed_slider: Option<MainSlider>,
    controller: AppController,
    #[cfg(feature = "gstreamer-backend")]
    playback_backend: Option<GStreamerBackend>,
}

impl EguiFrontendState {
    pub fn new(options: PreviewOptions) -> Result<Self, String> {
        let mut app_state = AppState::default();
        if options.reset {
            app_state = AppState::default();
        }
        apply_preview_options_to_config(&mut app_state.config, &options)?;
        let active_skin = load_skin_from_config(&app_state)?;
        let scale_factor = app_state.config.scale_factor as f32;
        Ok(Self {
            preferences_open: options.open_preferences,
            selected_preferences_page: PreferencesPage::default(),
            texture_cache: EguiTextureCache::default(),
            scale_factor,
            dock_panels: true,
            runtime: EguiRuntime::default(),
            active_skin,
            main_pressed_push: None,
            main_pressed_toggle: None,
            main_pressed_slider: None,
            controller: AppController::new(app_state),
            #[cfg(feature = "gstreamer-backend")]
            playback_backend: GStreamerBackend::new().ok(),
        })
    }

    pub fn controller(&self) -> &AppController {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut AppController {
        &mut self.controller
    }

    pub fn dispatch(&mut self, command: impl Into<AppCommand>) {
        let effects = self.controller.handle_command(command.into());
        self.apply_effects(effects);
    }

    pub fn poll_playback_backend(&mut self) {
        #[cfg(feature = "gstreamer-backend")]
        if let Some(backend) = &self.playback_backend {
            match backend.poll_bus_events() {
                Ok(events) => {
                    for event in events {
                        let effects = self.controller.handle_playback_event(event);
                        self.runtime.apply_effects(effects);
                    }
                }
                Err(err) => self.runtime.pending_messages.push(err),
            }
        }
    }

    fn apply_effects(&mut self, effects: impl IntoIterator<Item = AppEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    fn apply_effect(&mut self, effect: AppEffect) {
        #[cfg(feature = "gstreamer-backend")]
        if let Some(backend) = &self.playback_backend {
            match &effect {
                AppEffect::StartPlaybackUri { uri, position_ms } => {
                    if let Err(err) = backend.play_uri(uri) {
                        self.runtime.pending_messages.push(err);
                    } else if *position_ms > 0 {
                        let _ = backend.seek_to_ms(*position_ms);
                    }
                }
                AppEffect::ResumePlayback => {
                    if let Err(err) = backend.unpause() {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::PausePlayback => {
                    if let Err(err) = backend.pause() {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::StopPlayback => {
                    if let Err(err) = backend.stop() {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::SeekPlayback(position_ms) => {
                    if let Err(err) = backend.seek_to_ms(*position_ms) {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::SetBackendVolume(volume) => backend.set_volume_percent(*volume),
                AppEffect::SetBackendBalance(balance) => backend.set_balance_percent(*balance),
                AppEffect::SetBackendEqualizer => {
                    let config = &self.controller.state().config;
                    backend.set_equalizer_from_positions(
                        config.equalizer_active,
                        config.equalizer_preamp_pos,
                        config.equalizer_band_pos,
                    );
                }
                _ => {}
            }
        }
        match effect {
            AppEffect::OpenFileDialog(request) => self.handle_file_dialog(request),
            other => self.runtime.apply_effect(other),
        }
    }

    fn handle_file_dialog(&mut self, request: FileDialogRequest) {
        match request {
            FileDialogRequest::AddAudioFiles => {
                if let Some(files) = rfd::FileDialog::new()
                    .set_title("Add audio files")
                    .pick_files()
                {
                    self.dispatch(PlaylistCommand::AddFiles(files));
                }
            }
            FileDialogRequest::AddAudioDirectory => {
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title("Add audio directory")
                    .pick_folder()
                {
                    self.dispatch(PlaylistCommand::AddFiles(vec![folder]));
                }
            }
            FileDialogRequest::LoadPlaylist => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Load playlist")
                    .pick_file()
                {
                    self.runtime.pending_messages.push(format!(
                        "playlist loading pending egui handler: {}",
                        path.display()
                    ));
                }
            }
            FileDialogRequest::SavePlaylist => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save playlist")
                    .save_file()
                {
                    self.runtime.pending_messages.push(format!(
                        "playlist saving pending egui handler: {}",
                        path.display()
                    ));
                }
            }
            FileDialogRequest::LoadEqualizerPreset
            | FileDialogRequest::SaveEqualizerPreset
            | FileDialogRequest::ImportSkin
            | FileDialogRequest::ExportSkin => self
                .runtime
                .pending_messages
                .push(format!("file dialog pending egui handler: {request:?}")),
        }
    }
}

impl eframe::App for EguiFrontendState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_playback_backend();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                main_player::show_main_player(ui, self);
                if self.controller.state().config.playlist_visible {
                    ui.separator();
                    playlist::show_playlist(ui, self);
                }
                if self.controller.state().config.equalizer_visible {
                    ui.separator();
                    equalizer::show_equalizer(ui, self);
                }
            });
        if self.preferences_open {
            preferences::show_preferences(ctx, self);
        }
    }
}

pub fn run_egui_frontend(options: PreviewOptions) -> Result<(), String> {
    let app = EguiFrontendState::new(options)?;
    let window_size = egui::vec2(
        MAIN_WINDOW_WIDTH as f32 * app.scale_factor,
        main_window_height(app.controller.state().config.main_shaded) as f32 * app.scale_factor,
    );
    eframe::run_native(
        "XMMS Renascene egui",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(window_size)
                .with_decorations(false)
                .with_resizable(false),
            ..eframe::NativeOptions::default()
        },
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|err| format!("failed to start egui frontend: {err}"))
}

fn load_skin_from_config(app_state: &AppState) -> Result<DefaultSkin, String> {
    match app_state.config.skin.as_deref() {
        Some(path) => DefaultSkin::load_from_path(Path::new(path))
            .map_err(|err| format!("failed to load skin '{}': {err}", path)),
        None => {
            DefaultSkin::load_bundled().map_err(|err| format!("failed to load bundled skin: {err}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::PanelCommand;

    #[test]
    fn egui_app_constructs_without_native_window() {
        let options = PreviewOptions {
            open_preferences: true,
            ..PreviewOptions::default()
        };

        let app = EguiFrontendState::new(options).unwrap();

        assert!(app.preferences_open);
        assert_eq!(app.selected_preferences_page, PreferencesPage::Options);
    }

    #[test]
    fn egui_dispatch_mutates_config_through_controller() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();

        app.dispatch(PanelCommand::SetPlaylistVisibility(true));

        assert!(app.controller().state().config.playlist_visible);
        assert!(app.runtime.repaint_requested);
    }
}
