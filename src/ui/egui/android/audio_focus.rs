//! Activity-side media volume access.
//!
//! Playback audio-focus ownership remains exclusively in
//! `XmmsPlaybackService`, where it can outlive the Activity. This module is the
//! Rust-side boundary for the related Activity media-stream volume calls; it
//! intentionally contains no second focus request or focus state machine.

use jni::objects::JValue;

use super::activity;

pub fn set_media_volume_percent(volume: i32) -> Result<(), String> {
    let context = activity::context()?;
    let context = context
        .as_ref()
        .ok_or_else(|| "Android activity is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android media-volume thread: {err}"))?;
    let applied = env
        .call_method(
            context.activity.as_obj(),
            "setMediaVolumePercent",
            "(I)Z",
            &[JValue::Int(volume)],
        )
        .and_then(|value| value.z())
        .map_err(|err| format!("failed to set Android media volume: {err}"))?;
    if applied {
        Ok(())
    } else {
        Err("Android media volume is unavailable".to_string())
    }
}

pub fn media_volume_percent() -> Result<i32, String> {
    let context = activity::context()?;
    let context = context
        .as_ref()
        .ok_or_else(|| "Android activity is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android media-volume thread: {err}"))?;
    let volume = env
        .call_method(
            context.activity.as_obj(),
            "getMediaVolumePercent",
            "()I",
            &[],
        )
        .and_then(|value| value.i())
        .map_err(|err| format!("failed to read Android media volume: {err}"))?;
    if volume >= 0 {
        Ok(volume.clamp(0, 100))
    } else {
        Err("Android media volume is unavailable".to_string())
    }
}
