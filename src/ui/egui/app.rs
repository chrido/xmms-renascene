//! eframe application lifecycle for the egui frontend.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::app::command::{
    AppCommand, AudioCommand, EqualizerCommand, PanelCommand, PlayerCommand, PlaylistCommand,
    UiCommand,
};
use crate::app::effect::{AppEffect, FileDialogRequest};
use crate::app::input::AppShortcut;
use crate::app::playlist_actions::playlist_row_click_commands;
use crate::app::preferences_model::{clamped_scale_factor, normalize_preferences_config};
use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app::store::AppStore;
use crate::app::view_model::{
    balance_to_eq_shaded_position, equalizer_view_model,
    playlist_footer_info as shared_playlist_footer_info,
    playlist_rows_render_state as shared_playlist_rows_render_state, playlist_view_model,
    volume_to_eq_shaded_position,
};
use crate::app_log_info;
use crate::app_state::AppState;
use crate::equalizer::{
    load_winamp_eqf_first, load_xmms_preset_file, save_winamp_eqf, save_xmms_preset_file,
    EqualizerPreset,
};
use crate::mpris::zbus_service::{EguiMprisService, MprisServiceRequest};
use crate::mpris::{
    app_action_for_mpris_command, mpris_player_properties, MprisAppAction, MprisCommand, MprisEvent,
};
use crate::playback::backend::{create_backend, PlaybackBackend, PlaybackBackendKind};
use crate::playback::model::{EqualizerBackendState, PlaybackEvent, PlayerState};
use crate::playlist::file_uri_to_path;
use crate::playlist::{DurationIndexResult, Playlist};
use crate::render::{
    docked_panel_size, equalizer_window_height, playlist_window_height, DockedPanelState,
    EqualizerControl, EqualizerRenderState, EqualizerSlider, MainPushButton, MainSlider,
    MainToggleButton, PlaylistMenuRenderKind, PlaylistMenuRenderState, VisualizationRenderState,
    EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH, PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH, PLAYLIST_MIN_HEIGHT, PLAYLIST_MIN_WIDTH,
};
use crate::session::default_config_dir;
use crate::skin::layout::{
    equalizer_control_rect, panel_title_button_rect, playlist_footer_button_rect,
    playlist_menu_button_rect, playlist_menu_popup_rect, snap_playlist_size, LayoutPanelKind,
    PanelTitleButton, PlaylistFooterButton, PlaylistMenuButton,
};
use crate::skin::widget::{Visualization, WidgetId};
use crate::skin::{discover_skins_in_dirs, skin_browser_search_dirs, DefaultSkin, SkinEntry};
use crate::socket_control::{
    start_socket_control, SocketCommand, SocketControl, SocketRequest, SocketUiCommand,
};
use crate::{app_log_debug, app_log_trace};

use super::file_info;
use super::menu::{self, EguiPrompt};
use super::preferences::{self, PreferencesPage, PreferencesViewportState};
use super::runtime::EguiRuntime;
use super::skin_texture::{
    pixel_snapped_rect, render_equalizer_color_image, render_playlist_color_image,
    render_playlist_menu_color_image,
};
use super::{equalizer, main_player, playlist};

const VISUALIZER_REPAINT_INTERVAL: Duration = Duration::from_millis(50);

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
    playlist_menu_open: Option<PlaylistMenuRenderKind>,
    playlist_scroll_offset: usize,
    playlist_total_entries: usize,
    playlist_row_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
enum DetachedPanelAction {
    Panel(PanelCommand),
    EqualizerControl(EqualizerControl),
    PlaylistFooter(PlaylistFooterButton),
    PlaylistMenu(PlaylistMenuButton),
    PlaylistMenuItem(PlaylistMenuButton, usize),
    ClosePlaylistMenu,
    PlaylistRowClick {
        index: usize,
        double: bool,
        ctrl: bool,
    },
    PlaylistScrollTo(usize),
    PlaylistScrollRows(i32),
}

#[derive(Debug, Default)]
struct DetachedViewportState {
    equalizer: Option<DetachedPanelSnapshot>,
    playlist: Option<DetachedPanelSnapshot>,
    equalizer_focused: bool,
    playlist_focused: bool,
    actions: Vec<DetachedPanelAction>,
    playlist_menu_hover: Option<(PlaylistMenuRenderKind, usize)>,
    playlist_last_click: Option<(usize, Instant)>,
    playlist_scrollbar_drag_offset: Option<i32>,
    playlist_resize_start: Option<(i32, i32)>,
    playlist_resize_request: Option<(i32, i32)>,
}

pub struct EguiFrontendState {
    pub main_menu_open: bool,
    pub preferences_open: bool,
    pub skin_browser_open: bool,
    pub file_info_open: bool,
    pub file_info_viewport: Arc<Mutex<file_info::FileInfoViewportState>>,
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
    pub playlist_width: i32,
    pub playlist_height: i32,
    pub playlist_resize_start: Option<i32>,
    visualization: Visualization,
    visualization_tick_counter: i32,
    #[cfg_attr(not(feature = "gstreamer-backend"), allow(dead_code))]
    duration_index_sender: Sender<DurationIndexResult>,
    duration_index_receiver: Receiver<DurationIndexResult>,
    socket_control: Option<SocketControl>,
    mpris_service: Option<EguiMprisService>,
    controller: AppStore,
    playback_backend: Option<Box<dyn PlaybackBackend>>,
    pending_backend_seek_ms: Option<i64>,
}

impl EguiFrontendState {
    pub fn new(options: PreviewOptions) -> Result<Self, String> {
        let mut app_state = AppState::default();
        if options.reset {
            app_state = AppState::default();
        }
        apply_preview_options_to_config(&mut app_state.config, &options)?;
        for path in &options.positional_paths {
            app_state
                .playlist
                .add_location(path)
                .map_err(|err| format!("failed to add playlist location '{path}': {err}"))?;
        }
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
        let initial_mpris_player_properties =
            mpris_player_properties(&app_state, app_state.config.playback_position_ms);
        let mpris_service = match EguiMprisService::new(initial_mpris_player_properties) {
            Ok(service) => Some(service),
            Err(err) => {
                app_log_debug!(mpris, "egui MPRIS service unavailable", err);
                None
            }
        };
        let playlist_size = options
            .playlist_size
            .map(|(width, height)| snap_playlist_size(width, height))
            .unwrap_or_else(|| snap_playlist_size(PLAYLIST_DEFAULT_WIDTH, PLAYLIST_DEFAULT_HEIGHT));
        let (duration_index_sender, duration_index_receiver) = mpsc::channel();
        let mut state = Self {
            main_menu_open: false,
            preferences_open: options.open_preferences,
            skin_browser_open: false,
            file_info_open: false,
            file_info_viewport: Arc::new(Mutex::new(file_info::FileInfoViewportState::default())),
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
            playlist_width: playlist_size.width,
            playlist_height: playlist_size.height,
            playlist_resize_start: None,
            visualization: Visualization::new(WidgetId(6), 24, 43, 76),
            visualization_tick_counter: 0,
            duration_index_sender,
            duration_index_receiver,
            socket_control,
            mpris_service,
            controller: AppStore::new(app_state),
            playback_backend: create_backend(PlaybackBackendKind::Auto).ok(),
            pending_backend_seek_ms: None,
        };
        state.apply_visualization_preferences();
        state.schedule_missing_local_playlist_durations();
        Ok(state)
    }

    pub fn controller(&self) -> &AppStore {
        &self.controller
    }

    pub fn controller_mut(&mut self) -> &mut AppStore {
        &mut self.controller
    }

    pub fn dispatch(&mut self, command: impl Into<AppCommand>) {
        let command = command.into();
        let should_index_durations = matches!(
            &command,
            AppCommand::Playlist(
                PlaylistCommand::AddUris(_)
                    | PlaylistCommand::AddLocations(_)
                    | PlaylistCommand::AddFiles(_)
            )
        );
        let result = self.controller.dispatch(command);
        self.sync_frontend_state_from_store();
        self.apply_effects(result.effects);
        if should_index_durations {
            self.schedule_missing_local_playlist_durations();
        }
    }

