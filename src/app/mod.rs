//! Frontend-neutral application orchestration.
//!
//! This module is the boundary between reusable application behavior and
//! concrete UI frontends such as GTK or future mobile frontends.

pub mod command;
pub mod controller;
pub mod effect;
pub mod panel;
pub mod playlist_actions;
pub mod preview;
pub mod view_model;
