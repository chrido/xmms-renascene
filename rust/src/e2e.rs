use crate::app_state::AppState;
use crate::config::Config;
use crate::render::{MainPushButton, MainSlider, MainToggleButton};
use crate::ui::{MainWindowUiState, UiAction};

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

    pub fn with_volume(mut self, volume: i32) -> Self {
        self.config.volume = volume.clamp(0, 100);
        self
    }

    pub fn with_balance(mut self, balance: i32) -> Self {
        self.config.balance = balance.clamp(-100, 100);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Player,
    Playlist,
    Equalizer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTarget {
    Push(MainPushButton),
    Toggle(MainToggleButton),
    Slider(MainSlider, i32),
}

impl MainTarget {
    pub const PLAYLIST: Self = Self::Toggle(MainToggleButton::Playlist);
    pub const EQUALIZER: Self = Self::Toggle(MainToggleButton::Equalizer);
    pub const SHUFFLE: Self = Self::Toggle(MainToggleButton::Shuffle);
    pub const REPEAT: Self = Self::Toggle(MainToggleButton::Repeat);
    pub const PLAY: Self = Self::Push(MainPushButton::Play);
    pub const PAUSE: Self = Self::Push(MainPushButton::Pause);
    pub const STOP: Self = Self::Push(MainPushButton::Stop);
    pub const CLOSE: Self = Self::Push(MainPushButton::Close);
    pub const SHADE: Self = Self::Push(MainPushButton::Shade);

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
                (rect.x + position, rect.y + rect.height / 2)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiE2e {
    main_visible: bool,
    state: MainWindowUiState,
    playlist_visible: bool,
    equalizer_visible: bool,
}

impl UiE2e {
    pub fn start_player(settings: PlayerSettings) -> Self {
        let mut harness = Self {
            main_visible: true,
            state: MainWindowUiState::from_app_state(AppState::from_config(settings.config)),
            playlist_visible: false,
            equalizer_visible: false,
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

    pub fn assert_window_visible(&self, window: Window) -> &Self {
        assert!(
            self.is_window_visible(window),
            "expected {window:?} window to be visible"
        );
        self
    }

    pub fn assert_window_hidden(&self, window: Window) -> &Self {
        assert!(
            !self.is_window_visible(window),
            "expected {window:?} window to be hidden"
        );
        self
    }

    pub fn is_window_visible(&self, window: Window) -> bool {
        match window {
            Window::Player => self.main_visible,
            Window::Playlist => self.playlist_visible,
            Window::Equalizer => self.equalizer_visible,
        }
    }

    fn apply_action(&mut self, action: UiAction) {
        if action == UiAction::Quit {
            self.main_visible = false;
            self.playlist_visible = false;
            self.equalizer_visible = false;
        }
    }

    fn sync_windows(&mut self) {
        if !self.main_visible {
            return;
        }
        let visibility = self.state.panel_visibility();
        self.playlist_visible = visibility.playlist;
        self.equalizer_visible = visibility.equalizer;
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
