//! Frontend-neutral dispatch and playback-effect coordinator.
//!
//! State and backend work happen here; the returned update describes only work
//! that a frontend still needs to interpret.

use crate::app::command::AppCommand;
use crate::app::effect::{
    owner, AppEffect, EffectOwner, FileDialogRequest, PlatformEffect, PlaybackEffect, RenderTarget,
    UiEffect,
};
use crate::app::playback_transition::{PlaybackTransition, TransitionEvent};
use crate::app::store::{AppStore, DispatchResult, StateChangeSet};
use crate::app_state::AppState;
use crate::config::Config;
use crate::playback::backend::PlaybackBackend;
use crate::playback::duration_indexer::DurationIndexer;
#[cfg(any(feature = "gtk-ui", test))]
use crate::playback::model::OutputDeviceGroups;
use crate::playback::model::{EqualizerBackendState, StreamInfo};
use crate::playback::runtime::PlaybackRuntime;
use crate::player::{PlaybackEvent, PlayerState};
use crate::playlist::{DurationIndexResult, Playlist};
use crate::render::VisualizationRenderState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackExecution {
    Local,
    AlreadyExecuted,
}

impl PlaybackExecution {
    pub const LOCAL: Self = Self::Local;

    pub fn after_external_backend_execution(backend_executed: bool) -> Self {
        if backend_executed {
            Self::AlreadyExecuted
        } else {
            Self::Local
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    Command(AppCommand),
    PlaybackStartPrepared(PendingPlaybackStart),
    PlaybackStartCancelled(PendingPlaybackStart),
    StopBackend,
    BeginStopFade,
    RefreshBackendEqualizer,
    OpenFileDialog(FileDialogRequest),
    SaveConfig,
    RequestRender(RenderTarget),
    Playback(PlaybackEvent),
    PlaylistEof,
    BackendPosition(i64),
    PlaybackPosition(i64),
    PlaybackTick(i64),
    TransitionTick(i64),
    TransitionVolume(i32),
    CompleteStopFade(i32),
    DurationBatch(Vec<DurationIndexResult>),
    ReplacePlaylist(Playlist),
    Preferences(Config),
    EqualizerPreset {
        preamp: i32,
        bands: crate::audio_model::EqualizerBandPositions,
    },
    ExternalOutputVolume(i32),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FrontendEffect {
    Ui(UiEffect),
    Platform(PlatformEffect),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPlaybackStart {
    pub uri: String,
    pub position_ms: i64,
    playlist_position: Option<usize>,
    sequence: u64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuntimeUpdate {
    pub changes: StateChangeSet,
    pub render_targets: Vec<RenderTarget>,
    pub(crate) frontend_effects: Vec<FrontendEffect>,
    pub(crate) playback_effects: Vec<PlaybackEffect>,
    pub pending_playback_starts: Vec<PendingPlaybackStart>,
    pub messages: Vec<String>,
    /// A pause or config-save requests an immediate platform persistence flush.
    pub force_persistence: bool,
    /// A countdown changed without a store revision; refresh the time display.
    pub transition_changed: bool,
}

impl RuntimeUpdate {
    fn render(&mut self, target: RenderTarget) {
        if target == RenderTarget::All {
            self.render_targets.clear();
        } else if self.render_targets.contains(&RenderTarget::All) {
            return;
        }
        if !self.render_targets.contains(&target) {
            self.render_targets.push(target);
        }
    }

    fn invalidate(&mut self, changes: StateChangeSet) {
        self.changes |= changes;
        if changes.contains(StateChangeSet::RENDER_ALL) {
            self.render(RenderTarget::All);
        } else {
            for (flag, target) in [
                (StateChangeSet::RENDER_MAIN, RenderTarget::Main),
                (StateChangeSet::RENDER_PLAYLIST, RenderTarget::Playlist),
                (StateChangeSet::RENDER_EQUALIZER, RenderTarget::Equalizer),
            ] {
                if changes.intersects(flag) {
                    self.render(target);
                }
            }
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.changes |= other.changes;
        for target in other.render_targets {
            self.render(target);
        }
        self.frontend_effects.extend(other.frontend_effects);
        self.playback_effects.extend(other.playback_effects);
        self.pending_playback_starts
            .extend(other.pending_playback_starts);
        self.messages.extend(other.messages);
        self.force_persistence |= other.force_persistence;
        self.transition_changed |= other.transition_changed;
    }
}

const STOP_FADE_DURATION_MS: i64 = 1_000;

pub struct FrontendRuntime {
    store: AppStore,
    playback: PlaybackRuntime,
    duration_indexer: DurationIndexer,
    stop_fade: Option<(i64, i32)>,
    pending_playback_start: Option<PendingPlaybackStart>,
    next_playback_start_sequence: u64,
    transition: PlaybackTransition,
    last_playback_request: Option<String>,
}

impl FrontendRuntime {
    pub fn new(state: AppState, backend: Option<Box<dyn PlaybackBackend>>) -> Self {
        Self::with_duration_indexer(state, backend, DurationIndexer::with_batch_size(1, || {}))
    }

    #[cfg(any(feature = "egui-ui", feature = "mobile-ui", test))]
    pub(crate) fn new_with_duration_wakeup(
        state: AppState,
        backend: Option<Box<dyn PlaybackBackend>>,
        on_batch: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self::with_duration_indexer(state, backend, DurationIndexer::new(on_batch))
    }

    fn with_duration_indexer(
        state: AppState,
        backend: Option<Box<dyn PlaybackBackend>>,
        duration_indexer: DurationIndexer,
    ) -> Self {
        let transition = PlaybackTransition::Idle.transition(TransitionEvent::StoppedPosition(
            state.config.playback_position_ms,
        ));
        Self {
            store: AppStore::new(state),
            playback: PlaybackRuntime::new(backend),
            duration_indexer,
            stop_fade: None,
            pending_playback_start: None,
            next_playback_start_sequence: 0,
            transition,
            last_playback_request: None,
        }
    }

    pub fn store(&self) -> &AppStore {
        &self.store
    }

    // Test fixtures may construct edge-case states; production mutations go through `handle`.
    #[cfg(test)]
    pub(crate) fn store_mut(&mut self) -> &mut AppStore {
        &mut self.store
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn use_installed_backend_only(&mut self) {
        self.playback.use_installed_backend_only();
    }

    pub(crate) fn has_backend(&self) -> bool {
        self.playback.has_backend()
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn install_gtk_backend(
        &mut self,
        backend: Box<dyn PlaybackBackend>,
    ) -> (OutputDeviceGroups, Option<String>) {
        let state = self.state();
        let device = state.config.output_device.clone();
        let volume = state.player.volume();
        let balance = state.player.balance();
        let equalizer = self.equalizer_state();
        self.playback
            .install_gtk_backend(backend, device.as_deref(), volume, balance, equalizer)
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn select_output_device(
        &mut self,
        device: Option<&str>,
    ) -> Option<(Result<(), String>, OutputDeviceGroups)> {
        self.playback.select_output_device(device)
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn install_backend_with_dsp(
        &mut self,
        backend: Box<dyn PlaybackBackend>,
    ) -> Result<(), String> {
        let balance = self.state().config.balance;
        let equalizer = self.equalizer_state();
        self.playback
            .install_backend_with_dsp(backend, balance, equalizer)
    }

    fn equalizer_state(&self) -> EqualizerBackendState {
        let config = &self.state().config;
        EqualizerBackendState {
            active: config.equalizer_active,
            preamp_position: config.equalizer_preamp_pos,
            band_positions: config.equalizer_band_pos,
        }
    }

    pub(crate) fn poll_playback_events(&self) -> Option<Result<Vec<PlaybackEvent>, String>> {
        self.playback.poll_events()
    }

    pub(crate) fn backend_stream_info(&self) -> Option<StreamInfo> {
        self.playback.stream_info()
    }

    pub(crate) fn backend_duration_ms(&self) -> Option<i64> {
        self.playback.duration_ms()
    }

    pub(crate) fn backend_position_ms(&self) -> Option<i64> {
        self.playback.position_ms()
    }

    pub(crate) fn seek_backend(&mut self, position_ms: i64) -> Option<Result<(), String>> {
        let previous_ms = self.playback.position_ms();
        let result = self.playback.seek(position_ms);
        if let Some(outcome) = &result {
            self.transition = self.transition.transition(if outcome.is_ok() {
                TransitionEvent::SeekApplied { previous_ms }
            } else {
                TransitionEvent::SeekFailed {
                    final_attempt: false,
                }
            });
        }
        result
    }

    pub(crate) fn pending_seek_ms(&self) -> Option<i64> {
        self.transition.pending_seek()
    }

    pub(crate) fn apply_pending_start_seek(&mut self, final_attempt: bool) -> RuntimeUpdate {
        let mut update = RuntimeUpdate::default();
        let Some(position_ms) = self.transition.pending_seek() else {
            return update;
        };
        if let Some(result) = self.seek_backend(position_ms) {
            match result {
                Ok(()) => update.transition_changed = true,
                Err(error) => {
                    if final_attempt {
                        self.transition = self.transition.transition(TransitionEvent::SeekFailed {
                            final_attempt: true,
                        });
                        update.messages.push(error);
                    }
                }
            }
        }
        update
    }

    pub(crate) fn transition(&self) -> PlaybackTransition {
        self.transition
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn last_playback_request(&self) -> Option<&str> {
        self.last_playback_request.as_deref()
    }

    pub(crate) fn cancel_eof_wait(&mut self) {
        if self.transition.wait_remaining().is_some() {
            self.transition = self.transition.transition(TransitionEvent::CancelWait);
        }
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn reset_stopped_position(&mut self) {
        self.transition = self.transition.transition(TransitionEvent::StoppedPosition(
            self.state().config.playback_position_ms,
        ));
    }

    #[cfg(test)]
    pub(crate) fn request_backend_seek(&mut self, position_ms: i64) {
        self.transition = self
            .transition
            .transition(TransitionEvent::SeekRequested(position_ms));
    }

    pub fn tick_playback_transition(&mut self, elapsed_ms: i64) -> RuntimeUpdate {
        let (next, changed, advance) = self.transition.tick(elapsed_ms);
        self.transition = next;
        let mut update = RuntimeUpdate {
            transition_changed: changed,
            ..RuntimeUpdate::default()
        };
        if advance {
            update.merge(self.advance_playlist_after_eof(PlaybackExecution::Local));
        }
        update
    }

    fn advance_playlist_after_eof(&mut self, execution: PlaybackExecution) -> RuntimeUpdate {
        let result = self.store.handle_playlist_eof();
        self.apply_dispatch_result(result, execution)
    }

    fn apply_dispatch_result(
        &mut self,
        result: DispatchResult,
        execution: PlaybackExecution,
    ) -> RuntimeUpdate {
        let mut update = RuntimeUpdate::default();
        update.invalidate(result.changes);
        self.apply_effects(result.effects, execution, update, false)
    }

    #[cfg(any(feature = "gtk-ui", feature = "egui-ui", test))]
    pub(crate) fn set_output_volume(&self, volume: i32) -> Option<String> {
        self.playback.set_output_volume(volume)
    }

    pub(crate) fn visualization_render_state(&self) -> VisualizationRenderState {
        self.playback
            .visualization_render_state(self.state().config.vis_vu_mode)
    }

    pub(crate) fn reset_visualization_tick(&mut self) {
        self.playback.reset_visualization_tick();
    }

    #[cfg(any(feature = "egui-ui", feature = "mobile-ui", test))]
    pub(crate) fn tick_visualization(&mut self, steps: usize) {
        let player = &self.state().player;
        let data = player
            .visualization_data_valid()
            .then(|| *player.visualization_data());
        self.playback
            .tick_visualization(data.as_ref().map(|values| values.as_slice()), steps);
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn advance_visualization_tick(&mut self, divisor: i32) {
        let player = &self.state().player;
        let data = player
            .visualization_data_valid()
            .then(|| *player.visualization_data());
        self.playback
            .advance_visualization_tick(divisor, data.as_ref().map(|values| values.as_slice()));
    }

    pub fn state(&self) -> &AppState {
        self.store.state()
    }

    pub fn stop_fade_active(&self) -> bool {
        self.stop_fade.is_some()
    }

    pub fn schedule_missing_durations(&self) {
        self.duration_indexer.schedule(&self.state().playlist);
    }

    pub fn drain_duration_updates(&mut self) -> RuntimeUpdate {
        let mut update = RuntimeUpdate::default();
        for batch in self.duration_indexer.drain() {
            update.merge(self.handle(RuntimeEvent::DurationBatch(batch), PlaybackExecution::Local));
        }
        update
    }

    #[cfg(any(feature = "gtk-ui", test))]
    pub(crate) fn enqueue_duration_batch(&self, batch: Vec<DurationIndexResult>) {
        self.duration_indexer.enqueue_batch(batch);
    }

    pub fn tick_stop_fade(&mut self, elapsed_ms: i64) -> RuntimeUpdate {
        let Some((remaining, initial_volume)) = self.stop_fade else {
            return RuntimeUpdate::default();
        };
        let remaining = remaining.saturating_sub(elapsed_ms.max(0)).max(0);
        if remaining == 0 {
            self.stop_fade = None;
            self.handle(
                RuntimeEvent::CompleteStopFade(initial_volume),
                PlaybackExecution::Local,
            )
        } else {
            self.stop_fade = Some((remaining, initial_volume));
            let volume = (i64::from(initial_volume) * remaining / STOP_FADE_DURATION_MS) as i32;
            self.handle(
                RuntimeEvent::TransitionVolume(volume),
                PlaybackExecution::Local,
            )
        }
    }

    #[cfg(any(feature = "gtk-ui", feature = "egui-ui", feature = "mobile-ui", test))]
    pub(crate) fn apply_visualization_preferences(&mut self) {
        self.playback
            .apply_visualization_preferences(&self.store.state().config);
    }

    // Keep a single dispatch/effect boundary for commands and backend-originated events.
    pub fn handle(&mut self, event: RuntimeEvent, execution: PlaybackExecution) -> RuntimeUpdate {
        if let RuntimeEvent::TransitionTick(elapsed) = event {
            return self.tick_playback_transition(elapsed);
        }
        let mut update = RuntimeUpdate::default();
        let prepared_start = matches!(event, RuntimeEvent::PlaybackStartPrepared(_));
        if matches!(event, RuntimeEvent::ReplacePlaylist(_)) {
            self.pending_playback_start = None;
            self.transition = self.transition.transition(TransitionEvent::Stop);
        }
        let effects = match event {
            RuntimeEvent::Command(command) => {
                let result = self.store.dispatch(command);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::Playback(event) => {
                if matches!(event, PlaybackEvent::Error(_) | PlaybackEvent::EndOfStream) {
                    self.transition = self.transition.transition(TransitionEvent::Stop);
                }
                let result = self.store.handle_playback_event(event);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::PlaylistEof => {
                // EOF waiting supersedes an in-progress stop fade, just as it did
                // when both states were represented by one transition enum.
                self.stop_fade = None;
                // The external backend may already have advanced (Android media
                // session). Only the runtime-owned backend may wait before advancing.
                self.transition = self.transition.transition(TransitionEvent::Stop);
                let position = self.store.update_playback_position_from_runtime(0);
                update.invalidate(position.changes);
                if execution == PlaybackExecution::Local && self.state().config.pause_between_songs
                {
                    let duration = i64::from(self.state().config.pause_between_songs_time) * 1_000;
                    if duration > 0 {
                        self.transition = self
                            .transition
                            .transition(TransitionEvent::EofWait(duration));
                        update.transition_changed = true;
                        return update;
                    }
                }
                let result = self.store.handle_playlist_eof();
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::BackendPosition(position) => {
                let (next, confirmed) = self.transition.observe_position(position);
                self.transition = next;
                let Some(position) = confirmed else {
                    return update;
                };
                let result = self.store.update_playback_position_from_runtime(position);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::PlaybackPosition(position) => {
                let result = self.store.update_playback_position_from_runtime(position);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::TransitionTick(_) => unreachable!(),
            RuntimeEvent::PlaybackTick(elapsed) => {
                let result = self.store.tick_playback_position(elapsed);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::TransitionVolume(volume) => {
                let result = self.store.set_runtime_volume_for_transition(volume);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::CompleteStopFade(volume) => {
                let result = self.store.complete_stop_fade(volume);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::DurationBatch(batch) => {
                let result = self.store.apply_duration_index_results(batch);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::ReplacePlaylist(playlist) => {
                let result = self.store.replace_playlist_for_file_load(playlist);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::Preferences(config) => {
                let result = self.store.apply_config_from_preferences(config);
                update.invalidate(result.changes);
                if !self.state().config.pause_between_songs {
                    self.cancel_eof_wait();
                }
                result.effects
            }
            RuntimeEvent::EqualizerPreset { preamp, bands } => {
                let result = self.store.apply_equalizer_preset_positions(preamp, bands);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::ExternalOutputVolume(volume) => {
                let result = self.store.sync_external_output_volume(volume);
                update.invalidate(result.changes);
                result.effects
            }
            RuntimeEvent::PlaybackStartPrepared(start) => {
                if self.pending_playback_start.as_ref() == Some(&start) {
                    // Consuming the token makes duplicate preparation harmless.
                    // Superseding transitions expire it before this branch.
                    self.pending_playback_start = None;
                    vec![AppEffect::StartPlaybackUri {
                        uri: start.uri,
                        position_ms: start.position_ms,
                    }]
                } else {
                    return update;
                }
            }
            RuntimeEvent::PlaybackStartCancelled(start) => {
                if self.pending_playback_start.as_ref() == Some(&start) {
                    self.pending_playback_start = None;
                }
                return update;
            }
            RuntimeEvent::StopBackend => vec![AppEffect::StopPlayback],
            RuntimeEvent::BeginStopFade => vec![AppEffect::BeginStopFade {
                start_volume: self.store.state().player.volume().max(0),
            }],
            RuntimeEvent::RefreshBackendEqualizer => vec![AppEffect::SetBackendEqualizer],
            RuntimeEvent::OpenFileDialog(request) => vec![AppEffect::OpenFileDialog(request)],
            RuntimeEvent::SaveConfig => vec![AppEffect::SaveConfig],
            RuntimeEvent::RequestRender(target) => vec![AppEffect::QueueRender(target)],
        };
        self.apply_effects(effects, execution, update, prepared_start)
    }

    fn apply_effects(
        &mut self,
        effects: Vec<AppEffect>,
        execution: PlaybackExecution,
        mut update: RuntimeUpdate,
        prepared_start: bool,
    ) -> RuntimeUpdate {
        // A stop/seek/pause or a changed playback target supersedes an unprepared
        // start. Equalizer-preset events during GTK preparation leave it intact.
        if !prepared_start {
            if let Some(start) = &self.pending_playback_start {
                let state = self.store.state();
                let current_uri = state
                    .playlist
                    .position()
                    .and_then(|position| state.playlist.entries().get(position))
                    .map(|entry| entry.filename.as_str());
                if state.player.state() == PlayerState::Stopped
                    || state.playlist.position() != start.playlist_position
                    || current_uri != Some(start.uri.as_str())
                    || effects.iter().any(|effect| {
                        matches!(
                            effect,
                            AppEffect::StopPlayback
                                | AppEffect::BeginStopFade { .. }
                                | AppEffect::SeekPlayback(_)
                                | AppEffect::PausePlayback
                                | AppEffect::ResumePlayback
                        )
                    })
                {
                    self.pending_playback_start = None;
                }
            }
        }
        for effect in effects {
            if let AppEffect::StartPlaybackUri { uri, position_ms } = &effect {
                // Only the prepared event may execute the start. The frontend can
                // configure track-specific playback or cancel before that event.
                if !prepared_start {
                    self.next_playback_start_sequence =
                        self.next_playback_start_sequence.wrapping_add(1);
                    let start = PendingPlaybackStart {
                        uri: uri.clone(),
                        position_ms: *position_ms,
                        playlist_position: self.store.state().playlist.position(),
                        sequence: self.next_playback_start_sequence,
                    };
                    self.pending_playback_start = Some(start.clone());
                    update.pending_playback_starts.push(start);
                    continue;
                }
            }
            match owner(effect) {
                EffectOwner::Playback(effect) => {
                    update.playback_effects.push(effect.clone());
                    match effect {
                        PlaybackEffect::BeginStopFade { start_volume } => {
                            if execution == PlaybackExecution::AlreadyExecuted {
                                update.merge(self.handle(
                                    RuntimeEvent::CompleteStopFade(start_volume),
                                    execution,
                                ));
                            } else {
                                self.stop_fade = Some((STOP_FADE_DURATION_MS, start_volume));
                            }
                        }
                        PlaybackEffect::Stop
                        | PlaybackEffect::StartUri { .. }
                        | PlaybackEffect::Seek(_) => {
                            // Starting another track or seeking supersedes a fade.
                            self.stop_fade = None;
                        }
                        _ => {}
                    }
                    if matches!(effect, PlaybackEffect::Stop) {
                        self.transition = self.transition.transition(TransitionEvent::Stop);
                    }
                    if matches!(effect, PlaybackEffect::Pause) {
                        update.force_persistence = true;
                    }
                    let previous_ms = if matches!(effect, PlaybackEffect::Seek(_)) {
                        self.playback.position_ms()
                    } else {
                        None
                    };
                    let equalizer = self.equalizer_state();
                    let errors = self.playback.apply_effect(
                        &effect,
                        equalizer,
                        execution == PlaybackExecution::Local,
                    );
                    match effect {
                        PlaybackEffect::StartUri { uri, position_ms } => {
                            self.last_playback_request = Some(uri);
                            self.transition = self.transition.transition(
                                if errors.is_empty() && execution == PlaybackExecution::Local {
                                    TransitionEvent::Start(position_ms)
                                } else {
                                    TransitionEvent::Stop
                                },
                            );
                        }
                        PlaybackEffect::Seek(position_ms) => {
                            // A direct seek is already sent to the backend. It
                            // still needs a confirming position sample when one
                            // is available; backend-less starts retain a request.
                            self.transition = if self.state().player.state() == PlayerState::Stopped
                            {
                                self.transition
                                    .transition(TransitionEvent::StoppedPosition(position_ms))
                            } else if position_ms > 0 && errors.is_empty() {
                                let pending = self
                                    .transition
                                    .transition(TransitionEvent::SeekRequested(position_ms));
                                if self.playback.has_backend() {
                                    // An externally executed seek has already
                                    // reached its backend; only confirm it.
                                    pending.transition(TransitionEvent::SeekApplied { previous_ms })
                                } else if execution == PlaybackExecution::Local {
                                    pending
                                } else {
                                    PlaybackTransition::Idle
                                }
                            } else {
                                self.transition.transition(TransitionEvent::Stop)
                            };
                        }
                        _ => {}
                    }
                    update.messages.extend(errors);
                }
                EffectOwner::Ui(UiEffect::QueueRender(target)) => update.render(target),
                EffectOwner::Ui(effect) => update.frontend_effects.push(FrontendEffect::Ui(effect)),
                EffectOwner::Platform(effect) => {
                    if matches!(effect, PlatformEffect::SaveConfig) {
                        update.force_persistence = true;
                    }
                    update
                        .frontend_effects
                        .push(FrontendEffect::Platform(effect));
                }
            }
        }
        update
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command::{AudioCommand, PlayerCommand, PlaylistCommand};
    use crate::playback::backend::AudioMetadataProbe;
    use crate::playback::model::EqualizerBackendState;
    use crate::playlist::DurationIndexItem;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;

    struct RecordingBackend {
        calls: Arc<Mutex<Vec<String>>>,
        fail_pause: bool,
    }

    impl PlaybackBackend for RecordingBackend {
        fn play_uri(&self, uri: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("play:{uri}"));
            Ok(())
        }
        fn pause(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("pause".into());
            if self.fail_pause {
                Err("pause failed".into())
            } else {
                Ok(())
            }
        }
        fn unpause(&self) -> Result<(), String> {
            Ok(())
        }
        fn stop(&self) -> Result<(), String> {
            self.calls.lock().unwrap().push("stop".into());
            Ok(())
        }
        fn seek(&self, position_ms: i64) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("seek:{position_ms}"));
            Ok(())
        }
        fn set_volume(&self, volume: i32) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("volume:{volume}"));
            Ok(())
        }
        fn set_balance(&self, _: i32) -> Result<(), String> {
            Ok(())
        }
        fn set_equalizer(&self, _: EqualizerBackendState) -> Result<(), String> {
            Ok(())
        }
    }

    fn recording_runtime(fail_pause: bool) -> (FrontendRuntime, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let backend = RecordingBackend {
            calls: Arc::clone(&calls),
            fail_pause,
        };
        (
            FrontendRuntime::new(AppState::default(), Some(Box::new(backend))),
            calls,
        )
    }

    struct ObservedBackend {
        calls: Arc<Mutex<Vec<String>>>,
        events: Mutex<Vec<PlaybackEvent>>,
        fail_next_seek: Mutex<bool>,
    }

    impl PlaybackBackend for ObservedBackend {
        fn play_uri(&self, uri: &str) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("play:{uri}"));
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
        fn seek(&self, position_ms: i64) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("seek:{position_ms}"));
            if std::mem::take(&mut *self.fail_next_seek.lock().unwrap()) {
                Err("seek failed".into())
            } else {
                Ok(())
            }
        }
        fn set_volume(&self, volume: i32) -> Result<(), String> {
            self.calls.lock().unwrap().push(format!("volume:{volume}"));
            Ok(())
        }
        fn set_balance(&self, balance: i32) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("balance:{balance}"));
            Ok(())
        }
        fn set_equalizer(&self, _: EqualizerBackendState) -> Result<(), String> {
            self.calls.lock().unwrap().push("equalizer".into());
            Ok(())
        }
        fn select_output_device(
            &mut self,
            selection: crate::playback::model::OutputDeviceSelection<'_>,
        ) -> Result<(), String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("device:{selection:?}"));
            Ok(())
        }
        fn output_device_groups(&self) -> OutputDeviceGroups {
            OutputDeviceGroups {
                local: vec![crate::playback::model::OutputDevice::system(
                    "speaker", "Speaker", "Audio", false,
                )],
                network: vec![],
            }
        }
        fn set_spectrum_layout(&self, layout: crate::audio_model::SpectrumLayout) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("layout:{layout:?}"));
        }
        fn poll_events(&self) -> Result<Vec<PlaybackEvent>, String> {
            Ok(std::mem::take(&mut *self.events.lock().unwrap()))
        }
        fn stream_info(&self) -> StreamInfo {
            StreamInfo {
                bitrate: Some(192),
                frequency: None,
                channels: None,
            }
        }
        fn duration_ms(&self) -> Option<i64> {
            Some(9000)
        }
        fn position_ms(&self) -> Option<i64> {
            Some(100)
        }
    }

    #[test]
    fn backend_boundary_installs_polls_seeks_and_observes_values() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let backend = ObservedBackend {
            calls: calls.clone(),
            events: Mutex::new(vec![PlaybackEvent::AsyncDone, PlaybackEvent::EndOfStream]),
            fail_next_seek: Mutex::new(true),
        };
        let mut state = AppState::default();
        state.config.output_device = Some("speaker".into());
        state.config.vis_mode = crate::skin::widget::VisMode::Analyzer;
        state.config.vis_analyzer_style = crate::skin::widget::VisAnalyzerStyle::Bars;
        let mut runtime = FrontendRuntime::new(state, None);
        runtime.use_installed_backend_only();
        let (groups, error) = runtime.install_gtk_backend(Box::new(backend));
        assert!(error.is_none());
        assert_eq!(groups.local[0].id, "speaker");
        assert_eq!(runtime.backend_stream_info().unwrap().bitrate, Some(192));
        assert_eq!(runtime.backend_duration_ms(), Some(9000));
        assert_eq!(runtime.backend_position_ms(), Some(100));
        runtime.apply_visualization_preferences();
        assert_eq!(
            runtime.poll_playback_events().unwrap().unwrap(),
            vec![PlaybackEvent::AsyncDone, PlaybackEvent::EndOfStream]
        );
        assert!(runtime.poll_playback_events().unwrap().unwrap().is_empty());
        let (_, switch_groups) = runtime.select_output_device(None).unwrap();
        assert_eq!(switch_groups.local[0].id, "speaker");
        runtime.playback.apply_effect(
            &PlaybackEffect::StartUri {
                uri: "song".into(),
                position_ms: 500,
            },
            runtime.equalizer_state(),
            true,
        );
        runtime.request_backend_seek(500);
        assert!(runtime.apply_pending_start_seek(false).messages.is_empty());
        assert_eq!(runtime.pending_seek_ms(), Some(500));
        assert!(runtime.apply_pending_start_seek(true).messages.is_empty());
        assert_eq!(
            runtime.transition(),
            PlaybackTransition::AwaitingSeek {
                target_ms: 500,
                previous_ms: Some(100),
                elapsed_ms: 0,
            }
        );
        assert!(runtime.apply_pending_start_seek(true).messages.is_empty());
        runtime.handle(RuntimeEvent::BackendPosition(800), PlaybackExecution::Local);
        assert_eq!(runtime.transition(), PlaybackTransition::Idle);
        assert_eq!(runtime.state().config.playback_position_ms, 800);
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                "device:System(\"speaker\")",
                "volume:100",
                "balance:0",
                "equalizer",
                "layout:AnalyzerBars",
                "layout:AnalyzerBars",
                "device:Automatic",
                "play:song",
                "seek:500",
                "seek:500"
            ]
        );
    }

    #[test]
    fn visualization_cadence_and_external_stop_stay_runtime_owned() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.playback.advance_visualization_tick(3, Some(&[0.5]));
        runtime.playback.advance_visualization_tick(3, Some(&[0.5]));
        assert_eq!(runtime.visualization_render_state().data[0], 0.0);
        runtime.playback.advance_visualization_tick(3, Some(&[0.5]));
        assert!(runtime.visualization_render_state().data[0] > 0.0);
        runtime.playback.advance_visualization_tick(3, Some(&[0.5]));
        runtime.reset_visualization_tick();
        runtime.playback.advance_visualization_tick(3, Some(&[0.0]));
        assert!(runtime.visualization_render_state().data[0] > 0.0);
        runtime
            .playback
            .apply_effect(&PlaybackEffect::Stop, runtime.equalizer_state(), false);
        assert_eq!(runtime.visualization_render_state().data[0], 0.0);
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn command_updates_state_and_interprets_playback_with_one_owner() {
        let (mut runtime, calls) = recording_runtime(true);
        runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::AddUris(vec!["song".into()]).into()),
            PlaybackExecution::Local,
        );
        let mut play = runtime.handle(
            RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
            PlaybackExecution::Local,
        );
        let start = play.pending_playback_starts.pop().unwrap();
        play.merge(runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start),
            PlaybackExecution::Local,
        ));
        assert!(play.changes.intersects(StateChangeSet::PLAYER));
        assert_eq!(*calls.lock().unwrap(), vec!["play:song"]);
        let pause = runtime.handle(
            RuntimeEvent::Command(PlayerCommand::Pause.into()),
            PlaybackExecution::Local,
        );
        assert_eq!(*calls.lock().unwrap(), vec!["play:song", "pause"]);
        assert_eq!(pause.messages, vec!["pause failed"]);
        assert!(pause.force_persistence);
        assert_eq!(pause.render_targets, vec![RenderTarget::All]);
        let volume = runtime.handle(
            RuntimeEvent::Command(AudioCommand::SetVolume(37).into()),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.state().player.volume(), 37);
        assert!(volume.changes.intersects(StateChangeSet::PLAYER));
        assert_eq!(volume.render_targets, vec![RenderTarget::All]);
        assert_eq!(
            volume.frontend_effects,
            vec![
                FrontendEffect::Platform(PlatformEffect::SetOutputVolume(37)),
                FrontendEffect::Platform(PlatformEffect::SaveConfig)
            ]
        );
        assert!(volume.force_persistence);
    }

    #[test]
    fn stop_fade_ticks_volume_then_stops_once_and_restores_volume() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.playback.tick_visualization(Some(&[0.5]), 1);
        runtime.store_mut().state_mut().player.set_volume(80);
        let begin = runtime.handle(RuntimeEvent::BeginStopFade, PlaybackExecution::Local);
        assert!(begin.messages.is_empty());
        assert!(runtime.stop_fade_active());
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(runtime.visualization_render_state().data[0], 0.5);

        runtime.tick_stop_fade(500);
        assert_eq!(runtime.state().player.volume(), 40);
        assert_eq!(*calls.lock().unwrap(), vec!["volume:40"]);
        runtime.tick_stop_fade(500);
        assert!(!runtime.stop_fade_active());
        assert_eq!(runtime.state().player.volume(), 80);
        assert_eq!(runtime.visualization_render_state().data[0], 0.0);
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["volume:40", "stop", "volume:80"]
        );
        runtime.tick_stop_fade(500);
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["volume:40", "stop", "volume:80"]
        );

        runtime.handle(RuntimeEvent::StopBackend, PlaybackExecution::Local);
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["volume:40", "stop", "volume:80", "stop"]
        );
    }

