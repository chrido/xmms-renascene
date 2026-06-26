//! egui preferences dialog/window.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferencesPage {
    #[default]
    Options,
    Audio,
    Playlist,
    Visualization,
    Titles,
}
