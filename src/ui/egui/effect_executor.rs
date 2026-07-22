//! Focused egui effect ownership and platform-policy execution.

use std::path::PathBuf;

use crate::app::effect::{AppEffect, FileDialogRequest, RenderTarget};
#[cfg(target_os = "android")]
use crate::app::store::StateChangeSet;
#[cfg(target_os = "android")]
use crate::app::view_model::{formatted_current_title, formatted_playlist_entry_title};
#[cfg(target_os = "android")]
use crate::app_state::AppState;
#[cfg(target_os = "android")]
use crate::playback::model::PlayerState;

#[cfg(target_os = "android")]
use super::android_runtime::AndroidRuntime;
use super::playback_runtime::PlaybackRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EffectExecution {
    pub playback_backend: bool,
}

impl EffectExecution {
    pub const LOCAL: Self = Self {
        playback_backend: true,
    };

    pub fn after_external_backend_execution(backend_executed: bool) -> Self {
        Self {
            playback_backend: !backend_executed,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PlaybackEffect {
    Start,
    StartFromCurrent,
    StartUri { uri: String, position_ms: i64 },
    Resume,
    Pause,
    Stop,
    BeginStopFade { start_volume: i32 },
    Seek(i64),
    SetBackendVolume(i32),
    SetBackendBalance(i32),
    SetBackendEqualizer,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UiEffect {
    QueueRender(RenderTarget),
    OpenFileDialog(FileDialogRequest),
    OpenPath(PathBuf),
    OpenFileInfoDialog,
    OpenPreferences,
    OpenSkinBrowser,
    OpenSkinEditor,
    ShowError(String),
    ShowMessage(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PlatformEffect {
    SetOutputVolume(i32),
    SaveConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EffectOwner {
    Playback(PlaybackEffect),
    Ui(UiEffect),
    Platform(PlatformEffect),
}

pub(crate) fn owner(effect: AppEffect) -> EffectOwner {
    match effect {
        AppEffect::StartPlayback => EffectOwner::Playback(PlaybackEffect::Start),
        AppEffect::StartPlaybackFromCurrent => {
            EffectOwner::Playback(PlaybackEffect::StartFromCurrent)
        }
        AppEffect::StartPlaybackUri { uri, position_ms } => {
            EffectOwner::Playback(PlaybackEffect::StartUri { uri, position_ms })
        }
        AppEffect::ResumePlayback => EffectOwner::Playback(PlaybackEffect::Resume),
        AppEffect::PausePlayback => EffectOwner::Playback(PlaybackEffect::Pause),
        AppEffect::StopPlayback => EffectOwner::Playback(PlaybackEffect::Stop),
        AppEffect::BeginStopFade { start_volume } => {
            EffectOwner::Playback(PlaybackEffect::BeginStopFade { start_volume })
        }
        AppEffect::SeekPlayback(position_ms) => {
            EffectOwner::Playback(PlaybackEffect::Seek(position_ms))
        }
        AppEffect::SetBackendVolume(volume) => {
            EffectOwner::Playback(PlaybackEffect::SetBackendVolume(volume))
        }
        AppEffect::SetBackendBalance(balance) => {
            EffectOwner::Playback(PlaybackEffect::SetBackendBalance(balance))
        }
        AppEffect::SetBackendEqualizer => {
            EffectOwner::Playback(PlaybackEffect::SetBackendEqualizer)
        }
        AppEffect::SetOutputVolume(volume) => {
            EffectOwner::Platform(PlatformEffect::SetOutputVolume(volume))
        }
        AppEffect::SaveConfig => EffectOwner::Platform(PlatformEffect::SaveConfig),
        AppEffect::QueueRender(target) => EffectOwner::Ui(UiEffect::QueueRender(target)),
        AppEffect::OpenFileDialog(request) => EffectOwner::Ui(UiEffect::OpenFileDialog(request)),
        AppEffect::OpenPath(path) => EffectOwner::Ui(UiEffect::OpenPath(path)),
        AppEffect::OpenFileInfoDialog => EffectOwner::Ui(UiEffect::OpenFileInfoDialog),
        AppEffect::OpenPreferences => EffectOwner::Ui(UiEffect::OpenPreferences),
        AppEffect::OpenSkinBrowser => EffectOwner::Ui(UiEffect::OpenSkinBrowser),
        AppEffect::OpenSkinEditor => EffectOwner::Ui(UiEffect::OpenSkinEditor),
        AppEffect::ShowError(message) => EffectOwner::Ui(UiEffect::ShowError(message)),
        AppEffect::ShowMessage(message) => EffectOwner::Ui(UiEffect::ShowMessage(message)),
    }
}

pub(crate) fn execute_platform_effect(
    effect: PlatformEffect,
    playback: &mut PlaybackRuntime,
    pending_messages: &mut Vec<String>,
    #[cfg(target_os = "android")] android: &mut AndroidRuntime,
) {
    #[cfg(target_os = "android")]
    let _ = playback;
    match effect {
        PlatformEffect::SetOutputVolume(volume) => {
            #[cfg(target_os = "android")]
            if let Err(error) = super::android::set_media_volume_percent(volume) {
                pending_messages.push(error);
            }
            #[cfg(not(target_os = "android"))]
            if let Some(error) = playback.set_output_volume(volume) {
                pending_messages.push(error);
            }
        }
        PlatformEffect::SaveConfig => {
            #[cfg(target_os = "android")]
            android.mark_persistence();
        }
    }
}

#[cfg(target_os = "android")]
pub(crate) fn apply_android_post_dispatch(android: &mut AndroidRuntime, changes: StateChangeSet) {
    let persistent_changes = StateChangeSet::PLAYER
        | StateChangeSet::PLAYLIST
        | StateChangeSet::EQUALIZER
        | StateChangeSet::PANELS
        | StateChangeSet::SKIN
        | StateChangeSet::CONFIG;
    if changes.intersects(persistent_changes) {
        android.mark_persistence();
    }
    if changes.intersects(StateChangeSet::PLAYER | StateChangeSet::PLAYLIST) {
        android.mark_media_projection();
    }
}

#[cfg(target_os = "android")]
pub(crate) fn flush_android_persistence(
    android: &mut AndroidRuntime,
    playback: &PlaybackRuntime,
    state: &AppState,
    pending_messages: &mut Vec<String>,
    force: bool,
) {
    if !super::android::is_foreground_activity(android.activity_generation()) {
        return;
    }
    if !android.take_persistence_due(force) {
        return;
    }
    let playback_position_ms = (state.player.state() != PlayerState::Stopped)
        .then(|| playback.position_ms())
        .flatten();
    if let Err(error) = super::android::persist_app_state(state, playback_position_ms) {
        android.mark_persistence();
        crate::app_log_error!(frontend, "failed to save Android session state", error);
        let message = format!("failed to save Android session state: {error}");
        if !pending_messages.contains(&message) {
            pending_messages.push(message);
        }
    }
}

#[cfg(target_os = "android")]
pub(crate) fn flush_android_media_projection(
    android: &mut AndroidRuntime,
    playback: &PlaybackRuntime,
    state: &AppState,
    pending_messages: &mut Vec<String>,
) {
    if !android.take_media_projection_pending() {
        return;
    }
    let activity_generation = android.activity_generation();
    if !super::android::is_current_activity(activity_generation) {
        android.mark_media_projection();
        return;
    }
    if android.playlist_changed(&state.playlist) {
        let titles = state
            .playlist
            .entries()
            .iter()
            .map(|entry| formatted_playlist_entry_title(state, entry))
            .collect();
        if !super::android::sync_media_playlist(activity_generation, &state.playlist, titles) {
            android.mark_media_projection();
            return;
        }
        android.remember_playlist(state.playlist.clone());
    }
    let playback_state = super::android::AndroidPlaybackState::from(state.player.state());
    let position_ms = playback
        .position_ms()
        .unwrap_or(state.config.playback_position_ms)
        .max(0);
    let has_entries = !state.playlist.is_empty();
    let current_index = state.playlist.position().map_or(-1, |index| index as i64);
    let playlist_len = state.playlist.len().min(i32::MAX as usize) as i32;
    match super::android::update_playback_notification(
        activity_generation,
        playback_state,
        &formatted_current_title(state),
        state.player.bitrate(),
        state.player.frequency(),
        state.player.channels(),
        state.player.duration_ms().unwrap_or(-1),
        position_ms,
        current_index,
        playlist_len,
        has_entries,
        has_entries,
    ) {
        Ok(true) => {}
        Ok(false) => android.mark_media_projection(),
        Err(error) => pending_messages.push(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_effect_has_one_focused_owner() {
        assert!(matches!(
            owner(AppEffect::PausePlayback),
            EffectOwner::Playback(PlaybackEffect::Pause)
        ));
        assert!(matches!(
            owner(AppEffect::QueueRender(RenderTarget::Main)),
            EffectOwner::Ui(UiEffect::QueueRender(RenderTarget::Main))
        ));
        assert!(matches!(
            owner(AppEffect::SaveConfig),
            EffectOwner::Platform(PlatformEffect::SaveConfig)
        ));
    }
}
