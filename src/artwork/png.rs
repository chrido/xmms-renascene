//! Worker-only opaque 512px PNG encoding using the existing image dependency.
use image::{ExtendedColorType, ImageEncoder};
use std::io::{self, Write};

pub const MAX_PNG_BYTES: usize = 1024 * 1024;

pub fn encode_uri_png(pixels: Vec<i32>) -> io::Result<Vec<u8>> {
    if pixels.len() != 512 * 512 || pixels.iter().any(|pixel| (*pixel as u32 >> 24) != 255) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid opaque artwork pixels",
        ));
    }
    // Consume ARGB storage before codec work; no high-resolution Java Bitmap.
    let mut rgb = Vec::with_capacity(512 * 512 * 3);
    for pixel in pixels {
        let [_, r, g, b] = pixel.to_be_bytes();
        rgb.extend_from_slice(&[r, g, b]);
    }
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_PNG_BYTES.saturating_sub(self.0.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "artwork PNG exceeds limit",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // Avoid capacity doubling beyond the explicit encoded-output reservation.
    let mut output = Bounded(Vec::with_capacity(MAX_PNG_BYTES));
    image::codecs::png::PngEncoder::new(&mut output)
        .write_image(&rgb, 512, 512, ExtendedColorType::Rgb8)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(output.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_preserves_exact_opaque_pixels_and_dimensions() {
        let word = i32::from_be_bytes([255, 200, 40, 10]);
        let encoded = encode_uri_png(vec![word; 512 * 512]).unwrap();
        assert!(encoded.len() <= MAX_PNG_BYTES);
        let decoded = image::load_from_memory(&encoded).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (512, 512));
        assert!(decoded.pixels().all(|p| p.0 == [200, 40, 10, 255]));
    }
    #[test]
    fn rejects_wrong_size_and_transparent_pixels_before_encoding() {
        assert!(encode_uri_png(vec![]).is_err());
        assert!(encode_uri_png(vec![0; 512 * 512]).is_err());
    }
}
