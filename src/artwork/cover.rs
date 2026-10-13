//! Bounded worker-only embedded cover discovery. Initial format support is ID3v2
//! APIC (including files with an ID3 prefix such as MP3) with PNG/BMP/JPEG pixels.
//! Unsupported/corrupt/oversized pictures return errors for generated fallback;
//! no audio stream is decoded, no image/file I/O belongs on the audio/UI thread.

use std::fs::File;
use std::io::{self, Cursor, Read};
use std::path::Path;

#[cfg(test)]
use id3::TagLike;
use image::{ImageFormat, ImageReader, Limits};

pub const MAX_EMBEDDED_TAG_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_EMBEDDED_PICTURE_BYTES: usize = 1024 * 1024;
const MAX_COVER_DIMENSION: u32 = 2048;

/// No picture/unsupported container is Ok(None); invalid supported tags/pictures
/// are Err. Both mean a Generated fallback in Auto mode, without retry storms.
pub fn load_embedded_cover(path: &Path, output_size: u32) -> io::Result<Option<Vec<i32>>> {
    if ![320, 512].contains(&output_size) {
        return Err(invalid("unsupported cover output size"));
    }
    if !std::fs::metadata(path)?.is_file() {
        return Err(invalid("cover source is not a regular file"));
    }
    let mut file = File::open(path)?;
    let mut header = [0u8; 10];
    match file.read_exact(&mut header) {
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err),
        Ok(()) => {}
    }
    if &header[..3] != b"ID3" {
        return Ok(None);
    }
    if ![3, 4].contains(&header[3]) || header[4] != 0 || header[5] != 0 {
        return Err(invalid("unsupported ID3 version or tag flags"));
    }
    if header[6..10].iter().any(|byte| byte & 0x80 != 0) {
        return Err(invalid("invalid ID3 size"));
    }
    let payload = header[6..10]
        .iter()
        .fold(0usize, |value, byte| (value << 7) | usize::from(*byte));
    if payload > MAX_EMBEDDED_TAG_BYTES {
        return Err(invalid("embedded tag exceeds limit"));
    }
    let mut bytes = Vec::with_capacity(payload + 10);
    bytes.extend_from_slice(&header);
    file.take(payload as u64).read_to_end(&mut bytes)?;
    if bytes.len() != payload + 10 {
        return Err(invalid("truncated embedded tag"));
    }
    // Parse only bounded APIC frames. Do not hand arbitrary compressed ID3
    // frames to a tag decoder that could inflate payload beyond our byte cap.
    let mut position = 10usize;
    let mut first = None;
    let mut frames = 0usize;
    while position + 10 <= bytes.len() {
        let frame = &bytes[position..position + 10];
        if frame[..4] == [0; 4] {
            break;
        }
        frames += 1;
        if frames > 4096 || frame[8] != 0 || frame[9] != 0 {
            return Err(invalid("unsupported or excessive ID3 frames"));
        }
        let size = if header[3] == 4 {
            if frame[4..8].iter().any(|byte| byte & 128 != 0) {
                return Err(invalid("invalid frame size"));
            }
            frame[4..8]
                .iter()
                .fold(0usize, |value, byte| (value << 7) | usize::from(*byte))
        } else {
            u32::from_be_bytes(
                frame[4..8]
                    .try_into()
                    .map_err(|_| invalid("invalid frame size"))?,
            ) as usize
        };
        position += 10;
        let end = position
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| invalid("truncated ID3 frame"))?;
        if &frame[..4] == b"APIC" {
            let (front, picture) = apic_picture(&bytes[position..end])?;
            if front {
                return decode_cover(picture, output_size).map(Some);
            }
            if first.is_none() {
                first = Some(picture);
            }
        }
        position = end;
    }
    first
        .map(|picture| decode_cover(picture, output_size))
        .transpose()
}

fn apic_picture(frame: &[u8]) -> io::Result<(bool, &[u8])> {
    let encoding = *frame
        .first()
        .ok_or_else(|| invalid("missing picture encoding"))?;
    let mime_end = frame[1..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|n| n + 1)
        .ok_or_else(|| invalid("missing picture MIME"))?;
    let kind = *frame
        .get(mime_end + 1)
        .ok_or_else(|| invalid("missing picture type"))?;
    let description = frame
        .get(mime_end + 2..)
        .ok_or_else(|| invalid("missing picture description"))?;
    let consumed = match encoding {
        0 | 3 => description
            .iter()
            .position(|byte| *byte == 0)
            .map(|n| n + 1),
        1 | 2 => description
            .chunks_exact(2)
            .position(|pair| pair == [0, 0])
            .map(|n| n * 2 + 2),
        _ => None,
    }
    .ok_or_else(|| invalid("unsupported picture description"))?;
    let picture = &description[consumed..];
    if picture.len() > MAX_EMBEDDED_PICTURE_BYTES {
        return Err(invalid("embedded picture exceeds limit"));
    }
    Ok((kind == 3, picture))
}

fn invalid(detail: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail)
}

