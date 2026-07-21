//! Reactive, frontend-neutral application store.
//!
//! `AppStore` is the single canonical mutation point for application behavior.
//! Frontends translate native input into [`AppCommand`] / runtime events, call
//! `dispatch`, then update their widgets/windows from the returned change set
//! and immutable state/view-model reads.

use crate::app::command::AppCommand;
use crate::app::controller::AppController;
use crate::app::effect::{AppEffect, RenderTarget};
use crate::app::logging::ConsoleLogLevel;
use crate::app_log;
use crate::app_state::{AppState, RuntimeSnapshot};
use crate::config::Config;
use crate::player::PlaybackEvent;
use crate::playlist::DurationIndexResult;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StateChangeSet(u64);

impl StateChangeSet {
    pub const NONE: Self = Self(0);
    pub const PLAYER: Self = Self(1 << 0);
    pub const PLAYLIST: Self = Self(1 << 1);
    pub const EQUALIZER: Self = Self(1 << 2);
    pub const PANELS: Self = Self(1 << 3);
    pub const DIALOGS: Self = Self(1 << 4);
    pub const SKIN: Self = Self(1 << 5);
    pub const PREFERENCES: Self = Self(1 << 6);
    pub const CONFIG: Self = Self(1 << 7);
    pub const RENDER_MAIN: Self = Self(1 << 8);
    pub const RENDER_PLAYLIST: Self = Self(1 << 9);
    pub const RENDER_EQUALIZER: Self = Self(1 << 10);
    pub const RENDER_ALL: Self =
        Self(Self::RENDER_MAIN.0 | Self::RENDER_PLAYLIST.0 | Self::RENDER_EQUALIZER.0);

    pub fn empty() -> Self {
        Self::NONE
    }

    pub fn all() -> Self {
        Self(u64::MAX)
    }

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn known_bits() -> u64 {
        Self::PLAYER.0
            | Self::PLAYLIST.0
            | Self::EQUALIZER.0
            | Self::PANELS.0
            | Self::DIALOGS.0
            | Self::SKIN.0
            | Self::PREFERENCES.0
            | Self::CONFIG.0
            | Self::RENDER_MAIN.0
            | Self::RENDER_PLAYLIST.0
            | Self::RENDER_EQUALIZER.0
    }
}

impl fmt::Display for StateChangeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("none");
        }

        let flags = [
            (Self::PLAYER, "player"),
            (Self::PLAYLIST, "playlist"),
            (Self::EQUALIZER, "equalizer"),
            (Self::PANELS, "panels"),
            (Self::DIALOGS, "dialogs"),
            (Self::SKIN, "skin"),
            (Self::PREFERENCES, "preferences"),
            (Self::CONFIG, "config"),
            (Self::RENDER_MAIN, "render-main"),
            (Self::RENDER_PLAYLIST, "render-playlist"),
            (Self::RENDER_EQUALIZER, "render-equalizer"),
        ];
        let mut separator = "";
        for (flag, label) in flags {
            if self.intersects(flag) {
                write!(f, "{separator}{label}")?;
                separator = "|";
            }
        }

        let unknown_bits = self.0 & !Self::known_bits();
        if unknown_bits != 0 {
            write!(f, "{separator}unknown({unknown_bits:#x})")?;
        }
        Ok(())
    }
}