    #[test]
    fn seek_and_eof_wait_cancel_an_in_progress_stop_fade() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.store_mut().state_mut().player.mark_playing();
        runtime.store_mut().state_mut().player.set_volume(80);
        runtime.handle(RuntimeEvent::BeginStopFade, PlaybackExecution::Local);
        runtime.tick_stop_fade(500);
        runtime.handle(
            RuntimeEvent::Command(PlayerCommand::SeekToMs(2_500).into()),
            PlaybackExecution::Local,
        );
        assert!(!runtime.stop_fade_active());
        runtime.tick_stop_fade(500);
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["volume:40", "seek:2500"],
            "a superseded fade must not stop playback"
        );

        let (mut runtime, calls) = recording_runtime(false);
        {
            let state = runtime.store_mut().state_mut();
            state.player.mark_playing();
            state.player.set_volume(80);
            state.config.pause_between_songs = true;
            state.config.pause_between_songs_time = 2;
            state.playlist.add_uri("one");
            state.playlist.add_uri("two");
            state.playlist.set_position(0);
        }
        runtime.handle(RuntimeEvent::BeginStopFade, PlaybackExecution::Local);
        runtime.tick_stop_fade(500);
        runtime.handle(RuntimeEvent::PlaylistEof, PlaybackExecution::Local);
        assert!(!runtime.stop_fade_active());
        assert_eq!(runtime.transition().wait_remaining(), Some(2_000));
        runtime.tick_stop_fade(500);
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["volume:40"],
            "fade completion must not stop or cancel pending EOF advancement"
        );
        runtime.handle(
            RuntimeEvent::TransitionTick(2_000),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.state().playlist.position(), Some(1));
        assert_eq!(runtime.transition(), PlaybackTransition::Idle);
    }

    #[test]
    fn eof_delay_depends_on_backend_execution_ownership() {
        for (execution, pause_seconds, waits) in [
            (PlaybackExecution::Local, 2, true),
            (PlaybackExecution::Local, 0, false),
            (PlaybackExecution::AlreadyExecuted, 2, false),
        ] {
            let (mut runtime, calls) = recording_runtime(false);
            runtime.store_mut().state_mut().config.pause_between_songs = true;
            runtime
                .store_mut()
                .state_mut()
                .config
                .pause_between_songs_time = pause_seconds;
            runtime.handle(
                RuntimeEvent::Command(
                    PlaylistCommand::AddUris(vec!["one".into(), "two".into()]).into(),
                ),
                PlaybackExecution::Local,
            );
            runtime.store_mut().state_mut().playlist.set_position(0);
            runtime.handle(
                RuntimeEvent::PlaybackPosition(4_000),
                PlaybackExecution::Local,
            );
            let update = runtime.handle(RuntimeEvent::PlaylistEof, execution);
            assert_eq!(runtime.state().config.playback_position_ms, 0);
            assert_eq!(
                runtime.transition().wait_remaining(),
                waits.then_some(2_000)
            );
            assert_eq!(
                runtime.state().playlist.position(),
                Some(if waits { 0 } else { 1 })
            );
            let start = if waits {
                assert!(update.transition_changed);
                assert!(update.pending_playback_starts.is_empty());
                assert!(
                    runtime
                        .handle(
                            RuntimeEvent::TransitionTick(1_000),
                            PlaybackExecution::Local
                        )
                        .transition_changed
                );
                assert_eq!(runtime.transition().wait_remaining(), Some(1_000));
                runtime
                    .handle(
                        RuntimeEvent::TransitionTick(1_000),
                        PlaybackExecution::Local,
                    )
                    .pending_playback_starts[0]
                    .clone()
            } else {
                update.pending_playback_starts[0].clone()
            };
            assert_eq!(start.uri, "two");
            assert!(
                calls.lock().unwrap().is_empty(),
                "start must be prepared first; externally executed EOF must not echo the backend"
            );
            runtime.handle(RuntimeEvent::PlaybackStartPrepared(start), execution);
            if execution == PlaybackExecution::Local {
                assert_eq!(*calls.lock().unwrap(), vec!["play:two"]);
            } else {
                assert!(calls.lock().unwrap().is_empty());
            }
        }
    }

    #[test]
    fn delayed_eof_uses_playlist_advance_policy() {
        for (repeat, shuffle, no_advance) in [
            (false, false, true),
            (false, false, false),
            (true, false, false),
            (false, true, false),
        ] {
            let (mut runtime, _) = recording_runtime(false);
            let state = runtime.store_mut().state_mut();
            state.config.pause_between_songs = true;
            state.config.pause_between_songs_time = 1;
            for uri in ["one", "two", "three"] {
                state.playlist.add_uri(uri);
            }
            state.playlist.set_repeat(repeat);
            state.playlist.set_shuffle(shuffle);
            state.playlist.set_no_advance(no_advance);
            state.playlist.set_position(if repeat { 2 } else { 0 });
            let mut expected = state.playlist.clone();
            let should_start = expected.eof_reached();
            runtime.handle(RuntimeEvent::PlaylistEof, PlaybackExecution::Local);
            let update = runtime.handle(
                RuntimeEvent::TransitionTick(1_000),
                PlaybackExecution::Local,
            );
            assert_eq!(runtime.state().playlist.position(), expected.position());
            assert_eq!(!update.pending_playback_starts.is_empty(), should_start);
            assert_eq!(runtime.transition(), PlaybackTransition::Idle);
        }
    }

    #[test]
    fn halt_cancels_eof_wait_without_starting_another_track() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.store_mut().state_mut().config.pause_between_songs = true;
        runtime
            .store_mut()
            .state_mut()
            .config
            .pause_between_songs_time = 2;
        runtime.handle(
            RuntimeEvent::Command(
                PlaylistCommand::AddUris(vec!["one".into(), "two".into()]).into(),
            ),
            PlaybackExecution::Local,
        );
        runtime.store_mut().state_mut().playlist.set_position(0);
        runtime.handle(RuntimeEvent::PlaylistEof, PlaybackExecution::Local);
        assert_eq!(runtime.transition().wait_remaining(), Some(2_000));
        runtime.handle(
            RuntimeEvent::Command(PlayerCommand::Halt.into()),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.transition(), PlaybackTransition::Idle);
        assert_eq!(runtime.state().player.state(), PlayerState::Stopped);
        assert!(runtime
            .handle(
                RuntimeEvent::TransitionTick(2_000),
                PlaybackExecution::Local
            )
            .pending_playback_starts
            .is_empty());
        assert_eq!(runtime.state().playlist.position(), Some(0));
        assert_eq!(*calls.lock().unwrap(), vec!["stop"]);
    }

    #[test]
    fn stopped_seek_is_retained_for_play_and_halt_returns_to_idle() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::AddUris(vec!["song".into()]).into()),
            PlaybackExecution::Local,
        );
        runtime.handle(
            RuntimeEvent::Command(PlayerCommand::SeekToMs(5_000).into()),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.transition(), PlaybackTransition::StoppedAt(5_000));
        let start = runtime
            .handle(
                RuntimeEvent::Command(PlayerCommand::Play.into()),
                PlaybackExecution::Local,
            )
            .pending_playback_starts[0]
            .clone();
        assert_eq!(start.position_ms, 5_000);
        runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.transition(), PlaybackTransition::PendingSeek(5_000));
        runtime.handle(
            RuntimeEvent::Command(PlayerCommand::Halt.into()),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.transition(), PlaybackTransition::Idle);
        assert!(!calls.lock().unwrap().is_empty());
    }

    #[test]
    fn externally_executed_seek_waits_for_confirmation_without_backend_echo() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.store_mut().state_mut().player.mark_playing();
        runtime.handle(
            RuntimeEvent::Command(PlayerCommand::SeekToMs(5_000).into()),
            PlaybackExecution::AlreadyExecuted,
        );
        assert_eq!(
            runtime.transition(),
            PlaybackTransition::AwaitingSeek {
                target_ms: 5_000,
                previous_ms: None,
                elapsed_ms: 0
            }
        );
        assert!(runtime.pending_seek_ms().is_none());
        assert!(!runtime.apply_pending_start_seek(true).transition_changed);
        assert!(calls.lock().unwrap().is_empty());
        runtime.handle(RuntimeEvent::BackendPosition(0), PlaybackExecution::Local);
        assert_eq!(runtime.state().config.playback_position_ms, 5_000);
        runtime.handle(
            RuntimeEvent::BackendPosition(5_100),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.transition(), PlaybackTransition::Idle);
        assert_eq!(runtime.state().config.playback_position_ms, 5_100);
    }

    #[test]
    fn pending_seek_requires_confirmation_and_stale_samples_cannot_undo_it() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.request_backend_seek(5_000);
        assert!(runtime
            .handle(RuntimeEvent::BackendPosition(100), PlaybackExecution::Local)
            .changes
            .is_empty());
        assert!(runtime.apply_pending_start_seek(true).transition_changed);
        assert_eq!(
            runtime.transition(),
            PlaybackTransition::AwaitingSeek {
                target_ms: 5_000,
                previous_ms: None,
                elapsed_ms: 0
            }
        );
        assert!(runtime
            .handle(RuntimeEvent::BackendPosition(100), PlaybackExecution::Local)
            .changes
            .is_empty());
        assert_eq!(runtime.state().config.playback_position_ms, 0);
        runtime.handle(
            RuntimeEvent::BackendPosition(4_900),
            PlaybackExecution::Local,
        );
        assert_eq!(runtime.state().config.playback_position_ms, 5_000);
        assert_eq!(runtime.transition(), PlaybackTransition::Idle);
        assert_eq!(*calls.lock().unwrap(), vec!["seek:5000"]);
    }

    #[test]
    fn duration_batches_pass_through_the_same_update_boundary() {
        let mut runtime = FrontendRuntime::new(AppState::default(), None);
        runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::AddUris(vec!["file:///song.ogg".into()]).into()),
            PlaybackExecution::Local,
        );
        runtime.enqueue_duration_batch(vec![DurationIndexResult {
            index: 0,
            uri: "file:///song.ogg".into(),
            length_ms: 42_000,
            title: Some("Indexed".into()),
        }]);
        let update = runtime.drain_duration_updates();
        assert!(update.changes.intersects(StateChangeSet::PLAYLIST));
        assert!(update.force_persistence);
        assert_eq!(update.render_targets, vec![RenderTarget::Playlist]);
        assert_eq!(runtime.state().playlist.entries()[0].length_ms, 42_000);
        assert!(runtime.drain_duration_updates().changes.is_empty());
    }

    struct IndexedDuration;

    impl AudioMetadataProbe for IndexedDuration {
        fn probe(&self, item: &DurationIndexItem) -> Result<Option<DurationIndexResult>, String> {
            Ok(Some(DurationIndexResult {
                index: item.index,
                uri: item.uri.clone(),
                length_ms: 42_000,
                title: None,
            }))
        }
    }

    #[test]
    fn runtime_owned_duration_indexer_wakes_and_drains_the_same_batch() {
        let path = std::env::temp_dir().join(format!(
            "xmms-runtime-duration-wakeup-{}-{:?}.wav",
            std::process::id(),
            std::thread::current().id(),
        ));
        std::fs::write(&path, b"test").unwrap();
        let mut state = AppState::default();
        state.playlist.add_path(&path);
        let (sender, receiver) = mpsc::channel();
        let mut runtime = FrontendRuntime::new_with_duration_wakeup(state, None, move || {
            sender.send(()).unwrap();
        });
        runtime
            .duration_indexer
            .schedule_with_probe(&runtime.state().playlist, IndexedDuration);
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        let update = runtime.drain_duration_updates();
        assert!(update.changes.intersects(StateChangeSet::PLAYLIST));
        assert_eq!(runtime.state().playlist.entries()[0].length_ms, 42_000);
        assert!(runtime.drain_duration_updates().changes.is_empty());
        assert!(receiver.try_recv().is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn externally_stopped_fade_completes_without_backend_echo() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.playback.tick_visualization(Some(&[0.5]), 1);
        runtime.store_mut().state_mut().player.set_volume(60);
        runtime.handle(
            RuntimeEvent::BeginStopFade,
            PlaybackExecution::AlreadyExecuted,
        );
        assert!(!runtime.stop_fade_active());
        assert_eq!(runtime.state().player.volume(), 60);
        assert_eq!(runtime.visualization_render_state().data[0], 0.0);
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn playback_start_requires_preparation_and_executes_once() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::AddUris(vec!["song".into()]).into()),
            PlaybackExecution::Local,
        );
        let mut update = runtime.handle(
            RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
            PlaybackExecution::Local,
        );
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(update.pending_playback_starts.len(), 1);
        let start = update.pending_playback_starts.pop().unwrap();
        runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start.clone()),
            PlaybackExecution::AlreadyExecuted,
        );
        assert!(calls.lock().unwrap().is_empty());
        let duplicate = runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start.clone()),
            PlaybackExecution::Local,
        );
        assert!(duplicate.changes.is_empty());
        assert!(calls.lock().unwrap().is_empty());

        let update = runtime.handle(
            RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
            PlaybackExecution::Local,
        );
        let next_start = update.pending_playback_starts[0].clone();
        assert_ne!(start, next_start);
        runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start),
            PlaybackExecution::Local,
        );
        assert!(calls.lock().unwrap().is_empty());
        let play = runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(next_start),
            PlaybackExecution::Local,
        );
        assert!(play.messages.is_empty());
        assert_eq!(*calls.lock().unwrap(), vec!["play:song"]);
        let playlist = runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::ToggleRepeat.into()),
            PlaybackExecution::Local,
        );
        assert!(playlist.changes.intersects(StateChangeSet::PLAYLIST));
        assert_eq!(playlist.render_targets, vec![RenderTarget::Playlist]);
    }

    #[test]
    fn halt_or_clear_expires_unprepared_playback_start() {
        for superseding_command in [PlayerCommand::Halt.into(), PlaylistCommand::Clear.into()] {
            let (mut runtime, calls) = recording_runtime(false);
            runtime.handle(
                RuntimeEvent::Command(PlaylistCommand::AddUris(vec!["song".into()]).into()),
                PlaybackExecution::Local,
            );
            let start = runtime
                .handle(
                    RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
                    PlaybackExecution::Local,
                )
                .pending_playback_starts
                .pop()
                .unwrap();
            runtime.handle(
                RuntimeEvent::Command(superseding_command),
                PlaybackExecution::Local,
            );
            assert_eq!(runtime.state().player.state(), PlayerState::Stopped);
            assert_eq!(*calls.lock().unwrap(), vec!["stop"]);
            let prepared = runtime.handle(
                RuntimeEvent::PlaybackStartPrepared(start),
                PlaybackExecution::Local,
            );
            assert!(prepared.playback_effects.is_empty());
            assert_eq!(*calls.lock().unwrap(), vec!["stop"]);
        }
    }

    #[test]
    fn changing_selected_track_expires_unprepared_start_even_with_same_uri() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.handle(
            RuntimeEvent::Command(
                PlaylistCommand::AddUris(vec!["song".into(), "song".into()]).into(),
            ),
            PlaybackExecution::Local,
        );
        let start = runtime
            .handle(
                RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
                PlaybackExecution::Local,
            )
            .pending_playback_starts
            .pop()
            .unwrap();
        runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::SetPosition(1).into()),
            PlaybackExecution::Local,
        );
        runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start),
            PlaybackExecution::Local,
        );
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn replacing_playlist_expires_unprepared_playback_start() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::AddUris(vec!["song".into()]).into()),
            PlaybackExecution::Local,
        );
        let start = runtime
            .handle(
                RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
                PlaybackExecution::Local,
            )
            .pending_playback_starts
            .pop()
            .unwrap();
        runtime.handle(
            RuntimeEvent::ReplacePlaylist(Playlist::default()),
            PlaybackExecution::Local,
        );
        runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start),
            PlaybackExecution::Local,
        );
        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn cancelled_playback_start_cannot_be_replayed() {
        let (mut runtime, calls) = recording_runtime(false);
        runtime.handle(
            RuntimeEvent::Command(PlaylistCommand::AddUris(vec!["song".into()]).into()),
            PlaybackExecution::Local,
        );
        let start = runtime
            .handle(
                RuntimeEvent::Command(PlayerCommand::StartCurrentTrack.into()),
                PlaybackExecution::Local,
            )
            .pending_playback_starts
            .pop()
            .unwrap();
        runtime.handle(
            RuntimeEvent::PlaybackStartCancelled(start.clone()),
            PlaybackExecution::Local,
        );
        runtime.handle(
            RuntimeEvent::PlaybackStartPrepared(start),
            PlaybackExecution::Local,
        );
        assert!(calls.lock().unwrap().is_empty());
    }
}