    pub(crate) fn apply_preferences_config(&mut self, mut config: crate::config::Config) {
        normalize_preferences_config(&mut config);
        let was_playlist_detached = self.controller.state().config.playlist_detached;
        let result = self.controller.apply_config_from_preferences(config);
        // When the playlist re-attaches to the main window, snap its width back to
        // the player width so the docked stack matches the player (GTK parity).
        if was_playlist_detached && !self.controller.state().config.playlist_detached {
            self.set_playlist_size(crate::render::PLAYLIST_MIN_WIDTH, self.playlist_height);
        }
        self.sync_frontend_state_from_store();
        self.sync_scale_factor_from_config();
        self.apply_visualization_preferences();
        self.apply_effects(result.effects);
    }

    fn sync_frontend_state_from_store(&mut self) {
        let ui = &self.controller.state().ui;
        self.preferences_open = ui.preferences_visible;
        self.main_menu_open = ui.main_menu_visible;
        self.skin_browser_open = ui.skin_browser_visible;
        self.file_info_open = ui.file_info_visible;
    }

    pub(crate) fn sync_scale_factor_from_config(&mut self) {
        self.scale_factor =
            clamped_scale_factor(self.controller.state().config.scale_factor) as f32;
    }

    pub(crate) fn visualization_render_state(&self) -> VisualizationRenderState {
        let config = &self.controller.state().config;
        VisualizationRenderState {
            mode: self.visualization.mode(),
            analyzer_style: self.visualization.analyzer_style(),
            analyzer_mode: self.visualization.analyzer_mode(),
            scope_mode: self.visualization.scope_mode(),
            peaks_enabled: self.visualization.peaks_enabled(),
            vu_mode: config.vis_vu_mode,
            data: *self.visualization.data(),
            peak: *self.visualization.peak(),
            milkdrop_energy: self.visualization.milkdrop_energy(),
            milkdrop_phase: self.visualization.milkdrop_phase(),
        }
    }

    pub(crate) fn apply_visualization_preferences(&mut self) {
        let config = &self.controller.state().config;
        self.visualization.set_mode(config.vis_mode);
        self.visualization
            .set_analyzer_mode(config.vis_analyzer_mode);
        self.visualization
            .set_analyzer_style(config.vis_analyzer_style);
        self.visualization.set_scope_mode(config.vis_scope_mode);
        self.visualization
            .set_peaks_enabled(config.vis_peaks_enabled);
        self.visualization
            .set_falloff(config.vis_analyzer_falloff, config.vis_peaks_falloff);
    }

    fn visualization_refresh_divisor(&self) -> i32 {
        self.controller
            .state()
            .config
            .vis_refresh_divisor
            .clamp(1, 8)
    }