impl std::ops::BitOr for StateChangeSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for StateChangeSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DispatchResult {
    pub revision: u64,
    pub changes: StateChangeSet,
    pub effects: Vec<AppEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConsoleEventLog {
    event: String,
    revision: u64,
    changes: StateChangeSet,
    effects: String,
}

impl ConsoleEventLog {
    fn new(event: impl Into<String>, result: &DispatchResult) -> Self {
        Self {
            event: event.into(),
            revision: result.revision,
            changes: result.changes,
            effects: format!("{:?}", result.effects),
        }
    }
}

impl fmt::Display for ConsoleEventLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "event: {}; revision={}; changes={}; effects={}",
            self.event, self.revision, self.changes, self.effects
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
struct StoreSnapshot {
    config: Config,
    runtime: RuntimeSnapshot,
    equalizer_active: bool,
    equalizer_auto: bool,
    equalizer_preamp_pos: i32,
    equalizer_band_pos: Vec<i32>,
    main_shaded: bool,
    playlist_shaded: bool,
    equalizer_shaded: bool,
    preferences_visible: bool,
    main_menu_visible: bool,
    skin_browser_visible: bool,
    file_info_visible: bool,
    skin: Option<String>,
    volume: i32,
    balance: i32,
    shuffle: bool,
    repeat: bool,
    no_advance: bool,
    playlist_queue: Vec<usize>,
}

impl StoreSnapshot {
    fn from_state(state: &AppState) -> Self {
        Self {
            config: state.config.clone(),
            runtime: state.snapshot(),
            equalizer_active: state.config.equalizer_active,
            equalizer_auto: state.config.equalizer_auto,
            equalizer_preamp_pos: state.config.equalizer_preamp_pos,
            equalizer_band_pos: state.config.equalizer_band_pos.to_vec(),
            main_shaded: state.config.main_shaded,
            playlist_shaded: state.config.playlist_shaded,
            equalizer_shaded: state.config.equalizer_shaded,
            preferences_visible: state.ui.preferences_visible,
            main_menu_visible: state.ui.main_menu_visible,
            skin_browser_visible: state.ui.skin_browser_visible,
            file_info_visible: state.ui.file_info_visible,
            skin: state.config.skin.clone(),
            volume: state.player.volume(),
            balance: state.player.balance(),
            shuffle: state.playlist.shuffle(),
            repeat: state.playlist.repeat(),
            no_advance: state.playlist.no_advance(),
            playlist_queue: state.playlist.queued_indices(),
        }
    }

    fn diff(&self, next: &Self) -> StateChangeSet {
        let mut changes = StateChangeSet::empty();
        if self.runtime.player_state != next.runtime.player_state
            || self.volume != next.volume
            || self.balance != next.balance
            || self.runtime.playlist_position != next.runtime.playlist_position
        {
            changes |= StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN;
        }
        if self.runtime.playlist_len != next.runtime.playlist_len
            || self.runtime.playlist_position != next.runtime.playlist_position
            || self.shuffle != next.shuffle
            || self.repeat != next.repeat
            || self.no_advance != next.no_advance
            || self.playlist_queue != next.playlist_queue
        {
            changes |= StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST;
        }
        if self.equalizer_active != next.equalizer_active
            || self.equalizer_auto != next.equalizer_auto
            || self.equalizer_preamp_pos != next.equalizer_preamp_pos
            || self.equalizer_band_pos != next.equalizer_band_pos
        {
            changes |= StateChangeSet::EQUALIZER | StateChangeSet::RENDER_EQUALIZER;
        }
        if self.main_shaded != next.main_shaded
            || self.runtime.playlist_visible != next.runtime.playlist_visible
            || self.runtime.playlist_detached != next.runtime.playlist_detached
            || self.playlist_shaded != next.playlist_shaded
            || self.runtime.equalizer_visible != next.runtime.equalizer_visible
            || self.runtime.equalizer_detached != next.runtime.equalizer_detached
            || self.equalizer_shaded != next.equalizer_shaded
        {
            changes |= StateChangeSet::PANELS | StateChangeSet::RENDER_ALL;
        }
        if self.preferences_visible != next.preferences_visible
            || self.main_menu_visible != next.main_menu_visible
            || self.skin_browser_visible != next.skin_browser_visible
            || self.file_info_visible != next.file_info_visible
        {
            changes |= StateChangeSet::DIALOGS;
        }
        if self.skin != next.skin {
            changes |= StateChangeSet::SKIN | StateChangeSet::RENDER_ALL;
        }
        if self.config != next.config {
            changes |= StateChangeSet::CONFIG;
        }
        changes
    }
}

#[derive(Debug, Clone)]
pub struct AppStore {
    controller: AppController,
    revision: u64,
}

impl AppStore {
    pub fn new(state: AppState) -> Self {
        Self {
            controller: AppController::new(state),
            revision: 0,
        }
    }

