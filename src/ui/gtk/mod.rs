//! GTK frontend modules.
//!
//! The large legacy `src/ui.rs` module still owns most GTK wiring while code is
//! migrated incrementally. New GTK-specific code should live under this module.

pub(crate) mod app;
pub(crate) mod dialogs;
pub(crate) mod equalizer_window;
pub(crate) mod file_info_dialog;
pub(crate) mod gestures;
pub(crate) mod main_menu;
pub(crate) mod main_window;
pub(crate) mod playlist_menu;
pub(crate) mod playlist_window;
pub(crate) mod preferences;
pub(crate) mod runtime;
pub(crate) mod skin_browser;
pub(crate) mod style;