    fn tick_visualization(&mut self) -> bool {
        if self.controller.state().player.state() != PlayerState::Playing {
            self.visualization_tick_counter = 0;
            return false;
        }

        self.visualization_tick_counter += 1;
        if self.visualization_tick_counter < self.visualization_refresh_divisor() {
            return false;
        }
        self.visualization_tick_counter = 0;

        let data = {
            let player = &self.controller.state().player;
            player
                .visualization_data_valid()
                .then(|| *player.visualization_data())
        };
        self.visualization
            .tick(data.as_ref().map(|values| values.as_slice()));
        true
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

    fn current_mpris_player_properties(&self) -> crate::mpris::MprisPlayerProperties {
        mpris_player_properties(
            self.controller.state(),
            self.controller.state().config.playback_position_ms,
        )
    }

    fn sync_mpris_properties(&mut self, extra_events: impl IntoIterator<Item = MprisEvent>) {
        let properties = self.current_mpris_player_properties();
        if let Some(service) = &mut self.mpris_service {
            let mut events = service.update_player_properties(properties);
            events.extend(extra_events);
            service.emit_events(&events);
        }
    }

    fn poll_mpris_requests(&mut self, ctx: &egui::Context) {
        let requests = self
            .mpris_service
            .as_mut()
            .map(EguiMprisService::drain_requests)
            .unwrap_or_default();
        if requests.is_empty() {
            return;
        }

        let mut events = Vec::new();
        for request in requests {
            events.extend(self.handle_mpris_request(ctx, request));
        }
        self.sync_mpris_properties(events);
        ctx.request_repaint();
    }

    fn handle_mpris_request(
        &mut self,
        ctx: &egui::Context,
        request: MprisServiceRequest,
    ) -> Vec<MprisEvent> {
        match request {
            MprisServiceRequest::Command(command) => self.handle_mpris_command(ctx, command),
            MprisServiceRequest::SetVolume(volume) => {
                self.dispatch(AudioCommand::SetVolume((volume * 100.0) as i32));
                vec![MprisEvent::PlaybackStatusChanged]
            }
        }
    }

    fn handle_mpris_command(
        &mut self,
        ctx: &egui::Context,
        command: MprisCommand,
    ) -> Vec<MprisEvent> {
        let current_position_ms = self.controller.state().config.playback_position_ms;
        match app_action_for_mpris_command(&command, current_position_ms) {
            MprisAppAction::Raise => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                vec![MprisEvent::Raised]
            }
            MprisAppAction::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                vec![MprisEvent::QuitRequested]
            }
            MprisAppAction::Dispatch(app_command) => {
                self.dispatch(app_command);
                match command {
                    MprisCommand::Seek { .. } | MprisCommand::SetPosition { .. } => {
                        vec![MprisEvent::Seeked(
                            self.controller.state().config.playback_position_ms * 1_000,
                        )]
                    }
                    MprisCommand::Next
                    | MprisCommand::Previous
                    | MprisCommand::Pause
                    | MprisCommand::PlayPause
                    | MprisCommand::Stop
                    | MprisCommand::Play => vec![MprisEvent::PlaybackStatusChanged],
                    MprisCommand::Raise | MprisCommand::Quit | MprisCommand::OpenUri(_) => {
                        Vec::new()
                    }
                }
            }
            MprisAppAction::OpenUri(uri) => {
                if self.open_mpris_uri(uri) {
                    vec![
                        MprisEvent::MetadataChanged,
                        MprisEvent::PlaybackStatusChanged,
                    ]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn open_mpris_uri(&mut self, uri: String) -> bool {
        self.dispatch(PlaylistCommand::Clear);
        self.dispatch(PlaylistCommand::AddLocations(vec![uri]));
        if self.controller.state().playlist.is_empty() {
            return false;
        }
        self.dispatch(PlaylistCommand::SetPosition(0));
        self.dispatch(PlayerCommand::StartCurrentTrack);
        true
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
        if let Some(backend) = &self.playback_backend {
            match backend.poll_events() {
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

            let stream_info = self
                .playback_backend
                .as_ref()
                .map(|backend| backend.stream_info());
            if let Some(stream_info) = stream_info {
                let result = self
                    .controller
                    .handle_playback_event(PlaybackEvent::StreamInfo(stream_info));
                self.sync_frontend_state_from_store();
                self.runtime.apply_effects(result.effects);
            }
        }
    }

    fn poll_duration_index_results(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.duration_index_receiver.try_recv() {
            let dispatch = self.controller.apply_duration_index_result(result);
            changed |= !dispatch.changes.is_empty();
            self.sync_frontend_state_from_store();
            self.apply_effects(dispatch.effects);
        }
        changed
    }

    fn schedule_missing_local_playlist_durations(&mut self) {
        let items = self
            .controller
            .state()
            .playlist
            .missing_duration_items()
            .into_iter()
            .filter(|item| file_uri_to_path(&item.uri).is_some_and(|path| path.exists()))
            .collect::<Vec<_>>();
        if items.is_empty() {
            return;
        }

        let sender = self.duration_index_sender.clone();
        thread::spawn(move || {
            #[cfg(feature = "rodio-backend")]
            {
                use crate::playback::backend::AudioMetadataProbe as _;

                let probe = crate::playback::rodio::RodioMetadataProbe;
                for item in items {
                    match probe.probe(&item) {
                        Ok(Some(result)) => {
                            if sender.send(result).is_err() {
                                return;
                            }
                        }
                        Ok(None) => {}
                        Err(err) => eprintln!(
                            "xmms-rs: failed to probe playlist item {} with rodio: {err}",
                            item.uri
                        ),
                    }
                }
            }
            #[cfg(all(not(feature = "rodio-backend"), feature = "gstreamer-backend"))]
            {
                if let Err(err) = gstreamer::init() {
                    eprintln!(
                        "xmms-rs: failed to initialize GStreamer for playlist durations: {err}"
                    );
                    return;
                }
                let discoverer =
                    match gstreamer_pbutils::Discoverer::new(gstreamer::ClockTime::from_seconds(5))
                    {
                        Ok(discoverer) => discoverer,
                        Err(err) => {
                            eprintln!(
                                "xmms-rs: failed to create playlist duration discoverer: {err}"
                            );
                            return;
                        }
                    };

                for item in items {
                    let Some(path) = file_uri_to_path(&item.uri).filter(|path| path.exists())
                    else {
                        continue;
                    };
                    let info = match discoverer.discover_uri(&item.uri) {
                        Ok(info) => info,
                        Err(err) => {
                            eprintln!(
                                "xmms-rs: failed to discover playlist item {}: {err}",
                                path.display()
                            );
                            continue;
                        }
                    };
                    let length_ms = info
                        .duration()
                        .map(|duration| duration.mseconds() as i64)
                        .unwrap_or(-1);
                    if sender
                        .send(DurationIndexResult {
                            index: item.index,
                            uri: item.uri,
                            length_ms,
                            title: None,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });
    }

    fn apply_pending_backend_seek(&mut self) {
        if let (Some(backend), Some(position_ms)) =
            (&self.playback_backend, self.pending_backend_seek_ms)
        {
            match backend.seek(position_ms) {
                Ok(()) => {
                    app_log_info!(backend, "egui applied pending start seek", position_ms);
                    self.pending_backend_seek_ms = None;
                }
                Err(err) => self.runtime.pending_messages.push(err),
            }
        }
    }

    fn tick_playback_position(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(VISUALIZER_REPAINT_INTERVAL);
        let visualizer_changed = self.tick_visualization();
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        if self.controller.state().player.state() != PlayerState::Playing {
            return;
        }
        let elapsed_ms = elapsed.as_millis().min(i64::MAX as u128) as i64;
        if elapsed_ms == 0 {
            if visualizer_changed {
                ctx.request_repaint();
            }
            return;
        }
        let result = self.controller.tick_playback_position(elapsed_ms);
        self.sync_frontend_state_from_store();
        self.apply_effects(result.effects);
        if visualizer_changed {
            ctx.request_repaint();
        }
    }

    pub(crate) fn apply_effects(&mut self, effects: impl IntoIterator<Item = AppEffect>) {
        for effect in effects {
            self.apply_effect(effect);
        }
    }

    pub(crate) fn apply_effect(&mut self, effect: AppEffect) {
        app_log_debug!(frontend_effect, "egui {effect:?}");
        let clear_visualization = matches!(
            &effect,
            AppEffect::StopPlayback | AppEffect::BeginStopFade { .. }
        );
        if let Some(backend) = &self.playback_backend {
            match &effect {
                AppEffect::StartPlaybackUri { uri, position_ms } => {
                    let pending_seek = *position_ms > 0;
                    app_log_info!(backend, "egui play_uri", uri, position_ms, pending_seek);
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
                    app_log_info!(backend, "egui seek", position_ms);
                    if let Err(err) = backend.seek(*position_ms) {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::SetBackendVolume(volume) => {
                    if let Err(err) = backend.set_volume(*volume) {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::SetBackendBalance(balance) => {
                    if let Err(err) = backend.set_balance(*balance) {
                        self.runtime.pending_messages.push(err);
                    }
                }
                AppEffect::SetBackendEqualizer => {
                    let config = &self.controller.state().config;
                    if let Err(err) = backend.set_equalizer(EqualizerBackendState {
                        active: config.equalizer_active,
                        preamp_position: config.equalizer_preamp_pos,
                        band_positions: config.equalizer_band_pos,
                    }) {
                        self.runtime.pending_messages.push(err);
                    }
                }
                _ => {}
            }
        }
        if clear_visualization {
            self.visualization_tick_counter = 0;
            self.visualization.clear_data();
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
                self.schedule_missing_local_playlist_durations();
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
            playlist_width: self.playlist_width,
            playlist_height: self.playlist_height,
            ..DockedPanelState::default()
        });
        egui::vec2(
            width as f32 * self.scale_factor,
            height as f32 * self.scale_factor,
        )
    }

    pub(crate) fn set_playlist_size(&mut self, width: i32, height: i32) -> bool {
        let size = snap_playlist_size(width, height);
        let changed = self.playlist_width != size.width || self.playlist_height != size.height;
        self.playlist_width = size.width;
        self.playlist_height = size.height;
        self.clamp_playlist_scroll_offset();
        changed
    }

    pub(crate) fn playlist_visible_rows(&self) -> usize {
        ((self.playlist_height - 58) / 11).max(1) as usize
    }

    pub(crate) fn playlist_max_scroll_offset(&self) -> usize {
        self.controller
            .state()
            .playlist
            .len()
            .saturating_sub(self.playlist_visible_rows())
    }

    pub(crate) fn clamp_playlist_scroll_offset(&mut self) {
        self.playlist_scroll_offset = self
            .playlist_scroll_offset
            .min(self.playlist_max_scroll_offset());
    }
}

impl eframe::App for EguiFrontendState {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_socket_control(ctx);
        self.poll_mpris_requests(ctx);
        self.poll_playback_backend();
        if self.poll_duration_index_results() {
            ctx.request_repaint();
        }
        self.tick_playback_position(ctx);
        handle_dropped_files(ctx, self);
        handle_global_shortcuts(ctx, self);
        self.sync_mpris_properties(std::iter::empty());
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.desired_window_size()));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        sync_root_viewport_focus(&ctx, self);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
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
        show_detached_panels(&ctx, self);
        show_detached_equalizer_popovers(&ctx, self);
        menu::show_main_menu(&ctx, self);
        menu::show_prompts(&ctx, self);
        if self.preferences_open {
            preferences::show_preferences(&ctx, self);
        }
        file_info::show_file_info_dialog(&ctx, self);
        if self.skin_browser_open {
            show_skin_browser_placeholder(&ctx, self);
        }
        menu::show_pending_messages(&ctx, self);
        app_log_trace!(render, "egui update size={:?}", self.desired_window_size());
    }
}

fn show_detached_equalizer_popovers(ctx: &egui::Context, app: &mut EguiFrontendState) {
    let config = &app.controller.state().config;
    if !(config.equalizer_visible && config.equalizer_detached) {
        return;
    }
    let height = equalizer_window_height(config.equalizer_shaded) as f32 * app.scale_factor;
    let rect = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(EQUALIZER_WINDOW_WIDTH as f32 * app.scale_factor, height),
    );
    equalizer::show_equalizer_presets_popover(ctx, app, rect);
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

fn sync_root_viewport_focus(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if ctx.input(|input| input.viewport().focused) == Some(true) {
        let mut state = app
            .detached_viewports
            .lock()
            .expect("detached viewport state poisoned");
        state.equalizer_focused = false;
        state.playlist_focused = false;
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
    let (actions, resize_request) = {
        let mut state = app
            .detached_viewports
            .lock()
            .expect("detached viewport state poisoned");
        (
            std::mem::take(&mut state.actions),
            state.playlist_resize_request.take(),
        )
    };
    for action in actions {
        apply_detached_panel_action(app, action);
    }
    if let Some((width, height)) = resize_request {
        app.set_playlist_size(width, height);
    }
}

fn apply_detached_panel_action(app: &mut EguiFrontendState, action: DetachedPanelAction) {
    match action {
        DetachedPanelAction::Panel(command) => app.dispatch(command),
        DetachedPanelAction::EqualizerControl(control) => {
            equalizer::dispatch_equalizer_control(app, control)
        }
        DetachedPanelAction::PlaylistFooter(button) => {
            playlist::dispatch_playlist_footer_button(app, button)
        }
        DetachedPanelAction::PlaylistMenu(menu) => {
            app.playlist_menu_hover = None;
            playlist::dispatch_playlist_menu_button(app, menu)
        }
        DetachedPanelAction::PlaylistMenuItem(menu, index) => {
            app.playlist_menu_hover = None;
            playlist::dispatch_playlist_menu_item(app, menu, index);
            app.playlist_menu_open = None;
        }
        DetachedPanelAction::ClosePlaylistMenu => {
            app.playlist_menu_hover = None;
            app.playlist_menu_open = None;
            app.playlist_sort_menu_open = false;
        }
        DetachedPanelAction::PlaylistRowClick {
            index,
            double,
            ctrl,
        } => {
            for command in playlist_row_click_commands(index, double, ctrl) {
                app.dispatch(command);
            }
        }
        DetachedPanelAction::PlaylistScrollTo(offset) => {
            app.playlist_scroll_offset = offset.min(app.playlist_max_scroll_offset());
        }
        DetachedPanelAction::PlaylistScrollRows(rows) => {
            scroll_playlist_by_wheel(app, rows as f32);
        }
    }
}

fn update_detached_panel_snapshots(app: &mut EguiFrontendState) {
    let config = app.controller().state().config.clone();
    let (equalizer_focused, playlist_focused, playlist_menu_hover) = {
        let state = app
            .detached_viewports
            .lock()
            .expect("detached viewport state poisoned");
        (
            state.equalizer_focused,
            state.playlist_focused,
            state.playlist_menu_hover,
        )
    };
    let equalizer = (config.equalizer_visible && config.equalizer_detached)
        .then(|| detached_equalizer_snapshot(app, equalizer_focused))
        .flatten();
    let playlist = (config.playlist_visible && config.playlist_detached)
        .then(|| detached_playlist_snapshot(app, playlist_focused, playlist_menu_hover))
        .flatten();
    let mut state = app
        .detached_viewports
        .lock()
        .expect("detached viewport state poisoned");
    state.equalizer = equalizer;
    state.playlist = playlist;
}

fn detached_equalizer_snapshot(
    app: &EguiFrontendState,
    focused: bool,
) -> Option<DetachedPanelSnapshot> {
    let view_model = equalizer_view_model(app.controller().state());
    let render_state = EqualizerRenderState {
        focused,
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
        playlist_menu_open: None,
        playlist_scroll_offset: 0,
        playlist_total_entries: 0,
        playlist_row_indices: Vec::new(),
        image,
    })
}

fn detached_playlist_snapshot(
    app: &EguiFrontendState,
    focused: bool,
    menu_hover: Option<(PlaylistMenuRenderKind, usize)>,
) -> Option<DetachedPanelSnapshot> {
    let view_model = playlist_view_model(app.controller().state());
    let playlist_row_indices: Vec<usize> = view_model.rows.iter().map(|row| row.index).collect();
    let playlist_total_entries = view_model.rows.len();
    let rows = shared_playlist_rows_render_state(
        app.controller().state(),
        app.playlist_scroll_offset,
        false,
        None,
        app.playlist_width,
        app.playlist_height,
    );
    let shaded_info = playlist::shaded_playlist_info(app);
    let footer_info = detached_playlist_footer_info(app);
    let (footer_min, footer_sec) = detached_playlist_footer_time_parts(app);
    let render_scale = app.scale_factor as f64;
    let mut image = render_playlist_color_image(
        &app.active_skin,
        focused,
        view_model.shaded,
        app.playlist_width,
        app.playlist_height,
        Some(&shaded_info),
        &rows,
        Some(&footer_info),
        Some(&footer_min),
        Some(&footer_sec),
        render_scale,
    )
    .ok()?;
    let playlist_menu_open = (!view_model.shaded)
        .then_some(app.playlist_menu_open)
        .flatten();
    if let Some(kind) = playlist_menu_open {
        let hover =
            menu_hover.and_then(|(hover_kind, index)| (hover_kind == kind).then_some(index));
        overlay_detached_playlist_menu(&mut image, app, kind, hover, render_scale).ok()?;
    }
    Some(DetachedPanelSnapshot {
        panel: LayoutPanelKind::Playlist,
        width: app.playlist_width,
        height: playlist_window_height(view_model.shaded, app.playlist_height),
        scale_factor: app.scale_factor,
        playlist_menu_open,
        playlist_scroll_offset: app.playlist_scroll_offset,
        playlist_total_entries,
        playlist_row_indices,
        image,
    })
}

fn overlay_detached_playlist_menu(
    image: &mut egui::ColorImage,
    app: &EguiFrontendState,
    kind: PlaylistMenuRenderKind,
    hover: Option<usize>,
    scale: f64,
) -> Result<(), crate::render::RenderError> {
    let popup = playlist_menu_popup_rect(kind, app.playlist_width, app.playlist_height);
    let menu = render_playlist_menu_color_image(
        &app.active_skin,
        PlaylistMenuRenderState { kind, hover },
        popup.width,
        popup.height,
        scale,
    )?;
    let scale = scale.max(1.0);
    let offset_x = ((popup.x as f64) * scale).round() as i32;
    let offset_y = ((popup.y as f64) * scale).round() as i32;
    let image_width = image.size[0];
    let image_height = image.size[1];
    for y in 0..menu.size[1] {
        let dest_y = offset_y + y as i32;
        if dest_y < 0 || dest_y as usize >= image_height {
            continue;
        }
        for x in 0..menu.size[0] {
            let dest_x = offset_x + x as i32;
            if dest_x < 0 || dest_x as usize >= image_width {
                continue;
            }
            let src = menu.pixels[y * menu.size[0] + x];
            if src.a() > 0 {
                image.pixels[dest_y as usize * image_width + dest_x as usize] = src;
            }
        }
    }
    Ok(())
}

fn detached_panel_viewport_size(snapshot: &DetachedPanelSnapshot) -> egui::Vec2 {
    egui::vec2(
        snapshot.width as f32 * snapshot.scale_factor,
        snapshot.height as f32 * snapshot.scale_factor,
    )
}

fn detached_panel_viewport_min_size(
    snapshot: &DetachedPanelSnapshot,
    equalizer_panel: bool,
) -> egui::Vec2 {
    // Do not use the current panel size as the viewport minimum: shade toggles
    // intentionally shrink detached panels, and Wayland rejects resizing a
    // surface below its configured minimum (`wl_surface: Invalid min/max size`).
    if equalizer_panel {
        egui::vec2(
            EQUALIZER_WINDOW_WIDTH as f32 * snapshot.scale_factor,
            equalizer_window_height(true) as f32 * snapshot.scale_factor,
        )
    } else {
        egui::vec2(
            PLAYLIST_MIN_WIDTH as f32 * snapshot.scale_factor,
            playlist_window_height(true, PLAYLIST_MIN_HEIGHT) as f32 * snapshot.scale_factor,
        )
    }
}

fn detached_panel_viewport_builder(
    title: &'static str,
    snapshot: &DetachedPanelSnapshot,
    equalizer_panel: bool,
) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(detached_panel_viewport_size(snapshot))
        .with_min_inner_size(detached_panel_viewport_min_size(snapshot, equalizer_panel))
        // Wayland implements `resizable = false` by locking min and max size
        // to the current size. Detached panels still need programmatic resizes
        // for shade/unshade, so keep the protocol surface resizable; only the
        // playlist exposes an in-app resize handle.
        .with_resizable(true)
        .with_decorations(false)
}

fn update_detached_panel_viewport_focus(
    shared: &Arc<Mutex<DetachedViewportState>>,
    equalizer_panel: bool,
    focused: bool,
) -> bool {
    let mut state = shared.lock().expect("detached viewport state poisoned");
    let before = (state.equalizer_focused, state.playlist_focused);
    if equalizer_panel {
        state.equalizer_focused = focused;
        if focused {
            state.playlist_focused = false;
        }
    } else {
        state.playlist_focused = focused;
        if focused {
            state.equalizer_focused = false;
        }
    }
    before != (state.equalizer_focused, state.playlist_focused)
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
    let size = detached_panel_viewport_size(&snapshot);
    let builder = detached_panel_viewport_builder(title, &snapshot, equalizer_panel);
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of(id),
        builder,
        move |ctx, class| {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
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
                egui::ViewportClass::EmbeddedWindow | egui::ViewportClass::Root => {
                    show_embedded_detached_snapshot(ctx, &shared, title, equalizer_panel);
                }
                egui::ViewportClass::Deferred | egui::ViewportClass::Immediate => {
                    let focus_changed =
                        ctx.input(|input| input.viewport().focused)
                            .is_some_and(|focused| {
                                update_detached_panel_viewport_focus(
                                    &shared,
                                    equalizer_panel,
                                    focused,
                                )
                            });
                    if focus_changed {
                        ctx.request_repaint();
                    }
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
        pixel_snapped_rect(ui.ctx(), rect),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    handle_detached_playlist_wheel(ui, shared, snapshot, rect);
    let resize_offset = {
        let state = shared.lock().expect("detached viewport state poisoned");
        state.playlist_resize_start
    };
    if let Some((offset_x, offset_y)) = resize_offset {
        if ui.ctx().input(|input| input.pointer.primary_down()) {
            if let Some(pointer) = ui.ctx().input(|input| input.pointer.latest_pos()) {
                let local_x = ((pointer.x - rect.left()) / snapshot.scale_factor).round() as i32;
                let local_y = ((pointer.y - rect.top()) / snapshot.scale_factor).round() as i32;
                let mut state = shared.lock().expect("detached viewport state poisoned");
                state.playlist_resize_request = Some((local_x + offset_x, local_y + offset_y));
            }
            ui.ctx().request_repaint();
            return;
        }
        let mut state = shared.lock().expect("detached viewport state poisoned");
        state.playlist_resize_start = None;
    }
    if update_detached_playlist_scrollbar_drag(ui, shared, snapshot, rect) {
        return;
    }
    update_detached_playlist_menu_interaction(ui, shared, snapshot, rect);
    // Compute the primary press-origin for this frame once. Under a WM-less /
    // software-rendered X server the synthetic press+drag can be coalesced or
    // delivered before hover is established, so egui never attributes the drag to
    // this widget and `response.drag_started()` stays false. The press-origin edge
    // is still reported, so keying grabs off it (rather than drag_started) makes
    // resize and titlebar drags reliable; using the origin (not the post-threshold
    // pointer) avoids absorbing the initial threshold motion.
    let press_origin = ui.ctx().input(|input| {
        input
            .pointer
            .button_pressed(egui::PointerButton::Primary)
            .then(|| input.pointer.press_origin())
            .flatten()
    });
    if let Some(origin) = press_origin {
        if detached_playlist_resize_region(snapshot, rect, origin) {
            let local_x = ((origin.x - rect.left()) / snapshot.scale_factor).round() as i32;
            let local_y = ((origin.y - rect.top()) / snapshot.scale_factor).round() as i32;
            let mut state = shared.lock().expect("detached viewport state poisoned");
            state.playlist_resize_start =
                Some((snapshot.width - local_x, snapshot.height - local_y));
            ui.ctx().request_repaint();
            return;
        }
    }
    // Start a window drag when the press begins on the titlebar (outside the
    // shade/close buttons).
    if let Some(pos) = press_origin {
        if detached_panel_titlebar_drag_region(snapshot, rect, pos) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            return;
        }
    }
    // Dispatch skinned button / row activations on the click release rather than
    // on the press edge. In a WM-less X server the synthetic press+release can be
    // coalesced into a single egui frame, so `is_pointer_button_down_on()` is
    // already false while the press edge fires; keying off `clicked()` (which is
    // still reported in that case) makes detached clicks reliable.
    if response.clicked() || response.double_clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            handle_detached_panel_click(shared, snapshot, rect, pos);
            let ctrl = ui
                .ctx()
                .input(|input| input.modifiers.ctrl || input.modifiers.command);
            handle_detached_playlist_row_press(shared, snapshot, rect, pos, ctrl);
            ui.ctx().request_repaint();
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        }
    }
}

fn handle_detached_playlist_wheel(
    ui: &mut egui::Ui,
    shared: &Arc<Mutex<DetachedViewportState>>,
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
) {
    if snapshot.panel != LayoutPanelKind::Playlist
        || snapshot.height <= crate::render::MAIN_TITLEBAR_HEIGHT
    {
        return;
    }
    let scroll_y = ui.ctx().input(|input| input.smooth_scroll_delta().y);
    if scroll_y == 0.0 {
        return;
    }
    let over_playlist = ui.ctx().input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|pos| base_rect.contains(pos))
    });
    if !over_playlist {
        return;
    }
    push_detached_action(
        shared,
        DetachedPanelAction::PlaylistScrollRows(if scroll_y > 0.0 { 3 } else { -3 }),
    );
    ui.ctx().request_repaint();
    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
}

fn update_detached_playlist_scrollbar_drag(
    ui: &mut egui::Ui,
    shared: &Arc<Mutex<DetachedViewportState>>,
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
) -> bool {
    if snapshot.panel != LayoutPanelKind::Playlist
        || snapshot.height <= crate::render::MAIN_TITLEBAR_HEIGHT
    {
        return false;
    }
    let active_offset = {
        let state = shared.lock().expect("detached viewport state poisoned");
        state.playlist_scrollbar_drag_offset
    };
    if let Some(offset) = active_offset {
        if ui.ctx().input(|input| input.pointer.primary_down()) {
            if let Some(pointer) = ui.ctx().input(|input| input.pointer.latest_pos()) {
                let (_, y) = detached_local_point(snapshot, base_rect, pointer);
                if let Some(scroll_offset) =
                    detached_playlist_scroll_offset_from_thumb_y(snapshot, y - offset)
                {
                    push_detached_action(
                        shared,
                        DetachedPanelAction::PlaylistScrollTo(scroll_offset),
                    );
                }
            }
            ui.ctx().request_repaint();
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
            return true;
        }
        let mut state = shared.lock().expect("detached viewport state poisoned");
        state.playlist_scrollbar_drag_offset = None;
        return true;
    }

    let primary_pressed = ui
        .ctx()
        .input(|input| input.pointer.button_pressed(egui::PointerButton::Primary));
    if !primary_pressed {
        return false;
    }
    let Some(pointer) = ui.ctx().input(|input| input.pointer.latest_pos()) else {
        return false;
    };
    let (x, y) = detached_local_point(snapshot, base_rect, pointer);
    if !detached_playlist_scrollbar_region(snapshot, x, y) {
        return false;
    }
    let Some((thumb_y, thumb_h)) = detached_playlist_scrollbar_geometry(snapshot) else {
        return false;
    };
    let offset = if y >= thumb_y && y < thumb_y + thumb_h {
        y - thumb_y
    } else {
        thumb_h / 2
    };
    {
        let mut state = shared.lock().expect("detached viewport state poisoned");
        state.playlist_scrollbar_drag_offset = Some(offset);
    }
    if let Some(scroll_offset) = detached_playlist_scroll_offset_from_thumb_y(snapshot, y - offset)
    {
        push_detached_action(shared, DetachedPanelAction::PlaylistScrollTo(scroll_offset));
    }
    ui.ctx().request_repaint();
    ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
    true
}

fn handle_detached_playlist_row_press(
    shared: &Arc<Mutex<DetachedViewportState>>,
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
    pos: egui::Pos2,
    ctrl: bool,
) {
    if snapshot.panel != LayoutPanelKind::Playlist
        || snapshot.height <= crate::render::MAIN_TITLEBAR_HEIGHT
        || snapshot.playlist_menu_open.is_some()
    {
        return;
    }
    let (x, y) = detached_local_point(snapshot, base_rect, pos);
    let Some(index) = detached_playlist_entry_at(snapshot, x, y) else {
        return;
    };
    let now = Instant::now();
    let double = {
        let mut state = shared.lock().expect("detached viewport state poisoned");
        let double = state
            .playlist_last_click
            .is_some_and(|(last_index, last_time)| {
                last_index == index && now.duration_since(last_time) <= Duration::from_millis(500)
            });
        state.playlist_last_click = Some((index, now));
        double
    };
    push_detached_action(
        shared,
        DetachedPanelAction::PlaylistRowClick {
            index,
            double,
            ctrl,
        },
    );
}

fn detached_playlist_entry_at(snapshot: &DetachedPanelSnapshot, x: i32, y: i32) -> Option<usize> {
    if x < 12 || x >= snapshot.width - 19 || y < 20 || y >= snapshot.height - 38 {
        return None;
    }
    let row = ((y - 20) / 11).max(0) as usize;
    let index = snapshot.playlist_scroll_offset.saturating_add(row);
    snapshot.playlist_row_indices.get(index).copied()
}

fn detached_playlist_scrollbar_region(snapshot: &DetachedPanelSnapshot, x: i32, y: i32) -> bool {
    x >= snapshot.width - 15 && x < snapshot.width - 7 && y >= 20 && y < snapshot.height - 38
}

fn detached_playlist_visible_entries(snapshot: &DetachedPanelSnapshot) -> usize {
    ((snapshot.height - 58) / 11).max(1) as usize
}

fn detached_playlist_scrollbar_geometry(snapshot: &DetachedPanelSnapshot) -> Option<(i32, i32)> {
    let visible = detached_playlist_visible_entries(snapshot);
    let total = snapshot.playlist_total_entries;
    if total <= visible {
        return None;
    }
    let list_h = snapshot.height - 58;
    let thumb_h = 18;
    let max_scroll = total - visible;
    let max_thumb_pos = (list_h - thumb_h).max(0);
    let thumb_y = 20
        + ((snapshot.playlist_scroll_offset.min(max_scroll) as i32 * max_thumb_pos)
            / max_scroll.max(1) as i32);
    Some((thumb_y, thumb_h))
}

fn detached_playlist_scroll_offset_from_thumb_y(
    snapshot: &DetachedPanelSnapshot,
    thumb_y: i32,
) -> Option<usize> {
    let visible = detached_playlist_visible_entries(snapshot);
    let total = snapshot.playlist_total_entries;
    if total <= visible {
        return Some(0);
    }
    let list_h = snapshot.height - 58;
    let thumb_h = 18;
    let max_scroll = total - visible;
    let max_thumb_pos = (list_h - thumb_h).max(0);
    if max_thumb_pos <= 0 {
        return Some(0);
    }
    let thumb_pos = (thumb_y - 20).clamp(0, max_thumb_pos);
    Some(
        ((thumb_pos as usize * max_scroll) + (max_thumb_pos as usize / 2)) / max_thumb_pos as usize,
    )
}

fn update_detached_playlist_menu_interaction(
    ui: &mut egui::Ui,
    shared: &Arc<Mutex<DetachedViewportState>>,
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
) {
    let Some(open_menu) = snapshot.playlist_menu_open else {
        return;
    };
    if ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
        push_detached_action(shared, DetachedPanelAction::ClosePlaylistMenu);
        ui.ctx().request_repaint();
        ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
        return;
    }

    let pointer_pos = ui.ctx().input(|input| input.pointer.latest_pos());
    let hover =
        pointer_pos.and_then(|pos| detached_playlist_menu_item_at(snapshot, base_rect, pos));
    let changed = {
        let mut state = shared.lock().expect("detached viewport state poisoned");
        if state.playlist_menu_hover == hover {
            false
        } else {
            state.playlist_menu_hover = hover;
            true
        }
    };
    if changed {
        ui.ctx().request_repaint();
        ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
    }

    let clicked_outside = ui.ctx().input(|input| {
        input.pointer.any_pressed()
            && pointer_pos.is_some_and(|pos| {
                let (x, y) = detached_local_point(snapshot, base_rect, pos);
                let popup = playlist_menu_popup_rect(open_menu, snapshot.width, snapshot.height);
                let button = playlist_menu_button_rect(open_menu, snapshot.width, snapshot.height);
                !popup.contains(x, y) && !button.contains(x, y)
            })
    });
    if clicked_outside {
        push_detached_action(shared, DetachedPanelAction::ClosePlaylistMenu);
        ui.ctx().request_repaint();
        ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
    }
}

fn detached_playlist_menu_item_at(
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
    pos: egui::Pos2,
) -> Option<(PlaylistMenuRenderKind, usize)> {
    let open_menu = snapshot.playlist_menu_open?;
    let (x, y) = detached_local_point(snapshot, base_rect, pos);
    let popup = playlist_menu_popup_rect(open_menu, snapshot.width, snapshot.height);
    if !popup.contains(x, y) {
        return None;
    }
    let index = ((y - popup.y) / 18).max(0) as usize;
    (index < open_menu.item_count()).then_some((open_menu, index))
}

fn detached_local_point(
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
    pos: egui::Pos2,
) -> (i32, i32) {
    (
        ((pos.x - base_rect.left()) / snapshot.scale_factor).floor() as i32,
        ((pos.y - base_rect.top()) / snapshot.scale_factor).floor() as i32,
    )
}

fn detached_playlist_resize_region(
    snapshot: &DetachedPanelSnapshot,
    base_rect: egui::Rect,
    pos: egui::Pos2,
) -> bool {
    if snapshot.panel != LayoutPanelKind::Playlist
        || snapshot.height <= crate::render::MAIN_TITLEBAR_HEIGHT
    {
        return false;
    }
    let x = ((pos.x - base_rect.left()) / snapshot.scale_factor).floor() as i32;
    let y = ((pos.y - base_rect.top()) / snapshot.scale_factor).floor() as i32;
    x >= snapshot.width - 20 && y >= snapshot.height - 20
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

    if snapshot.height <= crate::render::MAIN_TITLEBAR_HEIGHT {
        return;
    }
    match snapshot.panel {
        LayoutPanelKind::Equalizer => handle_detached_equalizer_click(shared, x, y),
        LayoutPanelKind::Playlist => handle_detached_playlist_click(shared, snapshot, x, y),
    }
}

fn handle_detached_equalizer_click(shared: &Arc<Mutex<DetachedViewportState>>, x: i32, y: i32) {
    for control in [
        EqualizerControl::On,
        EqualizerControl::Auto,
        EqualizerControl::Presets,
    ] {
        if equalizer_control_rect(control).contains(x, y) {
            push_detached_action(shared, DetachedPanelAction::EqualizerControl(control));
            return;
        }
    }
}

fn handle_detached_playlist_click(
    shared: &Arc<Mutex<DetachedViewportState>>,
    snapshot: &DetachedPanelSnapshot,
    x: i32,
    y: i32,
) {
    if let Some(open_menu) = snapshot.playlist_menu_open {
        let popup = playlist_menu_popup_rect(open_menu, snapshot.width, snapshot.height);
        if popup.contains(x, y) {
            let index = ((y - popup.y) / 18).max(0) as usize;
            if index < open_menu.item_count() {
                push_detached_action(
                    shared,
                    DetachedPanelAction::PlaylistMenuItem(open_menu, index),
                );
            }
            return;
        }
    }

    for menu in [
        PlaylistMenuButton::Add,
        PlaylistMenuButton::Remove,
        PlaylistMenuButton::Select,
        PlaylistMenuButton::Misc,
        PlaylistMenuButton::List,
    ] {
        if playlist_menu_button_rect(menu, snapshot.width, snapshot.height).contains(x, y) {
            push_detached_action(shared, DetachedPanelAction::PlaylistMenu(menu));
            return;
        }
    }

    for button in [
        PlaylistFooterButton::Previous,
        PlaylistFooterButton::Play,
        PlaylistFooterButton::Pause,
        PlaylistFooterButton::Stop,
        PlaylistFooterButton::Next,
        PlaylistFooterButton::Eject,
        PlaylistFooterButton::ScrollUp,
        PlaylistFooterButton::ScrollDown,
    ] {
        if playlist_footer_button_rect(button, snapshot.width, snapshot.height).contains(x, y) {
            push_detached_action(shared, DetachedPanelAction::PlaylistFooter(button));
            return;
        }
    }
}

fn push_detached_command(shared: &Arc<Mutex<DetachedViewportState>>, command: PanelCommand) {
    push_detached_action(shared, DetachedPanelAction::Panel(command));
}

fn push_detached_action(shared: &Arc<Mutex<DetachedViewportState>>, action: DetachedPanelAction) {
    shared
        .lock()
        .expect("detached viewport state poisoned")
        .actions
        .push(action);
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
    shared_playlist_footer_info(app.controller().state())
}

fn handle_global_shortcuts(ctx: &egui::Context, app: &mut EguiFrontendState) {
    if global_shortcuts_suspended(ctx, app) {
        return;
    }
    ctx.input(|input| {
        for shortcut in egui_shortcuts_from_input(input) {
            dispatch_app_shortcut(ctx, app, shortcut);
        }
        handle_playlist_shortcuts(input, app);
        handle_equalizer_shortcuts(input, app);
        handle_mouse_wheel(input, app);
    });
}

fn global_shortcuts_suspended(ctx: &egui::Context, app: &EguiFrontendState) -> bool {
    ctx.egui_wants_keyboard_input()
        || app.main_menu_open
        || app.prompt_open.is_some()
        || app.preferences_open
        || app.skin_browser_open
        || app.file_info_open
        || app.equalizer_presets_open
        || app.playlist_menu_open.is_some()
        || app.playlist_sort_menu_open
        || app.confirm_physical_delete_open
        || !app.runtime.pending_messages.is_empty()
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
    let scroll_y = input.smooth_scroll_delta().y;
    if scroll_y == 0.0 {
        return;
    }
    if pointer_over_docked_playlist(input, app)
        || (app.controller().state().config.playlist_visible && input.modifiers.shift)
    {
        scroll_playlist_by_wheel(app, scroll_y);
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

fn pointer_over_docked_playlist(input: &egui::InputState, app: &EguiFrontendState) -> bool {
    let Some(pos) = input.pointer.hover_pos() else {
        return false;
    };
    let config = &app.controller().state().config;
    if !config.playlist_visible || config.playlist_detached {
        return false;
    }
    let mut top = if config.main_shaded {
        crate::render::MAIN_TITLEBAR_HEIGHT
    } else {
        crate::render::MAIN_WINDOW_HEIGHT
    } as f32
        * app.scale_factor;
    if config.equalizer_visible && !config.equalizer_detached {
        top += if config.equalizer_shaded {
            crate::render::MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        } as f32
            * app.scale_factor;
    }
    let height = playlist_window_height(config.playlist_shaded, app.playlist_height) as f32
        * app.scale_factor;
    egui::Rect::from_min_size(
        egui::pos2(0.0, top),
        egui::vec2(app.playlist_width as f32 * app.scale_factor, height),
    )
    .contains(pos)
}

fn scroll_playlist_by_wheel(app: &mut EguiFrontendState, scroll_y: f32) {
    if scroll_y > 0.0 {
        app.playlist_scroll_offset = app.playlist_scroll_offset.saturating_sub(3);
    } else {
        app.playlist_scroll_offset =
            (app.playlist_scroll_offset + 3).min(app.playlist_max_scroll_offset());
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
    let visible_rows = app.playlist_visible_rows();
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
                .with_resizable(true),
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
                            app.apply_preferences_config(config);
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
            app.apply_preferences_config(config);
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
    use crate::audio_model::SPECTRUM_BANDS;
    use crate::player::PlaybackEvent;

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
    fn egui_startup_equalizer_flag_enables_visible_panel() {
        let app = EguiFrontendState::new(PreviewOptions {
            show_equalizer: true,
            ..PreviewOptions::default()
        })
        .unwrap();

        assert!(app.controller().state().config.equalizer_visible);
        assert!(!app.controller().state().config.equalizer_detached);
        assert!(app.desired_window_size().y > EQUALIZER_WINDOW_HEIGHT as f32 * app.scale_factor);
    }

    #[test]
    fn egui_startup_playlist_size_uses_skinned_dimensions() {
        let app = match EguiFrontendState::new(PreviewOptions {
            playlist_size: Some((325, 290)),
            ..PreviewOptions::default()
        }) {
            Ok(app) => app,
            Err(err) => panic!("failed to construct egui state: {err}"),
        };

        assert!(app.controller().state().config.playlist_visible);
        assert_eq!((app.playlist_width, app.playlist_height), (325, 290));
        assert_eq!(app.desired_window_size().x, 325.0 * app.scale_factor);
    }

    #[test]
    fn egui_reattaching_playlist_snaps_width_to_player() {
        let mut app = match EguiFrontendState::new(PreviewOptions {
            playlist_size: Some((325, 290)),
            playlist_detached: Some(true),
            ..PreviewOptions::default()
        }) {
            Ok(app) => app,
            Err(err) => panic!("failed to construct egui state: {err}"),
        };

        assert!(app.controller().state().config.playlist_detached);
        assert_eq!(app.playlist_width, 325);
        let detached_height = app.playlist_height;

        let mut config = app.controller().state().config.clone();
        config.playlist_detached = false;
        app.apply_preferences_config(config);

        assert!(!app.controller().state().config.playlist_detached);
        // Width snaps back to the player width; height is preserved (GTK parity).
        assert_eq!(app.playlist_width, crate::render::PLAYLIST_MIN_WIDTH);
        assert_eq!(app.playlist_height, detached_height);
        assert_eq!(
            app.desired_window_size().x,
            crate::render::MAIN_WINDOW_WIDTH as f32 * app.scale_factor
        );
    }

    #[test]
    fn detached_panel_viewport_constraints_allow_wayland_shade_resize() {
        let image = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]);
        let equalizer_snapshot = DetachedPanelSnapshot {
            panel: LayoutPanelKind::Equalizer,
            image: image.clone(),
            width: EQUALIZER_WINDOW_WIDTH,
            height: EQUALIZER_WINDOW_HEIGHT,
            scale_factor: 1.0,
            playlist_menu_open: None,
            playlist_scroll_offset: 0,
            playlist_total_entries: 0,
            playlist_row_indices: Vec::new(),
        };

        let equalizer_builder =
            detached_panel_viewport_builder("Equalizer", &equalizer_snapshot, true);

        assert_eq!(equalizer_builder.resizable, Some(true));
        assert_eq!(
            equalizer_builder.inner_size,
            Some(egui::vec2(
                EQUALIZER_WINDOW_WIDTH as f32,
                EQUALIZER_WINDOW_HEIGHT as f32
            ))
        );
        assert_eq!(
            equalizer_builder.min_inner_size,
            Some(egui::vec2(
                EQUALIZER_WINDOW_WIDTH as f32,
                equalizer_window_height(true) as f32
            ))
        );
        assert!(equalizer_builder.max_inner_size.is_none());
        assert!(
            equalizer_builder.min_inner_size.unwrap().y < equalizer_builder.inner_size.unwrap().y
        );

        let playlist_snapshot = DetachedPanelSnapshot {
            panel: LayoutPanelKind::Playlist,
            image,
            width: PLAYLIST_DEFAULT_WIDTH,
            height: PLAYLIST_DEFAULT_HEIGHT,
            scale_factor: 1.0,
            playlist_menu_open: None,
            playlist_scroll_offset: 0,
            playlist_total_entries: 0,
            playlist_row_indices: Vec::new(),
        };

        let playlist_builder =
            detached_panel_viewport_builder("Playlist", &playlist_snapshot, false);

        assert_eq!(playlist_builder.resizable, Some(true));
        assert_eq!(
            playlist_builder.min_inner_size,
            Some(egui::vec2(
                PLAYLIST_MIN_WIDTH as f32,
                playlist_window_height(true, PLAYLIST_MIN_HEIGHT) as f32
            ))
        );
        assert!(
            playlist_builder.min_inner_size.unwrap().y < playlist_builder.inner_size.unwrap().y
        );
    }

    #[test]
    fn detached_panel_snapshots_use_active_window_skin_state() {
        let mut app = EguiFrontendState::new(PreviewOptions {
            show_equalizer: true,
            show_playlist: true,
            equalizer_detached: Some(true),
            playlist_detached: Some(true),
            ..PreviewOptions::default()
        })
        .unwrap();

        update_detached_panel_snapshots(&mut app);
        let (inactive_equalizer, inactive_playlist) = {
            let state = app
                .detached_viewports
                .lock()
                .expect("detached viewport state poisoned");
            let equalizer = state.equalizer.as_ref().expect("equalizer snapshot");
            let playlist = state.playlist.as_ref().expect("playlist snapshot");
            assert!(!state.equalizer_focused);
            assert!(!state.playlist_focused);
            (equalizer.image.clone(), playlist.image.clone())
        };

        assert!(update_detached_panel_viewport_focus(
            &app.detached_viewports,
            true,
            true
        ));
        update_detached_panel_snapshots(&mut app);
        {
            let state = app
                .detached_viewports
                .lock()
                .expect("detached viewport state poisoned");
            let equalizer = state.equalizer.as_ref().expect("equalizer snapshot");
            assert!(state.equalizer_focused);
            assert!(!state.playlist_focused);
            assert_ne!(equalizer.image, inactive_equalizer);
        }

        assert!(update_detached_panel_viewport_focus(
            &app.detached_viewports,
            false,
            true
        ));
        update_detached_panel_snapshots(&mut app);
        {
            let state = app
                .detached_viewports
                .lock()
                .expect("detached viewport state poisoned");
            let playlist = state.playlist.as_ref().expect("playlist snapshot");
            assert!(!state.equalizer_focused);
            assert!(state.playlist_focused);
            assert_ne!(playlist.image, inactive_playlist);
        }
    }

    #[test]
    fn detached_playlist_snapshot_renders_open_skinned_menu() {
        let mut app = EguiFrontendState::new(PreviewOptions {
            show_playlist: true,
            playlist_detached: Some(true),
            ..PreviewOptions::default()
        })
        .unwrap();

        let closed =
            detached_playlist_snapshot(&app, false, None).expect("closed playlist snapshot");
        assert!(closed.playlist_menu_open.is_none());

        app.playlist_menu_open = Some(PlaylistMenuButton::Add);
        let open = detached_playlist_snapshot(&app, false, None).expect("open playlist snapshot");

        assert_eq!(open.playlist_menu_open, Some(PlaylistMenuButton::Add));
        assert_ne!(open.image, closed.image);

        let hover = detached_playlist_snapshot(&app, false, Some((PlaylistMenuButton::Add, 0)))
            .expect("hover playlist snapshot");
        assert_ne!(hover.image, open.image);
    }

    #[test]
    fn egui_preferences_scale_factor_updates_runtime_zoom() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let default_size = app.desired_window_size();
        let mut config = app.controller().state().config.clone();
        config.scale_factor = 1.25;
        config.doublesize = true;

        app.apply_preferences_config(config);

        assert!((app.controller().state().config.scale_factor - 1.25).abs() < f64::EPSILON);
        assert!((app.scale_factor - 1.25).abs() < f32::EPSILON);
        assert!(app.controller().state().config.doublesize);
        assert!(app.desired_window_size().x < default_size.x);
    }

    #[test]
    fn egui_preferences_scale_factor_is_clamped_and_updates_doublesize() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let mut config = app.controller().state().config.clone();
        config.scale_factor = 0.25;
        config.doublesize = true;

        app.apply_preferences_config(config);

        assert!((app.controller().state().config.scale_factor - 1.0).abs() < f64::EPSILON);
        assert!((app.scale_factor - 1.0).abs() < f32::EPSILON);
        assert!(!app.controller().state().config.doublesize);
    }

    #[test]
    fn egui_duration_index_results_update_playlist_footer_total() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.controller_mut()
            .state_mut()
            .playlist
            .add_uri("file:///tmp/song.ogg");

        assert_eq!(
            shared_playlist_footer_info(app.controller().state()),
            "0:00/?"
        );

        app.duration_index_sender
            .send(DurationIndexResult {
                index: 0,
                uri: "file:///tmp/song.ogg".to_string(),
                length_ms: 42_000,
                title: None,
            })
            .unwrap();

        assert!(app.poll_duration_index_results());
        assert_eq!(
            shared_playlist_footer_info(app.controller().state()),
            "0:00/0:42"
        );
    }

