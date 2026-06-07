use std::fmt;
use std::io::{BufReader, Cursor};

use image::ImageDecoder;

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
        let pr = ((u16::from(r) * u16::from(a) + 127) / 255) as u32;
        let pg = ((u16::from(g) * u16::from(a) + 127) / 255) as u32;
        let pb = ((u16::from(b) * u16::from(a) + 127) / 255) as u32;
        argb.push((u32::from(a) << 24) | (pr << 16) | (pg << 8) | pb);
    }

    XpmImage::from_argb_pixels(width as usize, height as usize, argb)
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
}
