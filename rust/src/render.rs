use std::fmt;

use cairo::{Context, Extend, Filter, Format, ImageSurface, Rectangle};

use crate::skin::xpm::XpmImage;

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
}
