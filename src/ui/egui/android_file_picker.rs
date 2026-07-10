//! Android Storage Access Framework bridge for the egui frontend.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use jni::objects::{GlobalRef, JObject, JObjectArray, JString, JValue};
use jni::sys::{jint, jlong, jobjectArray, jstring};
use jni::{JNIEnv, JavaVM};

use crate::app::effect::FileDialogRequest;
use crate::playback::backend::PlaybackBackend;
use crate::playback::model::{PlaybackEvent, PlayerState};
use crate::playback::rodio::RodioBackend;
use crate::playlist::Playlist;
use crate::session::{fallback_state_paths, load_saved_state};

struct AndroidPickerContext {
    vm: JavaVM,
    activity: GlobalRef,
}

pub struct AndroidPickerResult {
    pub request: FileDialogRequest,
    pub paths: Vec<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidMediaControl {
    PausePlayback,
    ResumePlayback,
    NextTrack,
    PreviousTrack,
    SeekToMs(i64),
    PlayMediaItem(usize),
    StopPlayback,
    PlaylistEof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AndroidMediaNotification {
    state: i32,
    title: String,
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

#[derive(Debug, Clone, Copy, Default)]
pub struct AndroidSystemInsets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

static CONTEXT: OnceLock<AndroidPickerContext> = OnceLock::new();
static RESULTS: OnceLock<Mutex<Vec<AndroidPickerResult>>> = OnceLock::new();
static MEDIA_CONTROLS: OnceLock<Mutex<Vec<AndroidMediaControl>>> = OnceLock::new();
static MEDIA_NOTIFICATION: OnceLock<Mutex<Option<AndroidMediaNotification>>> = OnceLock::new();
static MEDIA_PLAYLIST: OnceLock<Mutex<Option<AndroidMediaPlaylist>>> = OnceLock::new();
static PLAYBACK_BACKEND: OnceLock<Mutex<Option<RodioBackend>>> = OnceLock::new();
static SERVICE_PLAYBACK_EVENTS: OnceLock<Mutex<Vec<PlaybackEvent>>> = OnceLock::new();
static REPAINT_CONTEXT: OnceLock<egui::Context> = OnceLock::new();

pub fn initialize(app: &winit::platform::android::activity::AndroidApp) -> Result<(), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|err| format!("failed to access Android VM: {err}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android picker thread: {err}"))?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let activity = env
        .new_global_ref(activity)
        .map_err(|err| format!("failed to retain Android activity: {err}"))?;
    let files_dir = android_activity_directory(&mut env, activity.as_obj(), "getFilesDir")?;
    let cache_dir = android_activity_directory(&mut env, activity.as_obj(), "getCacheDir")?;
    std::env::set_var("XMMS_RS_CONFIG_DIR", files_dir.join("config"));
    std::env::set_var("XMMS_RS_CACHE_DIR", cache_dir);
    drop(env);
    CONTEXT
        .set(AndroidPickerContext { vm, activity })
        .map_err(|_| "Android file picker was initialized more than once".to_string())?;
    Ok(())
}

fn android_activity_directory(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    method: &str,
) -> Result<PathBuf, String> {
    let directory = env
        .call_method(activity, method, "()Ljava/io/File;", &[])
        .and_then(|value| value.l())
        .map_err(|err| format!("failed to resolve Android {method}: {err}"))?;
    let absolute_path = env
        .call_method(directory, "getAbsolutePath", "()Ljava/lang/String;", &[])
        .and_then(|value| value.l())
        .map_err(|err| format!("failed to resolve Android {method} path: {err}"))?;
    let absolute_path = JString::from(absolute_path);
    let absolute_path: String = env
        .get_string(&absolute_path)
        .map_err(|err| format!("failed to read Android {method} path: {err}"))?
        .into();
    Ok(PathBuf::from(absolute_path))
}

pub fn open(request: FileDialogRequest) -> Result<(), String> {
    if request == FileDialogRequest::AddAudioDirectory {
        let context = CONTEXT
            .get()
            .ok_or_else(|| "Android file picker is not initialized".to_string())?;
        let mut env = context
            .vm
            .attach_current_thread()
            .map_err(|err| format!("failed to attach Android picker thread: {err}"))?;
        env.call_method(
            context.activity.as_obj(),
            "openDirectory",
            "(I)V",
            &[JValue::Int(104)],
        )
        .map_err(|err| format!("failed to open Android directory picker: {err}"))?;
        return Ok(());
    }
    let (request_code, mime_type, multiple) = match request {
        FileDialogRequest::AddAudioFiles => (100, "audio/*", true),
        FileDialogRequest::LoadPlaylist => (101, "*/*", false),
        FileDialogRequest::LoadEqualizerPreset => (102, "*/*", false),
        FileDialogRequest::ImportSkin => (103, "*/*", false),
        FileDialogRequest::AddAudioDirectory => unreachable!(),
        FileDialogRequest::SavePlaylist
        | FileDialogRequest::SaveEqualizerPreset
        | FileDialogRequest::ExportSkin => {
            return Err(
                "Saving through the Android document picker is not supported yet".to_string(),
            );
        }
    };
    let context = CONTEXT
        .get()
        .ok_or_else(|| "Android file picker is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android picker thread: {err}"))?;
    let mime_type = env
        .new_string(mime_type)
        .map_err(|err| format!("failed to create picker MIME type: {err}"))?;
    env.call_method(
        context.activity.as_obj(),
        "openDocuments",
        "(ILjava/lang/String;Z)V",
        &[
            JValue::Int(request_code),
            JValue::Object(&mime_type),
            JValue::Bool(multiple.into()),
        ],
    )
    .map_err(|err| format!("failed to open Android document picker: {err}"))?;
    Ok(())
}

