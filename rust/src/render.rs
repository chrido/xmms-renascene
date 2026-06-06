use std::fmt;

use cairo::{Context, Extend, Filter, Format, ImageSurface, Rectangle};

use crate::skin::xpm::XpmImage;
use crate::skin::{DefaultSkin, SkinPixmapKind};

pub const MAIN_WINDOW_WIDTH: i32 = 275;
pub const MAIN_WINDOW_HEIGHT: i32 = 116;
pub const MAIN_TITLEBAR_HEIGHT: i32 = 14;

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
}
