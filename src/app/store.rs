//! Reactive, frontend-neutral application store.
//!
//! `AppStore` is the single canonical mutation point for application behavior.
//! Frontends translate native input into [`AppCommand`] / runtime events, call
//! `dispatch`, then update their widgets/windows from the returned change set
//! and immutable state/view-model reads.

use std::fmt;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::app::command::AppCommand;
use crate::app::controller::AppController;
use crate::app::effect::{AppEffect, RenderTarget};
use crate::app::logging::{console_log, ConsoleLogLevel};
use crate::app_state::{AppState, RuntimeSnapshot};
use crate::config::Config;
use crate::player::PlaybackEvent;
use crate::playlist::DurationIndexResult;

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

#[derive(Debug, Clone, PartialEq)]
pub struct StoreUpdate {
    pub revision: u64,
    pub changes: StateChangeSet,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoreSnapshot {
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
}

impl StoreSnapshot {
    fn from_state(state: &AppState) -> Self {
        Self {
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
        if self != next {
            changes |= StateChangeSet::CONFIG;
        }
        changes
    }
}

#[derive(Debug, Clone)]
pub struct AppStore {
    controller: AppController,
    revision: u64,
    subscribers: Vec<Sender<StoreUpdate>>,
}

impl AppStore {
    pub fn new(state: AppState) -> Self {
        Self {
            controller: AppController::new(state),
            revision: 0,
            subscribers: Vec::new(),
        }
    }

    pub fn state(&self) -> &AppState {
        self.controller.state()
    }

    /// Transitional escape hatch for loading files/preferences that have not
    /// yet been modeled as first-class commands. New app behavior should use
    /// `dispatch` instead.
    pub fn state_mut_for_migration(&mut self) -> &mut AppState {
        self.controller.state_mut()
    }

    /// Alias used while migrating existing frontend code to the store API.
    pub fn state_mut(&mut self) -> &mut AppState {
        self.state_mut_for_migration()
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn into_state(self) -> AppState {
        self.controller.into_state()
    }

    /// Transitional hook for frontends that still keep a legacy state mirror.
    /// This should disappear once all frontend mutations are expressed as
    /// commands/events.
    pub fn replace_state_for_migration(&mut self, state: AppState) {
        self.controller = AppController::new(state);
    }

    pub fn subscribe(&mut self) -> Receiver<StoreUpdate> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.push(sender);
        receiver
    }

    pub fn dispatch(&mut self, command: impl Into<AppCommand>) -> DispatchResult {
        let command = command.into();
        let event = format!("command {command:?}");
        let before = StoreSnapshot::from_state(self.state());
        let effects = self.controller.handle_command(command);
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(ConsoleLogLevel::Info, event, before.diff(&after), effects)
    }

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

    pub fn tick_playback_position(&mut self, elapsed_ms: i64) -> DispatchResult {
        let elapsed_ms = elapsed_ms.max(0);
        let before = StoreSnapshot::from_state(self.state());
        {
            let state = self.controller.state_mut();
            let duration_ms = state.player.duration_ms();
            state.config.playback_position_ms =
                state.config.playback_position_ms.saturating_add(elapsed_ms);
            if let Some(duration) = duration_ms {
                state.config.playback_position_ms = state.config.playback_position_ms.min(duration);
            }
        }
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            format!("playback-position-tick elapsed_ms={elapsed_ms}"),
            before.diff(&after) | StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN,
            vec![AppEffect::QueueRender(RenderTarget::All)],
        )
    }

    pub fn set_runtime_volume_for_transition(&mut self, volume: i32) -> DispatchResult {
        let requested_volume = volume;
        let before = StoreSnapshot::from_state(self.state());
        let volume = volume.clamp(0, 100);
        self.controller.state_mut().player.set_volume(volume);
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            format!(
                "runtime-volume-transition requested_volume={requested_volume} volume={volume}"
            ),
            before.diff(&after) | StateChangeSet::PLAYER | StateChangeSet::RENDER_MAIN,
            vec![
                AppEffect::SetBackendVolume(volume),
                AppEffect::QueueRender(RenderTarget::All),
            ],
        )
    }