pub fn drain_results() -> Vec<AndroidPickerResult> {
    let mut results = RESULTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap();
    std::mem::take(&mut *results)
}

pub fn register_repaint_context(context: &egui::Context) {
    let _ = REPAINT_CONTEXT.set(context.clone());
}

pub fn drain_media_controls() -> Vec<AndroidMediaControl> {
    let mut controls = MEDIA_CONTROLS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    std::mem::take(&mut *controls)
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

pub fn drain_service_playback_events() -> Vec<PlaybackEvent> {
    let mut events = SERVICE_PLAYBACK_EVENTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    std::mem::take(&mut *events)
}

pub fn update_playback_notification(
    state: i32,
    title: &str,
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

    let context = CONTEXT
        .get()
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
        "(ILjava/lang/String;JJJIZZ)V",
        &[
            JValue::Int(state),
            JValue::Object(&title),
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
    let context = CONTEXT
        .get()
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

pub fn system_insets_pixels() -> AndroidSystemInsets {
    let Some(context) = CONTEXT.get() else {
        return AndroidSystemInsets::default();
    };
    let Ok(mut env) = context.vm.attach_current_thread() else {
        return AndroidSystemInsets::default();
    };
    let mut inset = |side| {
        env.call_method(
            context.activity.as_obj(),
            "systemInset",
            "(I)I",
            &[JValue::Int(side)],
        )
        .and_then(|value| value.i())
        .unwrap_or(0)
        .max(0)
    };
    AndroidSystemInsets {
        left: inset(0),
        top: inset(1),
        right: inset(2),
        bottom: inset(3),
    }
}

pub fn request_playback_audio_focus() -> Result<(), String> {
    let context = CONTEXT
        .get()
        .ok_or_else(|| "Android activity is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android audio-focus thread: {err}"))?;
    let granted = env
        .call_method(
            context.activity.as_obj(),
            "requestPlaybackAudioFocus",
            "()Z",
            &[],
        )
        .and_then(|value| value.z())
        .map_err(|err| format!("failed to request Android audio focus: {err}"))?;
    if granted {
        Ok(())
    } else {
        Err("Android did not grant media audio focus".to_string())
    }
}

pub fn abandon_playback_audio_focus() {
    let Some(context) = CONTEXT.get() else {
        return;
    };
    let Ok(mut env) = context.vm.attach_current_thread() else {
        return;
    };
    let _ = env.call_method(
        context.activity.as_obj(),
        "abandonPlaybackAudioFocus",
        "()V",
        &[],
    );
}

fn request_from_code(code: jint) -> Option<FileDialogRequest> {
    match code {
        100 => Some(FileDialogRequest::AddAudioFiles),
        101 => Some(FileDialogRequest::LoadPlaylist),
        102 => Some(FileDialogRequest::LoadEqualizerPreset),
        103 => Some(FileDialogRequest::ImportSkin),
        104 => Some(FileDialogRequest::AddAudioDirectory),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnDocumentsSelected(
    mut env: JNIEnv,
    _activity: JObject,
    request_code: jint,
    paths: jobjectArray,
    error: JString,
) {
    let Some(request) = request_from_code(request_code) else {
        return;
    };
    let paths = unsafe { JObjectArray::from_raw(paths) };
    let path_count = env.get_array_length(&paths).unwrap_or(0);
    let mut selected_paths = Vec::with_capacity(path_count as usize);
    for index in 0..path_count {
        let Ok(path) = env.get_object_array_element(&paths, index) else {
            continue;
        };
        let path = JString::from(path);
        match env.get_string(&path) {
            Ok(path) => {
                selected_paths.push(PathBuf::from(path.to_string_lossy().into_owned()));
            }
            Err(_) => {}
        };
    }
    let error = if error.is_null() {
        None
    } else {
        env.get_string(&error)
            .ok()
            .map(|error| error.to_string_lossy().into_owned())
    };
    RESULTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(AndroidPickerResult {
            request,
            paths: selected_paths,
            error,
        });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnMediaControl(
    env: JNIEnv,
    _activity: JObject,
    control: jint,
) {
    let control = match control {
        1 => AndroidMediaControl::PausePlayback,
        2 => AndroidMediaControl::ResumePlayback,
        3 => AndroidMediaControl::NextTrack,
        _ => return,
    };
    handle_android_media_control(env, None, control);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeOnMediaControl(
    env: JNIEnv,
    service: JObject,
    control: jint,
    value: jlong,
) {
    let control = match control {
        1 => AndroidMediaControl::PausePlayback,
        2 => AndroidMediaControl::ResumePlayback,
        3 => AndroidMediaControl::NextTrack,
        4 => AndroidMediaControl::PreviousTrack,
        5 => AndroidMediaControl::SeekToMs(value.max(0)),
        6 => AndroidMediaControl::StopPlayback,
        7 => AndroidMediaControl::PlayMediaItem(value.max(0) as usize),
        _ => return,
    };
    handle_android_media_control(env, Some(service), control);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeInitializeMediaLibrary(
    mut env: JNIEnv,
    _service: JObject,
    files_dir: JString,
    cache_dir: JString,
) {
    let Ok(files_dir) = env
        .get_string(&files_dir)
        .map(|value| PathBuf::from(value.to_string_lossy().into_owned()))
    else {
        return;
    };
    let Ok(cache_dir) = env
        .get_string(&cache_dir)
        .map(|value| PathBuf::from(value.to_string_lossy().into_owned()))
    else {
        return;
    };
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

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeMediaItemCount(
    _env: JNIEnv,
    _service: JObject,
) -> jint {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .map_or(0, |shared| {
            shared.playlist.len().min(jint::MAX as usize) as jint
        })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeMediaItemTitle(
    env: JNIEnv,
    _service: JObject,
    index: jint,
) -> jstring {
    if index < 0 {
        return std::ptr::null_mut();
    }
    let title = MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .and_then(|shared| {
            let entry = shared.playlist.entries().get(index as usize)?;
            Some(
                shared
                    .titles
                    .get(index as usize)
                    .cloned()
                    .unwrap_or_else(|| media_item_title(&entry.title, &entry.filename)),
            )
        });
    title
        .and_then(|title| env.new_string(title).ok())
        .map_or(std::ptr::null_mut(), JString::into_raw)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeMediaItemDurationMs(
    _env: JNIEnv,
    _service: JObject,
    index: jint,
) -> jlong {
    if index < 0 {
        return -1;
    }
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .and_then(|shared| shared.playlist.entries().get(index as usize))
        .map_or(-1, |entry| entry.length_ms)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeCurrentMediaItemIndex(
    _env: JNIEnv,
    _service: JObject,
) -> jlong {
    MEDIA_PLAYLIST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .and_then(|shared| shared.playlist.position())
        .map_or(-1, |position| position.min(jlong::MAX as usize) as jlong)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativePollPlayback(
    mut env: JNIEnv,
    service: JObject,
) {
    let Ok(backend) = shared_playback_backend() else {
        return;
    };
    let Ok(events) = backend.poll_events() else {
        return;
    };
    let mut eof = false;
    let mut refresh_state = false;
    let mut queued = SERVICE_PLAYBACK_EVENTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    for event in events {
        if matches!(event, PlaybackEvent::EndOfStream) {
            eof = true;
        } else if let PlaybackEvent::Spectrum(data) = event {
            if let Some(existing) = queued
                .iter_mut()
                .find(|event| matches!(event, PlaybackEvent::Spectrum(_)))
            {
                *existing = PlaybackEvent::Spectrum(data);
            } else {
                queued.push(PlaybackEvent::Spectrum(data));
            }
        } else {
            refresh_state |= matches!(
                event,
                PlaybackEvent::DurationChanged(_) | PlaybackEvent::AsyncDone
            );
            queued.push(event);
        }
    }
    drop(queued);
    if eof {
        if let Err(err) = advance_after_end_of_stream(&backend) {
            eprintln!("xmms-rs: Android background playlist advance failed: {err}");
            let _ = backend.stop();
        }
        MEDIA_CONTROLS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(AndroidMediaControl::PlaylistEof);
        update_service_from_backend(&mut env, service);
    } else if refresh_state {
        update_service_from_backend(&mut env, service);
    }
}

fn handle_android_media_control(
    mut env: JNIEnv,
    service: Option<JObject>,
    control: AndroidMediaControl,
) {
    let result = execute_android_media_control(control);
    if let Err(err) = result {
        eprintln!("xmms-rs: Android media control failed: {err}");
        return;
    }
    MEDIA_CONTROLS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push(control);
    if let Some(service) = service {
        update_service_from_backend(&mut env, service);
    }
    if let Some(context) = REPAINT_CONTEXT.get() {
        context.request_repaint();
    }
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

fn update_service_from_backend(env: &mut JNIEnv, service: JObject) {
    let Ok(backend) = shared_playback_backend() else {
        return;
    };
    let state = match backend.state() {
        PlayerState::Stopped => 0,
        PlayerState::Playing => 1,
        PlayerState::Paused => 2,
    };
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
        "(ILjava/lang/String;JJJIZZ)V",
        &[
            JValue::Int(state),
            JValue::Object(&title),
            JValue::Long(duration_ms),
            JValue::Long(position_ms),
            JValue::Long(current_index),
            JValue::Int(playlist_len),
            JValue::Bool(has_entries.into()),
            JValue::Bool(has_entries.into()),
        ],
    );
}
