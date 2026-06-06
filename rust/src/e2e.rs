use std::path::Path;
use std::path::PathBuf;

use crate::app_state::AppState;
use crate::config::Config;
use crate::mpris::{
    gio_service::introspection_interfaces, MprisCommand, MprisEvent, BUS_NAME, OBJECT_PATH,
    PLAYER_INTERFACE, ROOT_INTERFACE,
};
use crate::player::{OutputDevice, OutputDeviceSelection, PlayerState};
use crate::playlist::PlaylistSortKey;
use crate::render::{
    equalizer_window_height, main_window_height, MainPushButton, MainSlider, MainToggleButton,
};
use crate::skin::widget::{
    VisAnalyzerMode, VisAnalyzerStyle, VisFalloffSpeed, VisMode, VisScopeMode, VisVuMode,
};
use crate::spotify::{SpotifyPlaylist, SpotifyTrack};
use crate::ui::{
    MainWindowUiState, PanelAction, PanelKind, PlaylistContextAction, PlaylistMenuKind,
    PlaylistSortAction, PreferencesPage, SpotifyChooserPage, UiAction,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSettings {
    config: Config,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self {
            config: Config::default(),
        }
    }
}

impl PlayerSettings {
    pub fn with_playlist_visible(mut self, visible: bool) -> Self {
        self.config.playlist_visible = visible;
        self
    }

    pub fn with_equalizer_visible(mut self, visible: bool) -> Self {
        self.config.equalizer_visible = visible;
        self
    }

    pub fn with_playlist_detached(mut self, detached: bool) -> Self {
        self.config.playlist_detached = detached;
        self
    }

    pub fn with_equalizer_detached(mut self, detached: bool) -> Self {
        self.config.equalizer_detached = detached;
        self
    }

    pub fn with_volume(mut self, volume: i32) -> Self {
        self.config.volume = volume.clamp(0, 100);
        self
    }

    pub fn with_balance(mut self, balance: i32) -> Self {
        self.config.balance = balance.clamp(-100, 100);
        self
    }

    pub fn with_visualization_mode(mut self, mode: VisMode) -> Self {
        self.config.vis_mode = mode;
        self
    }

    pub fn with_visualization_analyzer_style(mut self, style: VisAnalyzerStyle) -> Self {
        self.config.vis_analyzer_style = style;
        self
    }

    pub fn with_visualization_analyzer_mode(mut self, mode: VisAnalyzerMode) -> Self {
        self.config.vis_analyzer_mode = mode;
        self
    }

    pub fn with_visualization_scope_mode(mut self, mode: VisScopeMode) -> Self {
        self.config.vis_scope_mode = mode;
        self
    }

    pub fn with_visualization_peaks_enabled(mut self, enabled: bool) -> Self {
        self.config.vis_peaks_enabled = enabled;
        self
    }

    pub fn with_visualization_vu_mode(mut self, mode: VisVuMode) -> Self {
        self.config.vis_vu_mode = mode;
        self
    }

