//! Transient egui-only menus, prompts, and viewport state.

use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::render::{
    EqualizerControl, EqualizerSlider, MainPushButton, MainSlider, MainToggleButton,
    PlaylistMenuRenderKind,
};

use super::file_info::FileInfoViewportState;
use super::menu::EguiPrompt;
use super::preferences::{PreferencesPage, PreferencesViewportState};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum MainPressed {
    #[default]
    None,
    Push(MainPushButton),
    Toggle(MainToggleButton),
    Slider(MainSlider),
}

impl MainPressed {
    pub(crate) fn render_parts(
        self,
    ) -> (
        Option<MainPushButton>,
        Option<MainToggleButton>,
        Option<MainSlider>,
    ) {
        match self {
            Self::None => (None, None, None),
            Self::Push(button) => (Some(button), None, None),
            Self::Toggle(button) => (None, Some(button), None),
            Self::Slider(slider) => (None, None, Some(slider)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum EqualizerPressed {
    #[default]
    None,
    Control(EqualizerControl),
    Slider(EqualizerSlider),
}

impl EqualizerPressed {
    pub(crate) fn render_parts(self) -> (Option<EqualizerControl>, Option<EqualizerSlider>) {
        match self {
            Self::None => (None, None),
            Self::Control(control) => (Some(control), None),
            Self::Slider(slider) => (None, Some(slider)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ActiveOverlay {
    #[default]
    None,
    PlaylistMenu(PlaylistMenuRenderKind),
    PlaylistSort,
    EqualizerPresets,
    ConfirmPhysicalDelete,
}

impl ActiveOverlay {
    pub(crate) fn is_open(self) -> bool {
        self != Self::None
    }

    pub(crate) fn playlist_menu(self) -> Option<PlaylistMenuRenderKind> {
        match self {
            Self::PlaylistMenu(kind) => Some(kind),
            _ => None,
        }
    }
}

pub struct EguiUiState {
    /// Deferred OS viewport callbacks are `'static`, so they exchange only
    /// transient dialog state through this shared lock, never domain state.
    pub file_info_viewport: Arc<Mutex<FileInfoViewportState>>,
    pub prompt_open: Option<EguiPrompt>,
    pub prompt_text: String,
    pub selected_preferences_page: PreferencesPage,
    /// See `file_info_viewport`: the preferences viewport has the same
    /// deferred-callback boundary and releases its lock before app dispatch.
    pub preferences_viewport: Arc<Mutex<PreferencesViewportState>>,
    pub playlist_menu_hover: Option<(PlaylistMenuRenderKind, usize)>,
    pub(crate) active_overlay: ActiveOverlay,
}

impl EguiUiState {
    pub fn new(config: &Config, page: PreferencesPage, preferences_open: bool) -> Self {
        Self {
            file_info_viewport: Arc::new(Mutex::new(FileInfoViewportState::default())),
            prompt_open: None,
            prompt_text: String::new(),
            selected_preferences_page: page,
            preferences_viewport: Arc::new(Mutex::new(PreferencesViewportState::new(
                config,
                page,
                preferences_open,
            ))),
            playlist_menu_hover: None,
            active_overlay: ActiveOverlay::None,
        }
    }

    pub(crate) fn dismiss_playlist_overlay(&mut self) {
        if matches!(
            self.active_overlay,
            ActiveOverlay::PlaylistMenu(_) | ActiveOverlay::PlaylistSort
        ) {
            self.active_overlay = ActiveOverlay::None;
            self.playlist_menu_hover = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playlist::PlaylistMenuKind;

    #[test]
    fn pressed_state_replacement_cannot_represent_two_controls() {
        let mut main = MainPressed::Push(MainPushButton::Play);
        assert_eq!(
            main.render_parts(),
            (Some(MainPushButton::Play), None, None)
        );
        main = MainPressed::Slider(MainSlider::Volume);
        assert_eq!(main.render_parts(), (None, None, Some(MainSlider::Volume)));

        let mut equalizer = EqualizerPressed::Control(EqualizerControl::On);
        assert_eq!(equalizer.render_parts(), (Some(EqualizerControl::On), None));
        equalizer = EqualizerPressed::Slider(EqualizerSlider::Preamp);
        assert_eq!(
            equalizer.render_parts(),
            (None, Some(EqualizerSlider::Preamp))
        );
    }

    #[test]
    fn active_overlay_replaces_and_dismisses_previous_overlay() {
        let mut overlay = ActiveOverlay::PlaylistMenu(PlaylistMenuKind::Misc);
        assert!(overlay.is_open());
        assert_eq!(overlay.playlist_menu(), Some(PlaylistMenuKind::Misc));

        overlay = ActiveOverlay::EqualizerPresets;
        assert!(overlay.is_open());
        assert_eq!(overlay.playlist_menu(), None);

        overlay = ActiveOverlay::None;
        assert!(!overlay.is_open());
    }
}
