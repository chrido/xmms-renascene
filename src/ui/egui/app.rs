//! eframe application lifecycle for the egui frontend.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app::command::{
    AppCommand, EqualizerCommand, PanelCommand, PlayerCommand, PlaylistCommand, UiCommand,
};
use crate::app::effect::{AppEffect, FileDialogRequest};
use crate::app::input::AppShortcut;
use crate::app::logging::{console_log, ConsoleLogLevel};
use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app::store::AppStore;
use crate::app::view_model::{
    balance_to_eq_shaded_position, equalizer_view_model, format_playlist_footer_duration,
    playlist_view_model, volume_to_eq_shaded_position,
};
use crate::app_state::AppState;
use crate::equalizer::{
    load_winamp_eqf_first, load_xmms_preset_file, save_winamp_eqf, save_xmms_preset_file,
    EqualizerPreset,
};
#[cfg(not(feature = "gstreamer-backend"))]
use crate::player::PlayerState;
#[cfg(feature = "gstreamer-backend")]
use crate::player::{GStreamerBackend, PlaybackEvent, PlayerState};
use crate::playlist::Playlist;
use crate::render::{
    docked_panel_size, DockedPanelState, EqualizerControl, EqualizerRenderState, EqualizerSlider,
    MainPushButton, MainSlider, MainToggleButton, PlaylistMenuRenderKind, PlaylistRowRenderEntry,
    PlaylistRowsRenderState, EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH,
};
use crate::session::default_config_dir;
use crate::skin::layout::{panel_title_button_rect, LayoutPanelKind, PanelTitleButton};
use crate::skin::{discover_skins_in_dirs, skin_browser_search_dirs, DefaultSkin, SkinEntry};
use crate::socket_control::{
    start_socket_control, SocketCommand, SocketControl, SocketRequest, SocketUiCommand,
};

use super::file_info;
use super::menu::{self, EguiPrompt};
use super::preferences::{self, PreferencesPage, PreferencesViewportState};
use super::runtime::EguiRuntime;
use super::skin_texture::{render_equalizer_color_image, render_playlist_color_image};
use super::{equalizer, main_player, playlist};

#[derive(Debug, Default)]
pub struct EguiTextureCache {
    pub generation: u64,
}

#[derive(Debug, Clone)]
struct DetachedPanelSnapshot {
    panel: LayoutPanelKind,
    image: egui::ColorImage,
    width: i32,
    height: i32,
    scale_factor: f32,
}

#[derive(Debug, Default)]
struct DetachedViewportState {
    equalizer: Option<DetachedPanelSnapshot>,
    playlist: Option<DetachedPanelSnapshot>,
    commands: Vec<PanelCommand>,
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
    pub preferences_viewport: Arc<Mutex<PreferencesViewportState>>,
    detached_viewports: Arc<Mutex<DetachedViewportState>>,
    pub texture_cache: EguiTextureCache,
    pub last_tick: Instant,
    pub scale_factor: f32,
    pub dock_panels: bool,
    pub runtime: EguiRuntime,
    pub active_skin: DefaultSkin,
    pub main_pressed_push: Option<MainPushButton>,
    pub main_pressed_toggle: Option<MainToggleButton>,
    pub main_pressed_slider: Option<MainSlider>,
    pub equalizer_pressed_control: Option<EqualizerControl>,
    pub equalizer_pressed_slider: Option<EqualizerSlider>,
    pub equalizer_keyboard_slider: Option<EqualizerSlider>,
    pub equalizer_presets_open: bool,
    pub playlist_menu_hover: Option<(PlaylistMenuRenderKind, usize)>,
    pub playlist_menu_open: Option<PlaylistMenuRenderKind>,
    pub playlist_sort_menu_open: bool,
    pub confirm_physical_delete_open: bool,
    pub playlist_scroll_offset: usize,
    socket_control: Option<SocketControl>,
    controller: AppStore,
    #[cfg(feature = "gstreamer-backend")]
    playback_backend: Option<GStreamerBackend>,
    pending_backend_seek_ms: Option<i64>,
}

