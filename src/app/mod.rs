//! Frontend-neutral application orchestration.
//!
//! This module is the boundary between reusable application behavior and
//! concrete UI frontends such as GTK or future mobile frontends.

pub mod command;
mod controller;
pub mod effect;
pub mod equalizer_actions;
pub mod external_commands;
pub mod file_info;
pub mod input;
pub mod logging;
pub mod panel;
pub(crate) mod playback_transition;
pub mod playlist_actions;
pub mod preferences_model;
pub mod preview;
pub mod runtime;
pub mod screenshot_scenarios;
pub mod services;
pub mod store;
pub mod view_model;