    #[test]
    fn egui_visualizer_render_state_uses_spectrum_events() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let mut spectrum = [0.0; SPECTRUM_BANDS];
        spectrum[9] = 0.9;

        app.controller_mut().state_mut().player.mark_playing();
        let result = app
            .controller_mut()
            .handle_playback_event(PlaybackEvent::Spectrum(spectrum));
        app.apply_effects(result.effects);
        assert!(app.tick_visualization());

        let render_state = app.visualization_render_state();
        assert!(render_state.data[9] > 0.0);
        assert!(render_state.peak[9] > 0.0);
    }

    #[test]
    fn egui_visualizer_clears_on_stop_effect() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let mut spectrum = [0.0; SPECTRUM_BANDS];
        spectrum[4] = 0.9;

        app.controller_mut().state_mut().player.mark_playing();
        let result = app
            .controller_mut()
            .handle_playback_event(PlaybackEvent::Spectrum(spectrum));
        app.apply_effects(result.effects);
        assert!(app.tick_visualization());
        assert!(app.visualization_render_state().data[4] > 0.0);

        app.apply_effect(AppEffect::StopPlayback);

        assert_eq!(app.visualization_render_state().data[4], 0.0);
        assert_eq!(app.visualization_render_state().peak[4], 0.0);
    }

    #[test]
    fn egui_dispatch_mutates_config_through_controller() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();

        app.dispatch(PanelCommand::SetPlaylistVisibility(true));

        assert!(app.controller().state().config.playlist_visible);
        assert!(app.runtime.repaint_requested);
    }

    #[test]
    fn egui_global_shortcuts_pause_while_dialogs_or_menus_are_active() {
        let ctx = egui::Context::default();
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();

        assert!(!global_shortcuts_suspended(&ctx, &app));

        app.prompt_open = Some(EguiPrompt::OpenLocation);
        assert!(global_shortcuts_suspended(&ctx, &app));
        app.prompt_open = None;

        app.playlist_menu_open = Some(PlaylistMenuRenderKind::Misc);
        assert!(global_shortcuts_suspended(&ctx, &app));
        app.playlist_menu_open = None;

        app.runtime.pending_messages.push("message".to_string());
        assert!(global_shortcuts_suspended(&ctx, &app));
    }

    #[test]
    fn egui_playlist_wheel_scrolling_moves_three_rows_and_clamps() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        for index in 0..20 {
            app.controller_mut().state_mut().playlist.add_timed_uri(
                format!("file:///tmp/{index}.ogg"),
                format!("Song {index}"),
                12_000,
            );
        }

        scroll_playlist_by_wheel(&mut app, -1.0);
        assert_eq!(app.playlist_scroll_offset, 3);
        scroll_playlist_by_wheel(&mut app, 1.0);
        assert_eq!(app.playlist_scroll_offset, 0);
    }

    #[test]
    fn egui_mpris_open_uri_replaces_playlist_and_starts_playback() {
        let ctx = egui::Context::default();
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.dispatch(PlaylistCommand::AddUris(vec![
            "file:///music/old.ogg".to_string()
        ]));

        let events = app.handle_mpris_command(
            &ctx,
            MprisCommand::OpenUri("file:///music/mpris.ogg".to_string()),
        );

        assert_eq!(app.controller().state().playlist.len(), 1);
        assert_eq!(app.controller().state().playlist.position(), Some(0));
        assert_eq!(
            app.controller().state().playlist.entries()[0].filename,
            "file:///music/mpris.ogg"
        );
        assert_eq!(
            app.controller().state().player.state(),
            PlayerState::Playing
        );
        assert!(events.contains(&MprisEvent::MetadataChanged));
        assert!(events.contains(&MprisEvent::PlaybackStatusChanged));
    }

    #[test]
    fn egui_mpris_seek_updates_position_and_emits_seeked() {
        let ctx = egui::Context::default();
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.dispatch(PlaylistCommand::AddUris(vec![
            "file:///music/mpris.ogg".to_string()
        ]));
        app.dispatch(PlaylistCommand::SetPosition(0));
        app.dispatch(PlayerCommand::StartCurrentTrack);

        let events = app.handle_mpris_command(
            &ctx,
            MprisCommand::Seek {
                offset_us: 5_000_000,
            },
        );

        assert_eq!(app.controller().state().config.playback_position_ms, 5_000);
        assert_eq!(events, vec![MprisEvent::Seeked(5_000_000)]);
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