impl EguiFrontendState {
    pub fn new(options: PreviewOptions) -> Result<Self, String> {
        let mut app_state = AppState::default();
        if options.reset {
            app_state = AppState::default();
        }
        apply_preview_options_to_config(&mut app_state.config, &options)?;
        app_state.ui.preferences_visible = options.open_preferences;
        let active_skin = load_skin_from_config(&app_state)?;
        let skin_entries = discover_runtime_skins();
        let scale_factor = app_state.config.scale_factor as f32;
        let preferences_viewport = Arc::new(Mutex::new(PreferencesViewportState::new(
            &app_state.config,
            PreferencesPage::default(),
            options.open_preferences,
        )));
        let detached_viewports = Arc::new(Mutex::new(DetachedViewportState::default()));
        let socket_control = options.socket_port.map(start_socket_control).transpose()?;
        Ok(Self {
            main_menu_open: false,
            preferences_open: options.open_preferences,
            skin_browser_open: false,
            file_info_open: false,
            skin_entries,
            prompt_open: None,
            prompt_text: String::new(),
            selected_preferences_page: PreferencesPage::default(),
            preferences_viewport,
            detached_viewports,
            texture_cache: EguiTextureCache::default(),
            last_tick: Instant::now(),
            scale_factor,
            dock_panels: true,
            runtime: EguiRuntime::default(),
            active_skin,
            main_pressed_push: None,
            main_pressed_toggle: None,
            main_pressed_slider: None,
            equalizer_pressed_control: None,
            equalizer_pressed_slider: None,
            equalizer_keyboard_slider: None,
            equalizer_presets_open: false,
            playlist_menu_hover: None,
            playlist_menu_open: None,
            playlist_sort_menu_open: false,
            confirm_physical_delete_open: false,
            playlist_scroll_offset: 0,
            socket_control,
            controller: AppStore::new(app_state),
            #[cfg(feature = "gstreamer-backend")]
            playback_backend: GStreamerBackend::new().ok(),
            pending_backend_seek_ms: None,
        })
    }

    pub fn controller(&self) -> &AppStore {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut AppStore {
        &mut self.controller
    }

    pub fn dispatch(&mut self, command: impl Into<AppCommand>) {
        let result = self.controller.dispatch(command.into());
        self.sync_frontend_state_from_store();
        self.apply_effects(result.effects);
    }

    fn sync_frontend_state_from_store(&mut self) {
        let ui = &self.controller.state().ui;
        self.preferences_open = ui.preferences_visible;
        self.main_menu_open = ui.main_menu_visible;
        self.skin_browser_open = ui.skin_browser_visible;
        self.file_info_open = ui.file_info_visible;
    }

    fn poll_socket_control(&mut self, ctx: &egui::Context) {
        loop {
            let Some(request) = self
                .socket_control
                .as_ref()
                .and_then(SocketControl::try_recv)
            else {
                break;
            };
            self.handle_socket_request(ctx, request);
        }
    }

    fn handle_socket_request(&mut self, ctx: &egui::Context, request: SocketRequest) {
        match request.command.clone() {
            SocketCommand::App(command) => {
                self.dispatch(command);
                request.accept();
            }
            SocketCommand::Ui(command) => {
                self.handle_socket_ui_command(command);
                request.accept();
            }
            SocketCommand::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                request.accept();
            }
        }
    }

    fn handle_socket_ui_command(&mut self, command: SocketUiCommand) {
        let command = match command {
            SocketUiCommand::SetPreferencesVisible(visible) => {
                UiCommand::SetPreferencesVisible(visible)
            }
            SocketUiCommand::TogglePreferences => UiCommand::TogglePreferences,
            SocketUiCommand::SetMainMenuVisible(visible) => UiCommand::SetMainMenuVisible(visible),
            SocketUiCommand::SetSkinBrowserVisible(visible) => {
                UiCommand::SetSkinBrowserVisible(visible)
            }
            SocketUiCommand::ToggleSkinBrowser => UiCommand::ToggleSkinBrowser,
        };
        self.dispatch(command);
    }

    pub fn poll_playback_backend(&mut self) {
        #[cfg(feature = "gstreamer-backend")]
        if let Some(backend) = &self.playback_backend {
            match backend.poll_bus_events() {
                Ok(events) => {
                    let mut backend_ready = false;
                    for event in events {
                        if matches!(
                            event,
                            PlaybackEvent::AsyncDone | PlaybackEvent::DurationChanged(_)
                        ) {
                            backend_ready = true;
                        }
                        let result = self.controller.handle_playback_event(event);
                        self.sync_frontend_state_from_store();
                        self.runtime.apply_effects(result.effects);
                    }
                    if backend_ready {
                        self.apply_pending_backend_seek();
                    }
                }
                Err(err) => self.runtime.pending_messages.push(err),
            }
        }
    }

    fn apply_pending_backend_seek(&mut self) {
        #[cfg(feature = "gstreamer-backend")]
        if let (Some(backend), Some(position_ms)) =
            (&self.playback_backend, self.pending_backend_seek_ms)
        {
            match backend.seek_to_ms(position_ms) {
                Ok(()) => {
                    console_log(
                        ConsoleLogLevel::Info,
                        format_args!(
                            "backend: egui applied pending start seek position_ms={position_ms}"
                        ),
                    );
                    self.pending_backend_seek_ms = None;
                }
                Err(err) => self.runtime.pending_messages.push(err),
            }
        }
    }