    pub fn complete_stop_fade(&mut self, restore_volume: i32) -> DispatchResult {
        let requested_restore_volume = restore_volume;
        let before = StoreSnapshot::from_state(self.state());
        let restore_volume = restore_volume.clamp(0, 100);
        {
            let state = self.controller.state_mut();
            state.player.stop();
            state.player.clear_visualization_data();
            state.player.set_volume(restore_volume);
        }
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            format!(
                "complete-stop-fade requested_restore_volume={requested_restore_volume} restore_volume={restore_volume}"
            ),
            before.diff(&after) | StateChangeSet::PLAYER | StateChangeSet::RENDER_ALL,
            vec![
                AppEffect::StopPlayback,
                AppEffect::SetBackendVolume(restore_volume),
                AppEffect::QueueRender(RenderTarget::All),
            ],
        )
    }

    pub fn apply_config_from_preferences(&mut self, config: Config) -> DispatchResult {
        let before = StoreSnapshot::from_state(self.state());
        {
            let state = self.controller.state_mut();
            state.config = config;
            state.apply_config_to_runtime();
        }
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            "preferences-config-applied",
            before.diff(&after),
            vec![
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::All),
            ],
        )
    }

    pub fn apply_duration_index_result(&mut self, result: DurationIndexResult) -> DispatchResult {
        let event = format!("duration-index-result {result:?}");
        let before = StoreSnapshot::from_state(self.state());
        let changed = self
            .controller
            .state_mut()
            .playlist
            .apply_duration_index_result(result);
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Trace,
            event,
            if changed {
                before.diff(&after) | StateChangeSet::PLAYLIST | StateChangeSet::RENDER_PLAYLIST
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

    pub fn replace_playlist_for_file_load(
        &mut self,
        playlist: crate::playlist::Playlist,
    ) -> DispatchResult {
        let entry_count = playlist.entries().len();
        let before = StoreSnapshot::from_state(self.state());
        self.controller.state_mut().playlist = playlist;
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            format!("playlist-file-loaded entries={entry_count}"),
            before.diff(&after),
            vec![
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::Playlist),
            ],
        )
    }

    pub fn apply_equalizer_preset_positions(
        &mut self,
        preamp_position: i32,
        band_positions: crate::audio_model::EqualizerBandPositions,
    ) -> DispatchResult {
        let event = format!(
            "equalizer-preset preamp_position={preamp_position} band_positions={band_positions:?}"
        );
        let before = StoreSnapshot::from_state(self.state());
        {
            let config = &mut self.controller.state_mut().config;
            config.equalizer_preamp_pos = preamp_position.clamp(0, 100);
            config.equalizer_band_pos = band_positions;
        }
        let after = StoreSnapshot::from_state(self.state());
        self.finish_dispatch_logged(
            ConsoleLogLevel::Info,
            event,
            before.diff(&after),
            vec![
                AppEffect::SetBackendEqualizer,
                AppEffect::SaveConfig,
                AppEffect::QueueRender(RenderTarget::Equalizer),
            ],
        )
    }

    pub fn notify_external_mutation(&mut self, changes: StateChangeSet) -> DispatchResult {
        self.finish_dispatch_logged(
            ConsoleLogLevel::Debug,
            format!("external-mutation changes={changes}"),
            changes,
            Vec::new(),
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
        console_log(
            level,
            format_args!("{}", ConsoleEventLog::new(event, &result)),
        );
        result
    }

    fn finish_dispatch(
        &mut self,
        changes: StateChangeSet,
        effects: Vec<AppEffect>,
    ) -> DispatchResult {
        if !changes.is_empty() || !effects.is_empty() {
            self.revision = self.revision.saturating_add(1);
            self.notify_subscribers(changes);
        }
        DispatchResult {
            revision: self.revision,
            changes,
            effects,
        }
    }

    fn notify_subscribers(&mut self, changes: StateChangeSet) {
        let update = StoreUpdate {
            revision: self.revision,
            changes,
        };
        self.subscribers
            .retain(|subscriber| subscriber.send(update.clone()).is_ok());
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
    use crate::app::command::{PanelCommand, UiCommand};

    #[test]
    fn store_dispatch_updates_revision_and_notifies_subscribers() {
        let mut store = AppStore::default();
        let updates = store.subscribe();

        let result = store.dispatch(PanelCommand::SetPlaylistVisibility(true));

        assert_eq!(result.revision, 1);
        assert!(result.changes.contains(StateChangeSet::PANELS));
        assert!(store.state().config.playlist_visible);
        let update = updates.recv().expect("store update");
        assert_eq!(update.revision, result.revision);
        assert!(update.changes.contains(StateChangeSet::PANELS));
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
}
