//! Playback-service and media-browser boundary.
//!
//! `MEDIA_PLAYLIST` is a one-way projection while egui is active and the
//! authoritative fallback playlist while the Activity is absent.
//! `PLAYBACK_BACKEND` is process-wide so Activity and service callbacks use one
//! backend. Service transport JNI executes that shared backend immediately,
//! including while the Activity is paused, and tags the queued event with
//! `backend_executed` so egui only applies the matching domain transition.
//! Media-browser JNI queries are synchronous read-only endpoints required by
//! `MediaBrowserService`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use jni::objects::{JObject, JValue};
use jni::JNIEnv;

use crate::playback::backend::PlaybackBackend;
use crate::playback::model::{PlaybackEvent, PlayerState};
use crate::playback::rodio::RodioBackend;
use crate::playlist::Playlist;
use crate::session::{fallback_state_paths, load_saved_state};

use super::activity;
use super::events::{
    self, AndroidMediaControl, AndroidMediaControlEvent, AndroidPlatformEvent, AndroidPlaybackState,
};
use super::persistence;

#[derive(Debug, Clone, PartialEq, Eq)]
struct AndroidMediaNotification {
    state: AndroidPlaybackState,
    title: String,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    duration_ms: i64,
    current_index: i64,
    playlist_len: i32,
    has_previous: bool,
    has_next: bool,
}

#[derive(Clone)]
struct AndroidMediaPlaylist {
    playlist: Playlist,
    titles: Vec<String>,
}

static MEDIA_NOTIFICATION: OnceLock<Mutex<Option<AndroidMediaNotification>>> = OnceLock::new();
static MEDIA_PLAYLIST: OnceLock<Mutex<Option<AndroidMediaPlaylist>>> = OnceLock::new();
static PLAYBACK_BACKEND: OnceLock<Mutex<Option<RodioBackend>>> = OnceLock::new();

pub(crate) fn reset_notification() {
    *MEDIA_NOTIFICATION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = None;
}

pub fn shared_playback_backend() -> Result<RodioBackend, String> {
    let mut backend = PLAYBACK_BACKEND
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(backend) = backend.as_ref() {
        return Ok(backend.clone());
    }
    let created = RodioBackend::new()?;
    *backend = Some(created.clone());
    Ok(created)
}

pub fn sync_media_playlist(playlist: &Playlist, titles: Vec<String>) {
    *MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(AndroidMediaPlaylist {
        playlist: playlist.clone(),
        titles,
    });
}

#[allow(clippy::too_many_arguments)]
pub fn update_playback_notification(
    state: AndroidPlaybackState,
    title: &str,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    duration_ms: i64,
    position_ms: i64,
    current_index: i64,
    playlist_len: i32,
    has_previous: bool,
    has_next: bool,
) -> Result<(), String> {
    let notification = AndroidMediaNotification {
        state,
        title: title.to_string(),
        bitrate,
        frequency,
        channels,
        duration_ms,
        current_index,
        playlist_len,
        has_previous,
        has_next,
    };
    let mut previous = MEDIA_NOTIFICATION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if previous.as_ref() == Some(&notification) {
        return Ok(());
    }

    let context = activity::context()?;
    let context = context
        .as_ref()
        .ok_or_else(|| "Android activity is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android media-control thread: {err}"))?;
    let title = env
        .new_string(title)
        .map_err(|err| format!("failed to create Android media title: {err}"))?;
    env.call_method(
        context.activity.as_obj(),
        "updatePlaybackNotification",
        "(ILjava/lang/String;IIIJJJIZZ)V",
        &[
            JValue::Int(state as i32),
            JValue::Object(&title),
            JValue::Int(bitrate),
            JValue::Int(frequency),
            JValue::Int(channels),
            JValue::Long(duration_ms),
            JValue::Long(position_ms),
            JValue::Long(current_index),
            JValue::Int(playlist_len),
            JValue::Bool(has_previous.into()),
            JValue::Bool(has_next.into()),
        ],
    )
    .map_err(|err| format!("failed to update Android playback notification: {err}"))?;
    *previous = Some(notification);
    Ok(())
}

pub fn complete_media_control() -> Result<(), String> {
    let context = activity::context()?;
    let context = context
        .as_ref()
        .ok_or_else(|| "Android activity is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android media-control thread: {err}"))?;
    env.call_method(
        context.activity.as_obj(),
        "completeMediaControl",
        "()V",
        &[],
    )
    .map_err(|err| format!("failed to complete Android media control: {err}"))?;
    Ok(())
}