    fn tick_playback_position(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(250));
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        if self.controller.state().player.state() != PlayerState::Playing {
            return;
        }
        let elapsed_ms = elapsed.as_millis().min(i64::MAX as u128) as i64;
        if elapsed_ms == 0 {
            return;
        }
        let result = self.controller.tick_playback_position(elapsed_ms);
        self.sync_frontend_state_from_store();
        self.apply_effects(result.effects);
    }

    pub(crate) fn apply_effects(&mut self, effects: impl IntoIterator<Item = AppEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    pub(crate) fn apply_effect(&mut self, effect: AppEffect) {
        console_log(
            ConsoleLogLevel::Debug,
            format_args!("frontend-effect: egui {effect:?}"),
        );
        #[cfg(feature = "gstreamer-backend")]
        if let Some(backend) = &self.playback_backend {
            match &effect {
                AppEffect::StartPlaybackUri { uri, position_ms } => {
                    console_log(
                        ConsoleLogLevel::Info,
                        format_args!(
                            "backend: egui play_uri uri={uri} start_position_ms={position_ms} pending_seek={}",
                            *position_ms > 0
                        ),
                    );
                    if let Err(err) = backend.play_uri(uri) {
                        self.runtime.pending_messages.push(err);
                    } else if *position_ms > 0 {
                        self.pending_backend_seek_ms = Some(*position_ms);
                    } else {
                        self.pending_backend_seek_ms = None;
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
                AppEffect::BeginStopFade { .. } => {
                    if let Err(err) = backend.stop() {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::SeekPlayback(position_ms) => {
                    console_log(
                        ConsoleLogLevel::Info,
                        format_args!("backend: egui seek_to_ms position_ms={position_ms}"),
                    );
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
            AppEffect::OpenFileInfoDialog => self.dispatch(UiCommand::SetFileInfoVisible(true)),
            AppEffect::OpenPreferences => self.dispatch(UiCommand::SetPreferencesVisible(true)),
            AppEffect::OpenSkinBrowser => self.dispatch(UiCommand::SetSkinBrowserVisible(true)),
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
                    .add_filter("M3U playlists", &["m3u", "m3u8"])
                    .pick_file()
                {
                    self.load_playlist_file(&path);
                }
            }
            FileDialogRequest::SavePlaylist => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save playlist")
                    .add_filter("M3U playlists", &["m3u", "m3u8"])
                    .save_file()
                {
                    self.save_playlist_file(&path);
                }
            }
            FileDialogRequest::LoadEqualizerPreset => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Load equalizer preset")
                    .add_filter("Equalizer presets", &["preset", "eqf"])
                    .pick_file()
                {
                    self.load_equalizer_preset_file(&path);
                }
            }
            FileDialogRequest::SaveEqualizerPreset => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save equalizer preset")
                    .add_filter("Equalizer presets", &["preset", "eqf"])
                    .save_file()
                {
                    self.save_equalizer_preset_file(&path);
                }
            }
            FileDialogRequest::ImportSkin => import_skin_from_dialog(self),
            FileDialogRequest::ExportSkin => self
                .runtime
                .pending_messages
                .push("skin export from egui is not needed for playback parity".to_string()),
        }
    }

    fn load_playlist_file(&mut self, path: &Path) {
        match Playlist::load_m3u_file(path) {
            Ok(playlist) => {
                let result = self.controller.replace_playlist_for_file_load(playlist);
                self.playlist_scroll_offset = 0;
                self.sync_frontend_state_from_store();
                self.apply_effects(result.effects);
            }
            Err(err) => self.runtime.pending_messages.push(format!(
                "failed to load playlist '{}': {err}",
                path.display()
            )),
        }
    }

    fn save_playlist_file(&mut self, path: &Path) {
        if let Err(err) = self.controller.state().playlist.save_m3u_file(path) {
            self.runtime.pending_messages.push(format!(
                "failed to save playlist '{}': {err}",
                path.display()
            ));
        }
    }

    fn load_equalizer_preset_file(&mut self, path: &Path) {
        let loaded = if is_winamp_eqf(path) {
            load_winamp_eqf_first(path)
        } else {
            load_xmms_preset_file(path)
        };
        match loaded {
            Ok(Some(preset)) => self.apply_equalizer_preset(&preset),
            Ok(None) => self
                .runtime
                .pending_messages
                .push(format!("no equalizer preset found in '{}'", path.display())),
            Err(err) => self.runtime.pending_messages.push(format!(
                "failed to load equalizer preset '{}': {err}",
                path.display()
            )),
        }
    }

    fn save_equalizer_preset_file(&mut self, path: &Path) {
        let preset = self.current_equalizer_preset(if is_winamp_eqf(path) {
            "Entry1"
        } else {
            "File"
        });
        let saved = if is_winamp_eqf(path) {
            save_winamp_eqf(path, &preset)
        } else {
            save_xmms_preset_file(path, &preset)
        };
        if let Err(err) = saved {
            self.runtime.pending_messages.push(format!(
                "failed to save equalizer preset '{}': {err}",
                path.display()
            ));
        }
    }

    fn current_equalizer_preset(&self, name: &str) -> EqualizerPreset {
        let config = &self.controller.state().config;
        EqualizerPreset::from_positions(
            name,
            config.equalizer_preamp_pos,
            config.equalizer_band_pos,
        )
    }

    fn apply_equalizer_preset(&mut self, preset: &EqualizerPreset) {
        let result = self
            .controller
            .apply_equalizer_preset_positions(preset.preamp_position(), preset.band_positions());
        self.sync_frontend_state_from_store();
        self.apply_effects(result.effects);
    }
}

fn is_winamp_eqf(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("eqf"))
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
        self.poll_socket_control(ctx);
        self.poll_playback_backend();
        self.tick_playback_position(ctx);
        handle_dropped_files(ctx, self);
        handle_global_shortcuts(ctx, self);
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.desired_window_size()));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                main_player::show_main_player(ui, self);
                if self.controller.state().config.equalizer_visible
                    && !self.controller.state().config.equalizer_detached
                {
                    equalizer::show_equalizer(ui, self);
                }
                if self.controller.state().config.playlist_visible
                    && !self.controller.state().config.playlist_detached
                {
                    playlist::show_playlist(ui, self);
                }
            });
        show_detached_panels(ctx, self);
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
        console_log(
            ConsoleLogLevel::Trace,
            format_args!("render: egui update size={:?}", self.desired_window_size()),
        );
    }
}

