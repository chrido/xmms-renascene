//! eframe application lifecycle for the egui frontend.

use std::fs;
use std::path::{Path, PathBuf};

use crate::app::command::{AppCommand, PlaylistCommand};
use crate::app::controller::AppController;
use crate::app::effect::{AppEffect, FileDialogRequest};
use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app_state::AppState;
#[cfg(feature = "gstreamer-backend")]
use crate::player::GStreamerBackend;
use crate::render::{
    docked_panel_size, DockedPanelState, EqualizerControl, EqualizerSlider, MainPushButton,
    MainSlider, MainToggleButton, PlaylistMenuRenderKind, PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH,
};
use crate::session::default_config_dir;
use crate::skin::{discover_skins_in_dirs, skin_browser_search_dirs, DefaultSkin, SkinEntry};

use super::file_info;
use super::menu::{self, EguiPrompt};
use super::preferences::{self, PreferencesPage};
use super::runtime::EguiRuntime;
use super::{equalizer, main_player, playlist};

#[derive(Debug, Default)]
pub struct EguiTextureCache {
    pub generation: u64,
}

pub struct EguiFrontendState {
    pub main_menu_open: bool,
    pub preferences_open: bool,
    pub skin_browser_open: bool,
    pub file_info_open: bool,
    pub skin_entries: Vec<SkinEntry>,
    pub prompt_open: Option<EguiPrompt>,
    pub prompt_text: String,
    pub selected_preferences_page: PreferencesPage,
    pub texture_cache: EguiTextureCache,
    pub scale_factor: f32,
    pub dock_panels: bool,
    pub runtime: EguiRuntime,
    pub active_skin: DefaultSkin,
    pub main_pressed_push: Option<MainPushButton>,
    pub main_pressed_toggle: Option<MainToggleButton>,
    pub main_pressed_slider: Option<MainSlider>,
    pub equalizer_pressed_control: Option<EqualizerControl>,
    pub equalizer_pressed_slider: Option<EqualizerSlider>,
    pub equalizer_presets_open: bool,
    pub playlist_menu_hover: Option<(PlaylistMenuRenderKind, usize)>,
    pub playlist_menu_open: Option<PlaylistMenuRenderKind>,
    pub playlist_sort_menu_open: bool,
    pub playlist_scroll_offset: usize,
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
        let skin_entries = discover_runtime_skins();
        let scale_factor = app_state.config.scale_factor as f32;
        Ok(Self {
            main_menu_open: false,
            preferences_open: options.open_preferences,
            skin_browser_open: false,
            file_info_open: false,
            skin_entries,
            prompt_open: None,
            prompt_text: String::new(),
            selected_preferences_page: PreferencesPage::default(),
            texture_cache: EguiTextureCache::default(),
            scale_factor,
            dock_panels: true,
            runtime: EguiRuntime::default(),
            active_skin,
            main_pressed_push: None,
            main_pressed_toggle: None,
            main_pressed_slider: None,
            equalizer_pressed_control: None,
            equalizer_pressed_slider: None,
            equalizer_presets_open: false,
            playlist_menu_hover: None,
            playlist_menu_open: None,
            playlist_sort_menu_open: false,
            playlist_scroll_offset: 0,
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

    pub(crate) fn apply_effect(&mut self, effect: AppEffect) {
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
            AppEffect::OpenFileInfoDialog => self.file_info_open = true,
            AppEffect::OpenPreferences => self.preferences_open = true,
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

impl EguiFrontendState {
    fn desired_window_size(&self) -> egui::Vec2 {
        let config = &self.controller.state().config;
        let (width, height) = docked_panel_size(DockedPanelState {
            main_shaded: config.main_shaded,
            equalizer_visible: config.equalizer_visible,
            equalizer_detached: config.equalizer_detached,
            equalizer_shaded: config.equalizer_shaded,
            playlist_visible: config.playlist_visible,
            playlist_detached: config.playlist_detached,
            playlist_shaded: config.playlist_shaded,
            playlist_width: PLAYLIST_DEFAULT_WIDTH,
            playlist_height: PLAYLIST_DEFAULT_HEIGHT,
            ..DockedPanelState::default()
        });
        egui::vec2(
            width as f32 * self.scale_factor,
            height as f32 * self.scale_factor,
        )
    }
}

impl eframe::App for EguiFrontendState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_playback_backend();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.desired_window_size()));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                main_player::show_main_player(ui, self);
                if self.controller.state().config.equalizer_visible {
                    equalizer::show_equalizer(ui, self);
                }
                if self.controller.state().config.playlist_visible {
                    playlist::show_playlist(ui, self);
                }
            });
        menu::show_main_menu(ctx, self);
        menu::show_prompts(ctx, self);
        if self.preferences_open {
            preferences::show_preferences(ctx, self);
        }
        file_info::show_file_info_dialog(ctx, self);
        if self.skin_browser_open {
            show_skin_browser_placeholder(ctx, self);
        }
        menu::show_pending_messages(ctx, self);
    }
}

