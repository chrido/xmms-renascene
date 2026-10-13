//! JNI ABI adapters.
//!
//! `XmmsActivity` callbacks validate/convert inputs and either report lifecycle
//! state or enqueue typed events; they never mutate `AppStore` or the playback
//! backend. Repaint is only a callback request, not a liveness signal.
//! Deliberate synchronous exceptions are limited to authoritative
//! `XmmsPlaybackService` fallback transport/polling and media-library queries,
//! plus widget query/render endpoints that Android requires to return a value
//! immediately. Artwork snapshot/cancellation endpoints are memory-only;
//! blocking artwork resolution/render endpoints reject Android's main thread.

use std::path::PathBuf;

use jni::objects::{JLongArray, JObject, JObjectArray, JString};
use jni::sys::{jboolean, jbyteArray, jint, jintArray, jlong, jobjectArray, jstring};
use jni::JNIEnv;

use super::events::{self, AndroidMediaControl, AndroidPickerResult, AndroidPlatformEvent};
use super::{artwork, media_session, picker, widgets};

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnActivityResumed(
    mut env: JNIEnv,
    activity: JObject,
) {
    super::handle_activity_resumed(&mut env, &activity);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnActivityPaused(
    mut env: JNIEnv,
    activity: JObject,
) {
    super::handle_activity_paused(&mut env, &activity);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnActivityDestroyed(
    mut env: JNIEnv,
    activity: JObject,
) {
    super::handle_activity_destroyed(&mut env, &activity);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnDocumentsSelected(
    mut env: JNIEnv,
    activity: JObject,
    request_code: jint,
    operation_id: jlong,
    paths: jobjectArray,
    error: JString,
) {
    if !super::activity_callback_is_current(&mut env, &activity) {
        return;
    }
    let Ok(operation_id) = u64::try_from(operation_id) else {
        return;
    };
    if !picker::complete_operation(request_code, operation_id) {
        return;
    }
    let Some(request) = picker::request_from_code(request_code) else {
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
    events::push(AndroidPlatformEvent::Picker(AndroidPickerResult {
        request,
        paths: selected_paths,
        error,
        complete: true,
    }));
    events::request_registered_repaint();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnDocumentImportProgress(
    mut env: JNIEnv,
    activity: JObject,
    request_code: jint,
    operation_id: jlong,
    path: JString,
) {
    if !super::activity_callback_is_current(&mut env, &activity) {
        return;
    }
    let Ok(operation_id) = u64::try_from(operation_id) else {
        return;
    };
    if !picker::operation_is_active(request_code, operation_id) {
        return;
    }
    let Some(request) = picker::request_from_code(request_code) else {
        return;
    };
    let Ok(path) = env.get_string(&path) else {
        return;
    };
    events::push(AndroidPlatformEvent::Picker(AndroidPickerResult {
        request,
        paths: vec![PathBuf::from(path.to_string_lossy().into_owned())],
        error: None,
        complete: false,
    }));
    events::request_registered_repaint();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnMediaControl(
    mut env: JNIEnv,
    activity: JObject,
    control: jint,
) {
    let Some(control) = AndroidMediaControl::from_activity_jni_code(control) else {
        return;
    };
    super::handle_activity_media_control(&mut env, &activity, control);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnMediaVolumeChanged(
    mut env: JNIEnv,
    activity: JObject,
    volume_percent: jint,
) {
    if !super::activity_callback_is_current(&mut env, &activity) {
        return;
    }
    events::push(AndroidPlatformEvent::ExternalVolumeChanged(
        volume_percent.clamp(0, 100),
    ));
    events::request_registered_repaint();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeRequestRepaint(
    mut env: JNIEnv,
    activity: JObject,
) {
    super::request_activity_repaint(&mut env, &activity);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeOnMediaControl(
    env: JNIEnv,
    service: JObject,
    control: jint,
    value: jlong,
) {
    let Some(control) = AndroidMediaControl::from_jni_code(control, value) else {
        return;
    };
    media_session::handle_media_control(env, Some(service), control);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeInitializeMediaLibrary(
    mut env: JNIEnv,
    _service: JObject,
    files_dir: JString,
    cache_dir: JString,
) {
    let Ok(files_dir) = jstring_path(&mut env, &files_dir) else {
        return;
    };
    let Ok(cache_dir) = jstring_path(&mut env, &cache_dir) else {
        return;
    };
    media_session::initialize_media_library(files_dir, cache_dir);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlayerWidget_nativeRenderPlayerWidget(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    cache_dir: JString,
    pressed_control: jint,
) -> jintArray {
    let result = (|| {
        let files_dir = jstring_path(&mut env, &files_dir)?;
        let cache_dir = jstring_path(&mut env, &cache_dir)?;
        let image = widgets::render_player_widget(&files_dir, &cache_dir, pressed_control)?;
        color_image_to_jint_array(&mut env, &image)
    })();
    match result {
        Ok(pixels) => pixels,
        Err(err) => {
            eprintln!("xmms-rs: failed to render Android player widget: {err}");
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlayerInfoWidget_nativeRenderPlayerInfoWidget(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    cache_dir: JString,
    title: JString,
    bitrate: jint,
    frequency: jint,
    channels: jint,
    title_offset_px: jint,
) -> jintArray {
    let result = (|| {
        let files_dir = jstring_path(&mut env, &files_dir)?;
        let cache_dir = jstring_path(&mut env, &cache_dir)?;
        let title = env
            .get_string(&title)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .into_owned();
        let image = widgets::render_player_info_widget(
            &files_dir,
            &cache_dir,
            &title,
            bitrate,
            frequency,
            channels,
            title_offset_px,
        )?;
        color_image_to_jint_array(&mut env, &image)
    })();
    match result {
        Ok(pixels) => pixels,
        Err(err) => {
            eprintln!("xmms-rs: failed to render Android player info widget: {err}");
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlayerInfoWidget_nativeUpdateTitleMarquee(
    mut env: JNIEnv,
    _class: JObject,
    title: JString,
    playback_state: jint,
    elapsed_ms: jlong,
) -> jlong {
    let title = match env.get_string(&title) {
        Ok(title) => title.to_string_lossy().into_owned(),
        Err(err) => {
            eprintln!("xmms-rs: failed to read Android widget marquee title: {err}");
            return 0;
        }
    };
    widgets::update_title_marquee(&title, playback_state, elapsed_ms)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeMediaItemCount(
    _env: JNIEnv,
    _service: JObject,
) -> jint {
    media_session::media_item_count()
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
    media_session::media_item_title_at(index as usize)
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
    media_session::media_item_duration_ms(index as usize).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeMediaQueueSnapshot(
    env: JNIEnv,
    _service: JObject,
) -> jstring {
    env.new_string(media_session::media_queue_snapshot_json())
        .map_or(std::ptr::null_mut(), JString::into_raw)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeCurrentMediaItemIndex(
    _env: JNIEnv,
    _service: JObject,
) -> jlong {
    media_session::current_media_item_index()
        .map_or(-1, |position| position.min(jlong::MAX as usize) as jlong)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativePollPlayback(
    mut env: JNIEnv,
    service: JObject,
) {
    media_session::poll_playback(&mut env, &service);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeValidateMediaProjection(
    mut env: JNIEnv,
    _activity: JObject,
    token: JString,
) -> jboolean {
    if token.is_null() {
        return 0;
    }
    env.get_string(&token)
        .ok()
        .is_some_and(|token| media_session::projection_valid(&token.to_string_lossy(), false))
        as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeValidateProjection(
    mut env: JNIEnv,
    _service: JObject,
    token: JString,
    require_owner: jboolean,
) -> jboolean {
    if token.is_null() {
        return 0;
    }
    env.get_string(&token).ok().is_some_and(|token| {
        media_session::projection_valid(&token.to_string_lossy(), require_owner != 0)
    }) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeRefreshPlayback(
    mut env: JNIEnv,
    service: JObject,
) {
    media_session::refresh_service(&mut env, &service);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeDebugReversePlaylist(
    mut env: JNIEnv,
    service: JObject,
) -> jboolean {
    // Guard in JNI as well as Java: exported native symbols cannot bypass the
    // non-debuggable APK boundary by invoking them through another Java caller.
    let allowed = env
        .call_method(&service, "isArtworkDebuggable", "()Z", &[])
        .and_then(|value| value.z())
        .unwrap_or(false);
    (allowed && media_session::debug_reverse_playlist(&mut env, &service)) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeDebugReplaceCurrent(
    mut env: JNIEnv,
    service: JObject,
    clear: jboolean,
) -> jboolean {
    let allowed = env
        .call_method(&service, "isArtworkDebuggable", "()Z", &[])
        .and_then(|value| value.z())
        .unwrap_or(false);
    (allowed && media_session::debug_replace_current(&mut env, &service, clear != 0)) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeDebugPalette(
    mut env: JNIEnv,
    service: JObject,
    serial: jint,
) -> jboolean {
    let allowed = env
        .call_method(&service, "isArtworkDebuggable", "()Z", &[])
        .and_then(|value| value.z())
        .unwrap_or(false);
    if !allowed || !(0..=1000).contains(&serial) {
        return 0;
    }
    let store = crate::artwork::settings::process_artwork_settings();
    let snapshot = store.snapshot();
    let palette = std::array::from_fn(|i| {
        [
            ((i * 9 + serial as usize * 7) % 256) as u8,
            ((i * 5 + serial as usize * 3) % 256) as u8,
            ((i * 11 + serial as usize * 2) % 256) as u8,
        ]
    });
    store
        .publish_foreground(snapshot.selector.clone(), snapshot.policy, palette)
        .is_ok() as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsPlaybackService_nativeCurrentProjectionToken(
    env: JNIEnv,
    _service: JObject,
) -> jstring {
    env.new_string(media_session::current_projection_token())
        .map_or(std::ptr::null_mut(), JString::into_raw)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsArtworkWorker_nativeSnapshot(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
) -> jstring {
    jstring_path(&mut env, &files_dir)
        .ok()
        .and_then(|dir| artwork::snapshot(&dir))
        .and_then(|json| env.new_string(json).ok())
        .map_or(std::ptr::null_mut(), JString::into_raw)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsArtworkWorker_nativeSelect(
    _env: JNIEnv,
    _class: JObject,
    epoch: jlong,
    generation: jlong,
) {
    if let (Ok(epoch), Ok(generation)) = (u64::try_from(epoch), u64::try_from(generation)) {
        artwork::select(epoch, generation);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsArtworkWorker_nativeCancel(
    _env: JNIEnv,
    _class: JObject,
    epoch: jlong,
) {
    if let Ok(epoch) = u64::try_from(epoch) {
        artwork::cancel(epoch);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsArtworkWorker_nativeResolveSettings(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    epoch: jlong,
    generation: jlong,
) -> jint {
    if !artwork_worker_thread(&mut env) {
        return 0;
    }
    let (Ok(epoch), Ok(generation)) = (u64::try_from(epoch), u64::try_from(generation)) else {
        return 0;
    };
    match jstring_path(&mut env, &files_dir)
        .and_then(|dir| artwork::resolve_on_worker(&dir, epoch, generation))
    {
        Ok(fallback) => {
            if fallback {
                2
            } else {
                1
            }
        }
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsArtworkWorker_nativeRender(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    snapshot: JString,
    epoch: jlong,
    generation: jlong,
    timings: JLongArray,
    output_size: jint,
) -> jintArray {
    if !artwork_worker_thread(&mut env) {
        return std::ptr::null_mut();
    }
    let result = (|| {
        let dir = jstring_path(&mut env, &files_dir)?;
        let epoch = u64::try_from(epoch).map_err(|_| "invalid epoch")?;
        let generation = u64::try_from(generation).map_err(|_| "invalid generation")?;
        let length = env
            .call_method(&snapshot, "length", "()I", &[])
            .and_then(|value| value.i())
            .map_err(|_| "invalid snapshot")?;
        if !(0..=65536).contains(&length) {
            return Err("oversized snapshot".to_string());
        }
        let frozen = env
            .get_string(&snapshot)
            .map_err(|_| "invalid snapshot")?
            .to_string_lossy()
            .into_owned();
        if env
            .get_array_length(&timings)
            .map_err(|_| "invalid timings")?
            != 2
        {
            return Err("invalid timings length".into());
        }
        let started = std::time::Instant::now();
        let output_size = u32::try_from(output_size).map_err(|_| "invalid output size")?;
        let pixels = artwork::render_on_worker(&dir, &frozen, epoch, generation, output_size)?;
        let render_ns = started.elapsed().as_nanos().min(jlong::MAX as u128) as jlong;
        let transfer = std::time::Instant::now();
        let output = argb_pixels_to_jint_array(&mut env, &pixels)?;
        let transfer_ns = transfer.elapsed().as_nanos().min(jlong::MAX as u128) as jlong;
        env.set_long_array_region(&timings, 0, &[render_ns, transfer_ns])
            .map_err(|err| err.to_string())?;
        Ok(output)
    })();
    // Expected cancellation/stale/disabled results are silent; Java owns the
    // bounded retry/diagnostic policy. Never log a track/snapshot on failure.
    result.unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsArtworkWorker_nativeRenderPng(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    frozen: JString,
    epoch: jlong,
    generation: jlong,
) -> jbyteArray {
    if !artwork_worker_thread(&mut env) {
        return std::ptr::null_mut();
    }
    let result: Result<jbyteArray, String> = (|| {
        let dir = jstring_path(&mut env, &files_dir)?;
        let frozen = env
            .get_string(&frozen)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .into_owned();
        let epoch = u64::try_from(epoch).map_err(|_| "invalid epoch")?;
        let generation = u64::try_from(generation).map_err(|_| "invalid generation")?;
        let pixels = artwork::render_on_worker(&dir, &frozen, epoch, generation, 512)?;
        let png = crate::artwork::png::encode_uri_png(pixels).map_err(|error| error.to_string())?;
        if artwork::snapshot(&dir).as_deref() != Some(&frozen) {
            return Err("stale PNG".into());
        }
        Ok(env
            .byte_array_from_slice(&png)
            .map_err(|error| error.to_string())?
            .into_raw())
    })();
    result.unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsBrowseArtwork_nativeSnapshot(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    index: jint,
) -> jstring {
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null_mut();
    };
    jstring_path(&mut env, &files_dir)
        .ok()
        .and_then(|dir| artwork::snapshot_item(&dir, Some(index)))
        .and_then(|json| env.new_string(json).ok())
        .map_or(std::ptr::null_mut(), JString::into_raw)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsBrowseArtwork_nativePrepare(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    index: jint,
) -> jstring {
    if !artwork_worker_thread(&mut env) {
        return std::ptr::null_mut();
    }
    let result: Result<jstring, String> = (|| {
        let dir = jstring_path(&mut env, &files_dir)?;
        let index = usize::try_from(index).map_err(|_| "invalid browse index")?;
        artwork::resolve_browse_on_worker(&dir)?;
        let frozen = artwork::snapshot_item(&dir, Some(index)).ok_or("invalid browse item")?;
        Ok(env
            .new_string(frozen)
            .map_err(|error| error.to_string())?
            .into_raw())
    })();
    result.unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsBrowseArtwork_nativeRender(
    mut env: JNIEnv,
    _class: JObject,
    files_dir: JString,
    index: jint,
    frozen: JString,
) -> jbyteArray {
    if !artwork_worker_thread(&mut env) {
        return std::ptr::null_mut();
    }
    let result = (|| {
        let dir = jstring_path(&mut env, &files_dir)?;
        let index = usize::try_from(index).map_err(|_| "invalid browse index")?;
        let frozen = env
            .get_string(&frozen)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .into_owned();
        let pixels = artwork::render_item(&dir, &frozen, 0, 0, 512, Some(index))?;
        let png = crate::artwork::png::encode_uri_png(pixels).map_err(|error| error.to_string())?;
        if artwork::snapshot_item(&dir, Some(index)).as_deref() != Some(&frozen) {
            return Err("stale PNG".into());
        }
        env.byte_array_from_slice(&png)
            .map(|bytes| bytes.into_raw())
            .map_err(|error| error.to_string())
    })();
    result.unwrap_or(std::ptr::null_mut())
}

fn artwork_worker_thread(env: &mut JNIEnv<'_>) -> bool {
    let result = (|| {
        let current = env
            .call_static_method(
                "android/os/Looper",
                "myLooper",
                "()Landroid/os/Looper;",
                &[],
            )?
            .l()?;
        let main = env
            .call_static_method(
                "android/os/Looper",
                "getMainLooper",
                "()Landroid/os/Looper;",
                &[],
            )?
            .l()?;
        env.is_same_object(current, main).map(|same| !same)
    })();
    result.unwrap_or(false)
}

fn jstring_path(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Result<PathBuf, String> {
    Ok(PathBuf::from(
        env.get_string(value)
            .map_err(|err| err.to_string())?
            .to_string_lossy()
            .into_owned(),
    ))
}

fn color_image_to_jint_array(
    env: &mut JNIEnv<'_>,
    image: &egui::ColorImage,
) -> Result<jintArray, String> {
    let pixels: Vec<jint> = image
        .pixels
        .iter()
        .map(|pixel| {
            let [red, green, blue, alpha] = pixel.to_array();
            i32::from_be_bytes([alpha, red, green, blue])
        })
        .collect();
    argb_pixels_to_jint_array(env, &pixels)
}

fn argb_pixels_to_jint_array(env: &mut JNIEnv<'_>, pixels: &[jint]) -> Result<jintArray, String> {
    let length = jint::try_from(pixels.len()).map_err(|_| "too many bitmap pixels")?;
    let output = env.new_int_array(length).map_err(|err| err.to_string())?;
    env.set_int_array_region(&output, 0, pixels)
        .map_err(|err| err.to_string())?;
    Ok(output.into_raw())
}