fn handle_dropped_files(ctx: &egui::Context, app: &mut EguiFrontendState) {
    let dropped: Vec<PathBuf> = ctx.input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect()
    });
    if !dropped.is_empty() {
        app.dispatch(PlaylistCommand::AddFiles(dropped));
    }
}

fn show_detached_panels(ctx: &egui::Context, app: &mut EguiFrontendState) {
    apply_detached_panel_commands(app);
    update_detached_panel_snapshots(app);
    let shared = Arc::clone(&app.detached_viewports);
    let config = app.controller().state().config.clone();
    if config.equalizer_visible && config.equalizer_detached {
        show_detached_panel_viewport(
            ctx,
            shared.clone(),
            "xmms-egui-detached-equalizer",
            "Equalizer",
            true,
        );
    }
    if config.playlist_visible && config.playlist_detached {
        show_detached_panel_viewport(
            ctx,
            shared,
            "xmms-egui-detached-playlist",
            "Playlist",
            false,
        );
    }
}

fn apply_detached_panel_commands(app: &mut EguiFrontendState) {
    let commands = {
        let mut state = app
            .detached_viewports
            .lock()
            .expect("detached viewport state poisoned");
        std::mem::take(&mut state.commands)
    };
    for command in commands {
        app.dispatch(command);
    }
}

fn update_detached_panel_snapshots(app: &mut EguiFrontendState) {
    let config = app.controller().state().config.clone();
    let equalizer = (config.equalizer_visible && config.equalizer_detached)
        .then(|| detached_equalizer_snapshot(app))
        .flatten();
    let playlist = (config.playlist_visible && config.playlist_detached)
        .then(|| detached_playlist_snapshot(app))
        .flatten();
    let mut state = app
        .detached_viewports
        .lock()
        .expect("detached viewport state poisoned");
    state.equalizer = equalizer;
    state.playlist = playlist;
}

fn detached_equalizer_snapshot(app: &EguiFrontendState) -> Option<DetachedPanelSnapshot> {
    let view_model = equalizer_view_model(app.controller().state());
    let render_state = EqualizerRenderState {
        focused: true,
        shaded: view_model.shaded,
        active: view_model.active,
        automatic: view_model.auto,
        pressed_control: app.equalizer_pressed_control,
        pressed_slider: app.equalizer_pressed_slider,
        preamp_position: view_model.preamp_position,
        band_positions: view_model.band_positions,
        volume_position: volume_to_eq_shaded_position(app.controller().state().player.volume()),
        balance_position: balance_to_eq_shaded_position(app.controller().state().player.balance()),
    };
    let image = render_equalizer_color_image(&app.active_skin, &render_state).ok()?;
    Some(DetachedPanelSnapshot {
        panel: LayoutPanelKind::Equalizer,
        width: EQUALIZER_WINDOW_WIDTH,
        height: if view_model.shaded {
            crate::render::MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        },
        scale_factor: app.scale_factor,
        image,
    })
}