pub(crate) fn initialize_media_library(files_dir: PathBuf, cache_dir: PathBuf) {
    std::env::set_var("XMMS_RS_CONFIG_DIR", files_dir.join("config"));
    std::env::set_var("XMMS_RS_CACHE_DIR", cache_dir);
    if MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .is_some()
    {
        return;
    }

    let (config_path, playlist_path) = fallback_state_paths(&files_dir.join("config"));
    match load_saved_state(&config_path, &playlist_path, false) {
        Ok(state) => {
            let titles = state
                .playlist
                .entries()
                .iter()
                .map(|entry| media_item_title(&entry.title, &entry.filename))
                .collect();
            sync_media_playlist(&state.playlist, titles);
        }
        Err(err) => eprintln!("xmms-rs: failed to load Android Auto media library: {err}"),
    }
}

pub(crate) fn media_item_count() -> i32 {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .map_or(0, |shared| {
            shared.playlist.len().min(i32::MAX as usize) as i32
        })
}

pub(crate) fn media_item_title_at(index: usize) -> Option<String> {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .and_then(|shared| {
            let entry = shared.playlist.entries().get(index)?;
            Some(
                shared
                    .titles
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| media_item_title(&entry.title, &entry.filename)),
            )
        })
}

pub(crate) fn media_item_duration_ms(index: usize) -> Option<i64> {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .and_then(|shared| shared.playlist.entries().get(index))
        .map(|entry| entry.length_ms)
}

pub(crate) fn current_media_item_index() -> Option<usize> {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .and_then(|shared| shared.playlist.position())
}

pub(crate) fn poll_playback(env: &mut JNIEnv<'_>, service: &JObject<'_>) {
    let Ok(backend) = shared_playback_backend() else {
        return;
    };
    let Ok(playback_events) = backend.poll_events() else {
        return;
    };
    let mut eof = false;
    let mut refresh_state = false;
    let mut queued = Vec::new();
    for event in playback_events {
        if matches!(event, PlaybackEvent::EndOfStream) {
            eof = true;
        } else {
            refresh_state |= matches!(
                event,
                PlaybackEvent::DurationChanged(_)
                    | PlaybackEvent::StreamInfo(_)
                    | PlaybackEvent::AsyncDone
            );
            queued.push(AndroidPlatformEvent::Playback(event));
        }
    }
    events::push_all(queued);
    if eof {
        if let Err(err) = advance_after_end_of_stream(&backend) {
            eprintln!("xmms-rs: Android background playlist advance failed: {err}");
            let _ = backend.stop();
        }
        events::push(AndroidPlatformEvent::MediaControl(
            AndroidMediaControlEvent {
                control: AndroidMediaControl::PlaylistEof,
                backend_executed: true,
            },
        ));
        update_service_from_backend(env, service);
    } else if refresh_state {
        update_service_from_backend(env, service);
    }
    persistence::checkpoint_playback_position(&backend, current_media_item_index);
    update_service_position_from_backend(env, service, &backend);
}

pub(crate) fn handle_media_control(
    mut env: JNIEnv<'_>,
    service: Option<JObject<'_>>,
    control: AndroidMediaControl,
) {
    let _order = events::lock_media_control_order();
    if let Err(err) = execute_android_media_control(control) {
        eprintln!("xmms-rs: Android media control failed: {err}");
        return;
    }
    events::push(AndroidPlatformEvent::MediaControl(
        AndroidMediaControlEvent {
            control,
            backend_executed: true,
        },
    ));
    if let Some(service) = service {
        update_service_from_backend(&mut env, &service);
    }
    events::request_registered_repaint();
}

fn execute_android_media_control(control: AndroidMediaControl) -> Result<(), String> {
    let backend = shared_playback_backend()?;
    match control {
        AndroidMediaControl::PausePlayback => backend.pause(),
        AndroidMediaControl::ResumePlayback => {
            if backend.state() == PlayerState::Paused {
                backend.unpause()
            } else if backend.state() == PlayerState::Stopped {
                let uri = current_media_entry()
                    .map(|(uri, _, _)| uri)
                    .ok_or_else(|| "no current playlist entry to resume".to_string())?;
                backend.play_uri(&uri)
            } else {
                Ok(())
            }
        }
        AndroidMediaControl::NextTrack => change_media_track(true, &backend),
        AndroidMediaControl::PreviousTrack => change_media_track(false, &backend),
        AndroidMediaControl::SeekToMs(position_ms) => backend.seek(position_ms),
        AndroidMediaControl::PlayMediaItem(index) => play_media_item(index, &backend),
        AndroidMediaControl::StopPlayback => backend.stop(),
        AndroidMediaControl::PlaylistEof => Ok(()),
    }
}

