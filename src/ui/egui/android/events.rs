//! Process-wide Android ingress registry.
//!
//! JNI entry points cannot borrow the lifecycle-owned [`AndroidRuntime`], so the
//! inbox, ordering lock, and current repaint handle remain process registries.
//! `initialize`/`on_exit` replace or clear lifecycle-owned contents. Ordered
//! media controls are serialized with local player commands; replaceable volume
//! and spectrum samples are coalesced by [`AndroidEventInbox`].

use std::sync::{Mutex, MutexGuard, OnceLock};

pub use super::super::android_events::{
    AndroidEventInbox, AndroidMediaControl, AndroidMediaControlEvent, AndroidPickerResult,
    AndroidPlatformEvent, AndroidPlaybackState,
};

static EVENTS: OnceLock<Mutex<AndroidEventInbox>> = OnceLock::new();
static MEDIA_CONTROL_ORDER: OnceLock<Mutex<()>> = OnceLock::new();
static REPAINT_CONTEXT: OnceLock<Mutex<Option<egui::Context>>> = OnceLock::new();

pub(crate) fn reset() {
    *REPAINT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = None;
    EVENTS
        .get_or_init(|| Mutex::new(AndroidEventInbox::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
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

pub fn drain_platform_events() -> Vec<AndroidPlatformEvent> {
    let _order = lock_media_control_order();
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

pub fn register_repaint_context(context: &egui::Context) {
    *REPAINT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = Some(context.clone());
}

pub fn unregister_repaint_context() {
    *REPAINT_CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner()) = None;
}

pub fn begin_local_media_control() -> MutexGuard<'static, ()> {
    let order = lock_media_control_order();
    EVENTS
        .get_or_init(|| Mutex::new(AndroidEventInbox::default()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove_media_controls();
    order
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
        context.request_repaint();
    }
}