    pub fn with_visualization_refresh_divisor(mut self, divisor: i32) -> Self {
        self.config.vis_refresh_divisor = divisor.clamp(1, 8);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Player,
    Playlist,
    Equalizer,
    Preferences,
    OpenLocation,
    JumpTime,
    SkinBrowser,
    OutputDevicePicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTarget {
    Push(MainPushButton),
    Toggle(MainToggleButton),
    Slider(MainSlider, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelTarget {
    EqualizerShade,
    EqualizerClose,
    EqualizerOn,
    EqualizerAuto,
    EqualizerPresets,
    PlaylistShade,
    PlaylistClose,
    PlaylistAdd,
    PlaylistRemove,
    PlaylistSelect,
    PlaylistMisc,
    PlaylistList,
}

impl PanelTarget {
    fn point(self, state: &MainWindowUiState) -> (PanelKind, i32, i32) {
        let (playlist_width, playlist_height) = state.playlist_size();
        let playlist_button_y = playlist_height - 20;
        match self {
            Self::EqualizerShade => (PanelKind::Equalizer, 258, 7),
            Self::EqualizerClose => (PanelKind::Equalizer, 268, 7),
            Self::EqualizerOn => (PanelKind::Equalizer, 20, 24),
            Self::EqualizerAuto => (PanelKind::Equalizer, 50, 24),
            Self::EqualizerPresets => (PanelKind::Equalizer, 230, 24),
            Self::PlaylistShade => (PanelKind::Playlist, 258, 7),
            Self::PlaylistClose => (PanelKind::Playlist, 268, 7),
            Self::PlaylistAdd => (PanelKind::Playlist, 24, playlist_button_y),
            Self::PlaylistRemove => (PanelKind::Playlist, 53, playlist_button_y),
            Self::PlaylistSelect => (PanelKind::Playlist, 82, playlist_button_y),
            Self::PlaylistMisc => (PanelKind::Playlist, 111, playlist_button_y),
            Self::PlaylistList => (PanelKind::Playlist, playlist_width - 35, playlist_button_y),
        }
    }

    fn click(self, state: &mut MainWindowUiState) {
        let (kind, x, y) = self.point(state);
        match self {
            Self::EqualizerShade | Self::EqualizerClose => {
                state.panel_click(kind, x, y);
            }
            Self::EqualizerOn => {
                state.equalizer_press(x, y);
                state.equalizer_release(x, y);
            }
            Self::EqualizerAuto => {
                state.equalizer_press(x, y);
                state.equalizer_release(x, y);
            }
            Self::EqualizerPresets => {
                state.equalizer_press(x, y);
                state.equalizer_release(x, y);
            }
            Self::PlaylistShade
            | Self::PlaylistClose
            | Self::PlaylistAdd
            | Self::PlaylistRemove
            | Self::PlaylistSelect
            | Self::PlaylistMisc
            | Self::PlaylistList => {
                state.panel_click(kind, x, y);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    OpenFiles,
    OpenLocation,
    Preferences,
    SkinBrowser,
    Spotify,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    OpenFiles,
    OpenDirectory,
    ToggleRepeat,
    ToggleShuffle,
    Preferences,
    OpenLocation,
    ToggleNoAdvance,
    ShadeMain,
    JumpTime,
    SkinBrowser,
    ReloadSkin,
    TogglePlaylist,
    ToggleEqualizer,
    ShadePlaylist,
    ShadeEqualizer,
    TimerElapsed,
    TimerRemaining,
    ToggleSticky,
    ToggleDoubleSize,
    FileInfo,
    PlayFirst,
}

impl MainTarget {
    pub const MENU: Self = Self::Push(MainPushButton::Menu);
    pub const MINIMIZE: Self = Self::Push(MainPushButton::Minimize);
    pub const SHADE: Self = Self::Push(MainPushButton::Shade);
    pub const CLOSE: Self = Self::Push(MainPushButton::Close);
    pub const PREVIOUS: Self = Self::Push(MainPushButton::Previous);
    pub const PLAY: Self = Self::Push(MainPushButton::Play);
    pub const PAUSE: Self = Self::Push(MainPushButton::Pause);
    pub const STOP: Self = Self::Push(MainPushButton::Stop);
    pub const NEXT: Self = Self::Push(MainPushButton::Next);
    pub const EJECT: Self = Self::Push(MainPushButton::Eject);
    pub const PLAYLIST: Self = Self::Toggle(MainToggleButton::Playlist);
    pub const EQUALIZER: Self = Self::Toggle(MainToggleButton::Equalizer);
    pub const SHUFFLE: Self = Self::Toggle(MainToggleButton::Shuffle);
    pub const REPEAT: Self = Self::Toggle(MainToggleButton::Repeat);

    pub fn volume(position: i32) -> Self {
        Self::Slider(MainSlider::Volume, position)
    }

    pub fn balance(position: i32) -> Self {
        Self::Slider(MainSlider::Balance, position)
    }

    pub fn position(position: i32) -> Self {
        Self::Slider(MainSlider::Position, position)
    }

    fn point(self) -> (i32, i32) {
        match self {
            Self::Push(button) => center(push_button_rect(button)),
            Self::Toggle(toggle) => center(toggle_button_rect(toggle)),
            Self::Slider(slider, position) => {
                let rect = slider_rect(slider);
                let position = position.clamp(0, slider_max(slider));
                (
                    rect.x + position + slider_knob_width(slider) / 2,
                    rect.y + rect.height / 2,
                )
            }
        }
    }
}

#[derive(Debug)]
pub struct UiE2e {
    main_visible: bool,
    main_minimized: bool,
    state: MainWindowUiState,
    playlist_visible: bool,
    equalizer_visible: bool,
    preferences_visible: bool,
    open_location_visible: bool,
    jump_time_visible: bool,
    skin_browser_visible: bool,
    output_device_picker_visible: bool,
    file_dialog_visible: bool,
    directory_dialog_visible: bool,
}

impl UiE2e {
    pub fn start_player(settings: PlayerSettings) -> Self {
        Self::start_from_app_state(AppState::from_config(settings.config))
    }

    pub fn start_from_app_state(app_state: AppState) -> Self {
        let mut harness = Self {
            main_visible: true,
            main_minimized: false,
            state: MainWindowUiState::from_app_state(app_state),
            playlist_visible: false,
            equalizer_visible: false,
            preferences_visible: false,
            open_location_visible: false,
            jump_time_visible: false,
            skin_browser_visible: false,
            output_device_picker_visible: false,
            file_dialog_visible: false,
            directory_dialog_visible: false,
        };
        harness.sync_windows();
        harness
    }

    pub fn click(&mut self, target: MainTarget) -> &mut Self {
        let (x, y) = target.point();
        let action = self.state.click(x, y);
        self.apply_action(action);
        self.sync_windows();
        self
    }

    pub fn click_at(&mut self, x: i32, y: i32) -> &mut Self {
        let action = self.state.click(x, y);
        self.apply_action(action);
        self.sync_windows();
        self
    }

    pub fn click_panel(&mut self, target: PanelTarget) -> &mut Self {
        target.click(&mut self.state);
        self.sync_windows();
        self
    }

    pub fn click_docked_panel(&mut self, target: PanelTarget) -> &mut Self {
        let (kind, x, y) = target.point(&self.state);
        let mut offset_y = main_window_height(self.state.is_shaded());
        if kind == PanelKind::Playlist
            && self.state.app_state_mut().config.equalizer_visible
            && !self.state.app_state_mut().config.equalizer_detached
        {
            offset_y += equalizer_window_height(self.state.is_equalizer_shaded());
        }
        let Some((actual_kind, panel_x, panel_y)) = self.state.docked_panel_at(x, y + offset_y)
        else {
            panic!("expected {target:?} to hit a docked panel");
        };
        assert_eq!(actual_kind, kind);
        match actual_kind {
            PanelKind::Equalizer => {
                let title_action = self.state.panel_click(actual_kind, panel_x, panel_y);
                if title_action == PanelAction::None {
                    self.state.equalizer_release(panel_x, panel_y);
                }
            }
            PanelKind::Playlist => {
                self.state.panel_click(actual_kind, panel_x, panel_y);
            }
        }
        self.sync_windows();
        self
    }

    pub fn press_playlist_menu_item(&mut self, item: usize) -> &mut Self {
        let (x, y0) = self.playlist_menu_anchor();
        let y = 174 + item as i32 * 18 + 8;
        self.state.playlist_press(x, y0 + (y - 174));
        self
    }

    pub fn activate_playlist_menu_item(&mut self, item: usize) -> &mut Self {
        let (x, y0) = self.playlist_menu_anchor();
        let y = 174 + item as i32 * 18 + 8;
        self.state.playlist_press(x, y0 + (y - 174));
        match self.state.playlist_release(x, y0 + (y - 174)) {
            PanelAction::OpenDirectoryDialog => self.state.set_directory_dialog_visible(true),
            PanelAction::OpenFileDialog => self.state.set_file_dialog_visible(true),
            PanelAction::OpenLocationWindow => self.state.set_open_location_visible(true),
            PanelAction::OpenPlaylistLoadDialog => {
                self.state.set_playlist_load_dialog_visible(true)
            }
            PanelAction::OpenPlaylistSaveDialog => {
                self.state.set_playlist_save_dialog_visible(true)
            }
            PanelAction::ShowPlaylistSortMenu => {}
            _ => {}
        }
        self.sync_windows();
        self
    }

    pub fn activate_playlist_sort_action(&mut self, action: PlaylistSortAction) -> &mut Self {
        self.state.activate_playlist_sort_action(action);
        self.sync_windows();
        self
    }

    pub fn activate_playlist_context_action(&mut self, action: PlaylistContextAction) -> &mut Self {
        self.state.activate_playlist_context_action(action);
        self.sync_windows();
        self
    }

    pub fn accept_playlist_load(&mut self, path: &Path) -> &mut Self {
        self.state
            .load_playlist_file(path)
            .expect("playlist load should succeed");
        self.state.set_playlist_load_dialog_visible(false);
        self.sync_windows();
        self
    }

    pub fn accept_playlist_save(&mut self, path: &Path) -> &mut Self {
        self.state
            .save_playlist_file(path)
            .expect("playlist save should succeed");
        self.state.set_playlist_save_dialog_visible(false);
        self.sync_windows();
        self
    }

    pub fn start_playlist_search(&mut self) -> &mut Self {
        self.state.start_playlist_search();
        self.sync_windows();
        self
    }

    pub fn type_playlist_search(&mut self, text: &str) -> &mut Self {
        for ch in text.chars() {
            self.state.push_playlist_search_char(ch);
        }
        self.sync_windows();
        self
    }

    pub fn backspace_playlist_search(&mut self) -> &mut Self {
        self.state.pop_playlist_search_char();
        self.sync_windows();
        self
    }

    pub fn stop_playlist_search(&mut self) -> &mut Self {
        self.state.stop_playlist_search();
        self.sync_windows();
        self
    }

    pub fn hover_playlist_menu_item(&mut self, item: usize) -> &mut Self {
        let (x, y0) = self.playlist_menu_anchor();
        let y = 174 + item as i32 * 18 + 8;
        self.state.playlist_motion(x, y0 + (y - 174));
        self
    }

    pub fn resize_playlist(&mut self, width: i32, height: i32) -> &mut Self {
        self.state.set_playlist_size(width, height);
        self
    }

    pub fn resize_docked_playlist_vertically(&mut self, height: i32) -> &mut Self {
        let (_, current_height) = self.state.playlist_size();
        assert!(
            self.state.begin_docked_playlist_resize(current_height - 1),
            "expected docked playlist vertical resize to start"
        );
        let main_y = main_window_height(self.state.is_shaded())
            + if self.state.app_state_mut().config.equalizer_visible
                && !self.state.app_state_mut().config.equalizer_detached
            {
                equalizer_window_height(self.state.is_equalizer_shaded())
            } else {
                0
            }
            + height
            - 1;
        self.state.docked_playlist_resize_motion(main_y);
        self.state.end_docked_playlist_resize();
        self.sync_windows();
        self
    }

    pub fn drag_playlist_scrollbar_to_bottom(&mut self) -> &mut Self {
        let (width, height) = self.state.playlist_size();
        let x = width - 12;
        let start_y = 20;
        let end_y = height - 39;
        assert!(
            self.state.playlist_scrollbar_press(x, start_y),
            "expected playlist scrollbar press to start dragging"
        );
        self.state.playlist_scrollbar_motion(x, end_y);
        self.state.playlist_scrollbar_release();
        self.sync_windows();
        self
    }

    pub fn click_playlist_row(&mut self, index: usize) -> &mut Self {
        let y = self.playlist_row_y(index);
        assert!(
            self.state.playlist_press(20, y),
            "expected playlist row {index} press to select an entry"
        );
        self.state.playlist_entry_release();
        self.sync_windows();
        self
    }

    pub fn double_click_playlist_row(&mut self, index: usize) -> &mut Self {
        let y = self.playlist_row_y(index);
        assert!(
            self.state.playlist_press(20, y),
            "expected first playlist row {index} press to select an entry"
        );
        self.state.playlist_entry_release();
        assert!(
            self.state.playlist_press(20, y),
            "expected second playlist row {index} press to start playback"
        );
        self.state.playlist_entry_release();
        self.sync_windows();
        self
    }

    pub fn drag_playlist_row(&mut self, from: usize, to: usize) -> &mut Self {
        let from_y = self.playlist_row_y(from);
        let to_y = self.playlist_row_y(to);
        assert!(
            self.state.playlist_press(20, from_y),
            "expected playlist row {from} press to start dragging"
        );
        self.state.playlist_motion(20, to_y);
        self.state.playlist_entry_release();
        self.sync_windows();
        self
    }

    fn playlist_row_y(&self, index: usize) -> i32 {
        let offset = self.state.playlist_scroll_offset();
        assert!(
            index >= offset,
            "playlist entry {index} is above visible offset {offset}"
        );
        let visible_row = index - offset;
        assert!(
            visible_row < ((self.state.playlist_size().1 - 58).max(0) / 11) as usize,
            "playlist entry {index} is not visible"
        );
        20 + visible_row as i32 * 11 + 5
    }

    pub fn start_playlist_size(&mut self, width: i32, height: i32) -> &mut Self {
        self.state.set_playlist_size(width, height);
        self.state.set_playlist_visible(true);
        self.sync_windows();
        self
    }

    pub fn drag_equalizer_preamp(&mut self, position: i32) -> &mut Self {
        self.drag_equalizer_slider(21, position)
    }

    pub fn drag_equalizer_band(&mut self, band: usize, position: i32) -> &mut Self {
        self.drag_equalizer_slider(78 + band as i32 * 18, position)
    }

    pub fn drag_shaded_equalizer_volume(&mut self, position: i32) -> &mut Self {
        self.drag_equalizer_shaded_slider(61, position.clamp(0, 94))
    }

    pub fn drag_shaded_equalizer_balance(&mut self, position: i32) -> &mut Self {
        self.drag_equalizer_shaded_slider(164, position.clamp(0, 39))
    }

    pub fn apply_equalizer_preset(&mut self, preset: i32) -> &mut Self {
        self.state.apply_equalizer_preset(preset);
        self
    }

    pub fn click_menu_item(&mut self, item: MenuItem) -> &mut Self {
        self.state.set_menu_visible(false);
        match item {
            MenuItem::OpenFiles => {
                self.file_dialog_visible = true;
                self.state.set_file_dialog_visible(true);
            }
            MenuItem::OpenLocation => {
                self.state.set_open_location_visible(true);
            }
            MenuItem::Preferences => {
                self.state.set_preferences_visible(true);
            }
            MenuItem::SkinBrowser => {
                self.state.set_skin_browser_visible(true);
            }
            MenuItem::Spotify => {
                self.state.open_spotify_window();
            }
            MenuItem::Quit => {
                self.apply_action(UiAction::Quit);
            }
        }
        self.sync_windows();
        self
    }

    pub fn press_shortcut(&mut self, shortcut: Shortcut) -> &mut Self {
        match shortcut {
            Shortcut::Previous => {
                self.state.activate_push(MainPushButton::Previous);
            }
            Shortcut::Play => {
                self.state.activate_push(MainPushButton::Play);
            }
            Shortcut::Pause => {
                self.state.activate_push(MainPushButton::Pause);
            }
            Shortcut::Stop => {
                self.state.activate_push(MainPushButton::Stop);
            }
            Shortcut::Next => {
                self.state.activate_push(MainPushButton::Next);
            }
            Shortcut::OpenFiles => {
                self.file_dialog_visible = true;
                self.state.set_file_dialog_visible(true);
            }
            Shortcut::OpenDirectory => {
                self.directory_dialog_visible = true;
                self.state.set_directory_dialog_visible(true);
            }
            Shortcut::ToggleRepeat => {
                self.state.activate_toggle(MainToggleButton::Repeat);
            }
            Shortcut::ToggleShuffle => {
                self.state.activate_toggle(MainToggleButton::Shuffle);
            }
            Shortcut::Preferences => {
                self.state.set_preferences_visible(true);
            }
            Shortcut::OpenLocation => self.state.set_open_location_visible(true),
            Shortcut::ToggleNoAdvance => {
                let enabled = !self.state.no_advance();
                self.state.set_no_advance(enabled);
            }
            Shortcut::ShadeMain => {
                self.state.toggle_shaded();
            }
            Shortcut::JumpTime => self.state.set_jump_time_visible(true),
            Shortcut::SkinBrowser => {
                self.state.set_skin_browser_visible(true);
            }
            Shortcut::ReloadSkin => {
                self.state.reload_skin();
            }
            Shortcut::TogglePlaylist => {
                self.state.activate_toggle(MainToggleButton::Playlist);
            }
            Shortcut::ToggleEqualizer => {
                self.state.activate_toggle(MainToggleButton::Equalizer);
            }
            Shortcut::ShadePlaylist => {
                self.state.toggle_playlist_shaded();
            }
            Shortcut::ShadeEqualizer => {
                self.state.toggle_equalizer_shaded();
            }
            Shortcut::TimerElapsed => self.state.set_preference_timer_remaining(false),
            Shortcut::TimerRemaining => self.state.set_preference_timer_remaining(true),
            Shortcut::ToggleSticky => self.state.toggle_sticky(),
            Shortcut::ToggleDoubleSize => self.state.toggle_double_size(),
            Shortcut::FileInfo => self.state.show_current_file_info(),
            Shortcut::PlayFirst => self.state.play_first_playlist_entry(),
        }
        self.sync_windows();
        self
    }

    pub fn drop_on_main<I, S>(&mut self, uris: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.state.accept_dropped_uris(uris, true, true);
        self.sync_windows();
        self
    }

    pub fn drop_on_playlist<I, S>(&mut self, uris: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.state.accept_dropped_uris(uris, false, false);
        self.sync_windows();
        self
    }

    pub fn accept_file_dialog<I, S>(&mut self, uris: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.state.set_file_dialog_visible(false);
        self.file_dialog_visible = false;
        self.state.accept_opened_uris(uris);
        self.sync_windows();
        self
    }

    pub fn accept_playlist_add_file_dialog<I, S>(&mut self, uris: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.state.set_file_dialog_visible(false);
        self.file_dialog_visible = false;
        self.state.accept_dropped_uris(uris, false, false);
        self.sync_windows();
        self
    }

    pub fn accept_directory_dialog(&mut self, uri: &str) -> &mut Self {
        self.state.set_directory_dialog_visible(false);
        self.directory_dialog_visible = false;
        self.state.accept_opened_uris([uri]);
        self.sync_windows();
        self
    }

    pub fn accept_playlist_add_directory_dialog(&mut self, uri: &str) -> &mut Self {
        self.state.set_directory_dialog_visible(false);
        self.directory_dialog_visible = false;
        self.state.accept_dropped_uris([uri], false, false);
        self.sync_windows();
        self
    }

    pub fn add_spotify_entry(&mut self, uri: &str, title: &str, duration_ms: i64) -> &mut Self {
        self.state.add_spotify_entry(uri, title, duration_ms);
        self.sync_windows();
        self
    }

    pub fn set_spotify_authenticated(&mut self, authenticated: bool) -> &mut Self {
        self.state.set_spotify_authenticated(authenticated);
        self
    }

    pub fn assert_spotify_window_visible(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.spotify_window_visible(), expected);
        self
    }

    pub fn assert_spotify_auth_prompt_visible(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.spotify_auth_prompt_visible(), expected);
        self
    }

    pub fn assert_spotify_page(&mut self, expected: SpotifyChooserPage) -> &mut Self {
        assert_eq!(self.state.spotify_page(), expected);
        self
    }

    pub fn assert_spotify_status(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.spotify_status(), expected);
        self
    }

    pub fn receive_spotify_playlists(&mut self, playlists: Vec<SpotifyPlaylist>) -> &mut Self {
        self.state.set_spotify_playlists(playlists);
        self
    }

    pub fn assert_spotify_playlists(&mut self, expected: &[&str]) -> &mut Self {
        assert_eq!(self.state.spotify_playlist_names(), expected);
        self
    }

    pub fn select_spotify_playlist(&mut self, index: usize) -> &mut Self {
        assert!(self.state.select_spotify_playlist(index));
        self
    }

    pub fn assert_spotify_last_track_request(&mut self, expected: Option<&str>) -> &mut Self {
        assert_eq!(self.state.spotify_last_track_request(), expected);
        self
    }

    pub fn receive_spotify_tracks(&mut self, tracks: Vec<SpotifyTrack>) -> &mut Self {
        self.state.set_spotify_tracks(tracks);
        self
    }

    pub fn assert_spotify_tracks(&mut self, expected: &[&str]) -> &mut Self {
        let expected: Vec<String> = expected.iter().map(|value| value.to_string()).collect();
        assert_eq!(self.state.spotify_track_titles(), expected);
        self
    }

    pub fn spotify_back_to_playlists(&mut self) -> &mut Self {
        self.state.show_spotify_playlists_page();
        self
    }

    pub fn set_spotify_error(&mut self, message: &str) -> &mut Self {
        self.state.set_spotify_error(message);
        self
    }

    pub fn load_spotify_tracks_into_playlist(&mut self) -> &mut Self {
        assert!(self.state.load_spotify_tracks_into_playlist());
        self.sync_windows();
        self
    }

    pub fn close_spotify_window(&mut self) -> &mut Self {
        self.state.close_spotify_window();
        self
    }

    pub fn add_podcast_entry(
        &mut self,
        uri: &str,
        title: &str,
        feed: &str,
        guid: &str,
    ) -> &mut Self {
        self.state.add_podcast_entry(
            uri,
            Some(title.to_string()),
            Some(feed.to_string()),
            Some(guid.to_string()),
        );
        self.sync_windows();
        self
    }

    pub fn select_playlist_entry(&mut self, index: usize) -> &mut Self {
        self.state.set_playlist_entry_selected(index, true);
        self
    }

    pub fn sort_playlist_by(&mut self, key: PlaylistSortKey) -> &mut Self {
        self.state.sort_playlist_by(key);
        self.sync_windows();
        self
    }

    pub fn sort_selected_playlist_by(&mut self, key: PlaylistSortKey) -> &mut Self {
        self.state.sort_selected_playlist_by(key);
        self.sync_windows();
        self
    }

    pub fn reverse_playlist(&mut self) -> &mut Self {
        self.state.reverse_playlist();
        self.sync_windows();
        self
    }

    pub fn randomize_playlist(&mut self) -> &mut Self {
        self.state.randomize_playlist();
        self.sync_windows();
        self
    }

    pub fn index_missing_playlist_durations(&mut self) -> &mut Self {
        self.state.index_missing_playlist_durations_for_e2e();
        self.sync_windows();
        self
    }

    pub fn queue_playlist_duration_result(
        &mut self,
        index: usize,
        length_ms: i64,
        title: Option<&str>,
    ) -> &mut Self {
        self.state.queue_playlist_duration_result_for_e2e(
            index,
            length_ms,
            title.map(ToString::to_string),
        );
        self
    }

    pub fn update_timer_tick(&mut self, elapsed_ms: u32) -> &mut Self {
        self.state.update_timer_tick(elapsed_ms);
        self.sync_windows();
        self
    }

    pub fn set_stream_channels(&mut self, channels: i32) -> &mut Self {
        self.state.set_stream_channels_for_e2e(channels);
        self
    }

    pub fn save_runtime_snapshot(&mut self, config_path: &Path, playlist_path: &Path) -> &mut Self {
        self.state
            .save_runtime_snapshot_for_e2e(config_path, playlist_path)
            .expect("runtime snapshot should save");
        self
    }

    pub fn playlist_eof_reached(&mut self) -> &mut Self {
        self.state.playlist_eof_reached();
        self.sync_windows();
        self
    }

    pub fn show_jump_time_prompt(&mut self) -> &mut Self {
        self.state.set_jump_time_visible(true);
        self.sync_windows();
        self
    }

    pub fn accept_open_location(&mut self, text: &str) -> &mut Self {
        self.state.accept_open_location(text);
        self.sync_windows();
        self
    }

    pub fn accept_jump_time(&mut self, text: &str) -> &mut Self {
        self.state.accept_jump_time(text);
        self.sync_windows();
        self
    }

    pub fn assert_window_visible(&mut self, window: Window) -> &mut Self {
        assert!(
            self.is_window_visible(window),
            "expected {window:?} window to be visible"
        );
        self
    }

    pub fn assert_file_dialog_visible(&mut self) -> &mut Self {
        assert!(
            self.file_dialog_visible || self.state.is_file_dialog_visible(),
            "expected open file dialog to be visible"
        );
        self
    }

    pub fn assert_directory_dialog_visible(&mut self) -> &mut Self {
        assert!(
            self.directory_dialog_visible || self.state.is_directory_dialog_visible(),
            "expected open directory dialog to be visible"
        );
        self
    }

    pub fn assert_playlist_load_dialog_visible(&mut self) -> &mut Self {
        assert!(
            self.state.is_playlist_load_dialog_visible(),
            "expected playlist load dialog to be visible"
        );
        self
    }

    pub fn assert_playlist_save_dialog_visible(&mut self) -> &mut Self {
        assert!(
            self.state.is_playlist_save_dialog_visible(),
            "expected playlist save dialog to be visible"
        );
        self
    }

    pub fn assert_last_playlist_file_info(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.last_playlist_file_info(), Some(expected));
        self
    }

    pub fn assert_playlist_options_opened(&mut self) -> &mut Self {
        assert!(
            self.state.playlist_options_opened(),
            "expected playlist options action to be opened"
        );
        self
    }

    pub fn assert_window_hidden(&mut self, window: Window) -> &mut Self {
        assert!(
            !self.is_window_visible(window),
            "expected {window:?} window to be hidden"
        );
        self
    }

    pub fn assert_player_minimized(&mut self) -> &mut Self {
        assert!(
            self.main_minimized,
            "expected player window to be minimized"
        );
        self
    }

    pub fn assert_player_not_minimized(&mut self) -> &mut Self {
        assert!(
            !self.main_minimized,
            "expected player window not to be minimized"
        );
        self
    }

    pub fn assert_player_shaded(&mut self) -> &mut Self {
        assert!(self.state.is_shaded(), "expected player to be shaded");
        self
    }

    pub fn assert_player_unshaded(&mut self) -> &mut Self {
        assert!(!self.state.is_shaded(), "expected player to be unshaded");
        self
    }

    pub fn assert_menu_visible(&mut self) -> &mut Self {
        assert!(
            self.state.is_menu_visible(),
            "expected main menu to be visible"
        );
        self
    }

    pub fn assert_menu_hidden(&mut self) -> &mut Self {
        assert!(
            !self.state.is_menu_visible(),
            "expected main menu to be hidden"
        );
        self
    }

    pub fn assert_equalizer_shaded(&mut self) -> &mut Self {
        assert!(
            self.state.is_equalizer_shaded(),
            "expected equalizer to be shaded"
        );
        self
    }

    pub fn assert_equalizer_unshaded(&mut self) -> &mut Self {
        assert!(
            !self.state.is_equalizer_shaded(),
            "expected equalizer to be unshaded"
        );
        self
    }

    pub fn assert_playlist_shaded(&mut self) -> &mut Self {
        assert!(
            self.state.is_playlist_shaded(),
            "expected playlist to be shaded"
        );
        self
    }

    pub fn assert_playlist_unshaded(&mut self) -> &mut Self {
        assert!(
            !self.state.is_playlist_shaded(),
            "expected playlist to be unshaded"
        );
        self
    }

    pub fn assert_playlist_menu(&mut self, expected: PlaylistMenuKind) -> &mut Self {
        assert_eq!(
            self.state.playlist_menu(),
            Some(expected),
            "expected playlist {expected:?} menu to be open"
        );
        self
    }

    pub fn assert_no_playlist_menu(&mut self) -> &mut Self {
        assert_eq!(
            self.state.playlist_menu(),
            None,
            "expected no playlist menu to be open"
        );
        self
    }

    pub fn assert_playlist_menu_hover(&mut self, expected: Option<usize>) -> &mut Self {
        assert_eq!(
            self.state.playlist_menu_hover(),
            expected,
            "expected playlist menu hover to be {expected:?}"
        );
        self
    }

    pub fn assert_playlist_size(&mut self, width: i32, height: i32) -> &mut Self {
        assert_eq!(self.state.playlist_size(), (width, height));
        self
    }

    pub fn assert_playlist_scroll_offset(&mut self, expected: usize) -> &mut Self {
        assert_eq!(self.state.playlist_scroll_offset(), expected);
        self
    }

    pub fn assert_playlist_scrollbar_visible(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.playlist_scrollbar_visible(), expected);
        self
    }

    pub fn assert_playlist_entry_selected(&mut self, index: usize, expected: bool) -> &mut Self {
        assert_eq!(self.state.playlist_entry_selected(index), Some(expected));
        self
    }

    pub fn assert_playlist_footer_info(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.playlist_footer_info(), expected);
        self
    }

    pub fn assert_playlist_search_active(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.playlist_search_active(), expected);
        self
    }

