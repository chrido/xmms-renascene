//! eframe application lifecycle for the egui frontend.

use crate::app::command::AppCommand;
use crate::app::controller::AppController;
use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app::view_model::{equalizer_view_model, main_player_view_model, playlist_view_model};
use crate::app_state::AppState;

use super::preferences::PreferencesPage;
use super::runtime::EguiRuntime;
use super::{equalizer, main_player, playlist};

#[derive(Debug, Default)]
pub struct EguiTextureCache {
    pub generation: u64,
}

#[derive(Debug)]
pub struct EguiFrontendState {
    pub preferences_open: bool,
    pub selected_preferences_page: PreferencesPage,
    pub texture_cache: EguiTextureCache,
    pub scale_factor: f32,
    pub dock_panels: bool,
    pub runtime: EguiRuntime,
    controller: AppController,
}

impl EguiFrontendState {
    pub fn new(options: PreviewOptions) -> Result<Self, String> {
        let mut app_state = AppState::default();
        if options.reset {
            app_state = AppState::default();
        }
        apply_preview_options_to_config(&mut app_state.config, &options)?;
        let scale_factor = app_state.config.scale_factor as f32;
        Ok(Self {
            preferences_open: options.open_preferences,
            selected_preferences_page: PreferencesPage::default(),
            texture_cache: EguiTextureCache::default(),
            scale_factor,
            dock_panels: true,
            runtime: EguiRuntime::default(),
            controller: AppController::new(app_state),
        })
    }

    pub fn controller(&self) -> &AppController {
        &self.controller
    }

    pub fn dispatch(&mut self, command: impl Into<AppCommand>) {
        let effects = self.controller.handle_command(command.into());
        self.runtime.apply_effects(effects);
    }
}

impl eframe::App for EguiFrontendState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("XMMS Renascene egui");
            let main = main_player_view_model(self.controller.state());
            ui.label(main_player::main_player_title(&main));
            let playlist_model = playlist_view_model(self.controller.state());
            ui.label(format!("Playlist rows: {}", playlist::playlist_row_count(&playlist_model)));
            let equalizer_model = equalizer_view_model(self.controller.state());
            ui.label(format!(
                "Equalizer bands: {}",
                equalizer::equalizer_band_count(&equalizer_model)
            ));
            if self.preferences_open {
                ui.separator();
                ui.heading("Preferences");
                ui.label("egui preferences are being implemented incrementally");
            }
        });
    }
}

pub fn run_egui_frontend(options: PreviewOptions) -> Result<(), String> {
    let app = EguiFrontendState::new(options)?;
    eframe::run_native(
        "XMMS Renascene egui",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|err| format!("failed to start egui frontend: {err}"))
}
