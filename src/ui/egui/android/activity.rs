//! Replaceable `NativeActivity` JNI context.
//!
//! Free JNI callbacks and platform effects need a process-wide lookup, so this
//! cannot safely live only on `AndroidRuntime`. Each Activity creation replaces
//! the global reference and receives a generation. Actual Java resume/pause
//! callbacks update the stored lifecycle state; stale callbacks and egui exits
//! from replaced generations are ignored.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use jni::objects::{GlobalRef, JObject, JString};
use jni::{JNIEnv, JavaVM};

use super::super::android_media::AndroidActivityGeneration;

pub(crate) struct AndroidActivityContext {
    pub vm: Arc<JavaVM>,
    pub activity: GlobalRef,
    pub generation: AndroidActivityGeneration,
    pub resumed: bool,
}

pub(crate) struct AndroidActivityInitialization {
    pub generation: AndroidActivityGeneration,
    pub resumed: bool,
    pub files_dir: PathBuf,
    pub cache_dir: PathBuf,
}

static CONTEXT: OnceLock<Mutex<Option<AndroidActivityContext>>> = OnceLock::new();
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

pub(crate) fn initialize(
    app: &winit::platform::android::activity::AndroidApp,
) -> Result<AndroidActivityInitialization, String> {
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
    let resumed = env
        .call_method(activity.as_obj(), "isNativeActivityResumed", "()Z", &[])
        .and_then(|value| value.z())
        .unwrap_or(false);
    let generation = AndroidActivityGeneration(NEXT_GENERATION.fetch_add(1, Ordering::Relaxed));
    std::env::set_var("XMMS_RS_CONFIG_DIR", files_dir.join("config"));
    std::env::set_var("XMMS_RS_CACHE_DIR", &cache_dir);
    drop(env);
    *CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(AndroidActivityContext {
        vm: Arc::new(vm),
        activity,
        generation,
        resumed,
    });
    Ok(AndroidActivityInitialization {
        generation,
        resumed,
        files_dir,
        cache_dir,
    })
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

pub(crate) fn context_for_generation(
    generation: AndroidActivityGeneration,
) -> Option<MutexGuard<'static, Option<AndroidActivityContext>>> {
    let context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    (context.as_ref()?.generation == generation).then_some(context)
}

pub(crate) fn is_current(generation: AndroidActivityGeneration) -> bool {
    CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .is_some_and(|context| context.generation == generation)
}

pub(crate) fn is_current_object(env: &mut JNIEnv<'_>, activity: &JObject<'_>) -> bool {
    let context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    context.as_ref().is_some_and(|context| {
        env.is_same_object(context.activity.as_obj(), activity)
            .unwrap_or(false)
    })
}

pub(crate) fn generation_for_object(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
) -> Option<(AndroidActivityGeneration, bool)> {
    let context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let current = context.as_ref()?;
    env.is_same_object(current.activity.as_obj(), activity)
        .unwrap_or(false)
        .then_some((current.generation, current.resumed))
}

pub(crate) fn set_resumed(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    resumed: bool,
) -> Option<AndroidActivityGeneration> {
    let mut context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let current = context.as_mut()?;
    if !env
        .is_same_object(current.activity.as_obj(), activity)
        .unwrap_or(false)
    {
        return None;
    }
    current.resumed = resumed;
    Some(current.generation)
}

pub(crate) fn destroy_current(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
) -> Option<AndroidActivityGeneration> {
    let mut context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let current = context.as_ref()?;
    if !env
        .is_same_object(current.activity.as_obj(), activity)
        .unwrap_or(false)
    {
        return None;
    }
    let generation = current.generation;
    *context = None;
    Some(generation)
}

pub(crate) fn clear_generation(generation: AndroidActivityGeneration) -> bool {
    let mut context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if !context
        .as_ref()
        .is_some_and(|current| current.generation == generation)
    {
        return false;
    }
    *context = None;
    true
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