fn detached_playlist_snapshot(app: &EguiFrontendState) -> Option<DetachedPanelSnapshot> {
    let view_model = playlist_view_model(app.controller().state());
    let rows = PlaylistRowsRenderState {
        entries: view_model
            .rows
            .iter()
            .map(|row| PlaylistRowRenderEntry {
                title: row.title.clone(),
                length_ms: app
                    .controller()
                    .state()
                    .playlist
                    .entries()
                    .get(row.index)
                    .map(|entry| entry.length_ms)
                    .unwrap_or(-1),
                selected: row.selected,
                current: row.current,
            })
            .collect(),
        scroll_offset: app.playlist_scroll_offset,
        scrollbar_dragging: false,
        search_query: None,
        show_numbers: app.controller().state().config.show_numbers_in_pl,
        font_family: app.controller().state().config.playlist_font.clone(),
        width: PLAYLIST_DEFAULT_WIDTH,
        height: PLAYLIST_DEFAULT_HEIGHT,
    };
    let footer_info = detached_playlist_footer_info(app);
    let (footer_min, footer_sec) = detached_playlist_footer_time_parts(app);
    let image = render_playlist_color_image(
        &app.active_skin,
        true,
        view_model.shaded,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
        &rows,
        Some(&footer_info),
        Some(&footer_min),
        Some(&footer_sec),
    )
    .ok()?;
    Some(DetachedPanelSnapshot {
        panel: LayoutPanelKind::Playlist,
        width: PLAYLIST_DEFAULT_WIDTH,
        height: if view_model.shaded {
            crate::render::MAIN_TITLEBAR_HEIGHT
        } else {
            PLAYLIST_DEFAULT_HEIGHT
        },
        scale_factor: app.scale_factor,
        image,
    })
}

fn show_detached_panel_viewport(
    ctx: &egui::Context,
    shared: Arc<Mutex<DetachedViewportState>>,
    id: &'static str,
    title: &'static str,
    equalizer_panel: bool,
) {
    let snapshot = {
        let state = shared.lock().expect("detached viewport state poisoned");
        if equalizer_panel {
            state.equalizer.clone()
        } else {
            state.playlist.clone()
        }
    };
    let Some(snapshot) = snapshot else {
        return;
    };
    let size = egui::vec2(
        snapshot.width as f32 * snapshot.scale_factor,
        snapshot.height as f32 * snapshot.scale_factor,
    );
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_min_inner_size(size)
        .with_resizable(!equalizer_panel)
        .with_decorations(false);
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of(id),
        builder,
        move |ctx, class| {
            if ctx.input(|input| input.viewport().close_requested()) {
                push_detached_command(
                    &shared,
                    if equalizer_panel {
                        PanelCommand::SetEqualizerVisibility(false)
                    } else {
                        PanelCommand::SetPlaylistVisibility(false)
                    },
                );
                return;
            }
            match class {
                egui::ViewportClass::Embedded | egui::ViewportClass::Root => {
                    show_embedded_detached_snapshot(ctx, &shared, title, equalizer_panel);
                }
                egui::ViewportClass::Deferred | egui::ViewportClass::Immediate => {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| show_detached_snapshot(ui, &shared, &snapshot));
                }
            }
        },
    );
}

fn show_embedded_detached_snapshot(
    ctx: &egui::Context,
    shared: &Arc<Mutex<DetachedViewportState>>,
    title: &str,
    equalizer_panel: bool,
) {
    let snapshot = {
        let state = shared.lock().expect("detached viewport state poisoned");
        if equalizer_panel {
            state.equalizer.clone()
        } else {
            state.playlist.clone()
        }
    };
    let Some(snapshot) = snapshot else {
        return;
    };
    let mut open = true;
    egui::Window::new(title)
        .open(&mut open)
        .resizable(!equalizer_panel)
        .constrain(false)
        .show(ctx, |ui| show_detached_snapshot(ui, shared, &snapshot));
    if !open {
        push_detached_command(
            shared,
            if equalizer_panel {
                PanelCommand::SetEqualizerVisibility(false)
            } else {
                PanelCommand::SetPlaylistVisibility(false)
            },
        );
    }
}

fn show_detached_snapshot(
    ui: &mut egui::Ui,
    shared: &Arc<Mutex<DetachedViewportState>>,
    snapshot: &DetachedPanelSnapshot,
) {
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    let size = egui::vec2(
        snapshot.width as f32 * snapshot.scale_factor,
        snapshot.height as f32 * snapshot.scale_factor,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let texture = ui.ctx().load_texture(
        format!("xmms-detached-{:?}", snapshot.panel),
        snapshot.image.clone(),
        egui::TextureOptions::NEAREST,
    );
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    if let Some(pos) = response.interact_pointer_pos() {
        if response.drag_started() && detached_panel_titlebar_drag_region(snapshot, rect, pos) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            return;
        }
        if response.clicked() {
            handle_detached_panel_click(shared, snapshot, rect, pos);
        }
    }
}

fn detached_panel_titlebar_drag_region(
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
    pos: egui::Pos2,
) -> bool {
    let x = ((pos.x - base_rect.left()) / snapshot.scale_factor).floor() as i32;
    let y = ((pos.y - base_rect.top()) / snapshot.scale_factor).floor() as i32;
    if y < 0 || y >= crate::render::MAIN_TITLEBAR_HEIGHT {
        return false;
    }
    ![PanelTitleButton::Shade, PanelTitleButton::Close]
        .into_iter()
        .any(|button| {
            panel_title_button_rect(snapshot.panel, button, snapshot.width).contains(x, y)
        })
}

