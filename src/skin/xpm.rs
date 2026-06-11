use std::fmt;
use std::io::{BufReader, Cursor};

use image::ImageDecoder;

use super::layout::SkinRect;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XpmImage {
    width: usize,
    height: usize,
    argb: Vec<u32>,
}

impl XpmImage {
    pub fn from_argb_pixels(width: usize, height: usize, argb: Vec<u32>) -> Result<Self, XpmError> {
        if width == 0 || height == 0 || argb.len() != width * height {
            return Err(XpmError::InvalidPixels {
                width,
                height,
                len: argb.len(),
            });
        }
        Ok(Self {
            width,
            height,
            argb,
        })
    }

    pub fn parse(contents: &str) -> Result<Self, XpmError> {
        parse_with_library(contents)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels_argb(&self) -> &[u32] {
        &self.argb
    }

    pub fn set_pixel_rgba(&mut self, x: usize, y: usize, rgba: [u8; 4]) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let offset = y * self.width + x;
        let pixel = premultiply_rgba(rgba);
        if self.argb[offset] == pixel {
            return false;
        }
        self.argb[offset] = pixel;
        true
    }

    pub fn fill_rect_rgba(&mut self, rect: SkinRect, rgba: [u8; 4]) -> bool {
        if rect.width <= 0 || rect.height <= 0 {
            return false;
        }

        let start_x = rect.x.max(0) as usize;
        let start_y = rect.y.max(0) as usize;
        let end_x = i64::from(rect.x)
            .saturating_add(i64::from(rect.width))
            .clamp(0, self.width as i64) as usize;
        let end_y = i64::from(rect.y)
            .saturating_add(i64::from(rect.height))
            .clamp(0, self.height as i64) as usize;

        if start_x >= end_x || start_y >= end_y {
            return false;
        }

        let pixel = premultiply_rgba(rgba);
        let mut changed = false;
        for y in start_y..end_y {
            let row_start = y * self.width;
            for x in start_x..end_x {
                let offset = row_start + x;
                if self.argb[offset] != pixel {
                    self.argb[offset] = pixel;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn pixel_argb(&self, x: usize, y: usize) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        self.argb.get(y * self.width + x).copied()
    }
}

fn parse_with_library(contents: &str) -> Result<XpmImage, XpmError> {
    let reader = BufReader::new(Cursor::new(contents.trim_start().as_bytes()));
    let decoder = image_extras::xpm::XpmDecoder::new(reader)
        .map_err(|err| XpmError::Decode(err.to_string()))?;
    let (width, height) = decoder.dimensions();
    let mut rgba16 = vec![0; decoder.total_bytes() as usize];
    decoder
        .read_image(&mut rgba16)
        .map_err(|err| XpmError::Decode(err.to_string()))?;

    let mut argb = Vec::with_capacity((width as usize) * (height as usize));
    for pixel in rgba16.chunks_exact(8) {
        let r = (u16::from_ne_bytes([pixel[0], pixel[1]]) / 257) as u8;
        let g = (u16::from_ne_bytes([pixel[2], pixel[3]]) / 257) as u8;
        let b = (u16::from_ne_bytes([pixel[4], pixel[5]]) / 257) as u8;
        let a = (u16::from_ne_bytes([pixel[6], pixel[7]]) / 257) as u8;
        argb.push(premultiply_rgba([r, g, b, a]));
    }

    XpmImage::from_argb_pixels(width as usize, height as usize, argb)
}

fn premultiply_rgba([r, g, b, a]: [u8; 4]) -> u32 {
    let pr = ((u16::from(r) * u16::from(a) + 127) / 255) as u32;
    let pg = ((u16::from(g) * u16::from(a) + 127) / 255) as u32;
    let pb = ((u16::from(b) * u16::from(a) + 127) / 255) as u32;
    (u32::from(a) << 24) | (pr << 16) | (pg << 8) | pb
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XpmError {
    InvalidPixels {
        width: usize,
        height: usize,
        len: usize,
    },
    Decode(String),
}

impl fmt::Display for XpmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XpmError::InvalidPixels { width, height, len } => {
                write!(
                    f,
                    "invalid ARGB pixel buffer for {width}x{height}: {len} pixels"
                )
            }
            XpmError::Decode(err) => write!(f, "XPM decoder failed: {err}"),
        }
    }
}

impl std::error::Error for XpmError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_decoder_parses_canonical_xpm() {
        let image = parse_with_library(
            r#"/* XPM */
            static char *x[] = {
            "1 1 1 1",
            ". c #010203",
            "."};
            "#,
        )
        .unwrap();

        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(image.pixel_argb(0, 0), Some(0xff01_0203));
    }

    #[test]
    fn parses_basic_xpm_with_library_decoder() {
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "2 2 2 1",
            "a c None",
            "b c #0f0",
            "ab",
            "ba"
            };"#,
        )
        .unwrap();

        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        assert_eq!(image.pixel_argb(0, 0), Some(0));
        assert_eq!(image.pixel_argb(1, 0), Some(0xff00_ff00));
        assert_eq!(image.pixel_argb(2, 0), None);
    }

    #[test]
    fn set_pixel_rgba_stores_premultiplied_argb() {
        let mut image = XpmImage::from_argb_pixels(2, 2, vec![0; 4]).unwrap();

        assert!(image.set_pixel_rgba(1, 0, [128, 64, 32, 128]));

        assert_eq!(image.pixel_argb(1, 0), Some(0x8040_2010));
        assert_eq!(image.pixel_argb(0, 0), Some(0));
    }

    #[test]
    fn set_pixel_rgba_noops_when_out_of_bounds_or_unchanged() {
        let mut image = XpmImage::from_argb_pixels(1, 1, vec![0xff01_0203]).unwrap();

        assert!(!image.set_pixel_rgba(1, 0, [255, 0, 0, 255]));
        assert_eq!(image.pixel_argb(0, 0), Some(0xff01_0203));
        assert!(!image.set_pixel_rgba(0, 0, [1, 2, 3, 255]));
    }

    #[test]
    fn set_pixel_rgba_with_zero_alpha_stores_fully_transparent() {
        let mut image = XpmImage::from_argb_pixels(1, 1, vec![0xffff_ffff]).unwrap();

        assert!(image.set_pixel_rgba(0, 0, [255, 255, 255, 0]));

        assert_eq!(image.pixel_argb(0, 0), Some(0));
    }

    #[test]
    fn fill_rect_rgba_clamps_to_image_bounds() {
        let mut image = XpmImage::from_argb_pixels(3, 3, vec![0; 9]).unwrap();

        assert!(image.fill_rect_rgba(SkinRect::new(-1, 1, 3, 3), [10, 20, 30, 255]));

        assert_eq!(
            image.pixels_argb(),
            &[
                0,
                0,
                0,
                0xff0a_141e,
                0xff0a_141e,
                0,
                0xff0a_141e,
                0xff0a_141e,
                0,
            ]
        );
    }

    #[test]
    fn fill_rect_rgba_noops_for_empty_or_out_of_bounds_rects() {
        let mut image = XpmImage::from_argb_pixels(2, 2, vec![0; 4]).unwrap();

        assert!(!image.fill_rect_rgba(SkinRect::new(0, 0, 0, 2), [1, 2, 3, 255]));
        assert!(!image.fill_rect_rgba(SkinRect::new(-4, 0, 2, 2), [1, 2, 3, 255]));
        assert!(!image.fill_rect_rgba(SkinRect::new(0, 4, 2, 2), [1, 2, 3, 255]));
        assert_eq!(image.pixels_argb(), &[0, 0, 0, 0]);
    }
}
