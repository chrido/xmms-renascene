use std::fmt;

use cairo::{Context, Extend, Filter, Format, ImageSurface, Rectangle};

use crate::skin::layout::SpriteSpec;
use crate::skin::widget::TextBox;
use crate::skin::xpm::XpmImage;
use crate::skin::{DefaultSkin, SkinPixmapKind};

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

/// Render skin content at its native (1x) resolution into an offscreen buffer,
/// then scale that single image onto `cr` with nearest-neighbour filtering.
///
/// Skins are drawn as many adjacent slices. Scaling the cairo context and then
/// blitting each slice separately makes every slice round its own edges and
/// reset its own sampling phase, so at fractional scale factors adjacent slices
/// no longer reconstruct one continuous image: thin seams and texture
/// discontinuities appear where slices meet. Compositing every slice at integer
/// 1x coordinates first (pixel-perfect, seam-free) and scaling the finished
/// image exactly once removes those artifacts at every scale factor.
pub fn render_scaled<F>(
    cr: &Context,
    device_width: i32,
    device_height: i32,
    base_width: i32,
    base_height: i32,
    draw: F,
) -> Result<(), RenderError>
where
    F: FnOnce(&Context) -> Result<(), RenderError>,
{
    if device_width <= 0 || device_height <= 0 || base_width <= 0 || base_height <= 0 {
        return Ok(());
    }

    let base = ImageSurface::create(Format::ARgb32, base_width, base_height)?;
    {
        let base_cr = Context::new(&base)?;
        draw(&base_cr)?;
    }
    base.flush();

    cr.save()?;
    cr.scale(
        f64::from(device_width) / f64::from(base_width),
        f64::from(device_height) / f64::from(base_height),
    );
    cr.set_source_surface(&base, 0.0, 0.0)?;
    let pattern = cr.source();
    pattern.set_extend(Extend::Pad);
    pattern.set_filter(Filter::Nearest);
    cr.paint()?;
    cr.restore()?;

    Ok(())
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

pub(super) fn render_sprite_spec(
    cr: &Context,
    skin: &DefaultSkin,
    spec: SpriteSpec,
) -> Result<bool, RenderError> {
    blit_skin_rect(
        cr,
        skin,
        spec.kind,
        spec.source.x,
        spec.source.y,
        spec.dest.x,
        spec.dest.y,
        spec.source.width,
        spec.source.height,
    )
}

pub(super) fn render_surface_sprite_spec(
    cr: &Context,
    source: &ImageSurface,
    spec: SpriteSpec,
) -> Result<bool, RenderError> {
    blit_surface_rect(
        cr,
        source,
        spec.source.x,
        spec.source.y,
        spec.dest.x,
        spec.dest.y,
        spec.source.width,
        spec.source.height,
    )
}

pub(super) struct SliderRenderSpec {
    pub(super) kind: SkinPixmapKind,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) position: i32,
    pub(super) knob_source_x: i32,
    pub(super) knob_source_y: i32,
    pub(super) knob_width: i32,
    pub(super) knob_height: i32,
    pub(super) frame_height: i32,
    pub(super) frame_offset: i32,
    pub(super) frame: i32,
}

pub(super) fn render_horizontal_slider(
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
        spec.y + ((spec.height - spec.knob_height) / 2),
        spec.knob_width,
        spec.knob_height,
    )?;
    Ok(rendered)
}

pub(super) fn blit_skin_rect(
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

pub(super) fn render_text(
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

pub(super) fn set_rgb(cr: &Context, color: [u8; 3]) {
    cr.set_source_rgb(
        f64::from(color[0]) / 255.0,
        f64::from(color[1]) / 255.0,
        f64::from(color[2]) / 255.0,
    );
}