fn handle_detached_panel_click(
    shared: &Arc<Mutex<DetachedViewportState>>,
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
    pos: egui::Pos2,
) {
    let x = ((pos.x - base_rect.left()) / snapshot.scale_factor).floor() as i32;
    let y = ((pos.y - base_rect.top()) / snapshot.scale_factor).floor() as i32;
    for button in [PanelTitleButton::Shade, PanelTitleButton::Close] {
        if panel_title_button_rect(snapshot.panel, button, snapshot.width).contains(x, y) {
            let command = match (snapshot.panel, button) {
                (LayoutPanelKind::Equalizer, PanelTitleButton::Shade) => {
                    PanelCommand::ToggleEqualizerShade
                }
                (LayoutPanelKind::Equalizer, PanelTitleButton::Close) => {
                    PanelCommand::SetEqualizerVisibility(false)
                }
                (LayoutPanelKind::Playlist, PanelTitleButton::Shade) => {
                    PanelCommand::TogglePlaylistShade
                }
                (LayoutPanelKind::Playlist, PanelTitleButton::Close) => {
                    PanelCommand::SetPlaylistVisibility(false)
                }
            };
            push_detached_command(shared, command);
            return;
        }
    }
}

fn push_detached_command(shared: &Arc<Mutex<DetachedViewportState>>, command: PanelCommand) {
    shared
        .lock()
        .expect("detached viewport state poisoned")
        .commands
        .push(command);
}

fn detached_playlist_footer_time_parts(app: &EguiFrontendState) -> (String, String) {
    if app.controller().state().player.state() == PlayerState::Stopped {
        return ("   ".to_string(), "  ".to_string());
    }
    let total_seconds = app.controller().state().config.playback_position_ms.max(0) / 1_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    (format!("{minutes:>3}"), format!("{seconds:02}"))
}

fn detached_playlist_footer_info(app: &EguiFrontendState) -> String {
    let mut selected_ms = 0_i64;
    let mut total_ms = 0_i64;
    let mut selected_more = false;
    let mut total_more = false;
    let current = app.controller().state().playlist.position();
    for (index, entry) in app
        .controller()
        .state()
        .playlist
        .entries()
        .iter()
        .enumerate()
    {
        if entry.length_ms >= 0 {
            total_ms += entry.length_ms;
        } else {
            total_more = true;
        }
        if entry.selected || current == Some(index) {
            if entry.length_ms >= 0 {
                selected_ms += entry.length_ms;
            } else {
                selected_more = true;
            }
        }
    }
    format!(
        "{}/{}",
        format_playlist_footer_duration(selected_ms, selected_more),
        format_playlist_footer_duration(total_ms, total_more)
    )
}

fn handle_global_shortcuts(ctx: &egui::Context, app: &mut EguiFrontendState) {
    ctx.input(|input| {
        for shortcut in egui_shortcuts_from_input(input) {
            dispatch_app_shortcut(ctx, app, shortcut);
        }
        handle_playlist_shortcuts(input, app);
        handle_equalizer_shortcuts(input, app);
        handle_mouse_wheel(input, app);
    });
}

fn egui_shortcuts_from_input(input: &egui::InputState) -> Vec<AppShortcut> {
    let mut shortcuts = Vec::new();
    if input.key_pressed(egui::Key::Z) {
        shortcuts.push(AppShortcut::Previous);
    }
    if input.key_pressed(egui::Key::X) {
        shortcuts.push(AppShortcut::Play);
    }
    if input.key_pressed(egui::Key::C) {
        shortcuts.push(AppShortcut::Pause);
    }
    if input.key_pressed(egui::Key::V) {
        shortcuts.push(AppShortcut::Stop);
    }
    if input.key_pressed(egui::Key::B) {
        shortcuts.push(AppShortcut::Next);
    }
    if input.key_pressed(egui::Key::L) && input.modifiers.ctrl {
        shortcuts.push(AppShortcut::OpenLocation);
    } else if input.key_pressed(egui::Key::L) {
        shortcuts.push(AppShortcut::TogglePlaylist);
    }
    if input.key_pressed(egui::Key::E) {
        shortcuts.push(AppShortcut::ToggleEqualizer);
    }
    if input.key_pressed(egui::Key::P) && input.modifiers.ctrl {
        shortcuts.push(AppShortcut::Preferences);
    }
    if input.key_pressed(egui::Key::J) {
        shortcuts.push(AppShortcut::JumpTime);
    }
    if input.key_pressed(egui::Key::O) && input.modifiers.ctrl {
        shortcuts.push(AppShortcut::OpenFiles);
    }
    if input.key_pressed(egui::Key::S) && input.modifiers.ctrl {
        shortcuts.push(AppShortcut::SkinBrowser);
    }
    if input.key_pressed(egui::Key::N) {
        shortcuts.push(AppShortcut::ToggleNoAdvance);
    }
    if input.key_pressed(egui::Key::M) {
        shortcuts.push(AppShortcut::ShadeMain);
    }
    shortcuts
}