pub fn decode_cover(bytes: &[u8], output_size: u32) -> io::Result<Vec<i32>> {
    if bytes.len() > MAX_EMBEDDED_PICTURE_BYTES {
        return Err(invalid("embedded picture exceeds limit"));
    }
    if ![320, 512].contains(&output_size) {
        return Err(invalid("unsupported cover output size"));
    }
    let format = image::guess_format(bytes).map_err(|_| invalid("invalid cover image"))?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Bmp | ImageFormat::Jpeg
    ) {
        return Err(invalid("unsupported embedded picture format"));
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_COVER_DIMENSION);
    limits.max_image_height = Some(MAX_COVER_DIMENSION);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| invalid("cover decode failed or exceeds limits"))?;
    let rgba = decoded
        .resize_to_fill(
            output_size,
            output_size,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    // Flatten transparency on black, never recolor an embedded cover to the skin.
    Ok(rgba
        .pixels()
        .map(|p| {
            let [r, g, b, a] = p.0;
            let flatten = |channel: u8| ((u32::from(channel) * u32::from(a) + 127) / 255) as u8;
            i32::from_be_bytes([255, flatten(r), flatten(g), flatten(b)])
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn image_bytes(color: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(3, 2, image::Rgba(color));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn decoder_preserves_cover_colors_and_flattens_alpha() {
        assert!(decode_cover(&image_bytes([200, 40, 10, 255]), 320)
            .unwrap()
            .iter()
            .all(|p| *p == i32::from_be_bytes([255, 200, 40, 10])));
        assert!(decode_cover(&image_bytes([200, 40, 10, 128]), 320)
            .unwrap()
            .iter()
            .all(|p| *p == i32::from_be_bytes([255, 100, 20, 5])));
    }

    #[test]
    fn bounded_decoder_rejects_corrupt_unsupported_and_oversized_input() {
        for bytes in [
            b"corrupt".to_vec(),
            vec![0; MAX_EMBEDDED_PICTURE_BYTES + 1],
            b"GIF89a".to_vec(),
        ] {
            assert!(decode_cover(&bytes, 320).is_err());
        }
        assert!(decode_cover(&image_bytes([0; 4]), u32::MAX).is_err());
        let image = image::RgbaImage::from_pixel(2049, 1, image::Rgba([0; 4]));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        assert!(decode_cover(&bytes.into_inner(), 320).is_err());
    }

    #[test]
    fn jpeg_and_bmp_are_supported_without_palette_recoloring() {
        for format in [ImageFormat::Jpeg, ImageFormat::Bmp] {
            let image = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 40, 10]));
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, format).unwrap();
            let pixels = decode_cover(&bytes.into_inner(), 512).unwrap();
            assert_eq!(pixels.len(), 512 * 512);
            for pixel in pixels {
                let [alpha, r, g, b] = pixel.to_be_bytes();
                assert_eq!(alpha, 255);
                assert!(r.abs_diff(200) <= 3 && g.abs_diff(40) <= 3 && b.abs_diff(10) <= 3);
            }
        }
    }

    #[test]
    fn apic_description_and_frame_bomb_inputs_are_bounded() {
        assert!(apic_picture(b"\0image/png\0\x03\0").is_ok());
        assert!(apic_picture(b"\x01image/png\0\x03\xff\xfe\0\0pixels").is_ok());
        for frame in [
            b"".as_slice(),
            b"\0missing-mime",
            b"\0image/png\0",
            b"\xffimage/png\0\x03\0",
        ] {
            assert!(apic_picture(frame).is_err());
        }
        let path = std::env::temp_dir().join(format!(
            "xmms-cover-bounds-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for header in [
            b"ID3\x02\0\0\0\0\0\0",
            b"ID3\x04\0\x80\0\0\0\0",
            b"ID3\x04\x01\0\0\0\0\0",
        ] {
            std::fs::write(&path, header).unwrap();
            assert!(load_embedded_cover(&path, 320).is_err());
        }
        // Compressed frame payload is never inflated by a generic tag decoder.
        std::fs::write(&path, b"ID3\x03\0\0\0\0\0\x0aAPIC\0\0\0\0\0\x80").unwrap();
        assert!(load_embedded_cover(&path, 320).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn apic_extraction_has_bounded_tag_and_generated_fallback_cases() {
        let path = std::env::temp_dir().join(format!(
            "xmms-cover-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut tag = id3::Tag::new();
        tag.add_frame(id3::frame::Picture {
            mime_type: "image/png".into(),
            picture_type: id3::frame::PictureType::CoverFront,
            description: String::new(),
            data: image_bytes([4, 8, 12, 255]),
        });
        File::create(&path).unwrap();
        tag.write_to_path(&path, id3::Version::Id3v24).unwrap();
        assert!(load_embedded_cover(&path, 320)
            .unwrap()
            .unwrap()
            .iter()
            .all(|p| *p == i32::from_be_bytes([255, 4, 8, 12])));
        std::fs::write(&path, b"RIFF-not-id3").unwrap();
        assert!(load_embedded_cover(&path, 320).unwrap().is_none());
        std::fs::write(&path, b"ID3\x04\0\0\x7f\x7f\x7f\x7f").unwrap();
        assert!(load_embedded_cover(&path, 320).is_err());
        std::fs::write(&path, b"ID3\x04\0\0\0\0\0\x30").unwrap();
        assert!(load_embedded_cover(&path, 320).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