pub fn run_egui_frontend(options: PreviewOptions) -> Result<(), String> {
    let app = EguiFrontendState::new(options)?;
    let window_size = app.desired_window_size();
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

fn show_skin_browser_placeholder(ctx: &egui::Context, app: &mut EguiFrontendState) {
    let mut open = app.skin_browser_open;
    egui::Window::new("Skin selector")
        .open(&mut open)
        .resizable(true)
        .default_width(360.0)
        .show(ctx, |ui| {
            if ui.button("Refresh").clicked() {
                app.skin_entries = discover_runtime_skins();
            }
            ui.horizontal(|ui| {
                if ui.button("Default").clicked() {
                    match DefaultSkin::load_bundled() {
                        Ok(skin) => {
                            app.active_skin = skin;
                            app.controller_mut().state_mut().config.skin = None;
                        }
                        Err(err) => app.runtime.pending_messages.push(format!(
                            "failed to load bundled default skin: {err}"
                        )),
                    }
                }
                if ui.button("Add...").clicked() {
                    import_skin_from_dialog(app);
                }
                if ui.button("Close").clicked() {
                    app.skin_browser_open = false;
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for entry in app.skin_entries.clone() {
                    let selected = app
                        .controller()
                        .state()
                        .config
                        .skin
                        .as_deref()
                        == Some(entry.path.to_string_lossy().as_ref());
                    if ui.selectable_label(selected, &entry.name).clicked() {
                        select_skin_entry(app, &entry);
                    }
                }
                if app.skin_entries.is_empty() {
                    ui.label("No skins found in configured skin directories.");
                }
            });
        });
    app.skin_browser_open = open && app.skin_browser_open;
}

fn select_skin_entry(app: &mut EguiFrontendState, entry: &SkinEntry) {
    match DefaultSkin::load_from_path(&entry.path) {
        Ok(skin) => {
            app.active_skin = skin;
            app.controller_mut().state_mut().config.skin = Some(entry.path.to_string_lossy().into_owned());
        }
        Err(err) => app.runtime.pending_messages.push(format!(
            "failed to load skin '{}': {err}",
            entry.path.display()
        )),
    }
}

fn import_skin_from_dialog(app: &mut EguiFrontendState) {
    let Some(path) = rfd::FileDialog::new().set_title("Add skin").pick_file() else {
        return;
    };
    match import_skin_to_user_dir(&path) {
        Ok(imported) => {
            app.skin_entries = discover_runtime_skins();
            let entry = SkinEntry {
                name: imported
                    .file_stem()
                    .or_else(|| imported.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("Imported skin")
                    .to_string(),
                path: imported,
            };
            select_skin_entry(app, &entry);
        }
        Err(err) => app
            .runtime
            .pending_messages
            .push(format!("failed to import skin '{}': {err}", path.display())),
    }
}

fn import_skin_to_user_dir(source: &Path) -> std::io::Result<PathBuf> {
    let user_skin_dir = default_config_dir().join("xmms").join("Skins");
    fs::create_dir_all(&user_skin_dir)?;
    let name = source.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("skin path has no file name: {}", source.display()),
        )
    })?;
    let destination = user_skin_dir.join(name);
    if source.is_dir() {
        copy_dir_recursive(source, &destination)?;
    } else {
        fs::copy(source, &destination)?;
    }
    Ok(destination)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn discover_runtime_skins() -> Vec<SkinEntry> {
    let home_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let system_skin_dir = std::env::var_os("XMMS_RS_SYSTEM_SKIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/xmms/Skins"));
    let skinsdir = std::env::var("SKINSDIR").ok();
    let dirs = skin_browser_search_dirs(
        &default_config_dir(),
        &home_dir,
        &system_skin_dir,
        skinsdir.as_deref(),
    );
    discover_skins_in_dirs(dirs).unwrap_or_default()
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
        assert_eq!(
            app.selected_preferences_page,
            PreferencesPage::AudioIoPlugins
        );
    }

    #[test]
    fn egui_dispatch_mutates_config_through_controller() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();

        app.dispatch(PanelCommand::SetPlaylistVisibility(true));

        assert!(app.controller().state().config.playlist_visible);
        assert!(app.runtime.repaint_requested);
    }
}
