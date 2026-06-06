use std::fmt;

use cairo::{Context, Extend, Filter, Format, ImageSurface, Rectangle};

use crate::skin::widget::{
    PlayStatusValue, TextBox, VisAnalyzerMode, VisAnalyzerStyle, VisMode, VisScopeMode, VisVuMode,
};
use crate::skin::xpm::XpmImage;
use crate::skin::{DefaultSkin, SkinPixmapKind};

pub const MAIN_WINDOW_WIDTH: i32 = 275;
pub const MAIN_WINDOW_HEIGHT: i32 = 116;
pub const MAIN_TITLEBAR_HEIGHT: i32 = 14;
pub const EQUALIZER_WINDOW_WIDTH: i32 = 275;
pub const EQUALIZER_WINDOW_HEIGHT: i32 = 116;
pub const PLAYLIST_DEFAULT_WIDTH: i32 = 275;
pub const PLAYLIST_DEFAULT_HEIGHT: i32 = 232;
pub const PLAYLIST_MIN_WIDTH: i32 = 275;
pub const PLAYLIST_MIN_HEIGHT: i32 = 116;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EqualizerRenderState {
    pub focused: bool,
    pub shaded: bool,
    pub active: bool,
    pub automatic: bool,
    pub pressed_control: Option<EqualizerControl>,
    pub preamp_position: i32,
    pub band_positions: [i32; 10],
}