fn dispatch_app_shortcut(_ctx: &egui::Context, app: &mut EguiFrontendState, shortcut: AppShortcut) {
    if let Some(command) = shortcut.command() {
        app.dispatch(command);
        return;
    }
    match shortcut {
        AppShortcut::OpenFiles => {
            app.apply_effect(AppEffect::OpenFileDialog(FileDialogRequest::AddAudioFiles));
        }
        AppShortcut::OpenLocation => {
            app.prompt_open = Some(EguiPrompt::OpenLocation);
            app.prompt_text.clear();
        }
        AppShortcut::JumpTime => {
            app.prompt_open = Some(EguiPrompt::JumpToTime);
            app.prompt_text.clear();
        }
        AppShortcut::OpenDirectory
        | AppShortcut::PresentMain
        | AppShortcut::ToggleTimerRemaining
        | AppShortcut::ToggleSticky
        | AppShortcut::DoubleScale
        | AppShortcut::HalfScale
        | AppShortcut::ToggleEasyMove
        | AppShortcut::StartOfList
        | AppShortcut::Previous
        | AppShortcut::Play
        | AppShortcut::Pause
        | AppShortcut::Stop
        | AppShortcut::Next
        | AppShortcut::ToggleRepeat
        | AppShortcut::ToggleShuffle
        | AppShortcut::Preferences
        | AppShortcut::ToggleNoAdvance
        | AppShortcut::ShadeMain
        | AppShortcut::SkinBrowser
        | AppShortcut::TogglePlaylist
        | AppShortcut::ToggleEqualizer
        | AppShortcut::ShadePlaylist
        | AppShortcut::ShadeEqualizer
        | AppShortcut::FileInfo => {}
    }
}

fn handle_equalizer_shortcuts(input: &egui::InputState, app: &mut EguiFrontendState) {
    if !app.controller().state().config.equalizer_visible {
        return;
    }
    if input.key_pressed(egui::Key::Q) {
        app.dispatch(EqualizerCommand::ToggleActive);
    }
    if input.key_pressed(egui::Key::W) {
        app.dispatch(EqualizerCommand::ToggleAuto);
    }
    if input.key_pressed(egui::Key::Tab) {
        app.equalizer_keyboard_slider = Some(next_equalizer_keyboard_slider(
            app.equalizer_keyboard_slider,
        ));
    }
    let Some(slider) = app.equalizer_keyboard_slider else {
        return;
    };
    let delta = if input.key_pressed(egui::Key::ArrowUp) {
        -1
    } else if input.key_pressed(egui::Key::ArrowDown) {
        1
    } else {
        0
    };
    if delta == 0 {
        return;
    }
    let config = &app.controller().state().config;
    match slider {
        EqualizerSlider::Preamp => app.dispatch(EqualizerCommand::SetPreamp(
            config.equalizer_preamp_pos.saturating_add(delta),
        )),
        EqualizerSlider::Band(band) => app.dispatch(EqualizerCommand::SetBand {
            band,
            position: config.equalizer_band_pos[band].saturating_add(delta),
        }),
        EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => {}
    }
}

fn next_equalizer_keyboard_slider(current: Option<EqualizerSlider>) -> EqualizerSlider {
    match current {
        None => EqualizerSlider::Preamp,
        Some(EqualizerSlider::Preamp) => EqualizerSlider::Band(0),
        Some(EqualizerSlider::Band(band)) if band + 1 < crate::audio_model::EQUALIZER_BANDS => {
            EqualizerSlider::Band(band + 1)
        }
        _ => EqualizerSlider::Preamp,
    }
}

fn handle_mouse_wheel(input: &egui::InputState, app: &mut EguiFrontendState) {
    let scroll_y = input.raw_scroll_delta.y;
    if scroll_y == 0.0 {
        return;
    }
    if app.controller().state().config.playlist_visible && input.modifiers.shift {
        let visible_rows = ((PLAYLIST_DEFAULT_HEIGHT - 58) / 11).max(1) as usize;
        let max_offset = app
            .controller()
            .state()
            .playlist
            .len()
            .saturating_sub(visible_rows);
        if scroll_y > 0.0 {
            app.playlist_scroll_offset = app.playlist_scroll_offset.saturating_sub(3);
        } else {
            app.playlist_scroll_offset = (app.playlist_scroll_offset + 3).min(max_offset);
        }
    } else {
        let step = app.controller().state().config.mouse_wheel_change;
        let volume = app.controller().state().player.volume();
        let next = if scroll_y > 0.0 {
            volume.saturating_add(step)
        } else {
            volume.saturating_sub(step)
        };
        app.dispatch(crate::app::command::AudioCommand::SetVolume(next));
    }
}

