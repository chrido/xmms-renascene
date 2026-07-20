//! egui frontend modules.
//!
//! This frontend must not import GTK/GIO/GLib/GDK. Shared behavior belongs in
//! `app`, `render`, `skin`, or another frontend-neutral module.

#[cfg(target_os = "android")]
pub(crate) mod android;
#[cfg(any(target_os = "android", test))]
pub mod android_events;
#[cfg(any(target_os = "android", test))]
pub mod android_runtime;
pub mod app;
pub(crate) mod effect_executor;
pub mod equalizer;
pub mod file_info;
#[cfg(any(target_os = "android", test))]
pub mod interaction;
pub mod layout;
pub mod main_player;
pub mod menu;
pub mod playback_runtime;
pub mod playlist;
pub mod preferences;
pub mod render_cache;
pub mod runtime;
pub mod screenshots;
pub mod skin_texture;
pub mod ui_state;