    pub fn state(&self) -> &AppState {
        self.controller.state()
    }

    /// Returns queued playlist positions in queue order.
    pub fn playlist_queue(&self) -> Vec<usize> {
        self.state().playlist.queued_indices()
    }

    #[cfg(test)]
    pub fn state_mut(&mut self) -> &mut AppState {
        self.controller.state_mut()
    }

    /// Returns the number of observable transitions completed by this store.
    ///
    /// A transition with state changes or effects increments the revision once.
    /// A transition with neither leaves it unchanged.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn into_state(self) -> AppState {
        self.controller.into_state()
    }

    pub fn dispatch(&mut self, command: impl Into<AppCommand>) -> DispatchResult {
        let command = command.into();
        let event = format!("command {command:?}");
        let before = StoreSnapshot::from_state(self.state());
        let effects = self.controller.handle_command(command);
        let after = StoreSnapshot::from_state(self.state());
        let changes = before.diff(&after);
        // Commands that don't mutate any state (e.g. idempotent UI-visibility
        // commands re-dispatched every frame) are noise at info level; keep them
        // at trace so meaningful state transitions stay visible.
        let level = if changes.is_empty() {
            ConsoleLogLevel::Trace
        } else {
            ConsoleLogLevel::Info
        };
        self.finish_dispatch_logged(level, event, changes, effects)
    }