fn handle_playlist_shortcuts(input: &egui::InputState, app: &mut EguiFrontendState) {
    if !app.controller().state().config.playlist_visible {
        return;
    }
    let len = app.controller().state().playlist.len();
    if len == 0 {
        return;
    }
    if input.key_pressed(egui::Key::Delete) {
        if input.modifiers.ctrl {
            app.dispatch(PlaylistCommand::CropToSelection);
        } else {
            app.dispatch(PlaylistCommand::RemoveSelectedOrCurrent);
        }
    }
    if input.key_pressed(egui::Key::A) && input.modifiers.ctrl {
        app.dispatch(PlaylistCommand::SelectAll);
    }
    if input.key_pressed(egui::Key::I) && input.modifiers.ctrl {
        app.dispatch(PlaylistCommand::InvertSelection);
    }
    if input.key_pressed(egui::Key::Enter) {
        app.dispatch(PlayerCommand::Play);
    }
    let current = app.controller().state().playlist.position().unwrap_or(0);
    let visible_rows = ((PLAYLIST_DEFAULT_HEIGHT - 58) / 11).max(1) as usize;
    let next = if input.key_pressed(egui::Key::ArrowDown)
        || (app.controller().state().config.vim_playlist_navigation
            && input.key_pressed(egui::Key::J))
    {
        Some((current + 1).min(len - 1))
    } else if input.key_pressed(egui::Key::ArrowUp)
        || (app.controller().state().config.vim_playlist_navigation
            && input.key_pressed(egui::Key::K))
    {
        Some(current.saturating_sub(1))
    } else if input.key_pressed(egui::Key::PageDown) {
        Some((current + visible_rows).min(len - 1))
    } else if input.key_pressed(egui::Key::PageUp) {
        Some(current.saturating_sub(visible_rows))
    } else if input.key_pressed(egui::Key::Home) {
        Some(0)
    } else if input.key_pressed(egui::Key::End) {
        Some(len - 1)
    } else {
        None
    };
    if let Some(position) = next {
        app.dispatch(PlaylistCommand::SetPosition(position));
        app.playlist_scroll_offset = app.playlist_scroll_offset.min(position);
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
                            let mut config = app.controller().state().config.clone();
                            config.skin = None;
                            let result = app.controller_mut().apply_config_from_preferences(config);
                            app.apply_effects(result.effects);
                        }
                        Err(err) => app
                            .runtime
                            .pending_messages
                            .push(format!("failed to load bundled default skin: {err}")),
                    }
                }
                if ui.button("Add...").clicked() {
                    import_skin_from_dialog(app);
                }
                if ui.button("Close").clicked() {
                    app.dispatch(UiCommand::SetSkinBrowserVisible(false));
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for entry in app.skin_entries.clone() {
                        let selected = app.controller().state().config.skin.as_deref()
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
    app.dispatch(UiCommand::SetSkinBrowserVisible(
        open && app.skin_browser_open,
    ));
}

fn select_skin_entry(app: &mut EguiFrontendState, entry: &SkinEntry) {
    match DefaultSkin::load_from_path(&entry.path) {
        Ok(skin) => {
            app.active_skin = skin;
            let mut config = app.controller().state().config.clone();
            config.skin = Some(entry.path.to_string_lossy().into_owned());
            let result = app.controller_mut().apply_config_from_preferences(config);
            app.apply_effects(result.effects);
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

    #[test]
    fn playlist_load_and_save_use_m3u_files() {
        let root =
            std::env::temp_dir().join(format!("xmms-rs-egui-playlist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let input = root.join("in.m3u");
        let output = root.join("out.m3u");
        std::fs::write(
            &input,
            "#EXTM3U\n#EXTINF:42,Loaded\nfile:///tmp/loaded.mp3\n",
        )
        .unwrap();

        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.load_playlist_file(&input);
        assert_eq!(app.controller().state().playlist.len(), 1);
        assert!(app.runtime.repaint_requested);

        app.save_playlist_file(&output);
        let saved = std::fs::read_to_string(&output).unwrap();
        assert!(saved.contains("#EXTM3U"));
        assert!(saved.contains("file:///tmp/loaded.mp3"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn equalizer_preset_load_and_save_use_shared_formats() {
        let root = std::env::temp_dir().join(format!("xmms-rs-egui-eq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("custom.preset");

        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.controller_mut().state_mut().config.equalizer_preamp_pos = 25;
        app.controller_mut().state_mut().config.equalizer_band_pos[0] = 75;
        app.save_equalizer_preset_file(&path);

        app.controller_mut().state_mut().config.equalizer_preamp_pos = 50;
        app.controller_mut().state_mut().config.equalizer_band_pos[0] = 50;
        app.load_equalizer_preset_file(&path);

        assert_eq!(app.controller().state().config.equalizer_preamp_pos, 25);
        assert_eq!(app.controller().state().config.equalizer_band_pos[0], 75);
        assert!(app.runtime.repaint_requested);

        let _ = std::fs::remove_dir_all(&root);
    }
}
