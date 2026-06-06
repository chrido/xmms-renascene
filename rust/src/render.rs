use std::fmt;

use cairo::{Context, Extend, Filter, Format, ImageSurface, Rectangle};

use crate::skin::widget::TextBox;
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

pub fn render_main_player_reset(cr: &Context, skin: &DefaultSkin) -> Result<bool, RenderError> {
    let mut rendered = render_main_player(cr, skin, true, false)?;

    let push_buttons = [
        (SkinPixmapKind::Titlebar, 0, 0, 6, 3, 9, 9),
        (SkinPixmapKind::Titlebar, 9, 0, 244, 3, 9, 9),
        (SkinPixmapKind::Titlebar, 0, 18, 254, 3, 9, 9),
        (SkinPixmapKind::Titlebar, 18, 0, 264, 3, 9, 9),
        (SkinPixmapKind::CButtons, 0, 0, 16, 88, 23, 18),
        (SkinPixmapKind::CButtons, 23, 0, 39, 88, 23, 18),
        (SkinPixmapKind::CButtons, 46, 0, 62, 88, 23, 18),
        (SkinPixmapKind::CButtons, 69, 0, 85, 88, 23, 18),
        (SkinPixmapKind::CButtons, 92, 0, 108, 88, 22, 18),
        (SkinPixmapKind::CButtons, 114, 0, 136, 89, 22, 16),
        (SkinPixmapKind::ShufRep, 28, 0, 164, 89, 46, 15),
        (SkinPixmapKind::ShufRep, 0, 0, 210, 89, 28, 15),
        (SkinPixmapKind::ShufRep, 0, 61, 219, 58, 23, 12),
        (SkinPixmapKind::ShufRep, 23, 61, 242, 58, 23, 12),
    ];
    for (kind, sx, sy, dx, dy, width, height) in push_buttons {
        rendered |= blit_skin_rect(cr, skin, kind, sx, sy, dx, dy, width, height)?;
    }

    render_text(cr, skin, "XMMS Resuscitated", 111, 27, 153)?;
    render_text(cr, skin, "", 111, 43, 15)?;
    render_text(cr, skin, "", 156, 43, 10)?;

    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::Volume,
            x: 107,
            y: 57,
            width: 68,
            height: 13,
            position: 51,
            knob_source_x: 15,
            knob_source_y: 422,
            knob_width: 14,
            knob_height: 11,
            frame_height: 15,
            frame_offset: 0,
            frame: 27,
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
            position: 12,
            knob_source_x: 15,
            knob_source_y: 422,
            knob_width: 14,
            knob_height: 11,
            frame_height: 15,
            frame_offset: 0,
            frame: 13,
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
            position: 0,
            knob_source_x: 248,
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

    render_visualization_reset(cr, skin, 24, 43, 76)?;
    rendered |= render_mono_stereo(cr, skin, 0, 212, 41)?;
    rendered |= blit_skin_rect(cr, skin, SkinPixmapKind::PlayPause, 0, 18, 24, 28, 11, 9)?;

    Ok(rendered)
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

pub fn render_playlist_frame(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
    width: i32,
    height: i32,
) -> Result<bool, RenderError> {
    let width = width.max(PLAYLIST_MIN_WIDTH);
    let height = playlist_window_height(shaded, height);
    let Some(pledit) = skin.get(SkinPixmapKind::PlEdit) else {
        return Ok(false);
    };
    let pledit = surface_from_xpm(pledit)?;
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

    if shaded {
        return Ok(true);
    }

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
