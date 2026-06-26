use std::fmt;

use cairo::{Context, Extend, Filter, Format, ImageSurface, Rectangle};

use crate::skin::layout::{SkinRect, SpriteSpec};
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
    mut source_rect: SkinRect,
    mut dest: (i32, i32),
) -> Result<bool, RenderError> {
    if source_rect.x < 0 {
        dest.0 -= source_rect.x;
        source_rect.width += source_rect.x;
        source_rect.x = 0;
    }
    if source_rect.y < 0 {
        dest.1 -= source_rect.y;
        source_rect.height += source_rect.y;
        source_rect.y = 0;
    }

    let surface_width = source.width();
    let surface_height = source.height();
    let Some(source_rect) = source_rect.clamp_to(surface_width, surface_height) else {
        return Ok(false);
    };

    let cairo_source_rect = source.create_for_rectangle(Rectangle::new(
        source_rect.x as f64,
        source_rect.y as f64,
        source_rect.width as f64,
        source_rect.height as f64,
    ))?;

    cr.save()?;
    cr.rectangle(
        dest.0 as f64,
        dest.1 as f64,
        source_rect.width as f64,
        source_rect.height as f64,
    );
    cr.clip();
    cr.set_source_surface(&cairo_source_rect, dest.0 as f64, dest.1 as f64)?;
    let pattern = cr.source();
    pattern.set_extend(Extend::Pad);
    pattern.set_filter(Filter::Nearest);
    cr.paint()?;
    cr.restore()?;

    Ok(true)
}

/// Identifies which kind of content a render pass should emit.
///
/// Skin bitmaps must be composited at native 1x and scaled once to stay
/// seamless, but vector text must be rasterised at the final device resolution
/// or it looks blurry. `render_scaled` therefore runs the render closure twice:
/// once for [`RenderPass::Bitmap`] into the offscreen 1x buffer, and once for
/// [`RenderPass::Text`] directly on the (scaled) device context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPass {
    Bitmap,
    Text,
}

impl RenderPass {
    pub fn is_bitmap(self) -> bool {
        matches!(self, RenderPass::Bitmap)
    }

    pub fn is_text(self) -> bool {
        matches!(self, RenderPass::Text)
    }
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
///
/// `draw` is invoked twice: with [`RenderPass::Bitmap`] while rendering the
/// offscreen buffer, and with [`RenderPass::Text`] on the scaled device context
/// so vector text is rasterised crisply at the real device resolution instead
/// of being scaled up as a bitmap.
pub fn render_scaled<F>(
    cr: &Context,
    device_width: i32,
    device_height: i32,
    base_width: i32,
    base_height: i32,
    draw: F,
) -> Result<(), RenderError>
where
    F: Fn(&Context, RenderPass) -> Result<(), RenderError>,
{
    if device_width <= 0 || device_height <= 0 || base_width <= 0 || base_height <= 0 {
        return Ok(());
    }

    let base = ImageSurface::create(Format::ARgb32, base_width, base_height)?;
    {
        let base_cr = Context::new(&base)?;
        draw(&base_cr, RenderPass::Bitmap)?;
    }
    base.flush();

    cr.save()?;
    cr.scale(
        f64::from(device_width) / f64::from(base_width),
        f64::from(device_height) / f64::from(base_height),
    );
    cr.set_source_surface(&base, 0.0, 0.0)?;
    {
        let pattern = cr.source();
        pattern.set_extend(Extend::Pad);
        pattern.set_filter(Filter::Nearest);
    }
    cr.paint()?;
    // The CTM is still the base->device scale, so text drawn here uses base
    // coordinates but is rasterised at the device resolution.
    draw(cr, RenderPass::Text)?;
    cr.restore()?;

    Ok(())
}

/// Composite `draw` into a `base_width` x `base_height` offscreen buffer at 1x,
/// then paint it at base coordinates `(base_x, base_y)` under `cr`'s current
/// transform with nearest-neighbour filtering.
///
/// This keeps a self-contained bitmap element (such as a popup menu) seam-free
/// while letting the caller place it above content that was drawn at device
/// resolution, e.g. on top of the crisp text emitted during a text pass.
pub fn paint_scaled<F>(
    cr: &Context,
    base_x: i32,
    base_y: i32,
    base_width: i32,
    base_height: i32,
    draw: F,
) -> Result<(), RenderError>
where
    F: FnOnce(&Context) -> Result<(), RenderError>,
{
    if base_width <= 0 || base_height <= 0 {
        return Ok(());
    }

    let surface = ImageSurface::create(Format::ARgb32, base_width, base_height)?;
    {
        let surface_cr = Context::new(&surface)?;
        draw(&surface_cr)?;
    }
    surface.flush();

    cr.save()?;
    cr.translate(f64::from(base_x), f64::from(base_y));
    cr.set_source_surface(&surface, 0.0, 0.0)?;
    {
        let pattern = cr.source();
        pattern.set_extend(Extend::Pad);
        pattern.set_filter(Filter::Nearest);
    }
    cr.rectangle(0.0, 0.0, f64::from(base_width), f64::from(base_height));
    cr.clip();
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
    blit_skin_rect(cr, skin, spec.kind, spec.source, (spec.dest.x, spec.dest.y))
}

pub(super) fn render_surface_sprite_spec(
    cr: &Context,
    source: &ImageSurface,
    spec: SpriteSpec,
) -> Result<bool, RenderError> {
    blit_surface_rect(cr, source, spec.source, (spec.dest.x, spec.dest.y))
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
        SkinRect::new(
            spec.frame_offset,
            spec.frame * spec.frame_height,
            spec.width,
            spec.height,
        ),
        (spec.x, spec.y),
    )?;
    rendered |= blit_skin_rect(
        cr,
        skin,
        spec.kind,
        SkinRect::new(
            spec.knob_source_x,
            spec.knob_source_y,
            spec.knob_width,
            spec.knob_height,
        ),
        (
            spec.x + spec.position,
            spec.y + ((spec.height - spec.knob_height) / 2),
        ),
    )?;
    Ok(rendered)
}

pub(super) fn blit_skin_rect(
    cr: &Context,
    skin: &DefaultSkin,
    kind: SkinPixmapKind,
    source_rect: SkinRect,
    dest: (i32, i32),
) -> Result<bool, RenderError> {
    let Some(image) = skin.get(kind) else {
        return Ok(false);
    };
    let surface = surface_from_xpm(image)?;
    blit_surface_rect(cr, &surface, source_rect, dest)
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
            SkinRect::new(sx, sy, TextBox::CHAR_WIDTH, TextBox::CHAR_HEIGHT),
            (dx, ydest),
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
