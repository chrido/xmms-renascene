//! Android platform boundary for the egui frontend.
//!
//! The module layout is responsibility-oriented:
//! - [`activity`] owns the replaceable `NativeActivity` JNI reference.
//! - [`events`] owns ordered process-wide ingress and repaint registration.
//! - [`picker`] owns SAF requests and activity-generation operation tokens.
//! - [`layout`] validates the compact Java window snapshot.
//! - [`media_session`] owns the service projection, shared backend, and the
//!   deliberate activity-absent playback fallback.
//! - [`audio_focus`] contains activity-side media-volume access; Android audio
//!   focus itself is owned exclusively by `XmmsPlaybackService`.
//! - [`persistence`] serializes in-process writes; snapshot replacement is
//!   atomic so Activity-absent readers and process recreation never see a
//!   partially written file.
//! - [`widgets`] owns synchronous widget rendering caches.
//! - [`jni`] contains JNI ABI adapters and documents the allowed exceptions.
//!
//! Activity callbacks run on the Android main thread and only validate, convert,
//! enqueue, or request repaint. The egui thread drains a bounded FIFO batch once
//! at the beginning of each frame, before local input is dispatched. Service
//! transport callbacks may execute the shared backend synchronously only while
//! no egui runtime is registered; the queued event records that execution.
//! Media-browser queries and widget rendering are synchronous because Android
//! requires an immediate return value.
//!
//! The generated manifest currently assigns no `android:process`, so Activity,
//! service, and widgets normally share one Linux process. Android may still
//! create those components without an Activity and may kill/recreate that
//! process at any time; durable coordination therefore uses atomically replaced
//! files rather than relying on these process-local registries.

mod activity;
mod audio_focus;
mod events;
mod jni;
mod layout;
mod media_session;
mod persistence;
mod picker;
pub(crate) mod playlist_manager;
mod widgets;

pub use audio_focus::{media_volume_percent, set_media_volume_percent};
pub use events::{
    begin_local_media_control, drain_platform_events, register_repaint_context,
    unregister_repaint_context, AndroidMediaControl, AndroidMediaControlEvent, AndroidPickerResult,
    AndroidPlatformEvent, AndroidPlaybackState,
};
pub use layout::window_layout_snapshot_pixels;
pub use media_session::{
    complete_media_control, shared_playback_backend, sync_media_playlist,
    update_playback_notification,
};
pub use persistence::persist_app_state;
pub use picker::{open, save_equalizer_preset};
pub use widgets::refresh_player_widgets;

pub fn initialize(app: &winit::platform::android::activity::AndroidApp) -> Result<(), String> {
    activity::initialize(app)?;
    events::reset();
    media_session::reset_notification();
    picker::replace_activity();
    Ok(())
}
