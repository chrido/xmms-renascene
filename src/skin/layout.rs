use super::SkinPixmapKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

impl Size {
    pub const fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl SkinRect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
            && self.width > 0
            && self.height > 0
    }

    pub fn center(self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinPixmapInfo {
    pub file_stem: &'static str,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteSpec {
    pub kind: SkinPixmapKind,
    pub source: SkinRect,
    pub dest: SkinRect,
}

impl SpriteSpec {
    pub const fn new(kind: SkinPixmapKind, source: SkinRect, dest: SkinRect) -> Self {
        Self { kind, source, dest }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliderLayout {
    pub rect: SkinRect,
    pub knob_size: Size,
    pub min: i32,
    pub max: i32,
    pub frame_height: i32,
    pub frame_offset: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainPushButton {
    Menu,
    Minimize,
    Shade,
    Close,
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainToggleButton {
    Shuffle,
    Repeat,
    Equalizer,
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainSlider {
    Volume,
    Balance,
    Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerControl {
    On,
    Auto,
    Presets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerSlider {
    Preamp,
    Band(usize),
    ShadedVolume,
    ShadedBalance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPanelKind {
    Equalizer,
    Playlist,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelTitleButton {
    Shade,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistMenuButton {
    Add,
    Remove,
    Select,
    Misc,
    List,
}

impl PlaylistMenuButton {
    pub fn item_count(self) -> usize {
        match self {
            Self::Add | Self::Select | Self::Misc | Self::List => 3,
            Self::Remove => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistFooterButton {
    Previous,
    Play,
    Pause,
    Stop,
    Next,
    Eject,
    ScrollUp,
    ScrollDown,
}

pub const MAIN_WINDOW_SIZE: Size = Size::new(275, 116);
pub const MAIN_WINDOW_WIDTH: i32 = MAIN_WINDOW_SIZE.width;
pub const MAIN_WINDOW_HEIGHT: i32 = MAIN_WINDOW_SIZE.height;
pub const MAIN_TITLEBAR_HEIGHT: i32 = 14;

pub const EQUALIZER_WINDOW_SIZE: Size = Size::new(275, 116);
pub const EQUALIZER_WINDOW_WIDTH: i32 = EQUALIZER_WINDOW_SIZE.width;
pub const EQUALIZER_WINDOW_HEIGHT: i32 = EQUALIZER_WINDOW_SIZE.height;

pub const PLAYLIST_DEFAULT_SIZE: Size = Size::new(275, 232);
pub const PLAYLIST_DEFAULT_WIDTH: i32 = PLAYLIST_DEFAULT_SIZE.width;
pub const PLAYLIST_DEFAULT_HEIGHT: i32 = PLAYLIST_DEFAULT_SIZE.height;

pub const PLAYLIST_MIN_SIZE: Size = Size::new(275, 116);
pub const PLAYLIST_MIN_WIDTH: i32 = PLAYLIST_MIN_SIZE.width;
pub const PLAYLIST_MIN_HEIGHT: i32 = PLAYLIST_MIN_SIZE.height;

pub const PLAYLIST_WIDTH_STEP: i32 = 25;
pub const PLAYLIST_HEIGHT_BASE: i32 = 58;
pub const PLAYLIST_HEIGHT_STEP: i32 = 29;

pub fn pixmap_info(kind: SkinPixmapKind) -> SkinPixmapInfo {
    match kind {
        SkinPixmapKind::Main => SkinPixmapInfo {
            file_stem: "main",
            width: MAIN_WINDOW_WIDTH as usize,
            height: MAIN_WINDOW_HEIGHT as usize,
        },
        SkinPixmapKind::CButtons => SkinPixmapInfo {
            file_stem: "cbuttons",
            width: 136,
            height: 36,
        },
        SkinPixmapKind::Titlebar => SkinPixmapInfo {
            file_stem: "titlebar",
            width: MAIN_WINDOW_WIDTH as usize,
            height: MAIN_WINDOW_HEIGHT as usize,
        },
        SkinPixmapKind::ShufRep => SkinPixmapInfo {
            file_stem: "shufrep",
            width: 28,
            height: 60,
        },
        SkinPixmapKind::Text => SkinPixmapInfo {
            file_stem: "text",
            width: 155,
            height: 18,
        },
        SkinPixmapKind::Volume => SkinPixmapInfo {
            file_stem: "volume",
            width: 68,
            height: 421,
        },
        SkinPixmapKind::Balance => SkinPixmapInfo {
            file_stem: "balance",
            width: 38,
            height: 421,
        },
        SkinPixmapKind::MonoStereo => SkinPixmapInfo {
            file_stem: "monoster",
            width: 56,
            height: 12,
        },
        SkinPixmapKind::PlayPause => SkinPixmapInfo {
            file_stem: "playpaus",
            width: 11,
            height: 9,
        },
        SkinPixmapKind::Numbers => SkinPixmapInfo {
            file_stem: "nums_ex",
            width: 108,
            height: 13,
        },
        SkinPixmapKind::PosBar => SkinPixmapInfo {
            file_stem: "posbar",
            width: 248,
            height: 10,
        },
        SkinPixmapKind::PlEdit => SkinPixmapInfo {
            file_stem: "pledit",
            width: 150,
            height: 18,
        },
        SkinPixmapKind::EqMain => SkinPixmapInfo {
            file_stem: "eqmain",
            width: EQUALIZER_WINDOW_WIDTH as usize,
            height: EQUALIZER_WINDOW_HEIGHT as usize,
        },
        SkinPixmapKind::EqEx => SkinPixmapInfo {
            file_stem: "eq_ex",
            width: EQUALIZER_WINDOW_WIDTH as usize,
            height: 50,
        },
    }
}

pub fn main_push_button_spec(button: MainPushButton, pressed: bool) -> SpriteSpec {
    match button {
        MainPushButton::Menu => SpriteSpec::new(
            SkinPixmapKind::Titlebar,
            SkinRect::new(0, if pressed { 9 } else { 0 }, 9, 9),
            SkinRect::new(6, 3, 9, 9),
        ),
        MainPushButton::Minimize => SpriteSpec::new(
            SkinPixmapKind::Titlebar,
            SkinRect::new(9, if pressed { 9 } else { 0 }, 9, 9),
            SkinRect::new(244, 3, 9, 9),
        ),
        MainPushButton::Shade => SpriteSpec::new(
            SkinPixmapKind::Titlebar,
            SkinRect::new(if pressed { 9 } else { 0 }, 18, 9, 9),
            SkinRect::new(254, 3, 9, 9),
        ),
        MainPushButton::Close => SpriteSpec::new(
            SkinPixmapKind::Titlebar,
            SkinRect::new(18, if pressed { 9 } else { 0 }, 9, 9),
            SkinRect::new(264, 3, 9, 9),
        ),
        MainPushButton::Previous => SpriteSpec::new(
            SkinPixmapKind::CButtons,
            SkinRect::new(0, if pressed { 18 } else { 0 }, 23, 18),
            SkinRect::new(16, 88, 23, 18),
        ),
        MainPushButton::Play => SpriteSpec::new(
            SkinPixmapKind::CButtons,
            SkinRect::new(23, if pressed { 18 } else { 0 }, 23, 18),
            SkinRect::new(39, 88, 23, 18),
        ),
        MainPushButton::Pause => SpriteSpec::new(
            SkinPixmapKind::CButtons,
            SkinRect::new(46, if pressed { 18 } else { 0 }, 23, 18),
            SkinRect::new(62, 88, 23, 18),
        ),
        MainPushButton::Stop => SpriteSpec::new(
            SkinPixmapKind::CButtons,
            SkinRect::new(69, if pressed { 18 } else { 0 }, 23, 18),
            SkinRect::new(85, 88, 23, 18),
        ),
        MainPushButton::Next => SpriteSpec::new(
            SkinPixmapKind::CButtons,
            SkinRect::new(92, if pressed { 18 } else { 0 }, 22, 18),
            SkinRect::new(108, 88, 22, 18),
        ),
        MainPushButton::Eject => SpriteSpec::new(
            SkinPixmapKind::CButtons,
            SkinRect::new(114, if pressed { 16 } else { 0 }, 22, 16),
            SkinRect::new(136, 89, 22, 16),
        ),
    }
}

pub fn main_push_button_rect(button: MainPushButton, shaded: bool) -> SkinRect {
    if shaded {
        match button {
            MainPushButton::Previous => return SkinRect::new(169, 4, 8, 7),
            MainPushButton::Play => return SkinRect::new(177, 4, 10, 7),
            MainPushButton::Pause => return SkinRect::new(187, 4, 10, 7),
            MainPushButton::Stop => return SkinRect::new(197, 4, 9, 7),
            MainPushButton::Next => return SkinRect::new(206, 4, 8, 7),
            MainPushButton::Eject => return SkinRect::new(216, 4, 9, 7),
            _ => {}
        }
    }

    main_push_button_spec(button, false).dest
}

pub fn main_toggle_button_spec(
    toggle: MainToggleButton,
    selected: bool,
    pressed: bool,
) -> SpriteSpec {
    let row = match (selected, pressed) {
        (false, false) => 0,
        (false, true) => 15,
        (true, false) => 30,
        (true, true) => 45,
    };
    match toggle {
        MainToggleButton::Shuffle => SpriteSpec::new(
            SkinPixmapKind::ShufRep,
            SkinRect::new(28, row, 46, 15),
            SkinRect::new(164, 89, 46, 15),
        ),
        MainToggleButton::Repeat => SpriteSpec::new(
            SkinPixmapKind::ShufRep,
            SkinRect::new(0, row, 28, 15),
            SkinRect::new(210, 89, 28, 15),
        ),
        MainToggleButton::Equalizer => {
            let x = match (selected, pressed) {
                (false, false) => 0,
                (false, true) => 46,
                (true, false) => 0,
                (true, true) => 46,
            };
            let y = if selected { 73 } else { 61 };
            SpriteSpec::new(
                SkinPixmapKind::ShufRep,
                SkinRect::new(x, y, 23, 12),
                SkinRect::new(219, 58, 23, 12),
            )
        }
        MainToggleButton::Playlist => {
            let x = match (selected, pressed) {
                (false, false) => 23,
                (false, true) => 69,
                (true, false) => 23,
                (true, true) => 69,
            };
            let y = if selected { 73 } else { 61 };
            SpriteSpec::new(
                SkinPixmapKind::ShufRep,
                SkinRect::new(x, y, 23, 12),
                SkinRect::new(242, 58, 23, 12),
            )
        }
    }
}

pub fn main_toggle_button_rect(toggle: MainToggleButton) -> SkinRect {
    main_toggle_button_spec(toggle, false, false).dest
}

pub fn main_slider_layout(slider: MainSlider, shaded: bool) -> SliderLayout {
    match (slider, shaded) {
        (MainSlider::Volume, _) => SliderLayout {
            rect: SkinRect::new(107, 57, 68, 13),
            knob_size: Size::new(14, 11),
            min: 0,
            max: 51,
            frame_height: 15,
            frame_offset: 0,
        },
        (MainSlider::Balance, _) => SliderLayout {
            rect: SkinRect::new(177, 57, 38, 13),
            knob_size: Size::new(14, 11),
            min: 0,
            max: 24,
            frame_height: 15,
            frame_offset: 9,
        },
        (MainSlider::Position, false) => SliderLayout {
            rect: SkinRect::new(16, 72, 248, 10),
            knob_size: Size::new(29, 10),
            min: 0,
            max: 219,
            frame_height: 1,
            frame_offset: 0,
        },
        (MainSlider::Position, true) => SliderLayout {
            rect: SkinRect::new(226, 4, 17, 7),
            knob_size: Size::new(3, 7),
            min: 1,
            max: 13,
            frame_height: 1,
            frame_offset: 0,
        },
    }
}

pub fn equalizer_control_rect(control: EqualizerControl) -> SkinRect {
    match control {
        EqualizerControl::On => SkinRect::new(14, 18, 25, 12),
        EqualizerControl::Auto => SkinRect::new(39, 18, 33, 12),
        EqualizerControl::Presets => SkinRect::new(217, 18, 44, 12),
    }
}

pub fn equalizer_control_at(x: i32, y: i32) -> Option<EqualizerControl> {
    [
        EqualizerControl::On,
        EqualizerControl::Auto,
        EqualizerControl::Presets,
    ]
    .into_iter()
    .find(|control| equalizer_control_rect(*control).contains(x, y))
}

pub fn equalizer_control_spec(
    control: EqualizerControl,
    selected: bool,
    pressed: bool,
) -> SpriteSpec {
    let source_origin = match control {
        EqualizerControl::On => match (selected, pressed) {
            (true, true) => (187, 119),
            (true, false) => (69, 119),
            (false, true) => (128, 119),
            (false, false) => (10, 119),
        },
        EqualizerControl::Auto => match (selected, pressed) {
            (true, true) => (212, 119),
            (true, false) => (94, 119),
            (false, true) => (153, 119),
            (false, false) => (35, 119),
        },
        EqualizerControl::Presets => (224, if pressed { 176 } else { 164 }),
    };
    let dest = equalizer_control_rect(control);
    SpriteSpec::new(
        SkinPixmapKind::EqMain,
        SkinRect::new(source_origin.0, source_origin.1, dest.width, dest.height),
        dest,
    )
}

pub fn equalizer_slider_layout(slider: EqualizerSlider) -> SliderLayout {
    match slider {
        EqualizerSlider::Preamp => SliderLayout {
            rect: SkinRect::new(21, 38, 14, 63),
            knob_size: Size::new(11, 11),
            min: 0,
            max: 100,
            frame_height: 1,
            frame_offset: 0,
        },
        EqualizerSlider::Band(band) => SliderLayout {
            rect: SkinRect::new(78 + band as i32 * 18, 38, 14, 63),
            knob_size: Size::new(11, 11),
            min: 0,
            max: 100,
            frame_height: 1,
            frame_offset: 0,
        },
        EqualizerSlider::ShadedVolume => SliderLayout {
            rect: SkinRect::new(61, 4, 97, 8),
            knob_size: Size::new(3, 7),
            min: 0,
            max: 94,
            frame_height: 4,
            frame_offset: 61,
        },
        EqualizerSlider::ShadedBalance => SliderLayout {
            rect: SkinRect::new(164, 4, 42, 8),
            knob_size: Size::new(3, 7),
            min: 0,
            max: 39,
            frame_height: 4,
            frame_offset: 164,
        },
    }
}

pub fn equalizer_slider_at(x: i32, y: i32) -> Option<EqualizerSlider> {
    if equalizer_slider_layout(EqualizerSlider::Preamp)
        .rect
        .contains(x, y)
    {
        return Some(EqualizerSlider::Preamp);
    }

    (0..10).find_map(|band| {
        let slider = EqualizerSlider::Band(band);
        equalizer_slider_layout(slider)
            .rect
            .contains(x, y)
            .then_some(slider)
    })
}

pub fn equalizer_shaded_slider_at(x: i32, y: i32) -> Option<EqualizerSlider> {
    [
        EqualizerSlider::ShadedVolume,
        EqualizerSlider::ShadedBalance,
    ]
    .into_iter()
    .find(|slider| equalizer_slider_layout(*slider).rect.contains(x, y))
}

pub fn equalizer_slider_position(slider: EqualizerSlider, coordinate: i32) -> i32 {
    let layout = equalizer_slider_layout(slider);
    match slider {
        EqualizerSlider::Preamp | EqualizerSlider::Band(_) => {
            ((coordinate - layout.rect.y) * 100 / layout.rect.height).clamp(layout.min, layout.max)
        }
        EqualizerSlider::ShadedVolume | EqualizerSlider::ShadedBalance => {
            (coordinate - layout.rect.x).clamp(layout.min, layout.max)
        }
    }
}

pub fn panel_title_button_rect(
    panel: LayoutPanelKind,
    button: PanelTitleButton,
    playlist_width: i32,
) -> SkinRect {
    let x = match (panel, button) {
        (LayoutPanelKind::Equalizer, PanelTitleButton::Shade) => 254,
        (LayoutPanelKind::Equalizer, PanelTitleButton::Close) => 264,
        (LayoutPanelKind::Playlist, PanelTitleButton::Shade) => playlist_width - 21,
        (LayoutPanelKind::Playlist, PanelTitleButton::Close) => playlist_width - 11,
    };
    SkinRect::new(x, 3, 9, 9)
}

pub fn panel_title_button_at(
    panel: LayoutPanelKind,
    x: i32,
    y: i32,
    playlist_width: i32,
) -> Option<PanelTitleButton> {
    [PanelTitleButton::Shade, PanelTitleButton::Close]
        .into_iter()
        .find(|button| panel_title_button_rect(panel, *button, playlist_width).contains(x, y))
}

pub fn playlist_button_y(height: i32) -> i32 {
    height - 29
}

pub fn playlist_menu_button_rect(menu: PlaylistMenuButton, width: i32, height: i32) -> SkinRect {
    let x = match menu {
        PlaylistMenuButton::Add => 12,
        PlaylistMenuButton::Remove => 41,
        PlaylistMenuButton::Select => 70,
        PlaylistMenuButton::Misc => 99,
        PlaylistMenuButton::List => width - 46,
    };
    let width = match menu {
        PlaylistMenuButton::List => 23,
        _ => 25,
    };
    SkinRect::new(x, playlist_button_y(height), width, 18)
}

pub fn playlist_menu_button_at(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Option<PlaylistMenuButton> {
    [
        PlaylistMenuButton::Add,
        PlaylistMenuButton::Remove,
        PlaylistMenuButton::Select,
        PlaylistMenuButton::Misc,
        PlaylistMenuButton::List,
    ]
    .into_iter()
    .find(|menu| playlist_menu_button_rect(*menu, width, height).contains(x, y))
}

pub fn playlist_menu_popup_rect(menu: PlaylistMenuButton, width: i32, height: i32) -> SkinRect {
    let button = playlist_menu_button_rect(menu, width, height);
    let item_height = 18;
    let items = menu.item_count() as i32;
    SkinRect::new(
        button.x - 1,
        button.y - ((items - 1) * item_height) - 1,
        25,
        items * item_height,
    )
}

pub fn playlist_footer_button_rect(
    button: PlaylistFooterButton,
    width: i32,
    height: i32,
) -> SkinRect {
    match button {
        PlaylistFooterButton::Previous => SkinRect::new(width - 144, height - 16, 8, 7),
        PlaylistFooterButton::Play => SkinRect::new(width - 138, height - 16, 10, 7),
        PlaylistFooterButton::Pause => SkinRect::new(width - 128, height - 16, 10, 7),
        PlaylistFooterButton::Stop => SkinRect::new(width - 118, height - 16, 9, 7),
        PlaylistFooterButton::Next => SkinRect::new(width - 109, height - 16, 8, 7),
        PlaylistFooterButton::Eject => SkinRect::new(width - 100, height - 16, 9, 7),
        PlaylistFooterButton::ScrollUp => SkinRect::new(width - 14, height - 35, 8, 5),
        PlaylistFooterButton::ScrollDown => SkinRect::new(width - 14, height - 30, 8, 5),
    }
}

pub fn playlist_footer_button_at(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Option<PlaylistFooterButton> {
    [
        PlaylistFooterButton::Previous,
        PlaylistFooterButton::Play,
        PlaylistFooterButton::Pause,
        PlaylistFooterButton::Stop,
        PlaylistFooterButton::Next,
        PlaylistFooterButton::Eject,
        PlaylistFooterButton::ScrollUp,
        PlaylistFooterButton::ScrollDown,
    ]
    .into_iter()
    .find(|button| playlist_footer_button_rect(*button, width, height).contains(x, y))
}

pub fn main_window_height(shaded: bool) -> i32 {
    if shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        MAIN_WINDOW_HEIGHT
    }
}

pub fn equalizer_window_height(shaded: bool) -> i32 {
    if shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        EQUALIZER_WINDOW_HEIGHT
    }
}

pub fn playlist_window_height(shaded: bool, height: i32) -> i32 {
    if shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        height.max(PLAYLIST_MIN_HEIGHT)
    }
}

pub fn snap_playlist_size(width: i32, height: i32) -> Size {
    let width_blocks = (width - PLAYLIST_MIN_WIDTH) / PLAYLIST_WIDTH_STEP;
    let width = (width_blocks * PLAYLIST_WIDTH_STEP + PLAYLIST_MIN_WIDTH).max(PLAYLIST_MIN_WIDTH);
    let height_blocks = (height - PLAYLIST_HEIGHT_BASE) / PLAYLIST_HEIGHT_STEP;
    let height =
        (height_blocks * PLAYLIST_HEIGHT_STEP + PLAYLIST_HEIGHT_BASE).max(PLAYLIST_MIN_HEIGHT);
    Size::new(width, height)
}