fn advance_after_end_of_stream(backend: &RodioBackend) -> Result<(), String> {
    let mut shared = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let Some(current) = shared.as_ref().cloned() else {
        return backend.stop();
    };
    let mut updated = current;
    if !updated.playlist.eof_reached() {
        return backend.stop();
    }
    let position = updated
        .playlist
        .position()
        .ok_or_else(|| "Android media playlist has no entry after EOF".to_string())?;
    let uri = updated.playlist.entries()[position].filename.clone();
    backend.play_uri(&uri)?;
    *shared = Some(updated);
    Ok(())
}

fn change_media_track(next: bool, backend: &RodioBackend) -> Result<(), String> {
    let mut shared = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let current = shared
        .as_ref()
        .cloned()
        .ok_or_else(|| "Android media playlist is unavailable".to_string())?;
    let mut updated = current;
    let advanced = if next {
        updated.playlist.advance()
    } else {
        updated.playlist.previous()
    };
    if !advanced {
        return backend.seek(0);
    }
    let position = updated
        .playlist
        .position()
        .ok_or_else(|| "Android media playlist has no current entry".to_string())?;
    let uri = updated.playlist.entries()[position].filename.clone();
    backend.play_uri(&uri)?;
    *shared = Some(updated);
    Ok(())
}

fn play_media_item(index: usize, backend: &RodioBackend) -> Result<(), String> {
    let mut shared = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let current = shared
        .as_ref()
        .cloned()
        .ok_or_else(|| "Android media playlist is unavailable".to_string())?;
    let mut updated = current;
    let uri = updated
        .playlist
        .entries()
        .get(index)
        .map(|entry| entry.filename.clone())
        .ok_or_else(|| format!("Android media item index {index} is out of range"))?;
    updated.playlist.set_position(index);
    backend.play_uri(&uri)?;
    *shared = Some(updated);
    Ok(())
}

fn media_item_title(title: &str, filename: &str) -> String {
    if !title.trim().is_empty() {
        return title.to_string();
    }
    Path::new(filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Unknown track")
        .to_string()
}

fn current_media_entry() -> Option<(String, String, i64)> {
    let shared = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let shared = shared.as_ref()?;
    let position = shared.playlist.position()?;
    let entry = shared.playlist.entries().get(position)?;
    let title = shared
        .titles
        .get(position)
        .cloned()
        .unwrap_or_else(|| entry.title.clone());
    Some((entry.filename.clone(), title, entry.length_ms))
}

fn update_service_position_from_backend(
    env: &mut JNIEnv<'_>,
    service: &JObject<'_>,
    backend: &RodioBackend,
) {
    let Some(position_ms) = backend.position_ms().map(|position| position.max(0)) else {
        return;
    };
    let _ = env.call_method(
        service,
        "applyNativePlaybackPosition",
        "(J)V",
        &[JValue::Long(position_ms)],
    );
}

fn update_service_from_backend(env: &mut JNIEnv<'_>, service: &JObject<'_>) {
    let Ok(backend) = shared_playback_backend() else {
        return;
    };
    let state = AndroidPlaybackState::from(backend.state());
    let (_, title, entry_duration_ms) = current_media_entry().unwrap_or_else(|| {
        (
            String::new(),
            "XMMS Renascene".to_string(),
            backend.duration_ms().unwrap_or(-1),
        )
    });
    let duration_ms = (entry_duration_ms >= 0)
        .then_some(entry_duration_ms)
        .or_else(|| backend.duration_ms())
        .unwrap_or(-1);
    let position_ms = backend.position_ms().unwrap_or(0).max(0);
    let stream_info = backend.stream_info();
    let (current_index, playlist_len) = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .map_or((-1, 0), |shared| {
            (
                shared.playlist.position().map_or(-1, |index| index as i64),
                shared.playlist.len().min(i32::MAX as usize) as i32,
            )
        });
    let has_entries = playlist_len > 0;
    let Ok(title) = env.new_string(title) else {
        return;
    };
    let _ = env.call_method(
        service,
        "applyNativePlaybackState",
        "(ILjava/lang/String;IIIJJJIZZ)V",
        &[
            JValue::Int(state as i32),
            JValue::Object(&title),
            JValue::Int(stream_info.bitrate.unwrap_or_default()),
            JValue::Int(stream_info.frequency.unwrap_or_default()),
            JValue::Int(stream_info.channels.unwrap_or_default()),
            JValue::Long(duration_ms),
            JValue::Long(position_ms),
            JValue::Long(current_index),
            JValue::Int(playlist_len),
            JValue::Bool(has_entries.into()),
            JValue::Bool(has_entries.into()),
        ],
    );
}
