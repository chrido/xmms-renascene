//! Replaceable `NativeActivity` JNI context.
//!
//! Free JNI callbacks and platform effects need a process-wide lookup, so this
//! cannot safely live only on `AndroidRuntime`. Each Activity creation replaces
//! the global reference; callers must tolerate the Activity being absent.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use jni::objects::{GlobalRef, JObject, JString};
use jni::{JNIEnv, JavaVM};

pub(crate) struct AndroidActivityContext {
    pub vm: Arc<JavaVM>,
    pub activity: GlobalRef,
}

static CONTEXT: OnceLock<Mutex<Option<AndroidActivityContext>>> = OnceLock::new();

pub(crate) fn initialize(
    app: &winit::platform::android::activity::AndroidApp,
) -> Result<(), String> {
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .map_err(|err| format!("failed to access Android VM: {err}"))?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|err| format!("failed to attach Android picker thread: {err}"))?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let activity = env
        .new_global_ref(activity)
        .map_err(|err| format!("failed to retain Android activity: {err}"))?;
    let files_dir = activity_directory(&mut env, activity.as_obj(), "getFilesDir")?;
    let cache_dir = activity_directory(&mut env, activity.as_obj(), "getCacheDir")?;
    std::env::set_var("XMMS_RS_CONFIG_DIR", files_dir.join("config"));
    std::env::set_var("XMMS_RS_CACHE_DIR", cache_dir);
    drop(env);
    *CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(AndroidActivityContext {
        vm: Arc::new(vm),
        activity,
    });
    Ok(())
}

pub(crate) fn context() -> Result<MutexGuard<'static, Option<AndroidActivityContext>>, String> {
    let context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if context.is_none() {
        return Err("Android activity is not initialized".to_string());
    }
    Ok(context)
}

pub(crate) fn reference() -> Result<(Arc<JavaVM>, GlobalRef), String> {
    let context = context()?;
    let context = context
        .as_ref()
        .ok_or_else(|| "Android activity is not initialized".to_string())?;
    Ok((Arc::clone(&context.vm), context.activity.clone()))
}

fn activity_directory(
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
