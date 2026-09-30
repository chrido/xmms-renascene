//! eframe application lifecycle for the egui frontend.

#[cfg(target_os = "android")]
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app::command::{
    AppCommand, EqualizerCommand, PanelCommand, PlayerCommand, PlaylistCommand, UiCommand,
};
use crate::app::effect::{FileDialogRequest, RenderTarget, UiEffect};
#[cfg(feature = "desktop-egui")]
use crate::app::external_commands::{
    mpris_open_uri_playback_events, mpris_service_events, translate_mpris_command,
};
use crate::app::external_commands::{translate_socket_command, FrontendAction};
use crate::app::input::AppShortcut;
use crate::app::playlist_actions::playlist_row_click_commands;
use crate::app::preferences_model::{clamped_scale_factor, normalize_preferences_config};
use crate::app::preview::{
    apply_preview_options_to_config, apply_preview_playlist, PreviewOptions,
};
use crate::app::runtime::{
    FrontendEffect, FrontendRuntime, PlaybackExecution as EffectExecution, RuntimeEvent,
    RuntimeUpdate,
};
use crate::app::store::{AppStore, StateChangeSet};
use crate::app::view_model::{
    balance_to_eq_shaded_position, equalizer_view_model,
    playlist_footer_info as shared_playlist_footer_info,
    playlist_rows_render_state as shared_playlist_rows_render_state, playlist_view_model,
    volume_to_eq_shaded_position, PlaylistProjection, TitleMarquee,
};
#[cfg(target_os = "android")]
use crate::app_log_error;
use crate::app_log_info;
use crate::app_state::AppState;
#[cfg(feature = "desktop-egui")]
use crate::equalizer::save_winamp_eqf;
#[cfg(target_os = "android")]
use crate::equalizer::serialize_winamp_eqf;
use crate::equalizer::{load_winamp_eqf_first, EqualizerPreset};
#[cfg(feature = "desktop-egui")]
use crate::mpris::zbus_service::{EguiMprisService, MprisServiceRequest};
#[cfg(feature = "desktop-egui")]
use crate::mpris::{mpris_player_properties, MprisCommand, MprisEvent};
#[cfg(test)]
use crate::playback::backend::PlaybackBackend;
#[cfg(all(not(target_os = "android"), not(test)))]
use crate::playback::backend::{create_backend, PlaybackBackendKind};
use crate::playback::model::{PlaybackEvent, PlayerState};
#[cfg(test)]
use crate::playlist::DurationIndexResult;
use crate::playlist::Playlist;
#[cfg(target_os = "android")]
use crate::render::main_window_height;
use crate::render::{
    docked_panel_size, equalizer_window_height, playlist_window_height, DockedPanelState,
    EqualizerControl, EqualizerRenderState, EqualizerSlider, MainSlider, PlaylistMenuRenderKind,
    PlaylistMenuRenderState, VisualizationRenderState, EQUALIZER_WINDOW_HEIGHT,
    EQUALIZER_WINDOW_WIDTH, PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH, PLAYLIST_MIN_HEIGHT,
    PLAYLIST_MIN_WIDTH,
};
#[cfg(target_os = "android")]
use crate::session::{default_config_dir, fallback_state_paths, load_saved_state};
#[cfg(any(feature = "desktop-egui", target_os = "android"))]
use crate::skin::import_skin_to_user_dir;
use crate::skin::layout::{
    equalizer_control_rect, panel_title_button_rect, playlist_footer_button_rect,
    playlist_menu_button_rect, playlist_menu_popup_rect, snap_playlist_size, LayoutPanelKind,
    PanelTitleButton, PlaylistFooterButton, PlaylistMenuButton,
};
#[cfg(target_os = "android")]
use crate::skin::user_skin_import_dir;
use crate::skin::widget::VisMode;
use crate::skin::{discover_runtime_skins, DefaultSkin, SkinEntry};
use crate::socket_control::{start_socket_control_with_wakeup, SocketControl, SocketRequest};
use crate::{app_log_debug, app_log_trace};

#[cfg(target_os = "android")]
use super::android::playlist_manager::{PlaylistManagerAction, PlaylistManagerOutcome};
#[cfg(test)]
use super::android_runtime::{
    layout_extent_is_stable as android_layout_extent_is_stable,
    layout_snapshot_is_consistent as android_layout_snapshot_is_consistent,
    AndroidLayoutOrientation,
};
#[cfg(target_os = "android")]
use super::android_runtime::{
    AndroidLayoutOrientation, AndroidLayoutRepaint as AndroidLayoutReadiness,
    AndroidLayoutSnapshot, AndroidLayoutView, AndroidRuntime, AndroidStableLayout,
};
use super::effect_executor;
use super::file_info;
#[cfg(any(target_os = "android", test))]
use super::interaction::PlaylistTouchGesture;
use super::menu::{self, EguiPrompt};
use super::preferences::{self, PreferencesPage};
use super::render_cache::RenderCache;
#[cfg(target_os = "android")]
use super::runtime::AndroidLayoutRepaint;
use super::runtime::EguiRuntime;
use super::skin_texture::{
    pixel_snapped_rect, render_equalizer_color_image, render_playlist_color_image,
    render_playlist_menu_color_image, ImageRenderBuffer,
};
#[cfg(test)]
use super::ui_state::ActiveOverlay;
use super::ui_state::{EguiUiState, EqualizerPressed, MainPressed};
use super::{equalizer, main_player, playlist};

const VISUALIZER_REPAINT_INTERVAL: Duration = Duration::from_millis(20);
const POSITION_REPAINT_INTERVAL: Duration = Duration::from_millis(250);

#[cfg(any(target_os = "android", test))]
fn android_player_command_for_media_control(
    control: super::android_events::AndroidMediaControl,
) -> Option<PlayerCommand> {
    use super::android_events::AndroidMediaControl;

    match control {
        AndroidMediaControl::PausePlayback => Some(PlayerCommand::Pause),
        AndroidMediaControl::ResumePlayback => Some(PlayerCommand::Play),
        AndroidMediaControl::NextTrack => Some(PlayerCommand::NextTrack),
        AndroidMediaControl::PreviousTrack => Some(PlayerCommand::PreviousTrack),
        AndroidMediaControl::SeekToMs(position_ms) => Some(PlayerCommand::SeekToMs(position_ms)),
        AndroidMediaControl::HaltPlayback => Some(PlayerCommand::Halt),
        AndroidMediaControl::PlayMediaItem(_) | AndroidMediaControl::PlaylistEof => None,
    }
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum DetachedFocus {
    #[default]
    None,
    Equalizer,
    Playlist,
}

/// Shared only with deferred viewport callbacks.
///
/// `show_viewport_deferred` requires `'static` closures that can run outside
/// the root viewport callback. The mutex protects transient snapshots and the
/// action handoff across that callback boundary; it never owns domain state,
/// and callers release each lock before dispatching commands or rendering.
#[derive(Debug, Default)]
struct DetachedViewportState {
    equalizer: Option<DetachedPanelSnapshot>,
    playlist: Option<DetachedPanelSnapshot>,
    equalizer_requested_size: Option<egui::Vec2>,
    playlist_requested_size: Option<egui::Vec2>,
    focus: DetachedFocus,
    actions: Vec<DetachedPanelAction>,
    playlist_menu_hover: Option<(PlaylistMenuRenderKind, usize)>,
    playlist_last_click: Option<(usize, Instant)>,
    playlist_scrollbar_drag_offset: Option<i32>,
    playlist_resize_start: Option<(i32, i32)>,
    playlist_resize_request: Option<(i32, i32)>,
}

impl DetachedViewportState {
    // Drain while holding the viewport lock; dispatch and resizing happen after unlock.
    fn take_handoff(&mut self) -> (Vec<DetachedPanelAction>, Option<(i32, i32)>) {
        (
            std::mem::take(&mut self.actions),
            self.playlist_resize_request.take(),
        )
    }

    fn replace_snapshots(
        &mut self,
        equalizer: Option<DetachedPanelSnapshot>,
        playlist: Option<DetachedPanelSnapshot>,
    ) {
        if equalizer.is_none() {
            self.equalizer_requested_size = None;
        }
        if playlist.is_none() {
            self.playlist_requested_size = None;
        }
        self.equalizer = equalizer;
        self.playlist = playlist;
    }
}

#[derive(Default)]
pub(crate) struct SkinCatalog {
    pub(crate) entries: Vec<SkinEntry>,
    discovered: bool,
}

impl SkinCatalog {
    fn refresh(&mut self) {
        self.entries = discover_runtime_skins().unwrap_or_default();
        self.discovered = true;
    }

    fn ensure_discovered(&mut self) {
        if !self.discovered {
            self.refresh();
        }
    }
}

struct RootViewportTiming {
    repaint_context: Arc<Mutex<Option<egui::Context>>>,
    last_playback_tick: Instant,
    #[cfg(not(target_os = "android"))]
    last_requested_size: Option<egui::Vec2>,
}

impl RootViewportTiming {
    fn new(repaint_context: Arc<Mutex<Option<egui::Context>>>) -> Self {
        Self {
            repaint_context,
            last_playback_tick: Instant::now(),
            #[cfg(not(target_os = "android"))]
            last_requested_size: None,
        }
    }

    fn playback_elapsed(&mut self, now: Instant) -> Duration {
        let elapsed = now.saturating_duration_since(self.last_playback_tick);
        self.last_playback_tick = now;
        elapsed
    }

    #[cfg(not(target_os = "android"))]
    fn request_size_if_changed(&mut self, desired: egui::Vec2) -> Option<egui::Vec2> {
        if self.last_requested_size == Some(desired) {
            return None;
        }
        self.last_requested_size = Some(desired);
        Some(desired)
    }
}

pub(crate) struct MainInteraction {
    pub(crate) pressed: MainPressed,
    pub(crate) slider_drag_position: Option<(MainSlider, i32)>,
    pub(crate) title_marquee: TitleMarquee,
    pub(crate) last_marquee_tick: Instant,
}

impl Default for MainInteraction {
    fn default() -> Self {
        Self {
            pressed: MainPressed::None,
            slider_drag_position: None,
            title_marquee: TitleMarquee::default(),
            last_marquee_tick: Instant::now(),
        }
    }
}

impl MainInteraction {
    pub(crate) fn update_marquee(
        &mut self,
        title: &str,
        player_state: PlayerState,
        shaded: bool,
        now: Instant,
    ) -> bool {
        let elapsed = now.saturating_duration_since(self.last_marquee_tick);
        self.last_marquee_tick = now;
        self.title_marquee.update(
            title,
            crate::render::MAIN_TITLE_TEXT_WIDTH,
            player_state,
            !shaded,
            elapsed,
        );
        self.title_marquee.is_scrolling(player_state, !shaded)
    }
}

#[derive(Default)]
pub(crate) struct EqualizerInteraction {
    pub(crate) pressed: EqualizerPressed,
    pub(crate) keyboard_slider: Option<EqualizerSlider>,
}

pub(crate) struct PlaylistPresentation {
    pub(crate) scroll_offset: usize,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) resize_start: Option<i32>,
    #[cfg(target_os = "android")]
    pub(crate) touch_gesture: PlaylistTouchGesture,
}

impl PlaylistPresentation {
    fn new(width: i32, height: i32) -> Self {
        Self {
            scroll_offset: 0,
            width,
            height,
            resize_start: None,
            #[cfg(target_os = "android")]
            touch_gesture: PlaylistTouchGesture::default(),
        }
    }

    fn visible_rows(&self) -> usize {
        ((self.height - 58) / 11).max(1) as usize
    }

    fn resize(&mut self, width: i32, height: i32, entry_count: usize) -> bool {
        let changed = self.width != width || self.height != height;
        self.width = width;
        self.height = height;
        self.scroll_offset = self
            .scroll_offset
            .min(entry_count.saturating_sub(self.visible_rows()));
        changed
    }
}