    pub fn assert_playlist_search_query(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.playlist_search_query(), expected);
        self
    }

    pub fn assert_visible_playlist_entry(&mut self, row: usize, expected: &str) -> &mut Self {
        assert_eq!(self.state.visible_playlist_entry_uri(row), Some(expected));
        self
    }

    pub fn focus_panel(&mut self, panel: PanelKind, focused: bool) -> &mut Self {
        self.state.set_panel_focused(panel, focused);
        self
    }

    pub fn detach_panel(&mut self, panel: PanelKind) -> &mut Self {
        self.state.set_panel_detached(panel, true);
        self.sync_windows();
        self
    }

    pub fn dock_panel(&mut self, panel: PanelKind) -> &mut Self {
        self.state.set_panel_detached(panel, false);
        self.sync_windows();
        self
    }

    pub fn assert_panel_detached(&mut self, panel: PanelKind, expected: bool) -> &mut Self {
        assert_eq!(self.state.is_panel_detached(panel), expected);
        self
    }

    pub fn assert_panel_focused(&mut self, panel: PanelKind, expected: bool) -> &mut Self {
        assert_eq!(
            self.state.is_panel_focused(panel),
            expected,
            "expected {panel:?} focused state to be {expected}"
        );
        self
    }

    pub fn assert_docked_panel_size(&mut self, expected: (i32, i32)) -> &mut Self {
        assert_eq!(self.state.docked_panel_size(), expected);
        self
    }

    pub fn open_preferences_page(&mut self, page: PreferencesPage) -> &mut Self {
        self.state.set_preferences_visible(true);
        self.state.set_preferences_page(page);
        self.sync_windows();
        self
    }

    pub fn assert_preferences_page(&mut self, expected: PreferencesPage) -> &mut Self {
        assert_eq!(self.state.preferences_page(), expected);
        self
    }

    pub fn assert_preferences_saved(&mut self) -> &mut Self {
        assert!(self.state.preferences_saved());
        self
    }

    pub fn reset_preferences_to_defaults(&mut self) -> &mut Self {
        self.state.reset_preferences_to_defaults();
        self.sync_windows();
        self
    }

    pub fn set_preference_output_device(&mut self, device: Option<&str>) -> &mut Self {
        self.state
            .set_preference_output_device(device.map(ToString::to_string));
        self
    }

    pub fn assert_preference_output_device(&mut self, expected: Option<&str>) -> &mut Self {
        assert_eq!(self.state.preference_output_device(), expected);
        self
    }

    pub fn set_preference_volume(&mut self, volume: i32) -> &mut Self {
        self.state.set_preference_volume(volume);
        self
    }

    pub fn set_preference_balance(&mut self, balance: i32) -> &mut Self {
        self.state.set_preference_balance(balance);
        self
    }

    pub fn set_preference_repeat(&mut self, enabled: bool) -> &mut Self {
        self.state.set_preference_repeat(enabled);
        self
    }

    pub fn set_preference_shuffle(&mut self, enabled: bool) -> &mut Self {
        self.state.set_preference_shuffle(enabled);
        self
    }

    pub fn set_preference_no_playlist_advance(&mut self, enabled: bool) -> &mut Self {
        self.state.set_preference_no_playlist_advance(enabled);
        self
    }

    pub fn assert_no_playlist_advance(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.preference_no_playlist_advance(), expected);
        self
    }

    pub fn set_preference_timer_remaining(&mut self, enabled: bool) -> &mut Self {
        self.state.set_preference_timer_remaining(enabled);
        self
    }

    pub fn assert_preference_timer_remaining(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.preference_timer_remaining(), expected);
        self
    }

    pub fn set_preference_playlist_docked(&mut self, docked: bool) -> &mut Self {
        self.state.set_preference_playlist_docked(docked);
        self.sync_windows();
        self
    }

    pub fn set_preference_equalizer_docked(&mut self, docked: bool) -> &mut Self {
        self.state.set_preference_equalizer_docked(docked);
        self.sync_windows();
        self
    }

    pub fn set_preference_convert_underscore(&mut self, enabled: bool) -> &mut Self {
        self.state.set_preference_convert_underscore(enabled);
        self
    }

    pub fn assert_preference_convert_underscore(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.preference_convert_underscore(), expected);
        self
    }

    pub fn set_preference_convert_twenty(&mut self, enabled: bool) -> &mut Self {
        self.state.set_preference_convert_twenty(enabled);
        self
    }

    pub fn assert_preference_convert_twenty(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.preference_convert_twenty(), expected);
        self
    }

    pub fn set_preference_show_numbers_in_playlist(&mut self, enabled: bool) -> &mut Self {
        self.state.set_preference_show_numbers_in_playlist(enabled);
        self
    }

    pub fn assert_preference_show_numbers_in_playlist(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.preference_show_numbers_in_playlist(), expected);
        self
    }

    pub fn set_preference_playlist_font(&mut self, font: &str) -> &mut Self {
        self.state.set_preference_playlist_font(font);
        self
    }

    pub fn assert_preference_playlist_font(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.preference_playlist_font(), expected);
        self
    }

    pub fn set_preference_mainwin_font(&mut self, font: &str) -> &mut Self {
        self.state.set_preference_mainwin_font(font);
        self
    }

    pub fn assert_preference_mainwin_font(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.preference_mainwin_font(), expected);
        self
    }

    pub fn set_preference_title_format(&mut self, format: &str) -> &mut Self {
        self.state.set_preference_title_format(format);
        self
    }

    pub fn assert_preference_title_format(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.preference_title_format(), expected);
        self
    }

    pub fn set_preference_podcast_cache_ttl_days(&mut self, days: i32) -> &mut Self {
        self.state.set_preference_podcast_cache_ttl_days(days);
        self
    }

    pub fn assert_preference_podcast_cache_ttl_days(&mut self, expected: i32) -> &mut Self {
        assert_eq!(self.state.preference_podcast_cache_ttl_days(), expected);
        self
    }

    pub fn set_preference_podcast_refresh_interval_minutes(&mut self, minutes: i32) -> &mut Self {
        self.state
            .set_preference_podcast_refresh_interval_minutes(minutes);
        self
    }

    pub fn assert_preference_podcast_refresh_interval_minutes(
        &mut self,
        expected: i32,
    ) -> &mut Self {
        assert_eq!(
            self.state.preference_podcast_refresh_interval_minutes(),
            expected
        );
        self
    }

    pub fn scan_skin_browser_dirs(&mut self, dirs: &[PathBuf]) -> &mut Self {
        self.state
            .scan_skin_browser_dirs(dirs)
            .expect("skin browser directory scan should succeed");
        self
    }

    pub fn assert_skin_browser_entries(&mut self, expected: &[&str]) -> &mut Self {
        let actual: Vec<&str> = self
            .state
            .skin_browser_entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(actual, expected);
        self
    }

    pub fn select_skin_browser_index(&mut self, index: usize) -> &mut Self {
        assert!(
            self.state.select_skin_browser_index(index),
            "expected skin index {index} to be selectable"
        );
        self
    }

    pub fn assert_selected_skin_index(&mut self, expected: usize) -> &mut Self {
        assert_eq!(self.state.selected_skin_index(), expected);
        self
    }

    pub fn assert_selected_skin_path(&mut self, expected: Option<&Path>) -> &mut Self {
        assert_eq!(
            self.state.selected_skin().map(PathBuf::from),
            expected.map(PathBuf::from)
        );
        self
    }

    pub fn reload_skin(&mut self) -> &mut Self {
        self.state.reload_skin();
        self
    }

    pub fn assert_skin_reload_count(&mut self, expected: u32) -> &mut Self {
        assert_eq!(self.state.skin_reload_count(), expected);
        self
    }

    pub fn assert_sticky(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.sticky(), expected);
        self
    }

    pub fn assert_double_size(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.double_size(), expected);
        self
    }

    pub fn open_output_device_picker(&mut self) -> &mut Self {
        self.state.set_output_device_picker_visible(true);
        self.sync_windows();
        self
    }

    pub fn set_output_devices(
        &mut self,
        system_devices: Vec<OutputDevice>,
        spotify_devices: Vec<OutputDevice>,
    ) -> &mut Self {
        self.state
            .set_output_devices(system_devices, spotify_devices);
        self
    }

    pub fn assert_local_output_devices(&mut self, expected: &[&str]) -> &mut Self {
        let actual: Vec<&str> = self
            .state
            .output_device_groups()
            .local
            .iter()
            .map(|device| device.display_name.as_str())
            .collect();
        assert_eq!(actual, expected);
        self
    }

    pub fn assert_network_output_devices(&mut self, expected: &[&str]) -> &mut Self {
        let actual: Vec<&str> = self
            .state
            .output_device_groups()
            .network
            .iter()
            .map(|device| device.display_name.as_str())
            .collect();
        assert_eq!(actual, expected);
        self
    }

    pub fn assert_spotify_output_devices(&mut self, expected: &[&str]) -> &mut Self {
        let actual: Vec<&str> = self
            .state
            .output_device_groups()
            .spotify
            .iter()
            .map(|device| device.display_name.as_str())
            .collect();
        assert_eq!(actual, expected);
        self
    }

    pub fn select_output_device(&mut self, selection: OutputDeviceSelection<'_>) -> &mut Self {
        assert!(
            self.state.select_output_device(selection),
            "expected output device selection to succeed"
        );
        self
    }

    pub fn assert_selected_output_device(&mut self, expected: Option<&str>) -> &mut Self {
        assert_eq!(self.state.selected_output_device(), expected);
        self
    }

    pub fn assert_selected_spotify_output_device(&mut self, expected: Option<&str>) -> &mut Self {
        assert_eq!(self.state.selected_spotify_output_device(), expected);
        self
    }

    pub fn assert_output_switch_count(&mut self, expected: u32) -> &mut Self {
        assert_eq!(self.state.output_switch_count(), expected);
        self
    }

    pub fn assert_player_spotify_mode(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.player_spotify_mode(), expected);
        self
    }

    pub fn assert_player_spotify_uri(&mut self, expected: Option<&str>) -> &mut Self {
        assert_eq!(self.state.player_spotify_uri(), expected);
        self
    }

    pub fn assert_last_playback_request(&mut self, expected: Option<&str>) -> &mut Self {
        assert_eq!(self.state.last_playback_request(), expected);
        self
    }

    pub fn assert_player_spotify_position_ms(&mut self, expected: i64) -> &mut Self {
        assert_eq!(self.state.player_spotify_position_ms(), expected);
        self
    }

    pub fn assert_player_spotify_duration_ms(&mut self, expected: i64) -> &mut Self {
        assert_eq!(self.state.player_spotify_duration_ms(), expected);
        self
    }

    pub fn assert_spotify_playback_poll_requests(&mut self, expected: u32) -> &mut Self {
        assert_eq!(self.state.spotify_playback_poll_requests(), expected);
        self
    }

    pub fn assert_mpris_identity(&mut self) -> &mut Self {
        let root = self.state.mpris_root_properties();
        assert_eq!(BUS_NAME, "org.mpris.MediaPlayer2.xmms_resuscitated");
        assert_eq!(OBJECT_PATH, "/org/mpris/MediaPlayer2");
        assert_eq!(ROOT_INTERFACE, "org.mpris.MediaPlayer2");
        assert_eq!(PLAYER_INTERFACE, "org.mpris.MediaPlayer2.Player");
        assert!(root.can_quit);
        assert!(root.can_raise);
        assert!(!root.has_track_list);
        assert_eq!(root.identity, "XMMS Resuscitated");
        assert_eq!(root.desktop_entry, "org.xmms.Resuscitated");
        assert_eq!(root.supported_uri_schemes, ["file", "http", "https"]);
        assert!(root.supported_mime_types.contains(&"audio/mpeg"));
        assert!(root.supported_mime_types.contains(&"audio/ogg"));
        assert!(root.supported_mime_types.contains(&"audio/flac"));
        self
    }

    pub fn assert_mpris_dbus_introspection(&mut self) -> &mut Self {
        let interfaces = introspection_interfaces().expect("MPRIS introspection XML must parse");
        assert!(interfaces.contains(&ROOT_INTERFACE.to_string()));
        assert!(interfaces.contains(&PLAYER_INTERFACE.to_string()));
        self
    }

    pub fn assert_mpris_playback_status(&mut self, expected: &str) -> &mut Self {
        assert_eq!(
            self.state.mpris_player_properties().playback_status,
            expected
        );
        self
    }

    pub fn assert_mpris_volume(&mut self, expected: f64) -> &mut Self {
        assert!((self.state.mpris_player_properties().volume - expected).abs() < 0.001);
        self
    }

    pub fn set_mpris_volume(&mut self, volume: f64) -> &mut Self {
        self.state.set_mpris_volume(volume);
        self
    }

    pub fn assert_mpris_position_us(&mut self, expected: i64) -> &mut Self {
        assert_eq!(self.state.mpris_player_properties().position_us, expected);
        self
    }

    pub fn assert_mpris_metadata(
        &mut self,
        track_id: &str,
        title: Option<&str>,
        url: Option<&str>,
        length_us: Option<i64>,
    ) -> &mut Self {
        let metadata = self.state.mpris_player_properties().metadata;
        assert_eq!(metadata.track_id, track_id);
        assert_eq!(metadata.title.as_deref(), title);
        assert_eq!(metadata.url.as_deref(), url);
        assert_eq!(metadata.length_us, length_us);
        self
    }

    pub fn execute_mpris_command(&mut self, command: MprisCommand) -> &mut Self {
        self.state.execute_mpris_command(command);
        self.sync_windows();
        self
    }

    pub fn assert_mpris_event(&mut self, expected: MprisEvent) -> &mut Self {
        assert!(
            self.state.mpris_events().contains(&expected),
            "expected MPRIS event {expected:?}"
        );
        self
    }

    pub fn assert_mpris_quit_requested(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.mpris_quit_requested(), expected);
        self
    }

    pub fn add_playlist_uri(&mut self, uri: &str) -> &mut Self {
        self.state.app_state_mut().playlist.add_uri(uri);
        self
    }

    pub fn assert_main_title(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.formatted_current_title(), expected);
        self
    }

    pub fn assert_shaded_playlist_info(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.shaded_playlist_info().trim_end(), expected);
        self
    }

    pub fn assert_playlist_row_font(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.preference_playlist_font(), expected);
        self
    }

    pub fn set_visualization_mode(&mut self, mode: VisMode) -> &mut Self {
        self.state.set_visualization_mode(mode);
        self
    }

    pub fn assert_visualization_mode(&mut self, expected: VisMode) -> &mut Self {
        assert_eq!(self.state.visualization_mode(), expected);
        self
    }

    pub fn set_visualization_analyzer_style(&mut self, style: VisAnalyzerStyle) -> &mut Self {
        self.state.set_visualization_analyzer_style(style);
        self
    }

    pub fn assert_visualization_analyzer_style(&mut self, expected: VisAnalyzerStyle) -> &mut Self {
        assert_eq!(self.state.visualization_analyzer_style(), expected);
        self
    }

    pub fn set_visualization_analyzer_mode(&mut self, mode: VisAnalyzerMode) -> &mut Self {
        self.state.set_visualization_analyzer_mode(mode);
        self
    }

    pub fn assert_visualization_analyzer_mode(&mut self, expected: VisAnalyzerMode) -> &mut Self {
        assert_eq!(self.state.visualization_analyzer_mode(), expected);
        self
    }

    pub fn set_visualization_scope_mode(&mut self, mode: VisScopeMode) -> &mut Self {
        self.state.set_visualization_scope_mode(mode);
        self
    }

    pub fn assert_visualization_scope_mode(&mut self, expected: VisScopeMode) -> &mut Self {
        assert_eq!(self.state.visualization_scope_mode(), expected);
        self
    }

    pub fn set_visualization_peaks_enabled(&mut self, enabled: bool) -> &mut Self {
        self.state.set_visualization_peaks_enabled(enabled);
        self
    }

    pub fn assert_visualization_peaks_enabled(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.visualization_peaks_enabled(), expected);
        self
    }

    pub fn set_visualization_falloff(
        &mut self,
        analyzer: VisFalloffSpeed,
        peaks: VisFalloffSpeed,
    ) -> &mut Self {
        self.state.set_visualization_falloff(analyzer, peaks);
        self
    }

    pub fn set_visualization_vu_mode(&mut self, mode: VisVuMode) -> &mut Self {
        self.state.set_visualization_vu_mode(mode);
        self
    }

    pub fn assert_visualization_vu_mode(&mut self, expected: VisVuMode) -> &mut Self {
        assert_eq!(self.state.visualization_vu_mode(), expected);
        self
    }

    pub fn set_visualization_refresh_divisor(&mut self, divisor: i32) -> &mut Self {
        self.state.set_visualization_refresh_divisor(divisor);
        self
    }

    pub fn assert_visualization_refresh_divisor(&mut self, expected: i32) -> &mut Self {
        assert_eq!(self.state.visualization_refresh_divisor(), expected);
        self
    }

    pub fn feed_visualization_data(&mut self, band: usize, value: f32) -> &mut Self {
        let mut data = [0.0; 75];
        data[band.min(74)] = value;
        self.state
            .app_state_mut()
            .player
            .set_visualization_data(data);
        self
    }

    pub fn tick_visualization(&mut self, elapsed_ms: u32) -> &mut Self {
        self.state.app_state_mut().player.mark_playing();
        self.state.update_timer_tick(elapsed_ms);
        self
    }

    pub fn assert_visualization_band_at_least(&mut self, band: usize, expected: f32) -> &mut Self {
        assert!(
            self.state.visualization_render_state().data[band.min(74)] >= expected,
            "expected visualization band {band} to be at least {expected}"
        );
        self
    }

    pub fn assert_visualization_band_at_most(&mut self, band: usize, expected: f32) -> &mut Self {
        assert!(
            self.state.visualization_render_state().data[band.min(74)] <= expected,
            "expected visualization band {band} to be at most {expected}"
        );
        self
    }

    pub fn assert_visualization_peak_cleared(&mut self) -> &mut Self {
        assert!(
            self.state
                .visualization_render_state()
                .peak
                .iter()
                .all(|peak| *peak == 0.0),
            "expected visualization peaks to be cleared"
        );
        self
    }

    pub fn assert_panel_title_draggable(&mut self, panel: PanelKind) -> &mut Self {
        assert!(
            self.state.panel_title_drag_region(panel, 40, 7),
            "expected {panel:?} titlebar to start a window drag"
        );
        self
    }

    pub fn assert_panel_title_button_not_draggable(&mut self, panel: PanelKind) -> &mut Self {
        assert!(
            !self.state.panel_title_drag_region(panel, 268, 7),
            "expected {panel:?} close button not to start a window drag"
        );
        self
    }

    pub fn assert_equalizer_active(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.equalizer_active(), expected);
        self
    }

    pub fn assert_equalizer_automatic(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.equalizer_automatic(), expected);
        self
    }

    pub fn assert_equalizer_preamp_position(&mut self, expected: i32) -> &mut Self {
        assert_eq!(self.state.equalizer_preamp_position(), expected);
        self
    }

    pub fn assert_equalizer_band_position(&mut self, band: usize, expected: i32) -> &mut Self {
        assert_eq!(self.state.equalizer_band_position(band), Some(expected));
        self
    }

    pub fn assert_equalizer_preamp_db(&mut self, expected: f64) -> &mut Self {
        assert_eq!(self.state.equalizer_preamp_db(), expected);
        self
    }

    pub fn assert_equalizer_band_db(&mut self, band: usize, expected: f64) -> &mut Self {
        assert_eq!(self.state.equalizer_band_db(band), Some(expected));
        self
    }

    pub fn assert_equalizer_gstreamer_band_db_values(&mut self, expected: [f64; 10]) -> &mut Self {
        assert_eq!(self.state.equalizer_gstreamer_band_db_values(), expected);
        self
    }

    pub fn assert_equalizer_presets_pressed(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.equalizer_presets_pressed(), expected);
        self
    }

    pub fn assert_player_state(&mut self, expected: PlayerState) -> &mut Self {
        assert_eq!(self.state.player_state(), expected);
        self
    }

    pub fn assert_shuffle(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.shuffle(), expected);
        self
    }

    pub fn assert_repeat(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.repeat(), expected);
        self
    }

    pub fn assert_no_advance(&mut self, expected: bool) -> &mut Self {
        assert_eq!(self.state.no_advance(), expected);
        self
    }

    pub fn assert_volume(&mut self, expected: i32) -> &mut Self {
        assert_eq!(self.state.volume(), expected);
        self
    }

    pub fn assert_balance(&mut self, expected: i32) -> &mut Self {
        assert_eq!(self.state.balance(), expected);
        self
    }

    pub fn assert_position(&mut self, expected: i32) -> &mut Self {
        assert_eq!(self.state.position(), expected);
        self
    }

    pub fn assert_main_time_digits(&mut self, expected: [i32; 5]) -> &mut Self {
        assert_eq!(self.state.main_time_digits(), expected);
        self
    }

    pub fn assert_main_channels(&mut self, expected: i32) -> &mut Self {
        assert_eq!(self.state.main_channels(), expected);
        self
    }

    pub fn assert_playback_position_ms(&mut self, expected: i64) -> &mut Self {
        assert_eq!(self.state.playback_position_ms(), expected);
        self
    }

    pub fn assert_last_open_location(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.last_open_location(), Some(expected));
        self
    }

    pub fn assert_last_jump_time_ms(&mut self, expected: i64) -> &mut Self {
        assert_eq!(self.state.last_jump_time_ms(), Some(expected));
        self
    }

    pub fn assert_playlist_len(&mut self, expected: usize) -> &mut Self {
        assert_eq!(self.state.playlist_len(), expected);
        self
    }

    pub fn assert_playlist_entry(&mut self, index: usize, expected: &str) -> &mut Self {
        assert_eq!(self.state.playlist_entry_uri(index), Some(expected));
        self
    }

    pub fn assert_playlist_title(&mut self, index: usize, expected: &str) -> &mut Self {
        assert_eq!(self.state.playlist_entry_title(index), Some(expected));
        self
    }

    pub fn assert_playlist_length_ms(&mut self, index: usize, expected: i64) -> &mut Self {
        assert_eq!(self.state.playlist_entry_length_ms(index), Some(expected));
        self
    }

    pub fn assert_playlist_selected(&mut self, index: usize, expected: bool) -> &mut Self {
        assert_eq!(self.state.playlist_entry_selected(index), Some(expected));
        self
    }

    pub fn assert_playlist_position(&mut self, expected: Option<usize>) -> &mut Self {
        assert_eq!(self.state.playlist_position(), expected);
        self
    }

    pub fn assert_current_playlist_entry(&mut self, expected: &str) -> &mut Self {
        assert_eq!(self.state.current_playlist_entry_uri(), Some(expected));
        self
    }

    pub fn is_window_visible(&self, window: Window) -> bool {
        match window {
            Window::Player => self.main_visible,
            Window::Playlist => self.playlist_visible,
            Window::Equalizer => self.equalizer_visible,
            Window::Preferences => self.preferences_visible,
            Window::OpenLocation => self.open_location_visible,
            Window::JumpTime => self.jump_time_visible,
            Window::SkinBrowser => self.skin_browser_visible,
            Window::OutputDevicePicker => self.output_device_picker_visible,
        }
    }

    fn playlist_menu_anchor(&self) -> (i32, i32) {
        let menu = self
            .state
            .playlist_menu()
            .expect("expected a playlist menu to be open");
        let (width, height) = self.state.playlist_size();
        let (x, items) = match menu {
            PlaylistMenuKind::Add => (12, 3),
            PlaylistMenuKind::Remove => (41, 4),
            PlaylistMenuKind::Select => (70, 3),
            PlaylistMenuKind::Misc => (99, 3),
            PlaylistMenuKind::List => (width - 46, 3),
        };
        (x + 12, height - 29 - ((items - 1) * 18) - 1)
    }

    fn apply_action(&mut self, action: UiAction) {
        match action {
            UiAction::None | UiAction::Resize | UiAction::ShowMenu => {}
            UiAction::OpenFileDialog => {
                self.file_dialog_visible = true;
                self.state.set_file_dialog_visible(true);
            }
            UiAction::Minimize => self.main_minimized = true,
            UiAction::Quit => {
                self.main_visible = false;
                self.playlist_visible = false;
                self.equalizer_visible = false;
                self.preferences_visible = false;
                self.open_location_visible = false;
                self.jump_time_visible = false;
                self.skin_browser_visible = false;
                self.output_device_picker_visible = false;
                self.file_dialog_visible = false;
                self.directory_dialog_visible = false;
            }
        }
    }

    fn sync_windows(&mut self) {
        if !self.main_visible {
            return;
        }
        let visibility = self.state.panel_visibility();
        self.playlist_visible = visibility.playlist;
        self.equalizer_visible = visibility.equalizer;
        self.preferences_visible = self.state.is_preferences_visible();
        self.open_location_visible = self.state.is_open_location_visible();
        self.jump_time_visible = self.state.is_jump_time_visible();
        self.skin_browser_visible = self.state.is_skin_browser_visible();
        self.output_device_picker_visible = self.state.is_output_device_picker_visible();
        self.directory_dialog_visible = self.state.is_directory_dialog_visible();
    }

    fn drag_equalizer_slider(&mut self, x: i32, position: i32) -> &mut Self {
        let y = 38 + (position.clamp(0, 100) * 63 + 99) / 100;
        self.state.equalizer_press(x, y);
        self.state.equalizer_motion(x, y);
        self.state.equalizer_release(x, y);
        self
    }

    fn drag_equalizer_shaded_slider(&mut self, x: i32, position: i32) -> &mut Self {
        let y = 8;
        let x = x + position;
        self.state.equalizer_press(x, y);
        self.state.equalizer_motion(x, y);
        self.state.equalizer_release(x, y);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

const fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn center(rect: Rect) -> (i32, i32) {
    (rect.x + rect.width / 2, rect.y + rect.height / 2)
}

fn push_button_rect(button: MainPushButton) -> Rect {
    match button {
        MainPushButton::Menu => rect(6, 3, 9, 9),
        MainPushButton::Minimize => rect(244, 3, 9, 9),
        MainPushButton::Shade => rect(254, 3, 9, 9),
        MainPushButton::Close => rect(264, 3, 9, 9),
        MainPushButton::Previous => rect(16, 88, 23, 18),
        MainPushButton::Play => rect(39, 88, 23, 18),
        MainPushButton::Pause => rect(62, 88, 23, 18),
        MainPushButton::Stop => rect(85, 88, 23, 18),
        MainPushButton::Next => rect(108, 88, 22, 18),
        MainPushButton::Eject => rect(136, 89, 22, 16),
    }
}

fn toggle_button_rect(toggle: MainToggleButton) -> Rect {
    match toggle {
        MainToggleButton::Shuffle => rect(164, 89, 46, 15),
        MainToggleButton::Repeat => rect(210, 89, 28, 15),
        MainToggleButton::Equalizer => rect(219, 58, 23, 12),
        MainToggleButton::Playlist => rect(242, 58, 23, 12),
    }
}

fn slider_rect(slider: MainSlider) -> Rect {
    match slider {
        MainSlider::Volume => rect(107, 57, 68, 13),
        MainSlider::Balance => rect(177, 57, 38, 13),
        MainSlider::Position => rect(16, 72, 248, 10),
    }
}

fn slider_max(slider: MainSlider) -> i32 {
    match slider {
        MainSlider::Volume => 51,
        MainSlider::Balance => 24,
        MainSlider::Position => 219,
    }
}

fn slider_knob_width(slider: MainSlider) -> i32 {
    match slider {
        MainSlider::Volume | MainSlider::Balance => 14,
        MainSlider::Position => 29,
    }
}
