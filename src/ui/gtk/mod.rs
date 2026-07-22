//! GTK frontend modules.
//!
//! The large legacy `src/ui.rs` module still owns most GTK wiring while code is
//! migrated incrementally. Its persistent `AppStore` is the sole GTK
//! domain-state owner; `MainWindowUiState` keeps only transient widget and
//! integration state. New GTK-specific code should live under this module.

pub(crate) mod equalizer_window;
pub(crate) mod playlist_menu;
pub(crate) mod playlist_window;
pub(crate) mod preferences;