pub struct EguiFrontendState {
    pub(crate) skins: SkinCatalog,
    pub ui: EguiUiState,
    detached_viewports: Arc<Mutex<DetachedViewportState>>,
    root: RootViewportTiming,
    pub render_cache: RenderCache,
    #[cfg(target_os = "android")]
    pub(crate) android: AndroidRuntime,
    #[cfg(target_os = "android")]
    scale_factor: f32,
    pub runtime: EguiRuntime,
    pub active_skin: DefaultSkin,
    pub(crate) main: MainInteraction,
    pub(crate) equalizer: EqualizerInteraction,
    pub(crate) playlist: PlaylistPresentation,
    core: FrontendRuntime,
    socket_control: Option<SocketControl>,
    #[cfg(feature = "desktop-egui")]
    mpris_service: Option<EguiMprisService>,
}

impl EguiFrontendState {
    pub fn new(options: PreviewOptions) -> Result<Self, String> {
        #[cfg(target_os = "android")]
        let (config_path, playlist_path) = fallback_state_paths(&default_config_dir());
        #[cfg(target_os = "android")]
        let use_android_defaults = options.reset || !config_path.is_file();
        #[cfg(target_os = "android")]
        let mut app_state = load_saved_state(&config_path, &playlist_path, options.reset)
            .map_err(|err| format!("failed to load Android session state: {err}"))?;
        #[cfg(target_os = "android")]
        if use_android_defaults {
            app_state.config.playlist_visible = true;
        }
        #[cfg(target_os = "android")]
        match super::android::media_volume_percent() {
            Ok(volume) => {
                app_state.player.set_volume(volume);
            }
            Err(err) => app_log_debug!(frontend, "failed to read Android media volume", err),
        }
        #[cfg(not(target_os = "android"))]
        let mut app_state = AppState::default();
        #[cfg(not(target_os = "android"))]
        if options.reset {
            app_state = AppState::default();
        }
        apply_preview_options_to_config(&mut app_state.config, &options)?;
        apply_preview_playlist(&mut app_state, &options)?;
        app_state.ui.preferences_visible = options.open_preferences;
        let active_skin = load_skin_from_config(&app_state)?;
        let persistence_config = app_state.persistence_snapshot().config;
        let ui = EguiUiState::new(
            &persistence_config,
            PreferencesPage::default(),
            options.open_preferences,
        );
        let detached_viewports = Arc::new(Mutex::new(DetachedViewportState::default()));
        let repaint_context = Arc::new(Mutex::new(None::<egui::Context>));
        let socket_control = options
            .socket_port
            .map(|port| {
                let repaint_context = Arc::clone(&repaint_context);
                start_socket_control_with_wakeup(port, move || {
                    if let Some(ctx) = repaint_context
                        .lock()
                        .expect("egui repaint context poisoned")
                        .as_ref()
                    {
                        ctx.request_repaint();
                    }
                })
            })
            .transpose()?;
        #[cfg(feature = "desktop-egui")]
        let mpris_service = {
            let initial_properties =
                mpris_player_properties(&app_state, app_state.config.playback_position_ms);
            let repaint_context = Arc::clone(&repaint_context);
            match EguiMprisService::new_with_wakeup(initial_properties, move || {
                if let Some(ctx) = repaint_context
                    .lock()
                    .expect("egui repaint context poisoned")
                    .as_ref()
                {
                    ctx.request_repaint();
                }
            }) {
                Ok(service) => Some(service),
                Err(err) => {
                    app_log_debug!(mpris, "egui MPRIS service unavailable", err);
                    None
                }
            }
        };
        let playlist_size = options
            .playlist_size
            .map(|(width, height)| snap_playlist_size(width, height))
            .unwrap_or_else(|| snap_playlist_size(PLAYLIST_DEFAULT_WIDTH, PLAYLIST_DEFAULT_HEIGHT));
        #[cfg(target_os = "android")]
        let (playback_backend, playback_backend_error) = (None, None);
        #[cfg(all(not(target_os = "android"), not(test)))]
        let (playback_backend, playback_backend_error) =
            match create_backend(PlaybackBackendKind::Auto) {
                Ok(backend) => (Some(backend), None),
                Err(err) => (
                    None,
                    Some(format!(
                        "audio output is not ready; playback will retry: {err}"
                    )),
                ),
            };
        #[cfg(all(not(target_os = "android"), test))]
        let (playback_backend, playback_backend_error) = (None, None);
        let mut runtime = EguiRuntime::default();
        if let Some(error) = playback_backend_error {
            runtime.pending_messages.push(error);
        }
        #[cfg(target_os = "android")]
        let initial_scale_factor = app_state.config.scale_factor as f32;
        let core = FrontendRuntime::new_with_duration_wakeup(app_state, playback_backend, {
            let repaint_context = Arc::clone(&repaint_context);
            move || {
                #[cfg(target_os = "android")]
                super::android::request_background_repaint();
                if let Some(ctx) = repaint_context
                    .lock()
                    .expect("egui repaint context poisoned")
                    .as_ref()
                {
                    ctx.request_repaint();
                }
            }
        });
        let mut state = Self {
            skins: SkinCatalog::default(),
            ui,
            detached_viewports,
            root: RootViewportTiming::new(repaint_context),
            render_cache: RenderCache::default(),
            #[cfg(target_os = "android")]
            android: AndroidRuntime::new(),
            #[cfg(target_os = "android")]
            scale_factor: initial_scale_factor,
            runtime,
            active_skin,
            main: MainInteraction::default(),
            equalizer: EqualizerInteraction::default(),
            playlist: PlaylistPresentation::new(playlist_size.width, playlist_size.height),
            core,
            socket_control,
            #[cfg(feature = "desktop-egui")]
            mpris_service,
        };
        state.apply_visualization_preferences();
        state.schedule_missing_local_playlist_durations();
        Ok(state)
    }

    pub fn controller(&self) -> &AppStore {
        self.core.store()
    }

    pub(crate) fn playlist_render_parts(
        &mut self,
    ) -> (
        &AppState,
        &DefaultSkin,
        &PlaylistProjection,
        &mut ImageRenderBuffer,
    ) {
        let state = self.core.store().state();
        let skin = &self.active_skin;
        let (projection, staging) = self.render_cache.playlist_projection_and_staging(state);
        (state, skin, projection, staging)
    }

    #[cfg(test)]
    pub(crate) fn controller_mut(&mut self) -> &mut AppStore {
        self.core.store_mut()
    }

    pub fn dispatch(&mut self, command: impl Into<AppCommand>) {
        self.dispatch_all([command.into()]);
    }

    pub(crate) fn dispatch_all(&mut self, commands: impl IntoIterator<Item = AppCommand>) {
        let commands: Vec<_> = commands.into_iter().collect();
        #[cfg(target_os = "android")]
        let _media_control_order = if commands
            .iter()
            .any(|command| matches!(command, AppCommand::Player(_)))
        {
            let Some(order) =
                super::android::begin_local_media_control(self.android.activity_generation())
            else {
                return;
            };
            Some(order)
        } else {
            None
        };
        for command in commands {
            self.dispatch_one(command);
        }
    }

    fn dispatch_one(&mut self, command: AppCommand) {
        let should_index_durations = matches!(
            &command,
            AppCommand::Playlist(
                PlaylistCommand::AddUris(_)
                    | PlaylistCommand::AddLocations(_)
                    | PlaylistCommand::AddFiles(_)
            )
        );
        self.process_store_event(RuntimeEvent::Command(command), EffectExecution::LOCAL);
        if should_index_durations {
            self.schedule_missing_local_playlist_durations();
        }
    }

