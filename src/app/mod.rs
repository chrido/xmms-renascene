//! Frontend-neutral application orchestration.
//!
//! This module is the boundary between reusable application behavior and
//! concrete UI frontends such as GTK or future mobile frontends.

pub mod command;
pub mod controller;
pub mod effect;
pub mod equalizer_actions;
pub mod file_info;
pub mod input;
pub mod logging;
pub mod panel;
pub mod playlist_actions;
pub mod preferences_model;
pub mod preview;
pub mod screenshot_scenarios;
pub mod services;
pub mod store;
pub mod view_model;
