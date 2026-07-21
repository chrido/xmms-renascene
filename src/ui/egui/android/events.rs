//! Process-wide Android ingress registry.
//!
//! JNI entry points cannot borrow the lifecycle-owned [`AndroidRuntime`], so the
//! inbox, ordering lock, and current repaint handle remain process registries.
//! Repaint registration is tagged with the owning Activity generation and is
//! cleared on pause/replacement/exit; its presence is never treated as an
//! Activity-liveness signal. Ordered media controls are serialized with local
//! player commands; replaceable volume and spectrum samples are coalesced by
//! [`AndroidEventInbox`].

use std::sync::{Mutex, MutexGuard, OnceLock};

pub use super::super::android_events::{
    AndroidEventInbox, AndroidMediaControl, AndroidMediaControlEvent, AndroidPickerResult,
    AndroidPlatformEvent, AndroidPlaybackState,
};
use super::super::android_media::AndroidActivityGeneration;

static EVENTS: OnceLock<Mutex<AndroidEventInbox>> = OnceLock::new();
static MEDIA_CONTROL_ORDER: OnceLock<Mutex<()>> = OnceLock::new();
static REPAINT_CONTEXT: OnceLock<Mutex<Option<RegisteredRepaintContext>>> = OnceLock::new();

struct RegisteredRepaintContext {
    activity: AndroidActivityGeneration,
    context: egui::Context,
}

pub(crate) fn replace_activity() {
    *REPAINT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = None;
    EVENTS
        .get_or_init(|| Mutex::new(AndroidEventInbox::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .replace_activity();
}

pub(crate) fn push(event: AndroidPlatformEvent) {
    EVENTS
        .get_or_init(|| Mutex::new(AndroidEventInbox::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .push(event);
}

pub(crate) fn push_all(events: Vec<AndroidPlatformEvent>) {
    let mut inbox = EVENTS
        .get_or_init(|| Mutex::new(AndroidEventInbox::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    for event in events {
        inbox.push(event);
    }
}

pub(crate) fn drain_platform_events() -> Vec<AndroidPlatformEvent> {
    let mut events = EVENTS
        .get_or_init(|| Mutex::new(AndroidEventInbox::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let (drained, has_more) = events.drain_frame();
    drop(events);
    if has_more {
        request_registered_repaint();
    }
    drained
}

pub(crate) fn register_repaint_context(
    activity: AndroidActivityGeneration,
    context: &egui::Context,
) {
    *REPAINT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(RegisteredRepaintContext {
        activity,
        context: context.clone(),
    });
}

pub(crate) fn unregister_repaint_context(activity: AndroidActivityGeneration) {
    let mut repaint = REPAINT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if repaint
        .as_ref()
        .is_some_and(|registered| registered.activity == activity)
    {
        *repaint = None;
    }
}

pub(crate) fn remove_unexecuted_media_controls() {
    EVENTS
        .get_or_init(|| Mutex::new(AndroidEventInbox::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove_unexecuted_media_controls();
}

pub(crate) fn lock_media_control_order() -> MutexGuard<'static, ()> {
    MEDIA_CONTROL_ORDER
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

pub(crate) fn request_registered_repaint() {
    if let Some(context) = REPAINT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
    {
        context.context.request_repaint();
    }
}
