//! egui frontend modules.
//!
//! This frontend must not import GTK/GIO/GLib/GDK. Shared behavior belongs in
//! `app`, `render`, `skin`, or another frontend-neutral module.

pub mod app;
pub mod equalizer;
pub mod file_info;
pub mod layout;
pub mod main_player;
pub mod menu;
pub mod playlist;
pub mod preferences;
pub mod runtime;
pub mod screenshots;
pub mod skin_texture;