    /// Applies a backend-originated app event through the same revision,
    /// change-set, effect, and logging boundary as command dispatch.
    pub fn handle_playback_event(&mut self, event: PlaybackEvent) -> DispatchResult {
        let event_label = format!("playback {event:?}");
        let before = StoreSnapshot::from_state(self.state());
        let effects = self.controller.handle_playback_event(event);
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            event_label,
            before.diff(&after),
            effects,
        )
    }

    /// Applies the explicit playlist-EOF app event.
    pub fn handle_playlist_eof(&mut self) -> DispatchResult {
        let before = StoreSnapshot::from_state(self.state());
        let effects = self.controller.handle_playlist_eof();
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            "playlist-eof",
            before.diff(&after),
            effects,
        )
    }

    /// Applies the explicit playback-timer app event.
    pub fn tick_playback_position(&mut self, elapsed_ms: i64) -> DispatchResult {
        let elapsed_ms = elapsed_ms.max(0);
        let changed = {
            let state = self.controller.state_mut();
            let previous_position_ms = state.config.playback_position_ms;
            let duration_ms = state.player.duration_ms();
            state.config.playback_position_ms =
                state.config.playback_position_ms.saturating_add(elapsed_ms);
            if let Some(duration) = duration_ms {
                state.config.playback_position_ms = state.config.playback_position_ms.min(duration);
            }
            state.config.playback_position_ms != previous_position_ms
        };
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            format!("playback-position-tick elapsed_ms={elapsed_ms}"),
            if changed {
                StateChangeSet::PLAYER | StateChangeSet::CONFIG | StateChangeSet::RENDER_MAIN
            } else {
                StateChangeSet::empty()
            },
            vec![AppEffect::QueueRender(RenderTarget::All)],
        )
    }

    /// Applies a playback-runtime position observation without seeking the
    /// backend that produced it.
    pub fn update_playback_position_from_runtime(&mut self, position_ms: i64) -> DispatchResult {
        let requested_position_ms = position_ms;
        let position_ms = {
            let state = self.state();
            let position_ms = position_ms.max(0);
            state
                .player
                .duration_ms()
                .filter(|duration_ms| *duration_ms > 0)
                .map_or(position_ms, |duration_ms| position_ms.min(duration_ms))
        };
        let changed = self.state().config.playback_position_ms != position_ms;
        self.controller.state_mut().config.playback_position_ms = position_ms;
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            format!(
                "runtime-playback-position requested_position_ms={requested_position_ms} position_ms={position_ms}"
            ),
            if changed {
                StateChangeSet::PLAYER | StateChangeSet::CONFIG | StateChangeSet::RENDER_MAIN
            } else {
                StateChangeSet::empty()
            },
            if changed {
                vec![AppEffect::QueueRender(RenderTarget::All)]
            } else {
                Vec::new()
            },
        )
    }

    /// Applies an internal playback-transition volume event.
    pub fn set_runtime_volume_for_transition(&mut self, volume: i32) -> DispatchResult {
        let requested_volume = volume;
        let volume = volume.clamp(0, 100);
        let changed = self.state().player.volume() != volume;
        self.controller.state_mut().player.set_volume(volume);
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            format!(
                "runtime-volume-transition requested_volume={requested_volume} volume={volume}"
            ),
            if changed {
                StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN
            } else {
                StateChangeSet::empty()
            },
            vec![
                AppEffect::SetBackendVolume(volume),
                AppEffect::QueueRender(RenderTarget::All),
            ],
        )
    }

    /// Applies an external platform-volume app event without echoing an output
    /// effect back to the platform.
    pub fn sync_external_output_volume(&mut self, volume: i32) -> DispatchResult {
        let requested_volume = volume;
        let volume = volume.clamp(0, 100);
        let state = self.controller.state_mut();
        if state.player.volume() == volume {
            return self.finish_dispatch_logged(
                ConsoleLogLevel::Trace,
                format!(
                    "external-output-volume requested_volume={requested_volume} volume={volume}"
                ),
                StateChangeSet::empty(),
                Vec::new(),
            );
        }
        state.player.set_volume(volume);
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            format!("external-output-volume requested_volume={requested_volume} volume={volume}"),
            StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN,
            Vec::new(),
        )
    }

    /// Applies the explicit stop-fade completion app event.
    pub fn complete_stop_fade(&mut self, restore_volume: i32) -> DispatchResult {
        let requested_restore_volume = restore_volume;
        let restore_volume = restore_volume.clamp(0, 100);
        let position_changed = self.state().config.playback_position_ms != 0;
        {
            let state = self.controller.state_mut();
            state.player.stop();
            state.player.clear_visualization_data();
            state.player.set_volume(restore_volume);
            state.config.playback_position_ms = 0;
        }
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            format!(
                "complete-stop-fade requested_restore_volume={requested_restore_volume} restore_volume={restore_volume}"
            ),
            StateChangeSet::PLAYER
                | StateChangeSet::RENDER_ALL
                | if position_changed {
                    StateChangeSet::CONFIG
                } else {
                    StateChangeSet::empty()
                },
            vec![
                AppEffect::StopPlayback,
                AppEffect::SetBackendVolume(restore_volume),
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ],
        )
    }

    /// Applies the preferences-accepted app event.
    pub fn apply_config_from_preferences(&mut self, config: Config) -> DispatchResult {
        let previous_config = self.state().config.clone();
        let previous_volume = self.state().player.volume();
        let previous_balance = self.state().player.balance();
        let previous_shuffle = self.state().playlist.shuffle();
        let previous_repeat = self.state().playlist.repeat();
        let previous_no_advance = self.state().playlist.no_advance();
        let previous_playlist_position = self.state().playlist.position();
        {
            let state = self.controller.state_mut();
            state.config = config;
            state.apply_config_to_runtime();
        }
        let state = self.state();
        let config = &state.config;
        let mut changes = StateChangeSet::empty();
        if previous_config != *config {
            changes |= StateChangeSet::CONFIG;
        }
        if previous_volume != state.player.volume() || previous_balance != state.player.balance() {
            changes |= StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN;
        }
        if previous_shuffle != state.playlist.shuffle()
            || previous_repeat != state.playlist.repeat()
            || previous_no_advance != state.playlist.no_advance()
            || previous_playlist_position != state.playlist.position()
        {
            changes |= StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST;
        }
        if previous_config.equalizer_active != config.equalizer_active
            || previous_config.equalizer_auto != config.equalizer_auto
            || previous_config.equalizer_preamp_pos != config.equalizer_preamp_pos
            || previous_config.equalizer_band_pos != config.equalizer_band_pos
        {
            changes |= StateChangeSet::EQUALIZER | StateChangeSet::RENDER_EQUALIZER;
        }
        if previous_config.main_shaded != config.main_shaded
            || previous_config.playlist_visible != config.playlist_visible
            || previous_config.playlist_detached != config.playlist_detached
            || previous_config.playlist_shaded != config.playlist_shaded
            || previous_config.equalizer_visible != config.equalizer_visible
            || previous_config.equalizer_detached != config.equalizer_detached
            || previous_config.equalizer_shaded != config.equalizer_shaded
        {
            changes |= StateChangeSet::PANELS | StateChangeSet::RENDER_ALL;
        }
        if previous_config.skin != config.skin {
            changes |= StateChangeSet::SKIN | StateChangeSet::RENDER_ALL;
        }
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            "preferences-config-applied",
            changes,
            vec![
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ],
        )
    }

    /// Applies an asynchronous duration-index result app event.
    pub fn apply_duration_index_result(&mut self, result: DurationIndexResult) -> DispatchResult {
        let event = format!("duration-index-result {result:?}");
        let changed = self
            .controller
            .state_mut()
            .playlist
            .apply_duration_index_result(result);
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            event,
            if changed {
                StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST
            } else {
                StateChangeSet::empty()
            },
            if changed {
                vec![
                    AppEffect::SaveConfig,
                    AppEffect::QueueRender(RenderTarget::Playlist),
                ]
            } else {
                Vec::new()
            },
        )
    }

    /// Applies a completed playlist-file-load app event.
    pub fn replace_playlist_for_file_load(
        &mut self,
        playlist: crate::playlist::Playlist,
    ) -> DispatchResult {
        let entry_count = playlist.entries().len();
        let changed = self.state().playlist != playlist;
        self.controller.state_mut().playlist = playlist;
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            format!("playlist-file-loaded entries={entry_count}"),
            if changed {
                StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST
            } else {
                StateChangeSet::empty()
            },
            vec![
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::Playlist),
            ],
        )
    }

    /// Applies a completed equalizer-preset-load app event.
    pub fn apply_equalizer_preset_positions(
        &mut self,
        preamp_position: i32,
        band_positions: crate::audio_model::EqualizerBandPositions,
    ) -> DispatchResult {
        let event = format!(
            "equalizer-preset preamp_position={preamp_position} band_positions={band_positions:?}"
        );
        let preamp_position = preamp_position.clamp(0, 100);
        let changed = self.state().config.equalizer_preamp_pos != preamp_position
            || self.state().config.equalizer_band_pos != band_positions;
        {
            let config = &mut self.controller.state_mut().config;
            config.equalizer_preamp_pos = preamp_position;
            config.equalizer_band_pos = band_positions;
        }
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            event,
            if changed {
                StateChangeSet::EQUALIZER
                    | StateChangeSet::CONFIG
                    | StateChangeSet::RENDER_EQUALIZER
            } else {
                StateChangeSet::empty()
            },
            vec![
                AppEffect::SetBackendEqualizer,
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::Equalizer),
            ],
        )
    }

    fn finish_dispatch_logged(
        &mut self,
        level: ConsoleLogLevel,
        event: impl Into<String>,
        changes: StateChangeSet,
        effects: Vec<AppEffect>,
    ) -> DispatchResult {
        let result = self.finish_dispatch(changes, effects);
        let log_line = ConsoleEventLog::new(event, &result);
        app_log!(level, store, "{log_line}");
        result
    }

    fn finish_dispatch(
        &mut self,
        changes: StateChangeSet,
        effects: Vec<AppEffect>,
    ) -> DispatchResult {
        // A revision identifies a completed observable transition. State-only
        // changes and effect-only transitions each advance it exactly once;
        // fully idempotent transitions do not.
        if !changes.is_empty() || !effects.is_empty() {
            self.revision = self.revision.saturating_add(1);
        }
        DispatchResult {
            revision: self.revision,
            changes,
            effects,
        }
    }
}

