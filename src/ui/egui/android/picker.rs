//! Storage Access Framework operations.
//!
//! Operation IDs are process-wide because the completion JNI callback has no
//! `AndroidRuntime` reference. Activity replacement clears active operations,
//! making delayed results stale instead of attaching them to a new Activity.

use std::sync::{Mutex, OnceLock};

use jni::objects::JValue;
use jni::sys::{jint, jlong};

use crate::app::effect::FileDialogRequest;

use super::super::android_events::PickerOperationRegistry;
use super::activity;

static PICKER_OPERATIONS: OnceLock<Mutex<PickerOperationRegistry>> = OnceLock::new();

pub(crate) fn replace_activity() {
    PICKER_OPERATIONS
        .get_or_init(|| Mutex::new(PickerOperationRegistry::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .replace_activity();
}

fn begin_operation(request_code: jint) -> Result<u64, String> {
    PICKER_OPERATIONS
        .get_or_init(|| Mutex::new(PickerOperationRegistry::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .begin(request_code)
        .ok_or_else(|| format!("Android picker request {request_code} is already active"))
}

fn cancel_operation(request_code: jint, operation_id: u64) {
    PICKER_OPERATIONS
        .get_or_init(|| Mutex::new(PickerOperationRegistry::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .cancel(request_code, operation_id);
}

pub(crate) fn complete_operation(request_code: jint, operation_id: u64) -> bool {
    PICKER_OPERATIONS
        .get_or_init(|| Mutex::new(PickerOperationRegistry::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .complete(request_code, operation_id)
}

pub(crate) fn operation_is_active(request_code: jint, operation_id: u64) -> bool {
    PICKER_OPERATIONS
        .get_or_init(|| Mutex::new(PickerOperationRegistry::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .is_active(request_code, operation_id)
}

pub fn open(request: FileDialogRequest) -> Result<(), String> {
    if request == FileDialogRequest::AddAudioDirectory {
        let request_code = 104;
        let context = activity::context()?;
        let context = context
            .as_ref()
            .ok_or_else(|| "Android file picker is not initialized".to_string())?;
        let mut env = context
            .vm
            .attach_current_thread()
            .map_err(|err| format!("failed to attach Android picker thread: {err}"))?;
        let operation_id = begin_operation(request_code)?;
        env.call_method(
            context.activity.as_obj(),
            "openDirectory",
            "(IJ)V",
            &[
                JValue::Int(request_code),
                JValue::Long(operation_id as jlong),
            ],
        )
        .map_err(|err| {
            cancel_operation(request_code, operation_id);
            format!("failed to open Android directory picker: {err}")
        })?;
        return Ok(());
    }
    let (request_code, mime_type, multiple) = match request {
        FileDialogRequest::AddAudioFiles => (100, "audio/*", true),
        FileDialogRequest::LoadPlaylist | FileDialogRequest::ImportPlaylist => (107, "*/*", false),
        FileDialogRequest::LoadEqualizerPreset => (102, "*/*", false),
        FileDialogRequest::ImportSkin => (103, "*/*", false),
        FileDialogRequest::AddAudioDirectory => unreachable!(),
        FileDialogRequest::SavePlaylist | FileDialogRequest::ExportSkin => {
            return Err(
                "Saving through the Android document picker is not supported yet".to_string(),
            );
        }
        FileDialogRequest::SaveEqualizerPreset => unreachable!(),
    };
    let context = activity::context()?;
    let context = context
        .as_ref()
        .ok_or_else(|| "Android file picker is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android picker thread: {err}"))?;
    let mime_type = env
        .new_string(mime_type)
        .map_err(|err| format!("failed to create picker MIME type: {err}"))?;
    let operation_id = begin_operation(request_code)?;
    env.call_method(
        context.activity.as_obj(),
        "openDocuments",
        "(IJLjava/lang/String;Z)V",
        &[
            JValue::Int(request_code),
            JValue::Long(operation_id as jlong),
            JValue::Object(&mime_type),
            JValue::Bool(multiple.into()),
        ],
    )
    .map_err(|err| {
        cancel_operation(request_code, operation_id);
        format!("failed to open Android document picker: {err}")
    })?;
    Ok(())
}

pub fn save_equalizer_preset(contents: &[u8]) -> Result<(), String> {
    create_document(105, "application/octet-stream", "preset.eqf", contents)
}

pub fn save_playlist(contents: &[u8], name: &str) -> Result<(), String> {
    create_document(106, "audio/x-mpegurl", name, contents)
}

fn create_document(
    request_code: jint,
    mime_type: &str,
    title: &str,
    contents: &[u8],
) -> Result<(), String> {
    let context = activity::context()?;
    let context = context
        .as_ref()
        .ok_or_else(|| "Android file picker is not initialized".to_string())?;
    let mut env = context
        .vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android picker thread: {err}"))?;
    let mime_type = env
        .new_string(mime_type)
        .map_err(|err| format!("failed to create document MIME type: {err}"))?;
    let title = env
        .new_string(title)
        .map_err(|err| format!("failed to create document file name: {err}"))?;
    let contents = env
        .byte_array_from_slice(contents)
        .map_err(|err| format!("failed to create document contents: {err}"))?;
    let operation_id = begin_operation(request_code)?;
    env.call_method(
        context.activity.as_obj(),
        "createDocument",
        "(IJLjava/lang/String;Ljava/lang/String;[B)V",
        &[
            JValue::Int(request_code),
            JValue::Long(operation_id as jlong),
            JValue::Object(&mime_type),
            JValue::Object(&title),
            JValue::Object(&contents),
        ],
    )
    .map_err(|err| {
        cancel_operation(request_code, operation_id);
        format!("failed to open Android document save dialog: {err}")
    })?;
    Ok(())
}

pub(crate) fn request_from_code(code: jint) -> Option<FileDialogRequest> {
    match code {
        100 => Some(FileDialogRequest::AddAudioFiles),
        101 => Some(FileDialogRequest::LoadPlaylist),
        102 => Some(FileDialogRequest::LoadEqualizerPreset),
        103 => Some(FileDialogRequest::ImportSkin),
        104 => Some(FileDialogRequest::AddAudioDirectory),
        105 => Some(FileDialogRequest::SaveEqualizerPreset),
        106 => Some(FileDialogRequest::SavePlaylist),
        107 => Some(FileDialogRequest::ImportPlaylist),
        _ => None,
    }
}
