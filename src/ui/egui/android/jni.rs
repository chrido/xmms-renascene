//! JNI ABI adapters.
//!
//! `XmmsActivity` callbacks validate/convert inputs and enqueue typed events;
//! they never mutate `AppStore`, playlists, or the playback backend. Repaint is
//! a lifecycle signal rather than a domain mutation. Deliberate synchronous
//! exceptions are limited to `XmmsPlaybackService` fallback transport/polling
//! and media-library queries, plus widget query/render endpoints that Android
//! requires to return a value immediately.

use std::path::PathBuf;

use jni::objects::{JObject, JObjectArray, JString};
use jni::sys::{jint, jintArray, jlong, jobjectArray, jstring};
use jni::JNIEnv;

use super::events::{self, AndroidMediaControl, AndroidPickerResult, AndroidPlatformEvent};
use super::{media_session, picker, widgets};

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnDocumentsSelected(
    mut env: JNIEnv,
    _activity: JObject,
    request_code: jint,
    operation_id: jlong,
    paths: jobjectArray,
    error: JString,
) {
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
    }));
    events::request_registered_repaint();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnMediaControl(
    _env: JNIEnv,
    _activity: JObject,
    control: jint,
) {
    let control = match control {
        1 => AndroidMediaControl::PausePlayback,
        2 => AndroidMediaControl::ResumePlayback,
        3 => AndroidMediaControl::NextTrack,
        _ => return,
    };
    events::push(AndroidPlatformEvent::MediaControl(
        super::events::AndroidMediaControlEvent {
            control,
            backend_executed: false,
        },
    ));
    events::request_registered_repaint();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeOnMediaVolumeChanged(
    _env: JNIEnv,
    _activity: JObject,
    volume_percent: jint,
) {
    events::push(AndroidPlatformEvent::ExternalVolumeChanged(
        volume_percent.clamp(0, 100),
    ));
    events::request_registered_repaint();
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_xmms_renascene_XmmsActivity_nativeRequestRepaint(
    _env: JNIEnv,
    _activity: JObject,
) {
    events::request_registered_repaint();
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
    let output = env
        .new_int_array(pixels.len() as jint)
        .map_err(|err| err.to_string())?;
    env.set_int_array_region(&output, 0, &pixels)
        .map_err(|err| err.to_string())?;
    Ok(output.into_raw())
}