impl Default for EqualizerRenderState {
    fn default() -> Self {
        Self {
            focused: true,
            shaded: false,
            active: true,
            automatic: false,
            pressed_control: None,
            preamp_position: 50,
            band_positions: [50; 10],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqualizerControl {
    On,
    Auto,
    Presets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistMenuRenderState {
    pub kind: PlaylistMenuRenderKind,
    pub hover: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRowRenderEntry {
    pub title: String,
    pub length_ms: i64,
    pub selected: bool,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRowsRenderState {
    pub entries: Vec<PlaylistRowRenderEntry>,
    pub scroll_offset: usize,
    pub scrollbar_dragging: bool,
    pub search_query: Option<String>,
    pub show_numbers: bool,
    pub font_family: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistMenuRenderKind {
    Add,
    Remove,
    Select,
    Misc,
    List,
}

impl PlaylistMenuRenderKind {
    pub fn item_count(self) -> usize {
        match self {
            Self::Add | Self::Select | Self::Misc | Self::List => 3,
            Self::Remove => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockedPanelState {
    pub main_focused: bool,
    pub main_shaded: bool,
    pub equalizer_visible: bool,
    pub equalizer_detached: bool,
    pub equalizer_focused: bool,
    pub equalizer_shaded: bool,
    pub playlist_visible: bool,
    pub playlist_detached: bool,
    pub playlist_focused: bool,
    pub playlist_shaded: bool,
    pub playlist_width: i32,
    pub playlist_height: i32,
}

impl Default for DockedPanelState {
    fn default() -> Self {
        Self {
            main_focused: true,
            main_shaded: false,
            equalizer_visible: false,
            equalizer_detached: false,
            equalizer_focused: true,
            equalizer_shaded: false,
            playlist_visible: false,
            playlist_detached: false,
            playlist_focused: true,
            playlist_shaded: false,
            playlist_width: PLAYLIST_DEFAULT_WIDTH,
            playlist_height: PLAYLIST_DEFAULT_HEIGHT,
        }
    }
}

#[derive(Debug)]
pub enum RenderError {
    Cairo(cairo::Error),
    SurfaceData(cairo::BorrowError),
    DimensionTooLarge { width: usize, height: usize },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::Cairo(err) => write!(f, "cairo error: {err}"),
            RenderError::SurfaceData(err) => {
                write!(f, "could not access cairo surface data: {err}")
            }
            RenderError::DimensionTooLarge { width, height } => {
                write!(
                    f,
                    "image dimensions are too large for cairo: {width}x{height}"
                )
            }
        }
    }
}

impl std::error::Error for RenderError {}

impl From<cairo::Error> for RenderError {
    fn from(value: cairo::Error) -> Self {
        RenderError::Cairo(value)
    }
}

impl From<cairo::BorrowError> for RenderError {
    fn from(value: cairo::BorrowError) -> Self {
        RenderError::SurfaceData(value)
    }
}

pub fn surface_from_xpm(image: &XpmImage) -> Result<ImageSurface, RenderError> {
    let width = i32::try_from(image.width()).map_err(|_| RenderError::DimensionTooLarge {
        width: image.width(),
        height: image.height(),
    })?;
    let height = i32::try_from(image.height()).map_err(|_| RenderError::DimensionTooLarge {
        width: image.width(),
        height: image.height(),
    })?;

    let mut surface = ImageSurface::create(Format::ARgb32, width, height)?;
    let stride = surface.stride() as usize;

    {
        let mut data = surface.data()?;
        for y in 0..image.height() {
            let row_start = y * stride;
            for x in 0..image.width() {
                let pixel = image.pixel_argb(x, y).unwrap_or(0xff00_0000);
                let offset = row_start + x * 4;
                data[offset..offset + 4].copy_from_slice(&pixel.to_ne_bytes());
            }
        }
    }

    surface.mark_dirty();
    Ok(surface)
}

pub fn blit_surface_rect(
    cr: &Context,
    source: &ImageSurface,
    mut xsrc: i32,
    mut ysrc: i32,
    mut xdest: i32,
    mut ydest: i32,
    mut width: i32,
    mut height: i32,
) -> Result<bool, RenderError> {
    if xsrc < 0 {
        xdest -= xsrc;
        width += xsrc;
        xsrc = 0;
    }
    if ysrc < 0 {
        ydest -= ysrc;
        height += ysrc;
        ysrc = 0;
    }

    let surface_width = source.width();
    let surface_height = source.height();
    if xsrc >= surface_width || ysrc >= surface_height {
        return Ok(false);
    }
    if xsrc + width > surface_width {
        width = surface_width - xsrc;
    }
    if ysrc + height > surface_height {
        height = surface_height - ysrc;
    }
    if width <= 0 || height <= 0 {
        return Ok(false);
    }

    let source_rect = source.create_for_rectangle(Rectangle::new(
        xsrc as f64,
        ysrc as f64,
        width as f64,
        height as f64,
    ))?;

    cr.save()?;
    cr.rectangle(xdest as f64, ydest as f64, width as f64, height as f64);
    cr.clip();
    cr.set_source_surface(&source_rect, xdest as f64, ydest as f64)?;
    let pattern = cr.source();
    pattern.set_extend(Extend::Pad);
    pattern.set_filter(Filter::Nearest);
    cr.paint()?;
    cr.restore()?;

    Ok(true)
}

pub fn clamp_scale_factor(scale: f64) -> f64 {
    scale.clamp(1.0, 5.0)
}

pub fn scale_coord(value: i32, scale: f64) -> i32 {
    (f64::from(value) * clamp_scale_factor(scale) + 0.5) as i32
}

pub fn scale_dim(value: i32, scale: f64) -> i32 {
    scale_coord(value, scale).max(1)
}

pub fn apply_window_scale(
    cr: &Context,
    actual_width: i32,
    actual_height: i32,
    base_width: i32,
    base_height: i32,
) -> bool {
    if actual_width <= 0 || actual_height <= 0 || base_width <= 0 || base_height <= 0 {
        return false;
    }

    cr.scale(
        f64::from(actual_width) / f64::from(base_width),
        f64::from(actual_height) / f64::from(base_height),
    );
    true
}

pub fn render_main_titlebar(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
) -> Result<bool, RenderError> {
    let Some(titlebar) = skin.get(SkinPixmapKind::Titlebar) else {
        return Ok(false);
    };
    let titlebar = surface_from_xpm(titlebar)?;
    let ysrc = match (shaded, focused) {
        (true, true) => 29,
        (true, false) => 42,
        (false, true) => 0,
        (false, false) => 15,
    };

    blit_surface_rect(
        cr,
        &titlebar,
        27,
        ysrc,
        0,
        0,
        MAIN_WINDOW_WIDTH,
        MAIN_TITLEBAR_HEIGHT,
    )
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

pub fn docked_panel_size(state: DockedPanelState) -> (i32, i32) {
    let playlist_width = state.playlist_width.max(PLAYLIST_MIN_WIDTH);
    let mut width = MAIN_WINDOW_WIDTH;
    let mut height = main_window_height(state.main_shaded);

    if state.equalizer_visible && !state.equalizer_detached {
        height += equalizer_window_height(state.equalizer_shaded);
    }
    if state.playlist_visible && !state.playlist_detached {
        height += playlist_window_height(state.playlist_shaded, state.playlist_height);
        width = width.max(playlist_width);
    }

    (width, height)
}

pub fn render_main_player(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
) -> Result<bool, RenderError> {
    let mut rendered = false;
    if !shaded {
        if let Some(main) = skin.get(SkinPixmapKind::Main) {
            let main = surface_from_xpm(main)?;
            rendered |=
                blit_surface_rect(cr, &main, 0, 0, 0, 0, MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT)?;
        }
    }

    rendered |= render_main_titlebar(cr, skin, focused, shaded)?;
    Ok(rendered)
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

#[derive(Debug, Clone, PartialEq)]
pub struct VisualizationRenderState {
    pub mode: VisMode,
    pub analyzer_style: VisAnalyzerStyle,
    pub analyzer_mode: VisAnalyzerMode,
    pub scope_mode: VisScopeMode,
    pub peaks_enabled: bool,
    pub vu_mode: VisVuMode,
    pub data: [f32; 75],
    pub peak: [f32; 75],
    pub milkdrop_energy: f32,
    pub milkdrop_phase: f32,
}

impl Default for VisualizationRenderState {
    fn default() -> Self {
        Self {
            mode: VisMode::Analyzer,
            analyzer_style: VisAnalyzerStyle::Bars,
            analyzer_mode: VisAnalyzerMode::Normal,
            scope_mode: VisScopeMode::Line,
            peaks_enabled: true,
            vu_mode: VisVuMode::Normal,
            data: [0.0; 75],
            peak: [0.0; 75],
            milkdrop_energy: 0.0,
            milkdrop_phase: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MainWindowRenderState {
    pub focused: bool,
    pub shaded: bool,
    pub title: String,
    pub bitrate_text: String,
    pub frequency_text: String,
    pub volume_position: i32,
    pub balance_position: i32,
    pub position_position: i32,
    pub shuffle_selected: bool,
    pub repeat_selected: bool,
    pub equalizer_selected: bool,
    pub playlist_selected: bool,
    pub pressed_push: Option<MainPushButton>,
    pub pressed_toggle: Option<MainToggleButton>,
    pub pressed_slider: Option<MainSlider>,
    pub play_status: PlayStatusValue,
    pub channels: i32,
    pub visualization: VisualizationRenderState,
}

impl Default for MainWindowRenderState {
    fn default() -> Self {
        Self {
            focused: true,
            shaded: false,
            title: "XMMS Resuscitated".to_string(),
            bitrate_text: String::new(),
            frequency_text: String::new(),
            volume_position: 51,
            balance_position: 12,
            position_position: 0,
            shuffle_selected: false,
            repeat_selected: false,
            equalizer_selected: false,
            playlist_selected: false,
            pressed_push: None,
            pressed_toggle: None,
            pressed_slider: None,
            play_status: PlayStatusValue::Stopped,
            channels: 0,
            visualization: VisualizationRenderState::default(),
        }
    }
}

pub fn render_main_player_reset(cr: &Context, skin: &DefaultSkin) -> Result<bool, RenderError> {
    render_main_player_state(cr, skin, &MainWindowRenderState::default())
}

pub fn render_main_player_state(
    cr: &Context,
    skin: &DefaultSkin,
    state: &MainWindowRenderState,
) -> Result<bool, RenderError> {
    let mut rendered = render_main_player(cr, skin, state.focused, state.shaded)?;

    for button in [
        MainPushButton::Menu,
        MainPushButton::Minimize,
        MainPushButton::Shade,
        MainPushButton::Close,
    ] {
        let (kind, sx, sy, dx, dy, width, height) =
            main_push_button_rect(button, state.pressed_push == Some(button));
        rendered |= blit_skin_rect(cr, skin, kind, sx, sy, dx, dy, width, height)?;
    }

    if state.shaded {
        render_windowshade_visualization(cr, skin, 79, 5, &state.visualization)?;
        return Ok(rendered);
    }

    for button in [
        MainPushButton::Previous,
        MainPushButton::Play,
        MainPushButton::Pause,
        MainPushButton::Stop,
        MainPushButton::Next,
        MainPushButton::Eject,
    ] {
        let (kind, sx, sy, dx, dy, width, height) =
            main_push_button_rect(button, state.pressed_push == Some(button));
        rendered |= blit_skin_rect(cr, skin, kind, sx, sy, dx, dy, width, height)?;
    }

    for toggle in [
        MainToggleButton::Shuffle,
        MainToggleButton::Repeat,
        MainToggleButton::Equalizer,
        MainToggleButton::Playlist,
    ] {
        let selected = match toggle {
            MainToggleButton::Shuffle => state.shuffle_selected,
            MainToggleButton::Repeat => state.repeat_selected,
            MainToggleButton::Equalizer => state.equalizer_selected,
            MainToggleButton::Playlist => state.playlist_selected,
        };
        let (kind, sx, sy, dx, dy, width, height) =
            main_toggle_button_rect(toggle, selected, state.pressed_toggle == Some(toggle));
        rendered |= blit_skin_rect(cr, skin, kind, sx, sy, dx, dy, width, height)?;
    }

    render_text(cr, skin, &state.title, 111, 27, 153)?;
    render_text(cr, skin, &state.bitrate_text, 111, 43, 15)?;
    render_text(cr, skin, &state.frequency_text, 156, 43, 10)?;

    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::Volume,
            x: 107,
            y: 57,
            width: 68,
            height: 13,
            position: state.volume_position.clamp(0, 51),
            knob_source_x: if state.pressed_slider == Some(MainSlider::Volume) {
                0
            } else {
                15
            },
            knob_source_y: 422,
            knob_width: 14,
            knob_height: 11,
            frame_height: 15,
            frame_offset: 0,
            frame: ((state.volume_position.clamp(0, 51) as f64 / 51.0) * 27.0) as i32,
        },
    )?;
    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::Balance,
            x: 177,
            y: 57,
            width: 38,
            height: 13,
            position: state.balance_position.clamp(0, 24),
            knob_source_x: if state.pressed_slider == Some(MainSlider::Balance) {
                0
            } else {
                15
            },
            knob_source_y: 422,
            knob_width: 14,
            knob_height: 11,
            frame_height: 15,
            frame_offset: 0,
            frame: ((state.balance_position.clamp(0, 24) as f64 / 24.0) * 27.0) as i32,
        },
    )?;
    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::PosBar,
            x: 16,
            y: 72,
            width: 248,
            height: 10,
            position: state.position_position.clamp(0, 219),
            knob_source_x: if state.pressed_slider == Some(MainSlider::Position) {
                278
            } else {
                248
            },
            knob_source_y: 0,
            knob_width: 29,
            knob_height: 10,
            frame_height: 1,
            frame_offset: 0,
            frame: 0,
        },
    )?;

    for x in [36, 48, 60, 78, 90] {
        rendered |= blit_skin_rect(cr, skin, SkinPixmapKind::Numbers, 90, 0, x, 26, 9, 13)?;
    }

    render_visualization(cr, skin, 24, 43, 76, &state.visualization)?;
    if state.shaded {
        render_windowshade_visualization(cr, skin, 79, 5, &state.visualization)?;
    }
    rendered |= render_mono_stereo(cr, skin, state.channels, 212, 41)?;
    let status_y = match state.play_status {
        PlayStatusValue::Playing => 0,
        PlayStatusValue::Paused => 9,
        PlayStatusValue::Stopped => 18,
    };
    rendered |= blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::PlayPause,
        0,
        status_y,
        24,
        28,
        11,
        9,
    )?;

    Ok(rendered)
}

fn main_push_button_rect(
    button: MainPushButton,
    pressed: bool,
) -> (SkinPixmapKind, i32, i32, i32, i32, i32, i32) {
    match button {
        MainPushButton::Menu => (
            SkinPixmapKind::Titlebar,
            0,
            if pressed { 9 } else { 0 },
            6,
            3,
            9,
            9,
        ),
        MainPushButton::Minimize => (
            SkinPixmapKind::Titlebar,
            9,
            if pressed { 9 } else { 0 },
            244,
            3,
            9,
            9,
        ),
        MainPushButton::Shade => (
            SkinPixmapKind::Titlebar,
            if pressed { 9 } else { 0 },
            18,
            254,
            3,
            9,
            9,
        ),
        MainPushButton::Close => (
            SkinPixmapKind::Titlebar,
            18,
            if pressed { 9 } else { 0 },
            264,
            3,
            9,
            9,
        ),
        MainPushButton::Previous => (
            SkinPixmapKind::CButtons,
            0,
            if pressed { 18 } else { 0 },
            16,
            88,
            23,
            18,
        ),
        MainPushButton::Play => (
            SkinPixmapKind::CButtons,
            23,
            if pressed { 18 } else { 0 },
            39,
            88,
            23,
            18,
        ),
        MainPushButton::Pause => (
            SkinPixmapKind::CButtons,
            46,
            if pressed { 18 } else { 0 },
            62,
            88,
            23,
            18,
        ),
        MainPushButton::Stop => (
            SkinPixmapKind::CButtons,
            69,
            if pressed { 18 } else { 0 },
            85,
            88,
            23,
            18,
        ),
        MainPushButton::Next => (
            SkinPixmapKind::CButtons,
            92,
            if pressed { 18 } else { 0 },
            108,
            88,
            22,
            18,
        ),
        MainPushButton::Eject => (
            SkinPixmapKind::CButtons,
            114,
            if pressed { 16 } else { 0 },
            136,
            89,
            22,
            16,
        ),
    }
}

fn main_toggle_button_rect(
    toggle: MainToggleButton,
    selected: bool,
    pressed: bool,
) -> (SkinPixmapKind, i32, i32, i32, i32, i32, i32) {
    let row = match (selected, pressed) {
        (false, false) => 0,
        (false, true) => 15,
        (true, false) => 30,
        (true, true) => 45,
    };
    match toggle {
        MainToggleButton::Shuffle => (SkinPixmapKind::ShufRep, 28, row, 164, 89, 46, 15),
        MainToggleButton::Repeat => (SkinPixmapKind::ShufRep, 0, row, 210, 89, 28, 15),
        MainToggleButton::Equalizer => {
            let x = match (selected, pressed) {
                (false, false) => 0,
                (false, true) => 46,
                (true, false) => 0,
                (true, true) => 46,
            };
            let y = if selected { 73 } else { 61 };
            (SkinPixmapKind::ShufRep, x, y, 219, 58, 23, 12)
        }
        MainToggleButton::Playlist => {
            let x = match (selected, pressed) {
                (false, false) => 23,
                (false, true) => 69,
                (true, false) => 23,
                (true, true) => 69,
            };
            let y = if selected { 73 } else { 61 };
            (SkinPixmapKind::ShufRep, x, y, 242, 58, 23, 12)
        }
    }
}

struct SliderRenderSpec {
    kind: SkinPixmapKind,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    position: i32,
    knob_source_x: i32,
    knob_source_y: i32,
    knob_width: i32,
    knob_height: i32,
    frame_height: i32,
    frame_offset: i32,
    frame: i32,
}

fn render_horizontal_slider(
    cr: &Context,
    skin: &DefaultSkin,
    spec: SliderRenderSpec,
) -> Result<bool, RenderError> {
    let mut rendered = blit_skin_rect(
        cr,
        skin,
        spec.kind,
        spec.frame_offset,
        spec.frame * spec.frame_height,
        spec.x,
        spec.y,
        spec.width,
        spec.height,
    )?;
    rendered |= blit_skin_rect(
        cr,
        skin,
        spec.kind,
        spec.knob_source_x,
        spec.knob_source_y,
        spec.x + spec.position,
        spec.y,
        spec.knob_width,
        spec.knob_height,
    )?;
    Ok(rendered)
}

fn blit_skin_rect(
    cr: &Context,
    skin: &DefaultSkin,
    kind: SkinPixmapKind,
    xsrc: i32,
    ysrc: i32,
    xdest: i32,
    ydest: i32,
    width: i32,
    height: i32,
) -> Result<bool, RenderError> {
    let Some(image) = skin.get(kind) else {
        return Ok(false);
    };
    let surface = surface_from_xpm(image)?;
    blit_surface_rect(cr, &surface, xsrc, ysrc, xdest, ydest, width, height)
}

fn render_text(
    cr: &Context,
    skin: &DefaultSkin,
    text: &str,
    xdest: i32,
    ydest: i32,
    width: i32,
) -> Result<(), RenderError> {
    let Some(image) = skin.get(SkinPixmapKind::Text) else {
        return Ok(());
    };
    let surface = surface_from_xpm(image)?;
    cr.save()?;
    cr.rectangle(
        f64::from(xdest),
        f64::from(ydest),
        f64::from(width),
        f64::from(TextBox::CHAR_HEIGHT),
    );
    cr.clip();
    for (index, ch) in text.chars().enumerate() {
        let Some((sx, sy)) = TextBox::glyph_source(ch) else {
            continue;
        };
        let dx = xdest + (index as i32 * TextBox::CHAR_WIDTH);
        if dx >= xdest + width {
            break;
        }
        blit_surface_rect(
            cr,
            &surface,
            sx,
            sy,
            dx,
            ydest,
            TextBox::CHAR_WIDTH,
            TextBox::CHAR_HEIGHT,
        )?;
    }
    cr.restore()?;
    Ok(())
}

fn render_visualization_reset(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    let bg = colors[0];
    cr.save()?;
    cr.rectangle(f64::from(xdest), f64::from(ydest), f64::from(width), 16.0);
    cr.clip();
    cr.set_source_rgb(
        f64::from(bg[0]) / 255.0,
        f64::from(bg[1]) / 255.0,
        f64::from(bg[2]) / 255.0,
    );
    cr.paint()?;
    let dot = colors[1];
    cr.set_source_rgb(
        f64::from(dot[0]) / 255.0,
        f64::from(dot[1]) / 255.0,
        f64::from(dot[2]) / 255.0,
    );
    for y in (1..16).step_by(2) {
        for x in (0..width.min(76)).step_by(2) {
            cr.rectangle(f64::from(xdest + x), f64::from(ydest + y), 1.0, 1.0);
        }
    }
    cr.fill()?;
    cr.restore()?;
    Ok(())
}

pub fn render_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    render_visualization_reset(cr, skin, xdest, ydest, width)?;
    if state.mode == VisMode::Off {
        return Ok(());
    }

    let mut levels = [0; 75];
    for (index, value) in state.data.iter().enumerate() {
        levels[index] = visualization_level(*value);
    }

    match state.mode {
        VisMode::Scope => render_scope_visualization(cr, skin, xdest, ydest, width, &levels, state),
        VisMode::Milkdrop => render_milkdrop_visualization(cr, skin, xdest, ydest, width, state),
        VisMode::Analyzer => {
            render_analyzer_visualization(cr, skin, xdest, ydest, width, &levels, state)
        }
        VisMode::Off => Ok(()),
    }
}

pub fn render_windowshade_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    cr.save()?;
    cr.rectangle(f64::from(xdest), f64::from(ydest), 38.0, 5.0);
    cr.clip();
    set_vis_color(cr, colors, 0);
    cr.paint()?;

    if state.mode == VisMode::Off {
        cr.restore()?;
        return Ok(());
    }

    if state.mode == VisMode::Scope {
        const SCOPE_COLORS: [usize; 5] = [20, 19, 18, 19, 20];
        for sx in 0..38 {
            let h = (visualization_level(state.data[(sx * 2) as usize]) / 3).clamp(0, 4);
            set_vis_color(cr, colors, SCOPE_COLORS[h as usize]);
            cr.rectangle(f64::from(xdest + sx), f64::from(ydest + 4 - h), 1.0, 1.0);
            cr.fill()?;
        }
        cr.restore()?;
        return Ok(());
    }

    const NORMAL_COLORS: [usize; 8] = [17, 17, 17, 12, 12, 12, 2, 2];
    for row in 0..2 {
        let level = (state.data[row].clamp(0.0, 1.0) * 37.0 + 0.5).clamp(0.0, 37.0) as i32;
        if state.vu_mode == VisVuMode::Smooth {
            for sx in 0..level.min(38) {
                set_vis_color(cr, colors, 17 - ((sx * 15) / 37) as usize);
                cr.rectangle(
                    f64::from(xdest + sx),
                    f64::from(ydest + row as i32 * 3),
                    1.0,
                    1.0,
                );
                cr.rectangle(
                    f64::from(xdest + sx),
                    f64::from(ydest + row as i32 * 3 + 1),
                    1.0,
                    1.0,
                );
                cr.fill()?;
            }
        } else {
            let bars = ((level * 7) / 37).clamp(0, 7);
            for sx in 0..bars {
                set_vis_color(cr, colors, NORMAL_COLORS[sx as usize]);
                cr.rectangle(
                    f64::from(xdest + sx * 5),
                    f64::from(ydest + row as i32 * 3),
                    3.0,
                    2.0,
                );
                cr.fill()?;
            }
        }
    }

    cr.restore()?;
    Ok(())
}

fn render_scope_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    levels: &[i32; 75],
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    const SCOPE_COLORS: [usize; 13] = [21, 21, 20, 20, 19, 19, 18, 19, 19, 20, 20, 21, 21];
    let colors = skin.vis_colors();
    for x in 0..75.min(width) {
        let h = levels[x as usize].clamp(0, 15);
        match state.scope_mode {
            VisScopeMode::Dot => draw_vis_pixel(
                cr,
                colors,
                xdest,
                ydest,
                width,
                x,
                15 - h,
                SCOPE_COLORS[h.clamp(0, 12) as usize],
            )?,
            VisScopeMode::Line => {
                let y1 = 15 - h;
                let y2 = if x < 74 && x + 1 < width {
                    15 - levels[(x + 1) as usize].clamp(0, 15)
                } else {
                    y1
                };
                for y in y1.min(y2)..=y1.max(y2) {
                    draw_vis_pixel(
                        cr,
                        colors,
                        xdest,
                        ydest,
                        width,
                        x,
                        y,
                        SCOPE_COLORS[(y - 3).clamp(0, 12) as usize],
                    )?;
                }
            }
            VisScopeMode::Solid => {
                let y1 = 15 - h;
                let color = SCOPE_COLORS[h.clamp(0, 12) as usize];
                for y in y1.min(9)..=y1.max(9) {
                    draw_vis_pixel(cr, colors, xdest, ydest, width, x, y, color)?;
                }
            }
        }
    }
    Ok(())
}

fn render_analyzer_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    levels: &[i32; 75],
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    for x in 0..75.min(width) {
        let h = if state.analyzer_style == VisAnalyzerStyle::Bars {
            if x % 4 == 3 {
                continue;
            }
            levels[(x >> 2) as usize]
        } else {
            levels[x as usize]
        }
        .clamp(0, 16);

        if h <= 0 {
            continue;
        }

        for y in 16 - h..16 {
            draw_vis_pixel(
                cr,
                colors,
                xdest,
                ydest,
                width,
                x,
                y,
                analyzer_color(state.analyzer_mode, y, h),
            )?;
        }

        if state.peaks_enabled {
            let peak_index = if state.analyzer_style == VisAnalyzerStyle::Bars {
                (x >> 2) as usize
            } else {
                x as usize
            };
            let peak_y = 16 - visualization_level(state.peak[peak_index]);
            if (0..16).contains(&peak_y) {
                draw_vis_pixel(cr, colors, xdest, ydest, width, x, peak_y, 23)?;
            }
        }
    }
    Ok(())
}

fn render_milkdrop_visualization(
    cr: &Context,
    skin: &DefaultSkin,
    xdest: i32,
    ydest: i32,
    width: i32,
    state: &VisualizationRenderState,
) -> Result<(), RenderError> {
    let colors = skin.vis_colors();
    let draw_width = width.min(76);
    let cx = (width - 1) as f32 * 0.5;
    let cy = 7.5_f32;
    let phase = state.milkdrop_phase;
    let energy = state.milkdrop_energy.clamp(0.0, 1.0);
    let rot = phase * (0.35 + energy * 0.25);
    let (s, c) = rot.sin_cos();

    for y in 0..16 {
        for x in 0..draw_width {
            let nx = (x as f32 - cx) / cx.max(1.0);
            let ny = (y as f32 - cy) / 8.0;
            let rx = nx * c - ny * s;
            let ry = nx * s + ny * c;
            let r = (rx * rx + ry * ry).sqrt();
            let angle = ry.atan2(rx);
            let warp =
                (angle * 5.0 + phase * 1.7).sin() * 0.18 + (r * 12.0 - phase * 2.4).sin() * 0.16;
            let tunnel = ((r + warp) * 18.0 - phase * 3.0).sin();
            let plasma = ((rx - ry) * 7.0 + phase).sin() + ((rx + ry) * 5.0 - phase * 1.3).cos();
            let color =
                (3.0 + (tunnel + plasma + 4.0 + energy * 2.0) * 2.6).clamp(2.0, 22.0) as usize;
            draw_vis_pixel(cr, colors, xdest, ydest, width, x, y, color)?;
        }
    }

    for x in 0..width.min(75) {
        let sample = state.data[x as usize];
        let y = (7.5 + (x as f32 * 0.19 + phase * 2.0).sin() * 3.0 - sample * 6.0).clamp(0.0, 15.0)
            as i32;
        draw_vis_pixel(cr, colors, xdest, ydest, width, x, y, 23)?;
        if y + 1 < 16 {
            draw_vis_pixel(cr, colors, xdest, ydest, width, x, y + 1, 18)?;
        }
    }

    for i in 0..28 {
        let a = phase * 0.9 + i as f32 * (std::f32::consts::TAU / 28.0);
        let radius = 2.0 + energy * 7.0 + (phase * 1.4 + i as f32 * 0.7).sin() * 1.4;
        let x = (cx + a.cos() * radius * 3.4).clamp(0.0, (width - 1) as f32) as i32;
        let y = (cy + a.sin() * radius).clamp(0.0, 15.0) as i32;
        draw_vis_pixel(cr, colors, xdest, ydest, width, x, y, 21 + (i % 3))?;
    }

    Ok(())
}

fn visualization_level(value: f32) -> i32 {
    (value * 16.0 + 0.5).clamp(0.0, 16.0) as i32
}

fn analyzer_color(mode: VisAnalyzerMode, row: i32, height: i32) -> usize {
    match mode {
        VisAnalyzerMode::Fire => (16 - height + row + 2).clamp(0, 23) as usize,
        VisAnalyzerMode::VerticalLines => (18 - height).clamp(0, 23) as usize,
        VisAnalyzerMode::Normal => (row + 2).clamp(0, 23) as usize,
    }
}

fn draw_vis_pixel(
    cr: &Context,
    colors: &[[u8; 3]; 24],
    xdest: i32,
    ydest: i32,
    width: i32,
    x: i32,
    y: i32,
    color_idx: usize,
) -> Result<(), RenderError> {
    if x < 0 || x >= width || y < 0 || y >= 16 {
        return Ok(());
    }
    set_vis_color(cr, colors, color_idx);
    cr.rectangle(f64::from(xdest + x), f64::from(ydest + y), 1.0, 1.0);
    cr.fill()?;
    Ok(())
}

fn set_vis_color(cr: &Context, colors: &[[u8; 3]; 24], color_idx: usize) {
    let color = colors[color_idx.min(23)];
    cr.set_source_rgb(
        f64::from(color[0]) / 255.0,
        f64::from(color[1]) / 255.0,
        f64::from(color[2]) / 255.0,
    );
}

fn render_mono_stereo(
    cr: &Context,
    skin: &DefaultSkin,
    channels: i32,
    xdest: i32,
    ydest: i32,
) -> Result<bool, RenderError> {
    let (stereo_y, mono_y) = match channels {
        2 => (0, 12),
        1 => (12, 0),
        _ => (12, 12),
    };
    let mut rendered = blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::MonoStereo,
        0,
        stereo_y,
        xdest,
        ydest,
        29,
        12,
    )?;
    rendered |= blit_skin_rect(
        cr,
        skin,
        SkinPixmapKind::MonoStereo,
        29,
        mono_y,
        xdest + 29,
        ydest,
        27,
        12,
    )?;
    Ok(rendered)
}

pub fn render_equalizer_background(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
) -> Result<bool, RenderError> {
    if shaded {
        let Some(eq_ex) = skin.get(SkinPixmapKind::EqEx) else {
            return Ok(false);
        };
        let eq_ex = surface_from_xpm(eq_ex)?;
        return blit_surface_rect(
            cr,
            &eq_ex,
            0,
            if focused { 0 } else { 15 },
            0,
            0,
            EQUALIZER_WINDOW_WIDTH,
            MAIN_TITLEBAR_HEIGHT,
        );
    }

    let Some(eqmain_image) = skin.get(SkinPixmapKind::EqMain) else {
        return Ok(false);
    };
    let eqmain = surface_from_xpm(eqmain_image)?;
    let mut rendered = blit_surface_rect(
        cr,
        &eqmain,
        0,
        0,
        0,
        0,
        EQUALIZER_WINDOW_WIDTH,
        EQUALIZER_WINDOW_HEIGHT,
    )?;
    if eqmain_image.height() >= 163 {
        rendered |= blit_surface_rect(
            cr,
            &eqmain,
            0,
            if focused { 134 } else { 149 },
            0,
            0,
            EQUALIZER_WINDOW_WIDTH,
            MAIN_TITLEBAR_HEIGHT,
        )?;
    }
    Ok(rendered)
}

pub fn render_equalizer_state(
    cr: &Context,
    skin: &DefaultSkin,
    state: &EqualizerRenderState,
) -> Result<bool, RenderError> {
    let mut rendered = render_equalizer_background(cr, skin, state.focused, state.shaded)?;
    if state.shaded {
        return Ok(rendered);
    }

    let Some(eqmain_image) = skin.get(SkinPixmapKind::EqMain) else {
        return Ok(rendered);
    };
    let eqmain = surface_from_xpm(eqmain_image)?;

    rendered |= draw_eq_toggle_button(
        cr,
        &eqmain,
        state.active,
        state.pressed_control == Some(EqualizerControl::On),
        (10, 119),
        (128, 119),
        (69, 119),
        (187, 119),
        14,
        18,
        25,
        12,
    )?;
    rendered |= draw_eq_toggle_button(
        cr,
        &eqmain,
        state.automatic,
        state.pressed_control == Some(EqualizerControl::Auto),
        (35, 119),
        (153, 119),
        (94, 119),
        (212, 119),
        39,
        18,
        33,
        12,
    )?;

    rendered |= blit_surface_rect(
        cr,
        &eqmain,
        224,
        if state.pressed_control == Some(EqualizerControl::Presets) {
            176
        } else {
            164
        },
        217,
        18,
        44,
        12,
    )?;

    rendered |= draw_eq_slider(cr, &eqmain, 21, state.preamp_position)?;
    for (idx, position) in state.band_positions.iter().enumerate() {
        rendered |= draw_eq_slider(cr, &eqmain, 78 + idx as i32 * 18, *position)?;
    }
    draw_eq_graph(cr, &state.band_positions)?;

    Ok(rendered)
}

fn draw_eq_toggle_button(
    cr: &Context,
    eqmain: &ImageSurface,
    selected: bool,
    pressed: bool,
    normal_unselected: (i32, i32),
    pressed_unselected: (i32, i32),
    normal_selected: (i32, i32),
    pressed_selected: (i32, i32),
    dest_x: i32,
    dest_y: i32,
    width: i32,
    height: i32,
) -> Result<bool, RenderError> {
    let (src_x, src_y) = match (selected, pressed) {
        (true, true) => pressed_selected,
        (true, false) => normal_selected,
        (false, true) => pressed_unselected,
        (false, false) => normal_unselected,
    };
    blit_surface_rect(cr, eqmain, src_x, src_y, dest_x, dest_y, width, height)
}

fn draw_eq_slider(
    cr: &Context,
    eqmain: &ImageSurface,
    x: i32,
    position: i32,
) -> Result<bool, RenderError> {
    let knob_y = 38 + (position.clamp(0, 100) * (63 - 11)) / 100;
    blit_surface_rect(cr, eqmain, 0, 164, x, knob_y, 14, 11)
}

fn draw_eq_graph(cr: &Context, band_positions: &[i32; 10]) -> Result<(), RenderError> {
    let graph_x = 86.0;
    let graph_y = 17.0;
    let graph_w = 113.0;
    let graph_h = 19.0;

    cr.set_source_rgb(0.0, 1.0, 0.0);
    cr.set_line_width(1.0);
    for (idx, position) in band_positions.iter().enumerate() {
        let x = graph_x + (idx as f64 * graph_w) / 9.0;
        let value = f64::from(50 - (*position).clamp(0, 100)) / 50.0;
        let y = graph_y + graph_h / 2.0 - value * (graph_h / 2.0 - 1.0);
        if idx == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    cr.stroke()?;
    Ok(())
}

pub fn render_playlist_frame(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
    width: i32,
    height: i32,
    shaded_info: Option<&str>,
) -> Result<bool, RenderError> {
    let width = width.max(PLAYLIST_MIN_WIDTH);
    let height = playlist_window_height(shaded, height);
    let Some(pledit) = skin.get(SkinPixmapKind::PlEdit) else {
        return Ok(false);
    };
    let pledit = surface_from_xpm(pledit)?;

    if shaded {
        blit_surface_rect(cr, &pledit, 72, 42, 0, 0, 25, 14)?;
        let count = (width - 75) / 25;
        for i in 0..count {
            blit_surface_rect(cr, &pledit, 72, 57, (i * 25) + 25, 0, 25, 14)?;
        }
        blit_surface_rect(
            cr,
            &pledit,
            99,
            if focused { 42 } else { 57 },
            width - 50,
            0,
            50,
            14,
        )?;
        draw_playlist_shaded_info(cr, width, shaded_info.unwrap_or(""))?;
        return Ok(true);
    }

    let title_y = if focused { 0 } else { 21 };

    let colors = skin.playlist_colors();
    cr.set_source_rgb(
        f64::from(colors.normal_bg[0]) / 255.0,
        f64::from(colors.normal_bg[1]) / 255.0,
        f64::from(colors.normal_bg[2]) / 255.0,
    );
    cr.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
    cr.fill()?;

    blit_surface_rect(cr, &pledit, 0, title_y, 0, 0, 25, 20)?;
    let mut count = (width - 150) / 25;
    for i in 0..count / 2 {
        blit_surface_rect(cr, &pledit, 127, title_y, (i * 25) + 25, 0, 25, 20)?;
        blit_surface_rect(
            cr,
            &pledit,
            127,
            title_y,
            (i * 25) + (width / 2) + 50,
            0,
            25,
            20,
        )?;
    }
    if count & 1 == 1 {
        blit_surface_rect(
            cr,
            &pledit,
            127,
            title_y,
            ((count / 2) * 25) + 25,
            0,
            12,
            20,
        )?;
        blit_surface_rect(
            cr,
            &pledit,
            127,
            title_y,
            (width / 2) + ((count / 2) * 25) + 50,
            0,
            13,
            20,
        )?;
    }
    blit_surface_rect(cr, &pledit, 26, title_y, (width / 2) - 50, 0, 100, 20)?;
    blit_surface_rect(cr, &pledit, 153, title_y, width - 25, 0, 25, 20)?;

    for i in 0..(height - 58) / 29 {
        let ydest = (i * 29) + 20;
        blit_surface_rect(cr, &pledit, 0, 42, 0, ydest, 12, 29)?;
        blit_surface_rect(cr, &pledit, 32, 42, width - 19, ydest, 19, 29)?;
    }

    blit_surface_rect(cr, &pledit, 0, 72, 0, height - 38, 125, 38)?;

    count = (width - 275) / 25;
    if count >= 3 {
        count -= 3;
        blit_surface_rect(cr, &pledit, 205, 0, width - 225, height - 38, 75, 38)?;
    }
    for i in 0..count {
        blit_surface_rect(cr, &pledit, 179, 0, (i * 25) + 125, height - 38, 25, 38)?;
    }
    blit_surface_rect(cr, &pledit, 126, 72, width - 150, height - 38, 150, 38)?;

    cr.set_source_rgb(10.0 / 255.0, 18.0 / 255.0, 26.0 / 255.0);
    cr.rectangle(f64::from(width - 82), f64::from(height - 15), 28.0, 9.0);
    cr.fill()?;

    Ok(true)
}

pub fn render_playlist_rows(
    cr: &Context,
    skin: &DefaultSkin,
    state: &PlaylistRowsRenderState,
) -> Result<bool, RenderError> {
    let width = state.width.max(PLAYLIST_MIN_WIDTH);
    let height = state.height.max(PLAYLIST_MIN_HEIGHT);
    let list_x = 12;
    let list_y = 20;
    let list_w = width - 31;
    let list_h = height - 58;
    let entry_h = 11;
    let colors = skin.playlist_colors();

    set_rgb(cr, colors.normal_bg);
    cr.rectangle(
        f64::from(list_x),
        f64::from(list_y),
        f64::from(list_w),
        f64::from(list_h),
    );
    cr.fill()?;

    set_playlist_font(cr, &state.font_family);
    let baseline = cr.font_extents()?.ascent().ceil() as i32;
    let visible = (list_h / entry_h).max(0) as usize;
    for row in 0..visible {
        let Some(entry) = state.entries.get(row + state.scroll_offset) else {
            break;
        };
        let y = list_y + row as i32 * entry_h;
        if entry.selected {
            set_rgb(cr, colors.selected_bg);
            cr.rectangle(
                f64::from(list_x),
                f64::from(y),
                f64::from(list_w),
                f64::from(entry_h),
            );
            cr.fill()?;
        }

        if entry.current {
            set_rgb(cr, colors.current);
        } else {
            set_rgb(cr, colors.normal);
        }

        let mut text_w = list_w;
        if entry.length_ms > 0 {
            let duration = format_duration(entry.length_ms);
            let extents = cr.text_extents(&duration)?;
            cr.move_to(
                f64::from(list_x + list_w) - extents.width() - 2.0,
                f64::from(y + baseline),
            );
            cr.show_text(&duration)?;
            text_w = (f64::from(list_w) - extents.width() - 5.0).max(1.0) as i32;
        }

        let mut title = normalize_playlist_text(&entry.title);
        if state.show_numbers {
            title = format!("{}. {title}", row + state.scroll_offset + 1);
        }
        let title = ellipsize_text(cr, title, text_w)?;
        cr.move_to(f64::from(list_x), f64::from(y + baseline));
        cr.show_text(&title)?;
    }

    if let Some((thumb_y, thumb_h)) =
        playlist_scrollbar_geometry(state.entries.len(), visible, state.scroll_offset, height)
    {
        if let Some(pledit) = skin.get(SkinPixmapKind::PlEdit) {
            let pledit = surface_from_xpm(pledit)?;
            blit_surface_rect(
                cr,
                &pledit,
                if state.scrollbar_dragging { 52 } else { 61 },
                53,
                width - 15,
                thumb_y,
                8,
                thumb_h,
            )?;
        }
    }

    if let Some(query) = state.search_query.as_ref() {
        draw_playlist_search(cr, colors, query, width, height)?;
    }

    Ok(true)
}

fn draw_playlist_search(
    cr: &Context,
    colors: crate::skin::PlaylistColors,
    query: &str,
    width: i32,
    height: i32,
) -> Result<(), RenderError> {
    let x = 10;
    let y = height - 48;
    let w = width - 20;
    let h = 14;

    set_rgb(cr, colors.normal_bg);
    cr.rectangle(f64::from(x), f64::from(y), f64::from(w), f64::from(h));
    cr.fill()?;

    set_rgb(cr, colors.selected_bg);
    cr.rectangle(
        f64::from(x) + 0.5,
        f64::from(y) + 0.5,
        f64::from(w - 1),
        f64::from(h - 1),
    );
    cr.stroke()?;

    set_playlist_font(cr, "Helvetica");
    set_rgb(cr, colors.normal);
    let display = ellipsize_text(cr, format!("/{query}"), w - 6)?;
    let extents = cr.text_extents(&display)?;
    cr.move_to(
        f64::from(x + 3),
        f64::from(y) + (f64::from(h) - extents.height()) / 2.0 - extents.y_bearing(),
    );
    cr.show_text(&display)?;
    Ok(())
}

fn playlist_scrollbar_geometry(
    total_entries: usize,
    visible_entries: usize,
    scroll_offset: usize,
    height: i32,
) -> Option<(i32, i32)> {
    if total_entries <= visible_entries || visible_entries == 0 {
        return None;
    }
    let list_y = 20;
    let list_h = height.max(PLAYLIST_MIN_HEIGHT) - 58;
    let thumb_h = 18;
    let max_scroll = total_entries - visible_entries;
    let max_thumb_pos = (list_h - thumb_h).max(0);
    let thumb_y = list_y
        + ((scroll_offset.min(max_scroll) as i32 * max_thumb_pos) / max_scroll.max(1) as i32);
    Some((thumb_y, thumb_h))
}

fn draw_playlist_shaded_info(cr: &Context, width: i32, text: &str) -> Result<(), RenderError> {
    cr.save()?;
    cr.set_source_rgb(0.58, 0.82, 0.58);
    set_playlist_font(cr, "Helvetica");
    cr.rectangle(4.0, 3.0, f64::from((width - 35).max(1)), 8.0);
    cr.clip();
    cr.move_to(4.0, 10.0);
    cr.show_text(text)?;
    cr.restore()?;
    Ok(())
}

fn set_playlist_font(cr: &Context, family: &str) {
    cr.select_font_face(
        if family.trim().is_empty() {
            "Helvetica"
        } else {
            family.trim()
        },
        cairo::FontSlant::Normal,
        cairo::FontWeight::Bold,
    );
    cr.set_font_size(9.0);
}

fn set_rgb(cr: &Context, color: [u8; 3]) {
    cr.set_source_rgb(
        f64::from(color[0]) / 255.0,
        f64::from(color[1]) / 255.0,
        f64::from(color[2]) / 255.0,
    );
}

fn format_duration(length_ms: i64) -> String {
    let total_seconds = (length_ms / 1000).max(0);
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn normalize_playlist_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect()
}

fn ellipsize_text(cr: &Context, mut text: String, max_width: i32) -> Result<String, RenderError> {
    if max_width <= 0 {
        return Ok(String::new());
    }
    if cr.text_extents(&text)?.width() <= f64::from(max_width) {
        return Ok(text);
    }
    while text.chars().count() > 4 {
        let mut truncated = text.chars().collect::<Vec<_>>();
        truncated.truncate(truncated.len() - 1);
        text = truncated.into_iter().collect();
        let candidate = format!("{text}...");
        if cr.text_extents(&candidate)?.width() <= f64::from(max_width) {
            return Ok(candidate);
        }
    }
    Ok(text)
}

pub fn render_playlist_menu(
    cr: &Context,
    skin: &DefaultSkin,
    state: PlaylistMenuRenderState,
) -> Result<bool, RenderError> {
    let Some(pledit) = skin.get(SkinPixmapKind::PlEdit) else {
        return Ok(false);
    };
    let pledit = surface_from_xpm(pledit)?;
    let items = playlist_menu_items(state.kind);
    for (idx, item) in items.iter().enumerate() {
        let selected = state.hover == Some(idx);
        let (src_x, src_y) = if selected {
            (item.selected_x, item.selected_y)
        } else {
            (item.normal_x, item.normal_y)
        };
        blit_surface_rect(cr, &pledit, src_x, src_y, 3, idx as i32 * 18, 22, 18)?;
    }
    let (border_x, border_y) = playlist_menu_border_source(state.kind);
    blit_surface_rect(
        cr,
        &pledit,
        border_x,
        border_y,
        0,
        0,
        3,
        items.len() as i32 * 18,
    )
}

#[derive(Debug, Clone, Copy)]
struct PlaylistMenuItemSource {
    normal_x: i32,
    normal_y: i32,
    selected_x: i32,
    selected_y: i32,
}

fn playlist_menu_items(kind: PlaylistMenuRenderKind) -> &'static [PlaylistMenuItemSource] {
    match kind {
        PlaylistMenuRenderKind::Add => &[
            PlaylistMenuItemSource {
                normal_x: 0,
                normal_y: 111,
                selected_x: 23,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 0,
                normal_y: 130,
                selected_x: 23,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 0,
                normal_y: 149,
                selected_x: 23,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::Remove => &[
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 168,
                selected_x: 77,
                selected_y: 168,
            },
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 111,
                selected_x: 77,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 130,
                selected_x: 77,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 54,
                normal_y: 149,
                selected_x: 77,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::Select => &[
            PlaylistMenuItemSource {
                normal_x: 104,
                normal_y: 111,
                selected_x: 127,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 104,
                normal_y: 130,
                selected_x: 127,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 104,
                normal_y: 149,
                selected_x: 127,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::Misc => &[
            PlaylistMenuItemSource {
                normal_x: 154,
                normal_y: 111,
                selected_x: 177,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 154,
                normal_y: 130,
                selected_x: 177,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 154,
                normal_y: 149,
                selected_x: 177,
                selected_y: 149,
            },
        ],
        PlaylistMenuRenderKind::List => &[
            PlaylistMenuItemSource {
                normal_x: 204,
                normal_y: 111,
                selected_x: 227,
                selected_y: 111,
            },
            PlaylistMenuItemSource {
                normal_x: 204,
                normal_y: 130,
                selected_x: 227,
                selected_y: 130,
            },
            PlaylistMenuItemSource {
                normal_x: 204,
                normal_y: 149,
                selected_x: 227,
                selected_y: 149,
            },
        ],
    }
}

fn playlist_menu_border_source(kind: PlaylistMenuRenderKind) -> (i32, i32) {
    match kind {
        PlaylistMenuRenderKind::Add => (48, 111),
        PlaylistMenuRenderKind::Remove => (100, 111),
        PlaylistMenuRenderKind::Select => (150, 111),
        PlaylistMenuRenderKind::Misc => (200, 111),
        PlaylistMenuRenderKind::List => (250, 111),
    }
}

pub fn render_docked_panels(
    cr: &Context,
    skin: &DefaultSkin,
    state: DockedPanelState,
) -> Result<bool, RenderError> {
    let mut y = 0;
    let mut rendered = render_main_player(cr, skin, state.main_focused, state.main_shaded)?;
    y += main_window_height(state.main_shaded);

    if state.equalizer_visible && !state.equalizer_detached {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        rendered |=
            render_equalizer_background(cr, skin, state.equalizer_focused, state.equalizer_shaded)?;
        cr.restore()?;
        y += equalizer_window_height(state.equalizer_shaded);
    }

    if state.playlist_visible && !state.playlist_detached {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        rendered |= render_playlist_frame(
            cr,
            skin,
            state.playlist_focused,
            state.playlist_shaded,
            state.playlist_width,
            state.playlist_height,
            None,
        )?;
        cr.restore()?;
    }

    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_cairo_surface_from_xpm_pixels() {
        let image = XpmImage::parse(
            r#"static char *x[] = {
            "2 1 2 1",
            "a c #000000",
            "b c None",
            "ab"
            };"#,
        )
        .unwrap();

        let surface = surface_from_xpm(&image).unwrap();
        assert_eq!(surface.width(), 2);
        assert_eq!(surface.height(), 1);

        let mut surface = surface;
        let data = surface.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(data[0..4].try_into().unwrap()),
            0xff00_0000
        );
        assert_eq!(u32::from_ne_bytes(data[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn blits_source_rect_to_destination() {
        let image = XpmImage::parse(
            r#"static char *x[] = {
            "2 1 2 1",
            "a c #010203",
            "b c #040506",
            "ab"
            };"#,
        )
        .unwrap();
        let source = surface_from_xpm(&image).unwrap();
        let mut dest = ImageSurface::create(Format::ARgb32, 2, 1).unwrap();
        let cr = Context::new(&dest).unwrap();

        assert!(blit_surface_rect(&cr, &source, 1, 0, 0, 0, 1, 1).unwrap());
        drop(cr);
        dest.flush();

        let data = dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(data[0..4].try_into().unwrap()),
            0xff04_0506
        );
        assert_eq!(u32::from_ne_bytes(data[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn blit_clips_negative_source_coordinates_like_c() {
        let image = XpmImage::parse(
            r#"static char *x[] = {
            "1 1 1 1",
            "a c #010203",
            "a"
            };"#,
        )
        .unwrap();
        let source = surface_from_xpm(&image).unwrap();
        let mut dest = ImageSurface::create(Format::ARgb32, 2, 1).unwrap();
        let cr = Context::new(&dest).unwrap();

        assert!(blit_surface_rect(&cr, &source, -1, 0, 0, 0, 2, 1).unwrap());
        drop(cr);
        dest.flush();

        let data = dest.data().unwrap();
        assert_eq!(u32::from_ne_bytes(data[0..4].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_ne_bytes(data[4..8].try_into().unwrap()),
            0xff01_0203
        );
    }

    #[test]
    fn scaling_helpers_match_c_rounding_and_clamping() {
        assert_eq!(clamp_scale_factor(0.5), 1.0);
        assert_eq!(clamp_scale_factor(6.0), 5.0);
        assert_eq!(scale_coord(11, 1.5), 17);
        assert_eq!(scale_dim(0, 1.0), 1);
    }

    #[test]
    fn applies_window_scale_from_actual_to_base_size() {
        let surface = ImageSurface::create(Format::ARgb32, 20, 20).unwrap();
        let cr = Context::new(&surface).unwrap();

        assert!(apply_window_scale(&cr, 20, 10, 10, 5));
        let matrix = cr.matrix();
        assert_eq!(matrix.xx(), 2.0);
        assert_eq!(matrix.yy(), 2.0);
        assert!(!apply_window_scale(&cr, 0, 10, 10, 5));
    }

    #[test]
    fn renders_main_titlebar_focused_and_unfocused_rows() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let titlebar = surface_from_xpm(skin.get(SkinPixmapKind::Titlebar).unwrap()).unwrap();

        let mut focused_dest =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let focused_cr = Context::new(&focused_dest).unwrap();
        assert!(render_main_titlebar(&focused_cr, &skin, true, false).unwrap());
        drop(focused_cr);
        focused_dest.flush();

        let mut unfocused_dest =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let unfocused_cr = Context::new(&unfocused_dest).unwrap();
        assert!(render_main_titlebar(&unfocused_cr, &skin, false, false).unwrap());
        drop(unfocused_cr);
        unfocused_dest.flush();

        let mut titlebar = titlebar;
        let titlebar_stride = titlebar.stride() as usize;
        let titlebar_data = titlebar.data().unwrap();
        let focused_source_offset = titlebar_stride * 0 + 27 * 4;
        let unfocused_source_offset = titlebar_stride * 15 + 27 * 4;
        let expected_focused = u32::from_ne_bytes(
            titlebar_data[focused_source_offset..focused_source_offset + 4]
                .try_into()
                .unwrap(),
        );
        let expected_unfocused = u32::from_ne_bytes(
            titlebar_data[unfocused_source_offset..unfocused_source_offset + 4]
                .try_into()
                .unwrap(),
        );

        let focused_data = focused_dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(focused_data[0..4].try_into().unwrap()),
            expected_focused
        );
        drop(focused_data);

        let unfocused_data = unfocused_dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(unfocused_data[0..4].try_into().unwrap()),
            expected_unfocused
        );
    }

    #[test]
    fn renders_normal_and_windowshade_main_player_backgrounds() {
        let skin = DefaultSkin::load_bundled().unwrap();

        let mut normal =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT).unwrap();
        let normal_cr = Context::new(&normal).unwrap();
        assert!(render_main_player(&normal_cr, &skin, true, false).unwrap());
        drop(normal_cr);
        normal.flush();

        let mut shaded =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let shaded_cr = Context::new(&shaded).unwrap();
        assert!(render_main_player(&shaded_cr, &skin, true, true).unwrap());
        drop(shaded_cr);
        shaded.flush();

        assert_eq!(main_window_height(false), MAIN_WINDOW_HEIGHT);
        assert_eq!(main_window_height(true), MAIN_TITLEBAR_HEIGHT);

        let normal_data = normal.data().unwrap();
        let shaded_data = shaded.data().unwrap();
        assert_ne!(u32::from_ne_bytes(normal_data[0..4].try_into().unwrap()), 0);
        assert_ne!(u32::from_ne_bytes(shaded_data[0..4].try_into().unwrap()), 0);
    }

    #[test]
    fn computes_and_renders_docked_panel_composition() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let state = DockedPanelState {
            equalizer_visible: true,
            playlist_visible: true,
            ..DockedPanelState::default()
        };
        let (width, height) = docked_panel_size(state);
        assert_eq!(width, MAIN_WINDOW_WIDTH);
        assert_eq!(
            height,
            MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT
        );

        let mut surface = ImageSurface::create(Format::ARgb32, width, height).unwrap();
        let cr = Context::new(&surface).unwrap();
        assert!(render_docked_panels(&cr, &skin, state).unwrap());
        drop(cr);
        surface.flush();

        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let main_pixel = u32::from_ne_bytes(data[0..4].try_into().unwrap());
        let playlist_offset = stride * (MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT) as usize;
        let playlist_pixel = u32::from_ne_bytes(
            data[playlist_offset..playlist_offset + 4]
                .try_into()
                .unwrap(),
        );
        assert_ne!(main_pixel, 0);
        assert_ne!(playlist_pixel, 0);
    }

    #[test]
    fn docked_panel_size_ignores_detached_panels() {
        let state = DockedPanelState {
            equalizer_visible: true,
            equalizer_detached: true,
            playlist_visible: true,
            playlist_detached: true,
            ..DockedPanelState::default()
        };

        assert_eq!(
            docked_panel_size(state),
            (MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT)
        );
    }
}