impl Default for AppStore {
    fn default() -> Self {
        Self::new(AppState::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::{AudioCommand, PanelCommand, PlaylistCommand, UiCommand};
    use crate::playlist::Playlist;

    #[test]
    fn store_dispatch_updates_revision() {
        let mut store = AppStore::default();

        let result = store.dispatch(PanelCommand::SetPlaylistVisibility(true));

        assert_eq!(result.revision, 1);
        assert!(result.changes.contains(StateChangeSet::PANELS));
        assert!(store.state().config.playlist_visible);
    }

    #[test]
    fn store_centralizes_dialog_visibility() {
        let mut store = AppStore::default();

        let result = store.dispatch(UiCommand::SetPreferencesVisible(true));

        assert!(store.state().ui.preferences_visible);
        assert!(result.changes.contains(StateChangeSet::DIALOGS));
    }

    #[test]
    fn state_change_set_displays_named_flags() {
        let changes = StateChangeSet::PLAYER | StateChangeSet::CONFIG | StateChangeSet::RENDER_MAIN;

        assert_eq!(changes.to_string(), "player|config|render-main");
        assert_eq!(StateChangeSet::empty().to_string(), "none");
    }

    #[test]
    fn console_event_log_formats_event_result() {
        let result = DispatchResult {
            revision: 7,
            changes: StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST,
            effects: vec![AppEffect::QueueRender(RenderTarget::Playlist)],
        };

        let line = ConsoleEventLog::new("command Playlist(Clear)", &result).to_string();

        assert_eq!(
            line,
            "event: command Playlist(Clear); revision=7; changes=playlist|render-playlist; effects=[QueueRender(Playlist)]"
        );
    }

    #[test]
    fn internal_volume_transitions_only_emit_backend_volume_effects() {
        let mut store = AppStore::default();

        let fading = store.set_runtime_volume_for_transition(25);
        assert_eq!(
            fading.changes,
            StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN
        );
        assert_eq!(fading.revision, 1);
        assert!(fading.effects.contains(&AppEffect::SetBackendVolume(25)));
        assert!(!fading
            .effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::SetOutputVolume(_))));

        store.state_mut().config.playback_position_ms = 42_000;
        let stopped = store.complete_stop_fade(60);
        assert_eq!(
            stopped.changes,
            StateChangeSet::PLAYER | StateChangeSet::CONFIG | StateChangeSet::RENDER_ALL
        );
        assert_eq!(stopped.revision, 2);
        assert_eq!(store.state().config.playback_position_ms, 0);
        assert!(stopped.effects.contains(&AppEffect::SetBackendVolume(60)));
        assert!(stopped.effects.contains(&AppEffect::SaveConfig));
        assert!(!stopped
            .effects
            .iter()
            .any(|effect| matches!(effect, AppEffect::SetOutputVolume(_))));
    }

