//! Transient egui-only menus, prompts, and viewport state.

use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::render::PlaylistMenuRenderKind;

use super::file_info::FileInfoViewportState;
use super::menu::EguiPrompt;
use super::preferences::{PreferencesPage, PreferencesViewportState};

pub struct EguiUiState {
    pub file_info_viewport: Arc<Mutex<FileInfoViewportState>>,
    pub prompt_open: Option<EguiPrompt>,
    pub prompt_text: String,
    pub selected_preferences_page: PreferencesPage,
    pub preferences_viewport: Arc<Mutex<PreferencesViewportState>>,
    pub equalizer_presets_open: bool,
    pub playlist_menu_hover: Option<(PlaylistMenuRenderKind, usize)>,
    pub playlist_menu_open: Option<PlaylistMenuRenderKind>,
    pub playlist_sort_menu_open: bool,
    pub confirm_physical_delete_open: bool,
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
            equalizer_presets_open: false,
            playlist_menu_hover: None,
            playlist_menu_open: None,
            playlist_sort_menu_open: false,
            confirm_physical_delete_open: false,
        }
    }
}
