use std::fs::{self, File};
use std::io::{self, Cursor, Write};
use std::path::Path;

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb, Rgba};
use zip::write::SimpleFileOptions;

use super::xpm::XpmImage;
use super::{DefaultSkin, PlaylistColors, RegionMasks, SkinMask, SkinPixmapKind, TextColors};

const TRANSPARENCY_KEY: [u8; 3] = [48, 255, 50];

impl DefaultSkin {
    pub fn get_mut(&mut self, kind: SkinPixmapKind) -> Option<&mut XpmImage> {
        self.pixmaps.get_mut(&kind)
    }

    pub fn set_vis_color(&mut self, index: usize, rgb: [u8; 3]) -> bool {
        let Some(color) = self.vis_colors.get_mut(index) else {
            return false;
        };
        if *color == rgb {
            return false;
        }
        *color = rgb;
        true
    }

    pub fn set_playlist_colors(&mut self, colors: PlaylistColors) -> bool {
        if self.playlist_colors == colors {
            return false;
        }
        self.playlist_colors = colors;
        true
    }

    pub fn set_text_colors(&mut self, colors: TextColors) -> bool {
        if self.text_colors == colors {
            return false;
        }
        recolor_text_pixmap(
            self.pixmaps.get_mut(&SkinPixmapKind::Text),
            self.text_colors,
            colors,
        );
        self.text_colors = colors;
        true
    }

    pub fn save_to_dir(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        for kind in SkinPixmapKind::ALL {
            let Some(image) = self.get(kind) else {
                continue;
            };
            let path = dir.join(format!("{}.png", kind.info().file_stem));
            fs::write(path, encode_pixmap_png(image)?)?;
        }
        fs::write(
            dir.join("viscolor.txt"),
            encode_vis_colors(self.vis_colors())?,
        )?;
        fs::write(
            dir.join("pledit.txt"),
            encode_playlist_colors(self.playlist_colors())?,
        )?;
        if let Some(region) = encode_region_masks(self.region_masks()) {
            fs::write(dir.join("region.txt"), region)?;
        }
        Ok(())
    }

    pub fn export_wsz(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for kind in SkinPixmapKind::ALL {
            let Some(image) = self.get(kind) else {
                continue;
            };
            let bytes = encode_pixmap_bmp(image)?;
            let name = format!("{}.bmp", kind.info().file_stem);
            zip.start_file(name, options)?;
            zip.write_all(&bytes)?;
            if kind == SkinPixmapKind::Numbers {
                zip.start_file("numbers.bmp", options)?;
                zip.write_all(&bytes)?;
            }
        }

        zip.start_file("viscolor.txt", options)?;
        zip.write_all(encode_vis_colors(self.vis_colors())?.as_bytes())?;
        zip.start_file("pledit.txt", options)?;
        zip.write_all(encode_playlist_colors(self.playlist_colors())?.as_bytes())?;
        if let Some(region) = encode_region_masks(self.region_masks()) {
            zip.start_file("region.txt", options)?;
            zip.write_all(region.as_bytes())?;
        }

        zip.finish()?;
        Ok(())
    }
}

pub fn encode_pixmap_png(image: &XpmImage) -> io::Result<Vec<u8>> {
    let mut buffer =
        ImageBuffer::<Rgba<u8>, Vec<u8>>::new(image.width() as u32, image.height() as u32);
    for y in 0..image.height() {
        for x in 0..image.width() {
            buffer.put_pixel(
                x as u32,
                y as u32,
                Rgba(unpremultiply_argb(image.pixel_argb(x, y).unwrap_or(0))),
            );
        }
    }
    write_dynamic_image(DynamicImage::ImageRgba8(buffer), ImageFormat::Png)
}

pub fn encode_pixmap_bmp(image: &XpmImage) -> io::Result<Vec<u8>> {
    let mut buffer =
        ImageBuffer::<Rgb<u8>, Vec<u8>>::new(image.width() as u32, image.height() as u32);
    for y in 0..image.height() {
        for x in 0..image.width() {
            let [r, g, b, a] = unpremultiply_argb(image.pixel_argb(x, y).unwrap_or(0));
            let rgb = if a == 0 { TRANSPARENCY_KEY } else { [r, g, b] };
            buffer.put_pixel(x as u32, y as u32, Rgb(rgb));
        }
    }
    write_dynamic_image(DynamicImage::ImageRgb8(buffer), ImageFormat::Bmp)
}

fn write_dynamic_image(image: DynamicImage, format: ImageFormat) -> io::Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, format)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(bytes.into_inner())
}