    #[test]
    fn external_output_volume_sync_updates_runtime_owner_without_output_effects() {
        let mut store = AppStore::default();
        let persisted_seed = store.state().config.volume;

        let result = store.sync_external_output_volume(37);

        assert_eq!(store.state().player.volume(), 37);
        assert_eq!(store.state().config.volume, persisted_seed);
        assert_eq!(store.state().persistence_snapshot().config.volume, 37);
        assert_eq!(
            result.changes,
            StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN
        );
        assert!(!result.effects.iter().any(|effect| matches!(
            effect,
            AppEffect::SetOutputVolume(_) | AppEffect::SetBackendVolume(_)
        )));
    }

    #[test]
    fn external_output_volume_sync_is_idempotent_when_unchanged() {
        let mut store = AppStore::default();
        let changed = store.sync_external_output_volume(37);

        let unchanged = store.sync_external_output_volume(37);

        assert_eq!(unchanged.revision, changed.revision);
        assert!(unchanged.changes.is_empty());
        assert!(unchanged.effects.is_empty());
    }

    #[test]
    fn effect_only_specialized_transitions_increment_revision_without_state_changes() {
        let mut store = AppStore::default();

        let volume = store.set_runtime_volume_for_transition(100);
        assert!(volume.changes.is_empty());
        assert_eq!(volume.revision, 1);

        let tick = store.tick_playback_position(0);
        assert!(tick.changes.is_empty());
        assert_eq!(tick.revision, 2);
    }