    pub(crate) fn apply_preferences_config(&mut self, mut config: crate::config::Config) {
        normalize_preferences_config(&mut config);
        #[cfg(target_os = "android")]
        let skin_changed = self.core.store().state().config.skin != config.skin;
        let was_playlist_detached = self.core.store().state().config.playlist_detached;
        self.process_store_event(RuntimeEvent::Preferences(config), EffectExecution::LOCAL);
        // When the playlist re-attaches to the main window, snap its width back to
        // the player width so the docked stack matches the player (GTK parity).
        if was_playlist_detached && !self.core.store().state().config.playlist_detached {
            self.set_playlist_size(crate::render::PLAYLIST_MIN_WIDTH, self.playlist.height);
        }
        #[cfg(target_os = "android")]
        self.sync_scale_factor_from_config();
        self.apply_visualization_preferences();
        #[cfg(target_os = "android")]
        {
            if skin_changed {
                self.flush_android_platform_policies(true);
                if let Err(err) = super::android::refresh_player_widgets() {
                    app_log_error!(frontend, "failed to refresh Android player widgets", err);
                    let message = format!("failed to refresh Android player widgets: {err}");
                    if !self.runtime.pending_messages.contains(&message) {
                        self.runtime.pending_messages.push(message);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "android")]
    fn flush_android_platform_policies(&mut self, force_persistence: bool) {
        effect_executor::flush_android_media_projection(
            &mut self.android,
            &self.core,
            self.core.store().state(),
            &mut self.runtime.pending_messages,
        );
        effect_executor::flush_android_persistence(
            &mut self.android,
            &self.core,
            self.core.store().state(),
            &mut self.runtime.pending_messages,
            force_persistence,
        );
    }

    pub(crate) fn preferences_open(&self) -> bool {
        self.core.store().state().ui.preferences_visible
    }

    pub(crate) fn main_menu_open(&self) -> bool {
        self.core.store().state().ui.main_menu_visible
    }

    pub(crate) fn skin_browser_open(&self) -> bool {
        self.core.store().state().ui.skin_browser_visible
    }

    pub(crate) fn file_info_open(&self) -> bool {
        self.core.store().state().ui.file_info_visible
    }

    pub(crate) fn scale_factor(&self) -> f32 {
        #[cfg(target_os = "android")]
        {
            self.scale_factor
        }
        #[cfg(not(target_os = "android"))]
        {
            clamped_scale_factor(self.core.store().state().config.scale_factor) as f32
        }
    }

    #[cfg(target_os = "android")]
    pub(crate) fn sync_scale_factor_from_config(&mut self) {
        #[cfg(target_os = "android")]
        {
            self.scale_factor =
                clamped_scale_factor(self.core.store().state().config.scale_factor) as f32;
        }
    }

    pub(crate) fn visualization_render_state(&self) -> VisualizationRenderState {
        self.core.visualization_render_state()
    }

    pub(crate) fn eof_pause_remaining_ms(&self) -> Option<i64> {
        self.core.transition().wait_remaining()
    }

    pub(crate) fn apply_visualization_preferences(&mut self) {
        self.core.apply_visualization_preferences();
    }

    fn visualization_refresh_divisor(&self) -> i32 {
        self.core
            .store()
            .state()
            .config
            .vis_refresh_divisor
            .clamp(1, 8)
    }

    fn tick_visualization(&mut self) -> bool {
        if !self.visualization_should_animate() {
            self.core.reset_visualization_tick();
            return false;
        }

        self.core
            .tick_visualization(self.visualization_refresh_divisor() as usize);
        true
    }

    fn visualization_should_animate(&self) -> bool {
        if self.core.store().state().player.state() != PlayerState::Playing
            || self.core.visualization_render_state().mode == VisMode::Off
        {
            return false;
        }
        #[cfg(target_os = "android")]
        if !super::android::is_foreground_activity(self.android.activity_generation()) {
            return false;
        }
        true
    }

    fn playback_repaint_interval(&self) -> Option<Duration> {
        if self.core.stop_fade_active() || self.core.transition().wait_remaining().is_some() {
            return Some(Duration::from_millis(20));
        }
        if self.core.store().state().player.state() != PlayerState::Playing {
            return None;
        }
        if self.visualization_should_animate() {
            return Some(VISUALIZER_REPAINT_INTERVAL * self.visualization_refresh_divisor() as u32);
        }
        Some(POSITION_REPAINT_INTERVAL)
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
        let translation = translate_socket_command(&request.command);
        for event in translation.events {
            let RuntimeEvent::Command(command) = event else {
                unreachable!()
            };
            self.dispatch(command);
        }
        if translation.action == Some(FrontendAction::Quit) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        request.accept();
    }

    #[cfg(feature = "desktop-egui")]
    fn current_mpris_player_properties(&self) -> crate::mpris::MprisPlayerProperties {
        mpris_player_properties(
            self.core.store().state(),
            self.core.store().state().config.playback_position_ms,
        )
    }

    #[cfg(feature = "desktop-egui")]
    fn sync_mpris_properties(&mut self, extra_events: impl IntoIterator<Item = MprisEvent>) {
        let properties = self.current_mpris_player_properties();
        if let Some(service) = &mut self.mpris_service {
            let mut events = service.update_player_properties(properties);
            events.extend(extra_events);
            service.emit_events(&events);
        }
    }

    #[cfg(feature = "desktop-egui")]
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

    #[cfg(feature = "desktop-egui")]
    fn handle_mpris_request(
        &mut self,
        ctx: &egui::Context,
        request: MprisServiceRequest,
    ) -> Vec<MprisEvent> {
        match request {
            MprisServiceRequest::Command(command) => self.handle_mpris_command(ctx, command),
            MprisServiceRequest::SetVolume(volume) => {
                self.dispatch(crate::app::command::AudioCommand::SetVolume(
                    (volume * 100.0) as i32,
                ));
                vec![MprisEvent::PlaybackStatusChanged]
            }
        }
    }

    #[cfg(feature = "desktop-egui")]
    fn handle_mpris_command(
        &mut self,
        ctx: &egui::Context,
        command: MprisCommand,
    ) -> Vec<MprisEvent> {
        let translation = translate_mpris_command(&command, self.core.store().state());
        match translation.action {
            Some(FrontendAction::Raise) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Some(FrontendAction::Quit) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            None => {}
        }
        for event in translation.events {
            let RuntimeEvent::Command(app_command) = event else {
                unreachable!()
            };
            self.dispatch(app_command);
        }
        for event in mpris_open_uri_playback_events(&command, self.core.store().state()) {
            let RuntimeEvent::Command(app_command) = event else {
                unreachable!()
            };
            self.dispatch(app_command);
        }
        mpris_service_events(
            &command,
            self.core.store().state(),
            translation.properties_before.as_ref(),
        )
    }

    pub fn poll_playback_backend(&mut self) {
        #[cfg(target_os = "android")]
        if !self.core.has_backend() {
            if let Some(backend) = super::android::existing_playback_backend() {
                match self.core.install_backend_with_dsp(Box::new(backend)) {
                    Ok(()) => {
                        app_log_info!(backend, "egui attached existing Android playback backend")
                    }
                    Err(err) => self.runtime.pending_messages.push(err),
                }
            }
        }

        if let Some(events) = self.core.poll_playback_events() {
            match events {
                Ok(events) => self.handle_playback_events(events),
                Err(err) => self.runtime.pending_messages.push(err),
            }

            let stream_info = self.core.backend_stream_info();
            if let Some(stream_info) = stream_info {
                self.process_store_event(
                    RuntimeEvent::Playback(PlaybackEvent::StreamInfo(stream_info)),
                    EffectExecution::LOCAL,
                );
            }
        }
    }

    fn handle_playback_events(&mut self, events: impl IntoIterator<Item = PlaybackEvent>) {
        let mut backend_ready = false;
        for event in events {
            backend_ready |= matches!(
                event,
                PlaybackEvent::AsyncDone | PlaybackEvent::DurationChanged(_)
            );
            let end_of_stream = matches!(event, PlaybackEvent::EndOfStream);
            self.process_store_event(RuntimeEvent::Playback(event), EffectExecution::LOCAL);
            if end_of_stream {
                self.process_store_event(RuntimeEvent::PlaylistEof, EffectExecution::LOCAL);
            }
        }
        if backend_ready {
            self.apply_pending_backend_seek(false);
        }
        if let Some(duration_ms) = self.core.backend_duration_ms() {
            self.process_store_event(
                RuntimeEvent::Playback(PlaybackEvent::DurationChanged(Some(duration_ms))),
                EffectExecution::LOCAL,
            );
            self.apply_pending_backend_seek(true);
        }
        if let Some(position_ms) = self.core.backend_position_ms() {
            self.process_store_event(
                RuntimeEvent::BackendPosition(position_ms),
                EffectExecution::LOCAL,
            );
        }
    }

    fn poll_duration_index_results(&mut self) -> bool {
        let update = self.core.drain_duration_updates();
        let changed = !update.changes.is_empty();
        let update = self.apply_runtime_update(update, false);
        #[cfg(target_os = "android")]
        {
            effect_executor::apply_android_post_dispatch(&mut self.android, update.changes);
            self.flush_android_platform_policies(update.force_persistence);
        }
        #[cfg(not(target_os = "android"))]
        let _ = update;
        changed
    }

    fn schedule_missing_local_playlist_durations(&mut self) {
        self.core.schedule_missing_durations();
    }

    fn apply_pending_backend_seek(&mut self, final_attempt: bool) {
        let Some(position_ms) = self.core.pending_seek_ms() else {
            return;
        };
        let update = self.core.apply_pending_start_seek(final_attempt);
        if update.transition_changed {
            app_log_info!(backend, "egui applied pending start seek", position_ms);
        }
        self.apply_runtime_update(update, false);
    }

    fn tick_playback_position(&mut self, ctx: &egui::Context) {
        let elapsed = self.root.playback_elapsed(Instant::now());
        let Some(repaint_interval) = self.playback_repaint_interval() else {
            return;
        };
        ctx.request_repaint_after(repaint_interval);
        let visualizer_changed = self.tick_visualization();
        let elapsed_ms = elapsed.as_millis().min(i64::MAX as u128) as i64;
        if elapsed_ms == 0 {
            if visualizer_changed {
                ctx.request_repaint();
            }
            return;
        }
        let waiting_between_songs = self.core.transition().wait_remaining().is_some();
        self.process_store_event(
            RuntimeEvent::TransitionTick(elapsed_ms),
            EffectExecution::LOCAL,
        );
        if self.core.stop_fade_active() {
            let update = self.core.tick_stop_fade(elapsed_ms);
            #[cfg(target_os = "android")]
            effect_executor::apply_android_post_dispatch(&mut self.android, update.changes);
            let update = self.apply_runtime_update(update, false);
            #[cfg(target_os = "android")]
            self.flush_android_platform_policies(update.force_persistence);
            #[cfg(not(target_os = "android"))]
            let _ = update;
        } else if self.core.store().state().player.state() == PlayerState::Playing
            && !waiting_between_songs
            && !self.core.transition().needs_fast_tick()
        {
            self.process_store_event(
                RuntimeEvent::PlaybackTick(elapsed_ms),
                EffectExecution::LOCAL,
            );
        }
        if visualizer_changed {
            ctx.request_repaint();
        }
    }

    pub(super) fn open_file_dialog(&mut self, request: FileDialogRequest) {
        self.process_store_event(
            RuntimeEvent::OpenFileDialog(request),
            EffectExecution::LOCAL,
        );
    }

    pub(super) fn request_config_save(&mut self) {
        self.process_store_event(RuntimeEvent::SaveConfig, EffectExecution::LOCAL);
    }

    pub(super) fn request_render(&mut self, target: RenderTarget) {
        self.process_store_event(RuntimeEvent::RequestRender(target), EffectExecution::LOCAL);
    }

    fn handle_runtime_event(
        &mut self,
        event: RuntimeEvent,
        execution: EffectExecution,
    ) -> RuntimeUpdate {
        let mut update = self.core.handle(event, execution);
        for start in std::mem::take(&mut update.pending_playback_starts) {
            #[cfg(all(target_os = "android", not(test)))]
            if execution == EffectExecution::LOCAL && !self.core.has_backend() {
                let preparation = super::android::shared_playback_backend()
                    .map_err(|err| format!("failed to initialize audio output: {err}"))
                    .and_then(|backend| self.core.install_backend_with_dsp(Box::new(backend)));
                if let Err(error) = preparation {
                    self.core
                        .handle(RuntimeEvent::PlaybackStartCancelled(start), execution);
                    self.handle_frontend_playback_error(error);
                    continue;
                }
            }
            update.merge(
                self.core
                    .handle(RuntimeEvent::PlaybackStartPrepared(start), execution),
            );
        }
        let start_uri = update
            .playback_effects
            .iter()
            .any(|effect| matches!(effect, crate::app::effect::PlaybackEffect::StartUri { .. }));
        self.apply_runtime_update(update, start_uri)
    }

    fn apply_runtime_update(&mut self, update: RuntimeUpdate, start_uri: bool) -> RuntimeUpdate {
        #[cfg(not(test))]
        if start_uri && !self.core.has_backend() && !update.messages.is_empty() {
            for error in &update.messages {
                self.handle_frontend_playback_error(error.clone());
            }
        } else {
            self.runtime
                .pending_messages
                .extend(update.messages.iter().cloned());
        }
        #[cfg(test)]
        self.runtime
            .pending_messages
            .extend(update.messages.iter().cloned());
        #[cfg(test)]
        let _ = start_uri;
        for target in &update.render_targets {
            self.runtime.apply_effect(UiEffect::QueueRender(*target));
        }
        for effect in &update.frontend_effects {
            match effect.clone() {
                FrontendEffect::Ui(effect) => self.execute_ui_effect(effect),
                FrontendEffect::Platform(effect) => effect_executor::execute_platform_effect(
                    effect,
                    &self.core,
                    &mut self.runtime.pending_messages,
                    #[cfg(target_os = "android")]
                    &mut self.android,
                ),
            }
        }
        update
    }

    fn execute_ui_effect(&mut self, effect: UiEffect) {
        app_log_debug!(frontend_effect, "egui {effect:?}");
        match effect {
            UiEffect::OpenFileDialog(request) => self.handle_file_dialog(request),
            UiEffect::OpenFileInfoDialog => self.dispatch(UiCommand::SetFileInfoVisible(true)),
            UiEffect::OpenPreferences => self.dispatch(UiCommand::SetPreferencesVisible(true)),
            UiEffect::OpenSkinBrowser => self.dispatch(UiCommand::SetSkinBrowserVisible(true)),
            UiEffect::QueueRender(target) => {
                self.runtime.apply_effect(UiEffect::QueueRender(target));
            }
            UiEffect::OpenPath(path) => self.runtime.apply_effect(UiEffect::OpenPath(path)),
            UiEffect::OpenSkinEditor => self.runtime.apply_effect(UiEffect::OpenSkinEditor),
            UiEffect::ShowError(message) => {
                self.runtime.apply_effect(UiEffect::ShowError(message));
            }
            UiEffect::ShowMessage(message) => {
                self.runtime.apply_effect(UiEffect::ShowMessage(message));
            }
        }
    }

    fn process_store_event(
        &mut self,
        event: RuntimeEvent,
        execution: EffectExecution,
    ) -> StateChangeSet {
        let update = self.handle_runtime_event(event, execution);
        #[cfg(target_os = "android")]
        {
            effect_executor::apply_android_post_dispatch(&mut self.android, update.changes);
            self.flush_android_platform_policies(update.force_persistence);
        }
        update.changes
    }

    #[cfg(not(test))]
    fn handle_frontend_playback_error(&mut self, error: String) {
        self.runtime.pending_messages.push(error.clone());
        self.process_store_event(
            RuntimeEvent::Playback(PlaybackEvent::Error(error)),
            EffectExecution::LOCAL,
        );
    }

    #[cfg(feature = "desktop-egui")]
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
            FileDialogRequest::ImportPlaylist => {
                self.runtime
                    .pending_messages
                    .push("playlist import is Android-only".to_string());
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
                    .add_filter("Winamp EQF files", &["eqf"])
                    .pick_file()
                {
                    self.load_equalizer_preset_file(&path);
                }
            }
            FileDialogRequest::SaveEqualizerPreset => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Save equalizer preset")
                    .add_filter("Winamp EQF files", &["eqf"])
                    .set_file_name("preset.eqf")
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

    #[cfg(target_os = "android")]
    fn handle_file_dialog(&mut self, request: FileDialogRequest) {
        let result = match request {
            FileDialogRequest::LoadPlaylist => {
                self.android.playlist_manager.open();
                return;
            }
            FileDialogRequest::SavePlaylist => {
                self.android.playlist_manager.open();
                return;
            }
            FileDialogRequest::SaveEqualizerPreset => {
                let preset = self.current_equalizer_preset("Entry1");
                super::android::save_equalizer_preset(&serialize_winamp_eqf(&preset))
            }
            _ => super::android::open(request),
        };
        if let Err(err) = result {
            self.runtime.pending_messages.push(err);
        }
    }

    #[cfg(all(not(feature = "desktop-egui"), not(target_os = "android")))]
    fn handle_file_dialog(&mut self, _request: FileDialogRequest) {
        self.runtime
            .pending_messages
            .push("file picking is not available on this platform".to_string());
    }

    fn load_playlist_file(&mut self, path: &Path) -> bool {
        match Playlist::load_m3u_file(path) {
            Ok(playlist) => {
                self.apply_loaded_playlist(playlist);
                true
            }
            Err(err) => {
                self.runtime.pending_messages.push(format!(
                    "failed to load playlist '{}': {err}",
                    path.display()
                ));
                false
            }
        }
    }

    fn apply_loaded_playlist(&mut self, playlist: Playlist) {
        self.playlist.scroll_offset = 0;
        self.process_store_event(
            RuntimeEvent::ReplacePlaylist(playlist),
            EffectExecution::LOCAL,
        );
        self.schedule_missing_local_playlist_durations();
    }

    #[cfg(feature = "desktop-egui")]
    fn save_playlist_file(&mut self, path: &Path) {
        if let Err(err) = self.core.store().state().playlist.save_m3u_file(path) {
            self.runtime.pending_messages.push(format!(
                "failed to save playlist '{}': {err}",
                path.display()
            ));
        }
    }

    fn load_equalizer_preset_file(&mut self, path: &Path) {
        match load_winamp_eqf_first(path) {
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

    #[cfg(feature = "desktop-egui")]
    fn save_equalizer_preset_file(&mut self, path: &Path) {
        let path = Self::path_with_extension(path, "eqf");
        let preset = self.current_equalizer_preset("Entry1");
        if let Err(err) = save_winamp_eqf(&path, &preset) {
            self.runtime.pending_messages.push(format!(
                "failed to save equalizer preset '{}': {err}",
                path.display()
            ));
        }
    }

    #[cfg(any(feature = "desktop-egui", target_os = "android"))]
    fn current_equalizer_preset(&self, name: &str) -> EqualizerPreset {
        let config = &self.core.store().state().config;
        EqualizerPreset::from_positions(
            name,
            config.equalizer_preamp_pos,
            config.equalizer_band_pos,
        )
    }

    #[cfg(feature = "desktop-egui")]
    fn path_with_extension(path: &Path, extension: &str) -> PathBuf {
        if path
            .extension()
            .is_some_and(|current| current.eq_ignore_ascii_case(extension))
        {
            path.to_path_buf()
        } else {
            path.with_extension(extension)
        }
    }

    fn apply_equalizer_preset(&mut self, preset: &EqualizerPreset) {
        self.process_store_event(
            RuntimeEvent::EqualizerPreset {
                preamp: preset.preamp_position(),
                bands: preset.band_positions(),
            },
            EffectExecution::LOCAL,
        );
    }

    #[cfg(target_os = "android")]
    fn handle_android_file_picker_result(&mut self, result: super::android::AndroidPickerResult) {
        if let Some(error) = result.error {
            self.runtime.pending_messages.push(error);
            return;
        }
        let audio_import = matches!(
            result.request,
            FileDialogRequest::AddAudioFiles | FileDialogRequest::AddAudioDirectory
        );
        if audio_import && result.complete && result.paths.is_empty() {
            self.schedule_missing_local_playlist_durations();
            return;
        }
        if result.is_cancelled() {
            return;
        }
        match result.request {
            FileDialogRequest::AddAudioFiles => {
                self.process_store_event(
                    RuntimeEvent::Command(PlaylistCommand::AddFiles(result.paths).into()),
                    EffectExecution::LOCAL,
                );
            }
            FileDialogRequest::AddAudioDirectory => {
                let paths = result
                    .paths
                    .into_iter()
                    .filter(|path| crate::playlist::is_media_file(path))
                    .collect();
                self.process_store_event(
                    RuntimeEvent::Command(PlaylistCommand::AddFiles(paths).into()),
                    EffectExecution::LOCAL,
                );
            }
            FileDialogRequest::LoadPlaylist => {
                if let Some(path) = result.paths.first() {
                    self.load_playlist_file(path);
                }
            }
            FileDialogRequest::ImportPlaylist => {
                if let Some(path) = result.paths.first() {
                    if let Err(err) = self.android.playlist_manager.import(path) {
                        self.runtime.pending_messages.push(err);
                    }
                }
            }
            FileDialogRequest::LoadEqualizerPreset => {
                if let Some(path) = result.paths.first() {
                    self.load_equalizer_preset_file(path);
                }
            }
            FileDialogRequest::ImportSkin => {
                if let Some(path) = result.paths.first() {
                    import_skin_path(self, path);
                }
            }
            FileDialogRequest::SavePlaylist
            | FileDialogRequest::SaveEqualizerPreset
            | FileDialogRequest::ExportSkin => {}
        }
    }

    #[cfg(target_os = "android")]
    fn handle_external_android_media_volume(&mut self, volume: i32) {
        self.process_store_event(
            RuntimeEvent::ExternalOutputVolume(volume),
            EffectExecution::LOCAL,
        );
    }

    #[cfg(target_os = "android")]
    fn handle_android_media_control(&mut self, event: super::android::AndroidMediaControlEvent) {
        let control = event.control;
        if matches!(control, super::android::AndroidMediaControl::PlaylistEof) {
            self.process_store_event(
                RuntimeEvent::PlaylistEof,
                EffectExecution::after_external_backend_execution(event.backend_executed),
            );
            return;
        }
        if let super::android::AndroidMediaControl::PlayMediaItem(index) = control {
            self.process_store_event(
                RuntimeEvent::Command(PlaylistCommand::SetPosition(index).into()),
                EffectExecution::LOCAL,
            );
            self.process_store_event(
                RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
                EffectExecution::after_external_backend_execution(event.backend_executed),
            );
            return;
        }
        let command = android_player_command_for_media_control(control)
            .expect("playlist media controls handled above");
        self.process_store_event(
            RuntimeEvent::Command(command.into()),
            EffectExecution::after_external_backend_execution(event.backend_executed),
        );
    }

    #[cfg(target_os = "android")]
    fn poll_android_platform_events(&mut self, ctx: &egui::Context) -> bool {
        let activity_generation = self.android.activity_generation();
        let Some(mut events) = super::android::drain_platform_events(activity_generation, ctx)
        else {
            return false;
        };
        let mut activity_media_control_handled = false;
        let mut playback_events = Vec::new();
        for event in &mut events {
            match event {
                super::android::AndroidPlatformEvent::Picker(result) => {
                    self.handle_android_file_picker_result(result);
                }
                super::android::AndroidPlatformEvent::MediaControl(control) => {
                    activity_media_control_handled |= control.complete_activity_control;
                    self.handle_android_media_control(control);
                }
                super::android::AndroidPlatformEvent::Playback(event) => {
                    playback_events.push(event);
                }
                super::android::AndroidPlatformEvent::ExternalVolumeChanged(volume) => {
                    self.handle_external_android_media_volume(volume);
                }
            }
        }
        self.handle_playback_events(playback_events);
        self.poll_playback_backend();
        activity_media_control_handled
    }

    #[cfg(target_os = "android")]
    fn update_android_layout_policy(&mut self) {
        let snapshot = super::android::window_layout_snapshot_pixels().and_then(|layout| {
            Some(AndroidLayoutSnapshot {
                width: layout.width,
                height: layout.height,
                orientation: AndroidLayoutOrientation::from_configuration(layout.orientation)?,
                insets: layout.insets,
                inset_width: layout.inset_width,
                inset_height: layout.inset_height,
                inset_orientation: layout.inset_orientation,
                config_generation: layout.config_generation,
                inset_generation: layout.inset_generation,
                insets_fresh: layout.insets_fresh,
            })
        });
        let update = self.android.observe_layout(snapshot);
        if update.orientation_changed {
            self.render_cache.playlist = None;
        }
        match update.repaint {
            AndroidLayoutReadiness::None => {}
            AndroidLayoutReadiness::AwaitingReadiness => self
                .runtime
                .request_android_layout_repaint(AndroidLayoutRepaint::AwaitingReadiness),
            AndroidLayoutReadiness::Stabilizing => self
                .runtime
                .request_android_layout_repaint(AndroidLayoutRepaint::Stabilizing),
        }
    }
}

impl EguiFrontendState {
    fn desired_window_size(&self) -> egui::Vec2 {
        let config = &self.core.store().state().config;
        let (width, height) = docked_panel_size(DockedPanelState {
            main_shaded: config.main_shaded,
            equalizer_visible: config.equalizer_visible,
            equalizer_detached: config.equalizer_detached,
            equalizer_shaded: config.equalizer_shaded,
            playlist_visible: config.playlist_visible,
            playlist_detached: config.playlist_detached,
            playlist_shaded: config.playlist_shaded,
            playlist_width: self.playlist.width,
            playlist_height: self.playlist.height,
            ..DockedPanelState::default()
        });
        egui::vec2(
            width as f32 * self.scale_factor(),
            height as f32 * self.scale_factor(),
        )
    }

    #[cfg(not(target_os = "android"))]
    fn take_root_viewport_resize(&mut self) -> Option<egui::Vec2> {
        let desired_size = self.desired_window_size();
        self.root.request_size_if_changed(desired_size)
    }

    pub(crate) fn set_playlist_size(&mut self, width: i32, height: i32) -> bool {
        let size = snap_playlist_size(width, height);
        let entry_count = self.core.store().state().playlist.len();
        self.playlist.resize(size.width, size.height, entry_count)
    }

    pub(crate) fn playlist_visible_rows(&self) -> usize {
        self.playlist.visible_rows()
    }

    pub(crate) fn playlist_max_scroll_offset(&self) -> usize {
        self.core
            .store()
            .state()
            .playlist
            .len()
            .saturating_sub(self.playlist_visible_rows())
    }
}

impl eframe::App for EguiFrontendState {
    #[cfg(target_os = "android")]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let activity_generation = self.android.activity_generation();
        self.android.mark_persistence();
        self.flush_android_platform_policies(true);
        super::android::runtime_exited(activity_generation);
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        {
            let mut repaint_context = self
                .root
                .repaint_context
                .lock()
                .expect("egui repaint context poisoned");
            if repaint_context.is_none() {
                *repaint_context = Some(ctx.clone());
            }
        }
        #[cfg(target_os = "android")]
        let android_activity_media_control_handled = self.poll_android_platform_events(ctx);
        #[cfg(target_os = "android")]
        self.update_android_layout_policy();
        self.poll_socket_control(ctx);
        #[cfg(feature = "desktop-egui")]
        self.poll_mpris_requests(ctx);
        #[cfg(not(target_os = "android"))]
        self.poll_playback_backend();
        self.poll_duration_index_results();
        self.tick_playback_position(ctx);
        #[cfg(target_os = "android")]
        {
            self.flush_android_platform_policies(false);
            if android_activity_media_control_handled {
                if let Err(err) =
                    super::android::complete_media_control(self.android.activity_generation())
                {
                    self.runtime.pending_messages.push(err);
                }
            }
        }
        handle_dropped_files(ctx, self);
        handle_global_shortcuts(ctx, self);
        #[cfg(feature = "desktop-egui")]
        self.sync_mpris_properties(std::iter::empty());
        #[cfg(not(target_os = "android"))]
        if let Some(size) = self.take_root_viewport_resize() {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        sync_root_viewport_focus(&ctx, self);
        #[cfg(target_os = "android")]
        apply_android_skin_visuals(&ctx, &self.active_skin);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                #[cfg(target_os = "android")]
                {
                    self.show_android_docked_layout(ui);
                }
                #[cfg(not(target_os = "android"))]
                {
                    main_player::show_main_player(ui, self);
                    if self.core.store().state().config.equalizer_visible
                        && !self.core.store().state().config.equalizer_detached
                    {
                        equalizer::show_equalizer(ui, self);
                    }
                    if self.core.store().state().config.playlist_visible
                        && !self.core.store().state().config.playlist_detached
                    {
                        playlist::show_playlist(ui, self);
                    }
                }
            });
        show_detached_panels(&ctx, self);
        show_detached_equalizer_popovers(&ctx, self);
        menu::show_main_menu(&ctx, self);
        menu::show_prompts(&ctx, self);
        if self.preferences_open() {
            preferences::show_preferences(&ctx, self);
        }
        file_info::show_file_info_dialog(&ctx, self);
        #[cfg(target_os = "android")]
        if self.android.playlist_manager.is_open() {
            let layout = self.android.ready_layout();
            let action = playlist::show_android_playlist_manager(
                &ctx,
                &mut self.android.playlist_manager,
                layout,
            );
            if let Some(action) = action {
                self.handle_android_playlist_manager_action(action);
            }
        }
        #[cfg(target_os = "android")]
        if self.skin_browser_open() {
            self.ui.selected_preferences_page = PreferencesPage::Skins;
            self.dispatch(UiCommand::SetSkinBrowserVisible(false));
            self.dispatch(UiCommand::SetPreferencesVisible(true));
        }
        #[cfg(not(target_os = "android"))]
        if self.skin_browser_open() {
            show_skin_browser_placeholder(&ctx, self);
        }
        menu::show_pending_messages(&ctx, self);
        #[cfg(target_os = "android")]
        {
            self.flush_android_platform_policies(false);
            if let Some(delay) = self.android.persistence_delay() {
                ctx.request_repaint_after(delay);
            }
        }
        self.flush_repaint_policy(&ctx);
        app_log_trace!(render, "egui update size={:?}", self.desired_window_size());
    }
}

#[cfg(target_os = "android")]
fn android_main_player_scale(available_width: f32) -> f32 {
    (available_width / crate::render::MAIN_WINDOW_WIDTH as f32).max(f32::EPSILON)
}

impl EguiFrontendState {
    fn flush_repaint_policy(&mut self, ctx: &egui::Context) {
        let schedule = self.runtime.take_repaint_schedule();
        if schedule.immediate {
            ctx.request_repaint();
        }
        if let Some(delay) = schedule.after {
            ctx.request_repaint_after(delay);
        }
    }

    pub(crate) fn close_player_menus(&mut self) {
        self.ui.dismiss_playlist_overlay();
        if self.main_menu_open() {
            self.dispatch(UiCommand::SetMainMenuVisible(false));
        }
    }

    fn replace_active_skin(&mut self, skin: DefaultSkin) {
        self.active_skin = skin;
        self.render_cache.invalidate();
    }

    pub(crate) fn refresh_runtime_skins(&mut self) {
        self.skins.refresh();
    }

    pub(crate) fn ensure_runtime_skins(&mut self) {
        self.skins.ensure_discovered();
    }

    #[cfg(target_os = "android")]
    pub(crate) fn select_default_skin(&mut self) {
        match DefaultSkin::load_bundled() {
            Ok(skin) => {
                self.replace_active_skin(skin);
                let mut config = self.controller().state().persistence_snapshot().config;
                config.skin = None;
                self.apply_preferences_config(config);
            }
            Err(err) => self
                .runtime
                .pending_messages
                .push(format!("failed to load bundled default skin: {err}")),
        }
    }

    #[cfg(target_os = "android")]
    pub(crate) fn select_skin_path(&mut self, path: PathBuf) {
        let entry = SkinEntry::from_path(path);
        select_skin_entry(self, &entry);
    }

    #[cfg(target_os = "android")]
    pub(crate) fn import_skin(&mut self) {
        import_skin_from_dialog(self);
    }

    #[cfg(target_os = "android")]
    fn handle_android_playlist_manager_action(&mut self, action: PlaylistManagerAction) {
        let outcome = {
            let current_playlist = &self.core.store().state().playlist;
            self.android
                .playlist_manager
                .handle_action(action, current_playlist)
        };
        match outcome {
            Ok(PlaylistManagerOutcome::None) => {}
            Ok(PlaylistManagerOutcome::ImportRequested) => {
                self.open_file_dialog(FileDialogRequest::ImportPlaylist);
            }
            Ok(PlaylistManagerOutcome::PlaylistLoaded(playlist)) => {
                self.apply_loaded_playlist(playlist);
            }
            Err(err) => self.runtime.pending_messages.push(err),
        }
    }

    #[cfg(target_os = "android")]
    pub(crate) fn delete_skin_path(&mut self, path: PathBuf) {
        if !is_user_imported_skin_path(&path) {
            self.runtime
                .pending_messages
                .push(format!("cannot delete unmanaged skin '{}'", path.display()));
            return;
        }
        let selected = self.controller().state().config.skin.as_deref()
            == Some(path.to_string_lossy().as_ref());
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        if let Err(err) = result {
            self.runtime
                .pending_messages
                .push(format!("failed to delete skin '{}': {err}", path.display()));
            return;
        }
        if selected {
            self.select_default_skin();
        }
        self.refresh_runtime_skins();
    }
}

#[cfg(target_os = "android")]
impl EguiFrontendState {
    fn show_android_docked_layout(&mut self, ui: &mut egui::Ui) {
        let pixels_per_point = ui.ctx().pixels_per_point();
        let layout = match self.android.layout_view() {
            AndroidLayoutView::Unavailable => return,
            AndroidLayoutView::Transitioning {
                orientation,
                width,
                height,
            } => {
                self.show_android_last_stable_layout(
                    ui,
                    orientation,
                    width,
                    height,
                    pixels_per_point,
                );
                return;
            }
            AndroidLayoutView::Ready(layout) => layout,
        };
        let orientation = layout.orientation;
        let insets = layout.insets;
        let left_inset = insets.left as f32 / pixels_per_point;
        let top_inset = insets.top as f32 / pixels_per_point;
        let right_inset = insets.right as f32 / pixels_per_point;
        let bottom_inset = insets.bottom as f32 / pixels_per_point;
        let window_size = egui::vec2(
            layout.width as f32 / pixels_per_point,
            layout.height as f32 / pixels_per_point,
        );
        ui.set_width(window_size.x);
        ui.set_height(window_size.y);
        ui.set_clip_rect(egui::Rect::from_min_size(ui.min_rect().min, window_size));
        let usable_width = window_size.x - left_inset - right_inset;
        let usable_height = window_size.y - top_inset - bottom_inset;

        ui.add_space(top_inset);
        ui.horizontal(|ui| {
            ui.add_space(left_inset);
            match orientation {
                AndroidLayoutOrientation::Landscape => {
                    self.scale_factor = self.android_landscape_scale(usable_width, usable_height);
                    self.adjust_android_landscape_playlist_size(usable_width, usable_height);
                    ui.vertical(|ui| self.show_android_player_column(ui));
                    if self.android_docked_playlist_visible() {
                        playlist::show_playlist(ui, self);
                    }
                }
                AndroidLayoutOrientation::Portrait => {
                    self.scale_factor = android_main_player_scale(usable_width);
                    self.adjust_android_playlist_height(usable_height);
                    ui.vertical(|ui| {
                        self.show_android_player_column(ui);
                        if self.android_docked_playlist_visible() {
                            playlist::show_playlist(ui, self);
                        }
                    });
                }
            }
            ui.add_space(right_inset);
        });
        let stable_layout = AndroidStableLayout {
            width: layout.width,
            height: layout.height,
            insets,
            scale_factor: self.scale_factor(),
            playlist_width: self.playlist.width,
            playlist_height: self.playlist.height,
        };
        self.android.remember_layout(orientation, stable_layout);
    }

    fn show_android_last_stable_layout(
        &mut self,
        ui: &mut egui::Ui,
        orientation: AndroidLayoutOrientation,
        current_width: i32,
        current_height: i32,
        pixels_per_point: f32,
    ) {
        let stable = self.android.stable_layout(orientation);
        let Some(stable) = stable else {
            return;
        };
        let current_size = egui::vec2(
            current_width.max(1) as f32 / pixels_per_point,
            current_height.max(1) as f32 / pixels_per_point,
        );
        let stable_size = egui::vec2(
            stable.width as f32 / pixels_per_point,
            stable.height as f32 / pixels_per_point,
        );
        let clip_size = egui::vec2(
            current_size.x.min(stable_size.x),
            current_size.y.min(stable_size.y),
        );
        ui.set_width(current_size.x);
        ui.set_height(current_size.y);
        ui.set_clip_rect(egui::Rect::from_min_size(ui.min_rect().min, clip_size));
        self.scale_factor = stable.scale_factor;
        self.playlist.resize(
            stable.playlist_width,
            stable.playlist_height,
            self.core.store().state().playlist.len(),
        );
        ui.add_space(stable.insets.top as f32 / pixels_per_point);
        ui.horizontal(|ui| {
            ui.add_space(stable.insets.left as f32 / pixels_per_point);
            match orientation {
                AndroidLayoutOrientation::Landscape => {
                    ui.vertical(|ui| self.show_android_player_column(ui));
                    if self.android_docked_playlist_visible() {
                        playlist::show_playlist(ui, self);
                    }
                }
                AndroidLayoutOrientation::Portrait => {
                    ui.vertical(|ui| {
                        self.show_android_player_column(ui);
                        if self.android_docked_playlist_visible() {
                            playlist::show_playlist(ui, self);
                        }
                    });
                }
            }
            ui.add_space(stable.insets.right as f32 / pixels_per_point);
        });
    }

    fn show_android_player_column(&mut self, ui: &mut egui::Ui) {
        main_player::show_main_player(ui, self);
        let config = &self.core.store().state().config;
        if config.equalizer_visible && !config.equalizer_detached {
            equalizer::show_equalizer(ui, self);
        }
    }

    fn android_docked_playlist_visible(&self) -> bool {
        let config = &self.core.store().state().config;
        config.playlist_visible && !config.playlist_detached
    }

    fn android_landscape_scale(&self, usable_width: f32, usable_height: f32) -> f32 {
        let config = &self.core.store().state().config;
        let mut left_height = main_window_height(config.main_shaded);
        if config.equalizer_visible && !config.equalizer_detached {
            left_height += equalizer_window_height(config.equalizer_shaded);
        }
        let width_scale = (usable_width * 0.5) / crate::render::MAIN_WINDOW_WIDTH as f32;
        let height_scale = usable_height / left_height.max(1) as f32;
        width_scale.min(height_scale).max(f32::EPSILON)
    }

    fn adjust_android_landscape_playlist_size(&mut self, usable_width: f32, usable_height: f32) {
        let config = &self.core.store().state().config;
        if !config.playlist_visible || config.playlist_detached || config.playlist_shaded {
            return;
        }
        let player_width = crate::render::MAIN_WINDOW_WIDTH as f32 * self.scale_factor();
        let width = ((usable_width - player_width) / self.scale_factor())
            .floor()
            .max(PLAYLIST_MIN_WIDTH as f32) as i32;
        let height = (usable_height / self.scale_factor())
            .floor()
            .max(PLAYLIST_MIN_HEIGHT as f32) as i32;
        self.playlist
            .resize(width, height, self.core.store().state().playlist.len());
    }

    fn adjust_android_playlist_height(&mut self, available_height: f32) {
        let config = &self.core.store().state().config;
        if !config.playlist_visible || config.playlist_detached || config.playlist_shaded {
            return;
        }
        let mut occupied_height = main_window_height(config.main_shaded);
        if config.equalizer_visible && !config.equalizer_detached {
            occupied_height += equalizer_window_height(config.equalizer_shaded);
        }
        let playlist_height =
            (available_height / self.scale_factor()).floor() as i32 - occupied_height;
        self.playlist.resize(
            crate::render::MAIN_WINDOW_WIDTH,
            playlist_height.max(PLAYLIST_MIN_HEIGHT),
            self.core.store().state().playlist.len(),
        );
    }
}

#[cfg(target_os = "android")]
fn apply_android_skin_visuals(ctx: &egui::Context, skin: &crate::skin::DefaultSkin) {
    let [red, green, blue] = skin.text_colors().foreground[0];
    ctx.all_styles_mut(|style| {
        style.visuals.override_text_color = Some(egui::Color32::from_rgb(red, green, blue));
    });
}

fn show_detached_equalizer_popovers(ctx: &egui::Context, app: &mut EguiFrontendState) {
    let config = &app.controller().state().config;
    if !(config.equalizer_visible && config.equalizer_detached) {
        return;
    }
    let height = equalizer_window_height(config.equalizer_shaded) as f32 * app.scale_factor();
    let rect = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(EQUALIZER_WINDOW_WIDTH as f32 * app.scale_factor(), height),
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
        state.focus = DetachedFocus::None;
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
        state.take_handoff()
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
            app.ui.playlist_menu_hover = None;
            playlist::dispatch_playlist_menu_button(app, menu)
        }
        DetachedPanelAction::PlaylistMenuItem(menu, index) => {
            app.ui.dismiss_playlist_overlay();
            playlist::dispatch_playlist_menu_item(app, menu, index);
        }
        DetachedPanelAction::ClosePlaylistMenu => {
            app.ui.dismiss_playlist_overlay();
        }
        DetachedPanelAction::PlaylistRowClick {
            index,
            double,
            ctrl,
        } => {
            app.dispatch_all(playlist_row_click_commands(index, double, ctrl));
        }
        DetachedPanelAction::PlaylistScrollTo(offset) => {
            app.playlist.scroll_offset = offset.min(app.playlist_max_scroll_offset());
        }
        DetachedPanelAction::PlaylistScrollRows(rows) => {
            scroll_playlist_by_wheel(app, rows as f32);
        }
    }
}

fn update_detached_panel_snapshots(app: &mut EguiFrontendState) {
    let config = app.controller().state().config.clone();
    let (focus, playlist_menu_hover) = {
        let state = app
            .detached_viewports
            .lock()
            .expect("detached viewport state poisoned");
        (state.focus, state.playlist_menu_hover)
    };
    let equalizer = (config.equalizer_visible && config.equalizer_detached)
        .then(|| detached_equalizer_snapshot(app, focus == DetachedFocus::Equalizer))
        .flatten();
    let playlist = (config.playlist_visible && config.playlist_detached)
        .then(|| {
            detached_playlist_snapshot(app, focus == DetachedFocus::Playlist, playlist_menu_hover)
        })
        .flatten();
    let mut state = app
        .detached_viewports
        .lock()
        .expect("detached viewport state poisoned");
    state.replace_snapshots(equalizer, playlist);
}

fn detached_equalizer_snapshot(
    app: &EguiFrontendState,
    focused: bool,
) -> Option<DetachedPanelSnapshot> {
    let view_model = equalizer_view_model(app.controller().state());
    let (pressed_control, pressed_slider) = app.equalizer.pressed.render_parts();
    let render_state = EqualizerRenderState {
        focused,
        shaded: view_model.shaded,
        active: view_model.active,
        automatic: view_model.auto,
        pressed_control,
        pressed_slider,
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
        scale_factor: app.scale_factor(),
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
        app.playlist.scroll_offset,
        false,
        None,
        app.playlist.width,
        app.playlist.height,
    );
    let shaded_info = playlist::shaded_playlist_info(app);
    let footer_info = detached_playlist_footer_info(app);
    let (footer_min, footer_sec) = detached_playlist_footer_time_parts(app);
    let render_scale = app.scale_factor() as f64;
    let mut image = render_playlist_color_image(
        &app.active_skin,
        focused,
        view_model.shaded,
        app.playlist.width,
        app.playlist.height,
        Some(&shaded_info),
        &rows,
        Some(&footer_info),
        Some(&footer_min),
        Some(&footer_sec),
        render_scale,
    )
    .ok()?;
    let playlist_menu_open = (!view_model.shaded)
        .then_some(app.ui.active_overlay.playlist_menu())
        .flatten();
    if let Some(kind) = playlist_menu_open {
        let hover =
            menu_hover.and_then(|(hover_kind, index)| (hover_kind == kind).then_some(index));
        overlay_detached_playlist_menu(&mut image, app, kind, hover, render_scale).ok()?;
    }
    Some(DetachedPanelSnapshot {
        panel: LayoutPanelKind::Playlist,
        width: app.playlist.width,
        height: playlist_window_height(view_model.shaded, app.playlist.height),
        scale_factor: app.scale_factor(),
        playlist_menu_open,
        playlist_scroll_offset: app.playlist.scroll_offset,
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
    let popup = playlist_menu_popup_rect(kind, app.playlist.width, app.playlist.height);
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
    let before = state.focus;
    let panel = if equalizer_panel {
        DetachedFocus::Equalizer
    } else {
        DetachedFocus::Playlist
    };
    if focused {
        state.focus = panel;
    } else if state.focus == panel {
        state.focus = DetachedFocus::None;
    }
    before != state.focus
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
    let resize_requested = {
        let mut state = shared.lock().expect("detached viewport state poisoned");
        let requested_size = if equalizer_panel {
            &mut state.equalizer_requested_size
        } else {
            &mut state.playlist_requested_size
        };
        if *requested_size == Some(size) {
            false
        } else {
            *requested_size = Some(size);
            true
        }
    };
    let builder = detached_panel_viewport_builder(title, &snapshot, equalizer_panel);
    ctx.show_viewport_deferred(
        egui::ViewportId::from_hash_of(id),
        builder,
        move |ctx, class| {
            if resize_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            }
            if ctx.input(|input| input.viewport().close_requested()) {
                push_detached_command(
                    &shared,
                    if equalizer_panel {
                        PanelCommand::SetEqualizerVisibility(false)
                    } else {
                        PanelCommand::SetPlaylistVisibility(false)
                    },
                );
                ctx.request_repaint_of(egui::ViewportId::ROOT);
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
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
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
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
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
            ui.ctx().request_repaint_of(egui::ViewportId::ROOT);
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
    #[cfg(target_os = "android")]
    if app.android.playlist_manager.is_open() {
        return true;
    }
    ctx.egui_wants_keyboard_input()
        || app.main_menu_open()
        || app.ui.prompt_open.is_some()
        || app.preferences_open()
        || app.skin_browser_open()
        || app.file_info_open()
        || app.ui.active_overlay.is_open()
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
            app.open_file_dialog(FileDialogRequest::AddAudioFiles);
        }
        AppShortcut::OpenLocation => {
            app.ui.prompt_open = Some(EguiPrompt::OpenLocation);
            app.ui.prompt_text.clear();
        }
        AppShortcut::JumpTime => {
            app.ui.prompt_open = Some(EguiPrompt::JumpToTime);
            app.ui.prompt_text.clear();
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
        app.equalizer.keyboard_slider = Some(next_equalizer_keyboard_slider(
            app.equalizer.keyboard_slider,
        ));
    }
    let Some(slider) = app.equalizer.keyboard_slider else {
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
        * app.scale_factor();
    if config.equalizer_visible && !config.equalizer_detached {
        top += if config.equalizer_shaded {
            crate::render::MAIN_TITLEBAR_HEIGHT
        } else {
            EQUALIZER_WINDOW_HEIGHT
        } as f32
            * app.scale_factor();
    }
    let height = playlist_window_height(config.playlist_shaded, app.playlist.height) as f32
        * app.scale_factor();
    egui::Rect::from_min_size(
        egui::pos2(0.0, top),
        egui::vec2(app.playlist.width as f32 * app.scale_factor(), height),
    )
    .contains(pos)
}

fn scroll_playlist_by_wheel(app: &mut EguiFrontendState, scroll_y: f32) {
    if scroll_y > 0.0 {
        app.playlist.scroll_offset = app.playlist.scroll_offset.saturating_sub(3);
    } else {
        app.playlist.scroll_offset =
            (app.playlist.scroll_offset + 3).min(app.playlist_max_scroll_offset());
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
        app.playlist.scroll_offset = app.playlist.scroll_offset.min(position);
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

#[cfg(target_os = "android")]
pub(crate) fn run_egui_frontend_android(
    options: PreviewOptions,
    android_app: winit::platform::android::activity::AndroidApp,
    activity_generation: super::android::AndroidActivityGeneration,
) -> Result<(), String> {
    let mut app = EguiFrontendState::new(options)?;
    app.android.bind_activity(activity_generation);
    eframe::run_native(
        "XMMS Renascene",
        eframe::NativeOptions {
            android_app: Some(android_app),
            renderer: eframe::Renderer::Glow,
            ..eframe::NativeOptions::default()
        },
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|err| format!("failed to start Android egui frontend: {err}"))
}

#[cfg(not(target_os = "android"))]
fn show_skin_browser_placeholder(ctx: &egui::Context, app: &mut EguiFrontendState) {
    app.ensure_runtime_skins();
    let mut open = app.skin_browser_open();
    egui::Window::new("Skin selector")
        .open(&mut open)
        .resizable(true)
        .default_width(360.0)
        .show(ctx, |ui| {
            if ui.button("Refresh").clicked() {
                app.refresh_runtime_skins();
            }
            ui.horizontal(|ui| {
                if ui.button("Default").clicked() {
                    #[cfg(target_os = "android")]
                    app.select_default_skin();
                    #[cfg(not(target_os = "android"))]
                    match DefaultSkin::load_bundled() {
                        Ok(skin) => {
                            app.replace_active_skin(skin);
                            let mut config = app.controller().state().persistence_snapshot().config;
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
                    for entry in app.skins.entries.clone() {
                        let selected = app.controller().state().config.skin.as_deref()
                            == Some(entry.path.to_string_lossy().as_ref());
                        if ui.selectable_label(selected, &entry.name).clicked() {
                            select_skin_entry(app, &entry);
                        }
                    }
                    if app.skins.entries.is_empty() {
                        ui.label("No skins found in configured skin directories.");
                    }
                });
        });
    if !open && app.skin_browser_open() {
        app.dispatch(UiCommand::SetSkinBrowserVisible(false));
    }
}

fn select_skin_entry(app: &mut EguiFrontendState, entry: &SkinEntry) {
    match DefaultSkin::load_from_path(&entry.path) {
        Ok(skin) => {
            app.replace_active_skin(skin);
            let mut config = app.controller().state().persistence_snapshot().config;
            config.skin = Some(entry.path.to_string_lossy().into_owned());
            app.apply_preferences_config(config);
        }
        Err(err) => app.runtime.pending_messages.push(format!(
            "failed to load skin '{}': {err}",
            entry.path.display()
        )),
    }
}

#[cfg(feature = "desktop-egui")]
fn import_skin_from_dialog(app: &mut EguiFrontendState) {
    let Some(path) = rfd::FileDialog::new().set_title("Add skin").pick_file() else {
        return;
    };
    match import_skin_to_user_dir(&path) {
        Ok(entry) => {
            app.refresh_runtime_skins();
            select_skin_entry(app, &entry);
        }
        Err(err) => app
            .runtime
            .pending_messages
            .push(format!("failed to import skin '{}': {err}", path.display())),
    }
}

#[cfg(target_os = "android")]
fn import_skin_from_dialog(app: &mut EguiFrontendState) {
    app.open_file_dialog(FileDialogRequest::ImportSkin);
}

#[cfg(all(not(feature = "desktop-egui"), not(target_os = "android")))]
fn import_skin_from_dialog(app: &mut EguiFrontendState) {
    app.runtime
        .pending_messages
        .push("skin importing is not available on this platform".to_string());
}

#[cfg(target_os = "android")]
fn import_skin_path(app: &mut EguiFrontendState, path: &Path) {
    match import_skin_to_user_dir(path) {
        Ok(entry) => {
            app.refresh_runtime_skins();
            select_skin_entry(app, &entry);
        }
        Err(err) => app
            .runtime
            .pending_messages
            .push(format!("failed to import skin '{}': {err}", path.display())),
    }
}

#[cfg(target_os = "android")]
pub(crate) fn is_user_imported_skin_path(path: &Path) -> bool {
    path.parent() == Some(user_skin_import_dir().as_path())
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
    use crate::playback::model::EqualizerBackendState;
    use crate::player::PlaybackEvent;

    #[test]
    fn android_media_controls_translate_to_store_commands() {
        use super::super::android_events::AndroidMediaControl;

        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::PausePlayback),
            Some(PlayerCommand::Pause)
        );
        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::ResumePlayback),
            Some(PlayerCommand::Play)
        );
        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::NextTrack),
            Some(PlayerCommand::NextTrack)
        );
        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::PreviousTrack),
            Some(PlayerCommand::PreviousTrack)
        );
        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::SeekToMs(4_200)),
            Some(PlayerCommand::SeekToMs(4_200))
        );
        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::HaltPlayback),
            Some(PlayerCommand::Halt)
        );
        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::PlayMediaItem(2)),
            None
        );
        assert_eq!(
            android_player_command_for_media_control(AndroidMediaControl::PlaylistEof),
            None
        );
    }

    #[test]
    fn service_executed_media_controls_disable_local_playback_execution() {
        assert_eq!(
            EffectExecution::after_external_backend_execution(true),
            EffectExecution::AlreadyExecuted
        );
        assert_eq!(
            EffectExecution::after_external_backend_execution(false),
            EffectExecution::LOCAL
        );
    }

    #[derive(Clone)]
    struct RecordingVolumeBackend {
        volumes: Arc<Mutex<Vec<i32>>>,
        starts: Option<Arc<Mutex<Vec<String>>>>,
    }

    impl PlaybackBackend for RecordingVolumeBackend {
        fn play_uri(&self, uri: &str) -> Result<(), String> {
            if let Some(starts) = &self.starts {
                starts.lock().unwrap().push(uri.to_string());
            }
            Ok(())
        }

        fn pause(&self) -> Result<(), String> {
            Ok(())
        }

        fn unpause(&self) -> Result<(), String> {
            Ok(())
        }

        fn stop(&self) -> Result<(), String> {
            Ok(())
        }

        fn seek(&self, _position_ms: i64) -> Result<(), String> {
            Ok(())
        }

        fn set_volume(&self, volume: i32) -> Result<(), String> {
            self.volumes
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .push(volume);
            Ok(())
        }

        fn set_balance(&self, _balance: i32) -> Result<(), String> {
            Ok(())
        }

        fn set_equalizer(&self, _state: EqualizerBackendState) -> Result<(), String> {
            Ok(())
        }
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn desktop_egui_applies_output_volume_to_playback_backend() {
        let volumes = Arc::new(Mutex::new(Vec::new()));
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.core
            .install_backend_with_dsp(Box::new(RecordingVolumeBackend {
                volumes: Arc::clone(&volumes),
                starts: None,
            }))
            .unwrap();

        app.dispatch(crate::app::command::AudioCommand::SetVolume(37));

        assert_eq!(
            *volumes.lock().unwrap_or_else(|poison| poison.into_inner()),
            vec![37]
        );
    }

    #[test]
    fn egui_command_prepares_and_starts_track_once() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let starts = Arc::new(Mutex::new(Vec::new()));
        app.core
            .install_backend_with_dsp(Box::new(RecordingVolumeBackend {
                volumes: Arc::new(Mutex::new(Vec::new())),
                starts: Some(Arc::clone(&starts)),
            }))
            .unwrap();
        app.dispatch(PlaylistCommand::AddUris(vec!["file:///song.ogg".into()]));
        app.dispatch(PlayerCommand::StartCurrentTrack);
        assert_eq!(*starts.lock().unwrap(), vec!["file:///song.ogg"]);
    }

    #[test]
    fn egui_local_eof_uses_the_gtk_lifecycle_contract() {
        use crate::app::playback_transition::PlaybackTransition;

        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let starts = Arc::new(Mutex::new(Vec::new()));
        app.core
            .install_backend_with_dsp(Box::new(RecordingVolumeBackend {
                volumes: Arc::new(Mutex::new(Vec::new())),
                starts: Some(Arc::clone(&starts)),
            }))
            .unwrap();
        let mut config = app.core.state().config.clone();
        config.pause_between_songs = true;
        config.pause_between_songs_time = 2;
        app.process_store_event(RuntimeEvent::Preferences(config), EffectExecution::LOCAL);
        app.dispatch(PlaylistCommand::AddUris(vec!["one".into(), "two".into()]));
        app.dispatch(PlayerCommand::StartCurrentTrack);
        app.handle_playback_events([PlaybackEvent::EndOfStream]);
        assert_eq!(
            app.core.transition(),
            PlaybackTransition::WaitingBetweenSongs(2_000)
        );
        assert_eq!(app.core.state().config.playback_position_ms, 0);
        assert_eq!(app.core.state().playlist.position(), Some(0));
        app.process_store_event(RuntimeEvent::TransitionTick(1_000), EffectExecution::LOCAL);
        assert_eq!(
            app.core.transition(),
            PlaybackTransition::WaitingBetweenSongs(1_000)
        );
        assert_eq!(*starts.lock().unwrap(), vec!["one"]);
        app.process_store_event(RuntimeEvent::TransitionTick(1_000), EffectExecution::LOCAL);
        assert_eq!(app.core.transition(), PlaybackTransition::Idle);
        assert_eq!(app.core.state().playlist.position(), Some(1));
        assert_eq!(*starts.lock().unwrap(), vec!["one", "two"]);
    }

    #[test]
    fn egui_seek_confirmation_recovers_when_first_sample_is_late() {
        use crate::app::playback_transition::PlaybackTransition;

        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.core
            .install_backend_with_dsp(Box::new(RecordingVolumeBackend {
                volumes: Arc::new(Mutex::new(Vec::new())),
                starts: None,
            }))
            .unwrap();
        app.dispatch(PlaylistCommand::AddUris(vec!["song".into()]));
        app.dispatch(PlayerCommand::StartCurrentTrack);
        app.dispatch(PlayerCommand::SeekToMs(5_000));
        app.process_store_event(RuntimeEvent::BackendPosition(100), EffectExecution::LOCAL);
        assert_eq!(app.core.state().config.playback_position_ms, 5_000);
        // The test backend has no pre-seek position observation. Do not trust
        // an out-of-window sample until the bounded confirmation interval.
        app.process_store_event(RuntimeEvent::BackendPosition(5_300), EffectExecution::LOCAL);
        assert!(matches!(
            app.core.transition(),
            PlaybackTransition::AwaitingSeek { .. }
        ));
        app.process_store_event(RuntimeEvent::TransitionTick(1_000), EffectExecution::LOCAL);
        app.process_store_event(RuntimeEvent::BackendPosition(5_300), EffectExecution::LOCAL);
        assert_eq!(app.core.transition(), PlaybackTransition::Idle);
        assert_eq!(app.core.state().config.playback_position_ms, 5_300);
    }

    #[test]
    fn egui_play_during_eof_wait_cancels_advance_and_starts_at_zero() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let mut config = app.core.state().config.clone();
        config.pause_between_songs = true;
        config.pause_between_songs_time = 2;
        app.process_store_event(RuntimeEvent::Preferences(config), EffectExecution::LOCAL);
        app.dispatch(PlaylistCommand::AddUris(vec!["one".into(), "two".into()]));
        app.dispatch(PlayerCommand::StartCurrentTrack);
        app.handle_playback_events([PlaybackEvent::EndOfStream]);
        app.dispatch(PlayerCommand::Play);
        assert_eq!(app.core.transition().wait_remaining(), None);
        assert_eq!(app.core.state().config.playback_position_ms, 0);
        app.process_store_event(RuntimeEvent::TransitionTick(2_000), EffectExecution::LOCAL);
        assert_eq!(app.core.state().playlist.position(), Some(0));
    }

    #[test]
    fn egui_app_constructs_without_native_window() {
        let options = PreviewOptions {
            open_preferences: true,
            ..PreviewOptions::default()
        };

        let app = EguiFrontendState::new(options).unwrap();

        assert!(app.preferences_open());
        assert_eq!(
            app.ui.selected_preferences_page,
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
        assert!(app.desired_window_size().y > EQUALIZER_WINDOW_HEIGHT as f32 * app.scale_factor());
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
        assert_eq!((app.playlist.width, app.playlist.height), (325, 290));
        assert_eq!(app.desired_window_size().x, 325.0 * app.scale_factor());
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn root_viewport_resize_is_requested_only_when_desired_size_changes() {
        let mut app = EguiFrontendState::new(PreviewOptions {
            playlist_size: Some((325, 290)),
            ..PreviewOptions::default()
        })
        .unwrap();

        assert_eq!(
            app.take_root_viewport_resize(),
            Some(app.desired_window_size())
        );
        assert_eq!(app.take_root_viewport_resize(), None);
        assert!(app.set_playlist_size(350, 290));
        let resized = app.desired_window_size();
        assert_eq!(app.take_root_viewport_resize(), Some(resized));
        assert_eq!(app.take_root_viewport_resize(), None);
    }

    #[test]
    fn main_interaction_marquee_transition_advances_clock() {
        let mut main = MainInteraction::default();
        let now = main.last_marquee_tick + Duration::from_millis(84);
        assert!(main.update_marquee(
            "A sufficiently long track title for scrolling",
            PlayerState::Playing,
            false,
            now,
        ));
        assert_eq!(main.last_marquee_tick, now);
        assert!(!main.update_marquee("Short", PlayerState::Stopped, true, now));
        assert_eq!(main.title_marquee.offset_px(), 0);
    }

    #[test]
    fn playlist_resize_clamps_scroll_without_losing_gesture_state() {
        let mut playlist = PlaylistPresentation::new(275, 116);
        playlist.scroll_offset = 18;
        playlist.resize_start = Some(7);
        assert!(playlist.resize(300, 168, 20));
        assert_eq!(playlist.scroll_offset, 10);
        assert_eq!(playlist.resize_start, Some(7));
        assert!(!playlist.resize(300, 168, 20));
        assert_eq!(playlist.scroll_offset, 10);
    }

    #[test]
    fn root_timing_advances_playback_clock_without_duplicate_resize() {
        let mut root = RootViewportTiming::new(Arc::new(Mutex::new(None)));
        let start = root.last_playback_tick;
        let later = start + Duration::from_millis(150);
        assert_eq!(root.playback_elapsed(later), Duration::from_millis(150));
        assert_eq!(root.playback_elapsed(later), Duration::ZERO);
        #[cfg(not(target_os = "android"))]
        {
            let size = egui::vec2(275.0, 116.0);
            assert_eq!(root.request_size_if_changed(size), Some(size));
            assert_eq!(root.request_size_if_changed(size), None);
        }
    }

    #[test]
    fn detached_handoff_drains_actions_and_resize_together() {
        let mut state = DetachedViewportState::default();
        state.actions.push(DetachedPanelAction::ClosePlaylistMenu);
        state.playlist_resize_request = Some((300, 200));
        state.equalizer_requested_size = Some(egui::vec2(275.0, 116.0));
        state.playlist_requested_size = Some(egui::vec2(300.0, 200.0));
        let (actions, resize) = state.take_handoff();
        assert!(matches!(
            actions.as_slice(),
            [DetachedPanelAction::ClosePlaylistMenu]
        ));
        assert_eq!(resize, Some((300, 200)));
        assert!(state.take_handoff().0.is_empty());
        assert!(state.take_handoff().1.is_none());
        state.replace_snapshots(None, None);
        assert!(state.equalizer_requested_size.is_none());
        assert!(state.playlist_requested_size.is_none());
    }

    #[test]
    fn discovered_empty_skin_catalog_does_not_repeat_discovery() {
        let mut catalog = SkinCatalog {
            entries: Vec::new(),
            discovered: true,
        };
        catalog.ensure_discovered();
        assert!(catalog.discovered);
        assert!(catalog.entries.is_empty());
    }

    #[test]
    fn android_layout_rejects_transient_rotation_extents() {
        assert!(!android_layout_extent_is_stable(1.0, 400.0));
        assert!(!android_layout_extent_is_stable(400.0, 1.0));
        assert!(!android_layout_extent_is_stable(f32::NAN, 400.0));
        assert!(android_layout_extent_is_stable(400.0, 800.0));
        assert!(android_layout_extent_is_stable(800.0, 400.0));
        assert_eq!(
            AndroidLayoutOrientation::from_configuration(1),
            Some(AndroidLayoutOrientation::Portrait)
        );
        assert_eq!(
            AndroidLayoutOrientation::from_configuration(2),
            Some(AndroidLayoutOrientation::Landscape)
        );
        assert_eq!(AndroidLayoutOrientation::from_configuration(0), None);
    }

    #[test]
    fn android_layout_rejects_stale_insets_and_preserves_safe_width() {
        let landscape = AndroidLayoutOrientation::Landscape;
        assert!(android_layout_snapshot_is_consistent(
            2400,
            1080,
            landscape,
            [96, 0, 72, 48],
            2400,
            1080,
            2,
            3,
            3,
            true,
        ));
        assert!(!android_layout_snapshot_is_consistent(
            2400,
            1080,
            landscape,
            [0, 96, 0, 72],
            1080,
            2400,
            1,
            3,
            2,
            false,
        ));
        assert!(!android_layout_snapshot_is_consistent(
            2400,
            1080,
            AndroidLayoutOrientation::Portrait,
            [96, 0, 72, 48],
            2400,
            1080,
            2,
            3,
            3,
            true,
        ));
        assert!(!android_layout_snapshot_is_consistent(
            2400,
            1080,
            landscape,
            [96, 0, 72, 48],
            2400,
            1080,
            2,
            4,
            3,
            false,
        ));

        let [left, _top, right, _bottom] = [96, 0, 72, 48];
        let usable_width = 2400 - left - right;
        assert_eq!(left + usable_width + right, 2400);
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
        assert_eq!(app.playlist.width, 325);
        let detached_height = app.playlist.height;

        let mut config = app.controller().state().persistence_snapshot().config;
        config.playlist_detached = false;
        app.apply_preferences_config(config);

        assert!(!app.controller().state().config.playlist_detached);
        // Width snaps back to the player width; height is preserved (GTK parity).
        assert_eq!(app.playlist.width, crate::render::PLAYLIST_MIN_WIDTH);
        assert_eq!(app.playlist.height, detached_height);
        assert_eq!(
            app.desired_window_size().x,
            crate::render::MAIN_WINDOW_WIDTH as f32 * app.scale_factor()
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
            assert_eq!(state.focus, DetachedFocus::None);
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
            assert_eq!(state.focus, DetachedFocus::Equalizer);
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
            assert_eq!(state.focus, DetachedFocus::Playlist);
            assert_ne!(playlist.image, inactive_playlist);
        }

        assert!(update_detached_panel_viewport_focus(
            &app.detached_viewports,
            false,
            false
        ));
        let state = app
            .detached_viewports
            .lock()
            .expect("detached viewport state poisoned");
        assert_eq!(state.focus, DetachedFocus::None);
    }

    #[test]
    fn detached_focus_transfers_and_clears_under_one_viewport_lock() {
        let shared = Arc::new(Mutex::new(DetachedViewportState::default()));

        assert!(update_detached_panel_viewport_focus(&shared, true, true));
        assert_eq!(
            shared
                .lock()
                .expect("detached viewport state poisoned")
                .focus,
            DetachedFocus::Equalizer
        );

        assert!(update_detached_panel_viewport_focus(&shared, false, true));
        assert_eq!(
            shared
                .lock()
                .expect("detached viewport state poisoned")
                .focus,
            DetachedFocus::Playlist
        );

        assert!(update_detached_panel_viewport_focus(&shared, false, false));
        assert_eq!(
            shared
                .lock()
                .expect("detached viewport state poisoned")
                .focus,
            DetachedFocus::None
        );
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

        app.ui.active_overlay = ActiveOverlay::PlaylistMenu(PlaylistMenuButton::Add);
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
        assert!((app.scale_factor() - 1.25).abs() < f32::EPSILON);
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
        assert!((app.scale_factor() - 1.0).abs() < f32::EPSILON);
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

        app.core.enqueue_duration_batch(vec![DurationIndexResult {
            index: 0,
            uri: "file:///tmp/song.ogg".to_string(),
            length_ms: 42_000,
            title: None,
        }]);

        assert!(app.poll_duration_index_results());
        assert_eq!(
            shared_playlist_footer_info(app.controller().state()),
            "0:00/0:42"
        );
    }

    #[cfg(feature = "rodio-backend")]
    fn assert_egui_rodio_indexes_m3u_entry(entry: impl FnOnce(&Path) -> String) {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "xmms-egui-duration-m3u-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let wav = root.join("song.wav");
        std::fs::write(
            &wav,
            crate::playback::rodio::test_wav_bytes(8_000, 1, 8_000),
        )
        .unwrap();
        let m3u = root.join("playlist.m3u");
        let location = entry(&wav);
        std::fs::write(&m3u, format!("#EXTM3U\n{location}\n")).unwrap();

        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        assert!(app.load_playlist_file(&m3u));
        assert_eq!(
            app.controller().state().playlist.entries()[0].filename,
            location
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.controller().state().playlist.entries()[0].length_ms < 0
            && Instant::now() < deadline
        {
            app.poll_duration_index_results();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            app.controller().state().playlist.entries()[0].length_ms,
            1_000
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "rodio-backend")]
    #[test]
    fn egui_rodio_indexes_plain_m3u_local_path() {
        assert_egui_rodio_indexes_m3u_entry(|path| path.display().to_string());
    }

    #[cfg(feature = "rodio-backend")]
    #[test]
    fn egui_rodio_indexes_localhost_file_uri_from_m3u() {
        assert_egui_rodio_indexes_m3u_entry(|path| format!("file://localhost{}", path.display()));
    }

    #[test]
    fn egui_startup_defers_skin_directory_discovery() {
        let app = EguiFrontendState::new(PreviewOptions {
            reset: true,
            ..PreviewOptions::default()
        })
        .unwrap();

        assert!(app.skins.entries.is_empty());
        assert!(!app.skins.discovered);
        assert!(app
            .active_skin
            .get(crate::skin::SkinPixmapKind::Main)
            .is_some());
    }

    #[test]
    fn egui_visualizer_render_state_uses_spectrum_events() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        let mut spectrum = [0.0; SPECTRUM_BANDS];
        spectrum[9] = 0.9;

        app.controller_mut().state_mut().player.mark_playing();
        app.process_store_event(
            RuntimeEvent::Playback(PlaybackEvent::Spectrum(spectrum)),
            EffectExecution::LOCAL,
        );
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
        app.process_store_event(
            RuntimeEvent::Playback(PlaybackEvent::Spectrum(spectrum)),
            EffectExecution::LOCAL,
        );
        assert!(app.tick_visualization());
        assert!(app.visualization_render_state().data[4] > 0.0);

        app.dispatch(PlayerCommand::Halt);

        assert_eq!(app.visualization_render_state().data[4], 0.0);
        assert_eq!(app.visualization_render_state().peak[4], 0.0);
    }

    #[test]
    fn egui_dispatch_mutates_config_through_controller() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();

        app.dispatch(PanelCommand::SetPlaylistVisibility(true));

        assert!(app.controller().state().config.playlist_visible);
        assert!(app.runtime.repaint_requested());
    }

    #[test]
    fn egui_flushes_repaint_policy() {
        let ctx = egui::Context::default();
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.runtime
            .apply_effect(UiEffect::QueueRender(crate::app::effect::RenderTarget::All));

        app.flush_repaint_policy(&ctx);

        assert!(!app.runtime.repaint_requested());
        assert!(app.runtime.dirty_targets().is_empty());
    }

    #[test]
    fn egui_playback_repaint_rate_adapts_to_visible_work() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        assert_eq!(app.playback_repaint_interval(), None);

        app.dispatch(PlaylistCommand::AddUris(vec![
            "file:///music/repaint.ogg".to_string()
        ]));
        app.dispatch(PlaylistCommand::SetPosition(0));
        app.dispatch(PlayerCommand::StartCurrentTrack);
        app.controller_mut().state_mut().config.vis_refresh_divisor = 4;
        assert_eq!(
            app.playback_repaint_interval(),
            Some(Duration::from_millis(80))
        );

        app.controller_mut().state_mut().config.vis_mode = VisMode::Off;
        app.apply_visualization_preferences();
        assert_eq!(
            app.playback_repaint_interval(),
            Some(POSITION_REPAINT_INTERVAL)
        );
    }

    #[test]
    fn egui_global_shortcuts_pause_while_dialogs_or_menus_are_active() {
        let ctx = egui::Context::default();
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();

        assert!(!global_shortcuts_suspended(&ctx, &app));

        app.ui.prompt_open = Some(EguiPrompt::OpenLocation);
        assert!(global_shortcuts_suspended(&ctx, &app));
        app.ui.prompt_open = None;

        for overlay in [
            ActiveOverlay::PlaylistMenu(PlaylistMenuRenderKind::Misc),
            ActiveOverlay::PlaylistSort,
            ActiveOverlay::EqualizerPresets,
            ActiveOverlay::ConfirmPhysicalDelete,
        ] {
            app.ui.active_overlay = overlay;
            assert!(global_shortcuts_suspended(&ctx, &app));
        }
        app.ui.active_overlay = ActiveOverlay::None;

        app.runtime.pending_messages.push("message".to_string());
        assert!(global_shortcuts_suspended(&ctx, &app));
    }

    #[test]
    fn closing_player_menus_dismisses_playlist_overlay_and_hover() {
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.ui.active_overlay = ActiveOverlay::PlaylistMenu(PlaylistMenuRenderKind::Select);
        app.ui.playlist_menu_hover = Some((PlaylistMenuRenderKind::Select, 1));

        app.close_player_menus();

        assert_eq!(app.ui.active_overlay, ActiveOverlay::None);
        assert_eq!(app.ui.playlist_menu_hover, None);
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
        assert_eq!(app.playlist.scroll_offset, 3);
        scroll_playlist_by_wheel(&mut app, 1.0);
        assert_eq!(app.playlist.scroll_offset, 0);
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
    fn egui_mpris_blank_open_uri_preserves_playlist_and_emits_nothing() {
        let ctx = egui::Context::default();
        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.dispatch(PlaylistCommand::AddUris(vec![
            "file:///music/old.ogg".into()
        ]));

        let events = app.handle_mpris_command(&ctx, MprisCommand::OpenUri(" \t ".into()));

        assert!(events.is_empty());
        assert_eq!(app.controller().state().playlist.len(), 1);
        assert_eq!(
            app.controller().state().playlist.entries()[0].filename,
            "file:///music/old.ogg"
        );
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
        assert!(app.runtime.repaint_requested());

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
        let path = root.join("custom.eqf");

        let mut app = EguiFrontendState::new(PreviewOptions::default()).unwrap();
        app.controller_mut().state_mut().config.equalizer_preamp_pos = 25;
        app.controller_mut().state_mut().config.equalizer_band_pos[0] = 75;
        app.save_equalizer_preset_file(&path);

        app.controller_mut().state_mut().config.equalizer_preamp_pos = 50;
        app.controller_mut().state_mut().config.equalizer_band_pos[0] = 50;
        app.load_equalizer_preset_file(&path);

        assert_eq!(app.controller().state().config.equalizer_preamp_pos, 25);
        assert_eq!(app.controller().state().config.equalizer_band_pos[0], 73);
        assert!(app.runtime.repaint_requested());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn playlist_touch_gesture_locks_axis_and_resets_on_release() {
        let mut gesture = PlaylistTouchGesture::default();
        gesture.begin(egui::pos2(10.0, 20.0), Some(4));
        gesture.observe(egui::vec2(2.0, 30.0));

        assert_eq!(gesture.drag(egui::vec2(2.0, 30.0), 10.0), -3);

        let release = gesture.release(Some(egui::pos2(12.0, 50.0))).unwrap();
        assert_eq!(release.row, Some(4));
        assert_eq!(release.delta, egui::vec2(2.0, 30.0));
        assert!(!gesture.is_active());
    }
}