fn unpremultiply_argb(argb: u32) -> [u8; 4] {
    let a = ((argb >> 24) & 0xff) as u8;
    let r = ((argb >> 16) & 0xff) as u8;
    let g = ((argb >> 8) & 0xff) as u8;
    let b = (argb & 0xff) as u8;
    if a == 0 || a == 255 {
        return [r, g, b, a];
    }

    [
        ((u32::from(r) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
        ((u32::from(g) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
        ((u32::from(b) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
        a,
    ]
}

fn recolor_text_pixmap(image: Option<&mut XpmImage>, old: TextColors, new: TextColors) {
    let Some(image) = image else {
        return;
    };
    for y in 0..image.height() {
        let color_index = y % 6;
        for x in 0..image.width() {
            let rgba = unpremultiply_argb(image.pixel_argb(x, y).unwrap_or(0));
            if rgba[3] == 0 {
                continue;
            }
            let rgb = [rgba[0], rgba[1], rgba[2]];
            let replacement = if rgb == old.background[color_index] || x == 151 {
                Some(new.background[color_index])
            } else if rgb == old.foreground[color_index] {
                Some(new.foreground[color_index])
            } else {
                None
            };
            if let Some([r, g, b]) = replacement {
                image.set_pixel_rgba(x, y, [r, g, b, rgba[3]]);
            }
        }
    }
}

fn encode_vis_colors(colors: &[[u8; 3]; 24]) -> io::Result<String> {
    let mut contents = String::new();
    for color in colors {
        contents.push_str(&format!("{},{},{}\n", color[0], color[1], color[2]));
    }
    Ok(contents)
}

fn encode_playlist_colors(colors: PlaylistColors) -> io::Result<String> {
    Ok(format!(
        "[Text]\nNormal=#{}\nCurrent=#{}\nNormalBG=#{}\nSelectedBG=#{}\n",
        hex_rgb(colors.normal),
        hex_rgb(colors.current),
        hex_rgb(colors.normal_bg),
        hex_rgb(colors.selected_bg)
    ))
}

fn hex_rgb(color: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

fn encode_region_masks(masks: &RegionMasks) -> Option<String> {
    let mut contents = String::new();
    append_region_mask(&mut contents, "Normal", masks.normal.as_ref());
    append_region_mask(&mut contents, "WindowShade", masks.window_shade.as_ref());
    append_region_mask(&mut contents, "Equalizer", masks.equalizer.as_ref());
    append_region_mask(&mut contents, "EqualizerWS", masks.equalizer_ws.as_ref());
    (!contents.is_empty()).then_some(contents)
}

fn append_region_mask(contents: &mut String, section: &str, mask: Option<&SkinMask>) {
    let Some(mask) = mask else {
        return;
    };
    contents.push_str(&format!("[{section}]\n"));
    contents.push_str("NumPoints=");
    for (index, polygon) in mask.polygons().iter().enumerate() {
        if index > 0 {
            contents.push(',');
        }
        contents.push_str(&polygon.len().to_string());
    }
    contents.push('\n');
    contents.push_str("PointList=");
    let mut first = true;
    for polygon in mask.polygons() {
        for point in polygon {
            if !first {
                contents.push(',');
            }
            first = false;
            contents.push_str(&format!("{},{}", point[0], point[1]));
        }
    }
    contents.push_str("\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "xmms-rs-{name}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    fn edited_skin() -> DefaultSkin {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        skin.get_mut(SkinPixmapKind::Main)
            .unwrap()
            .set_pixel_rgba(0, 0, [10, 20, 30, 255]);
        skin.get_mut(SkinPixmapKind::Main)
            .unwrap()
            .set_pixel_rgba(1, 0, [255, 255, 255, 0]);
        skin.set_vis_color(0, [1, 2, 3]);
        skin.set_playlist_colors(PlaylistColors {
            normal: [4, 5, 6],
            current: [7, 8, 9],
            normal_bg: [10, 11, 12],
            selected_bg: [13, 14, 15],
        });
        skin
    }

    #[test]
    fn save_to_dir_round_trips_losslessly() {
        let dir = unique_temp_path("skin-save-dir");
        let _ = fs::remove_dir_all(&dir);
        let skin = edited_skin();

        skin.save_to_dir(&dir).unwrap();
        let loaded = DefaultSkin::load_from_dir(&dir).unwrap();

        assert_eq!(
            loaded.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff0a_141e)
        );
        assert_eq!(
            loaded.get(SkinPixmapKind::Main).unwrap().pixel_argb(1, 0),
            Some(0)
        );
        assert_eq!(loaded.vis_colors()[0], [1, 2, 3]);
        assert_eq!(loaded.playlist_colors(), skin.playlist_colors());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_wsz_round_trips_through_archive_loader() {
        let mut path = unique_temp_path("skin-export");
        path.set_extension("wsz");
        let _ = fs::remove_file(&path);
        let skin = edited_skin();

        skin.export_wsz(&path).unwrap();
        let loaded = DefaultSkin::load_from_path(&path).unwrap();

        assert_eq!(
            loaded.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff0a_141e)
        );
        assert_eq!(
            loaded.get(SkinPixmapKind::Main).unwrap().pixel_argb(1, 0),
            Some(0)
        );
        assert_eq!(loaded.vis_colors()[0], [1, 2, 3]);
        assert_eq!(loaded.playlist_colors(), skin.playlist_colors());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn set_text_colors_updates_text_pixmap_for_persistence() {
        let mut skin = DefaultSkin::load_bundled().unwrap();
        let mut colors = skin.text_colors();
        colors.background[0] = [70, 71, 72];

        assert!(skin.set_text_colors(colors));

        assert_eq!(
            skin.get(SkinPixmapKind::Text).unwrap().pixel_argb(151, 0),
            Some(0xff46_4748)
        );
    }
}