    #[test]
    fn playback_tick_reports_only_position_changes() {
        let mut store = AppStore::default();

        let result = store.tick_playback_position(250);

        assert_eq!(
            result.changes,
            StateChangeSet::PLAYER | StateChangeSet::CONFIG | StateChangeSet::RENDER_MAIN
        );
        assert_eq!(result.revision, 1);
        assert_eq!(store.state().config.playback_position_ms, 250);
    }

    #[test]
    fn runtime_position_observations_are_clamped_and_idempotent() {
        let mut store = AppStore::default();
        store.handle_playback_event(PlaybackEvent::DurationChanged(Some(1_000)));

        let changed = store.update_playback_position_from_runtime(1_500);
        assert_eq!(store.state().config.playback_position_ms, 1_000);
        assert_eq!(
            changed.changes,
            StateChangeSet::PLAYER | StateChangeSet::CONFIG | StateChangeSet::RENDER_MAIN
        );

        let unchanged = store.update_playback_position_from_runtime(1_500);
        assert_eq!(unchanged.revision, changed.revision);
        assert!(unchanged.changes.is_empty());
        assert!(unchanged.effects.is_empty());
    }

    #[test]
    fn preferences_apply_reports_precise_config_runtime_and_render_changes() {
        let mut store = AppStore::default();
        let mut config = store.state().config.clone();
        config.volume = 37;
        config.shuffle = true;
        config.equalizer_preamp_pos = 25;
        config.playlist_visible = true;
        config.skin = Some("new-skin.wsz".to_string());

        let result = store.apply_config_from_preferences(config);

        assert_eq!(
            result.changes,
            StateChangeSet::CONFIG
                | StateChangeSet::PLAYER
                | StateChangeSet::PLAYLIST
                | StateChangeSet::EQUALIZER
                | StateChangeSet::PANELS
                | StateChangeSet::SKIN
                | StateChangeSet::RENDER_ALL
        );
        assert_eq!(result.revision, 1);
        assert!(result.effects.contains(&AppEffect::SaveConfig));

        let unchanged = store.apply_config_from_preferences(store.state().config.clone());
        assert!(unchanged.changes.is_empty());
        assert_eq!(unchanged.revision, 2);
    }

    #[test]
    fn duration_indexing_reports_playlist_changes_and_is_idempotent() {
        let mut store = AppStore::default();
        store.state_mut().playlist.add_uri("file:///song.ogg");
        let result = DurationIndexResult {
            index: 0,
            uri: "file:///song.ogg".to_string(),
            length_ms: 42_000,
            title: Some("Song".to_string()),
        };

        let changed = store.apply_duration_index_result(result.clone());
        assert_eq!(
            changed.changes,
            StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST
        );
        assert_eq!(changed.revision, 1);

        let unchanged = store.apply_duration_index_result(result);
        assert!(unchanged.changes.is_empty());
        assert_eq!(unchanged.revision, 1);
    }

    #[test]
    fn store_exposes_queue_and_reports_queue_only_transitions() {
        let mut store = AppStore::default();
        store.state_mut().playlist.add_uri("file:///one.ogg");
        store.state_mut().playlist.add_uri("file:///two.ogg");

        let changed = store.dispatch(PlaylistCommand::Enqueue(1));
        assert_eq!(store.playlist_queue(), vec![1]);
        assert_eq!(
            changed.changes,
            StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST
        );
        assert!(!changed.effects.contains(&AppEffect::SaveConfig));

        let unchanged = store.dispatch(PlaylistCommand::Enqueue(1));
        assert!(unchanged.changes.is_empty());
        assert!(unchanged.effects.is_empty());
        assert_eq!(unchanged.revision, changed.revision);
    }

    #[test]
    fn playlist_load_detects_content_changes_with_the_same_shape() {
        let mut store = AppStore::default();
        store.state_mut().playlist.add_uri("file:///old.ogg");
        store.state_mut().playlist.enqueue(0);
        let mut playlist = Playlist::new();
        playlist.add_uri("file:///new.ogg");

        let changed = store.replace_playlist_for_file_load(playlist.clone());
        assert_eq!(
            changed.changes,
            StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST
        );
        assert_eq!(changed.revision, 1);
        assert!(store.playlist_queue().is_empty());

        let unchanged = store.replace_playlist_for_file_load(playlist);
        assert!(unchanged.changes.is_empty());
        assert_eq!(unchanged.revision, 2);
    }

    #[test]
    fn equalizer_preset_reports_only_equalizer_config_changes() {
        let mut store = AppStore::default();
        let mut band_positions = [50; 10];
        band_positions[3] = 75;

        let changed = store.apply_equalizer_preset_positions(25, band_positions);
        assert_eq!(
            changed.changes,
            StateChangeSet::EQUALIZER | StateChangeSet::CONFIG | StateChangeSet::RENDER_EQUALIZER
        );
        assert_eq!(changed.revision, 1);

        let unchanged = store.apply_equalizer_preset_positions(25, band_positions);
        assert!(unchanged.changes.is_empty());
        assert_eq!(unchanged.revision, 2);
    }

    #[test]
    fn gtk_style_commands_and_runtime_events_keep_revision_monotonic() {
        let mut store = AppStore::default();
        let mut revisions = Vec::new();

        revisions.push(store.dispatch(PanelCommand::SetMainShade(true)).revision);
        revisions.push(
            store
                .dispatch(UiCommand::SetPreferencesVisible(true))
                .revision,
        );
        revisions.push(
            store
                .apply_equalizer_preset_positions(25, [40; 10])
                .revision,
        );
        revisions.push(
            store
                .handle_playback_event(PlaybackEvent::StreamInfo(crate::player::StreamInfo {
                    bitrate: Some(192),
                    frequency: Some(44_100),
                    channels: Some(2),
                }))
                .revision,
        );
        revisions.push(store.update_playback_position_from_runtime(250).revision);

        assert!(revisions.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(store.revision(), *revisions.last().unwrap());
    }

    #[test]
    fn runtime_commands_leave_persisted_seed_fields_to_the_projection() {
        let mut store = AppStore::default();
        store.state_mut().playlist.add_uri("file:///tmp/song.ogg");
        let persisted_seed = store.state().config.clone();

        let volume = store.dispatch(AudioCommand::SetVolume(37));
        store.dispatch(AudioCommand::SetBalance(-21));
        store.dispatch(PlaylistCommand::ToggleShuffle);
        store.dispatch(PlaylistCommand::ToggleRepeat);
        store.dispatch(PlaylistCommand::SetPosition(0));

        assert!(!volume.changes.contains(StateChangeSet::CONFIG));
        assert_eq!(store.state().config, persisted_seed);
        let projected = store.state().persistence_snapshot().config;
        assert_eq!(projected.volume, 37);
        assert_eq!(projected.balance, -21);
        assert!(projected.shuffle);
        assert!(projected.repeat);
        assert_eq!(projected.playlist_position, 0);
    }
}
