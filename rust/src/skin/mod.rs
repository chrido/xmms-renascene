pub mod widget;
pub mod xpm;

use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use image::GenericImageView;
use xpm::XpmImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkinPixmapKind {
    Main,
    CButtons,
    Titlebar,
    ShufRep,
    Text,
    Volume,
    Balance,
    MonoStereo,
    PlayPause,
    Numbers,
    PosBar,
    PlEdit,
    EqMain,
    EqEx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinPixmapInfo {
    pub file_stem: &'static str,
    pub width: usize,
    pub height: usize,
}

impl SkinPixmapKind {
    pub const ALL: [SkinPixmapKind; 14] = [
        SkinPixmapKind::Main,
        SkinPixmapKind::CButtons,
        SkinPixmapKind::Titlebar,
        SkinPixmapKind::ShufRep,
        SkinPixmapKind::Text,
        SkinPixmapKind::Volume,
        SkinPixmapKind::Balance,
        SkinPixmapKind::MonoStereo,
        SkinPixmapKind::PlayPause,
        SkinPixmapKind::Numbers,
        SkinPixmapKind::PosBar,
        SkinPixmapKind::PlEdit,
        SkinPixmapKind::EqMain,
        SkinPixmapKind::EqEx,
    ];

    pub fn info(self) -> SkinPixmapInfo {
        match self {
            SkinPixmapKind::Main => SkinPixmapInfo {
                file_stem: "main",
                width: 275,
                height: 116,
            },
            SkinPixmapKind::CButtons => SkinPixmapInfo {
                file_stem: "cbuttons",
                width: 136,
                height: 36,
            },
            SkinPixmapKind::Titlebar => SkinPixmapInfo {
                file_stem: "titlebar",
                width: 275,
                height: 116,
            },
            SkinPixmapKind::ShufRep => SkinPixmapInfo {
                file_stem: "shufrep",
                width: 28,
                height: 60,
            },
            SkinPixmapKind::Text => SkinPixmapInfo {
                file_stem: "text",
                width: 155,
                height: 18,
            },
            SkinPixmapKind::Volume => SkinPixmapInfo {
                file_stem: "volume",
                width: 68,
                height: 421,
            },
            SkinPixmapKind::Balance => SkinPixmapInfo {
                file_stem: "balance",
                width: 38,
                height: 421,
            },
            SkinPixmapKind::MonoStereo => SkinPixmapInfo {
                file_stem: "monoster",
                width: 56,
                height: 12,
            },
            SkinPixmapKind::PlayPause => SkinPixmapInfo {
                file_stem: "playpaus",
                width: 11,
                height: 9,
            },
            SkinPixmapKind::Numbers => SkinPixmapInfo {
                file_stem: "nums_ex",
                width: 108,
                height: 13,
            },
            SkinPixmapKind::PosBar => SkinPixmapInfo {
                file_stem: "posbar",
                width: 248,
                height: 10,
            },
            SkinPixmapKind::PlEdit => SkinPixmapInfo {
                file_stem: "pledit",
                width: 150,
                height: 18,
            },
            SkinPixmapKind::EqMain => SkinPixmapInfo {
                file_stem: "eqmain",
                width: 275,
                height: 116,
            },
            SkinPixmapKind::EqEx => SkinPixmapInfo {
                file_stem: "eq_ex",
                width: 275,
                height: 50,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultSkin {
    pixmaps: BTreeMap<SkinPixmapKind, XpmImage>,
    vis_colors: [[u8; 3]; 24],
    playlist_colors: PlaylistColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaylistColors {
    pub normal: [u8; 3],
    pub current: [u8; 3],
    pub normal_bg: [u8; 3],
    pub selected_bg: [u8; 3],
}

pub const DEFAULT_PLAYLIST_COLORS: PlaylistColors = PlaylistColors {
    normal: [0, 255, 0],
    current: [255, 255, 255],
    normal_bg: [0, 0, 0],
    selected_bg: [0, 0, 102],
};

pub const DEFAULT_VIS_COLORS: [[u8; 3]; 24] = [
    [9, 34, 53],
    [10, 18, 26],
    [0, 54, 108],
    [0, 58, 116],
    [0, 62, 124],
    [0, 66, 132],
    [0, 70, 140],
    [0, 74, 148],
    [0, 78, 156],
    [0, 82, 164],
    [0, 86, 172],
    [0, 92, 184],
    [0, 98, 196],
    [0, 104, 208],
    [0, 110, 220],
    [0, 116, 232],
    [0, 122, 244],
    [0, 128, 255],
    [0, 128, 255],
    [0, 104, 208],
    [0, 80, 160],
    [0, 56, 112],
    [0, 32, 64],
    [200, 200, 200],
];

impl DefaultSkin {
    pub fn load_bundled() -> io::Result<Self> {
        const BUNDLED_XPMS: &[(SkinPixmapKind, &str)] = &[
            (
                SkinPixmapKind::Main,
                include_str!("../../../data/defskin/main.xpm"),
            ),
            (
                SkinPixmapKind::CButtons,
                include_str!("../../../data/defskin/cbuttons.xpm"),
            ),
            (
                SkinPixmapKind::Titlebar,
                include_str!("../../../data/defskin/titlebar.xpm"),
            ),
            (
                SkinPixmapKind::ShufRep,
                include_str!("../../../data/defskin/shufrep.xpm"),
            ),
            (
                SkinPixmapKind::Text,
                include_str!("../../../data/defskin/text.xpm"),
            ),
            (
                SkinPixmapKind::Volume,
                include_str!("../../../data/defskin/volume.xpm"),
            ),
            (
                SkinPixmapKind::MonoStereo,
                include_str!("../../../data/defskin/monoster.xpm"),
            ),
            (
                SkinPixmapKind::PlayPause,
                include_str!("../../../data/defskin/playpaus.xpm"),
            ),
            (
                SkinPixmapKind::Numbers,
                include_str!("../../../data/defskin/nums_ex.xpm"),
            ),
            (
                SkinPixmapKind::PosBar,
                include_str!("../../../data/defskin/posbar.xpm"),
            ),
            (
                SkinPixmapKind::PlEdit,
                include_str!("../../../data/defskin/pledit.xpm"),
            ),
            (
                SkinPixmapKind::EqMain,
                include_str!("../../../data/defskin/eqmain.xpm"),
            ),
            (
                SkinPixmapKind::EqEx,
                include_str!("../../../data/defskin/eq_ex.xpm"),
            ),
        ];

        let mut pixmaps = BTreeMap::new();
        for (kind, contents) in BUNDLED_XPMS {
            let image = XpmImage::parse(contents).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("bundled {}.xpm: {err}", kind.info().file_stem),
                )
            })?;
            pixmaps.insert(*kind, image);
        }
        apply_balance_fallback(&mut pixmaps);
        Ok(Self {
            pixmaps,
            vis_colors: DEFAULT_VIS_COLORS,
            playlist_colors: DEFAULT_PLAYLIST_COLORS,
        })
    }

    pub fn load_from_dir(dir: &Path) -> io::Result<Self> {
        let mut pixmaps = BTreeMap::new();

        for kind in SkinPixmapKind::ALL {
            if kind == SkinPixmapKind::Balance {
                continue;
            }

            let mut numbers_fallback = false;
            let mut path = Self::find_skin_file(dir, kind.info().file_stem);
            if path.is_none() && kind == SkinPixmapKind::Numbers {
                path = Self::find_skin_file(dir, "numbers");
                numbers_fallback = path.is_some();
            }
            let Some(path) = path else {
                continue;
            };

            let mut image = Self::load_skin_image(&path)?;
            if numbers_fallback {
                image = expand_numbers_fallback(image);
            }
            pixmaps.insert(kind, image);
        }

        apply_balance_fallback(&mut pixmaps);
        let vis_colors = load_vis_colors_from_dir(dir)?;
        let playlist_colors = load_playlist_colors_from_dir(dir)?;

        Ok(Self {
            pixmaps,
            vis_colors,
            playlist_colors,
        })
    }

    pub fn load_from_path(path: &Path) -> io::Result<Self> {
        if path.is_dir() {
            Self::load_from_dir(path)
        } else {
            Self::load_from_archive(path)
        }
    }

    fn load_from_archive(path: &Path) -> io::Result<Self> {
        let entries = archive_entries(path)?;
        let mut pixmaps = BTreeMap::new();

        for kind in SkinPixmapKind::ALL {
            if kind == SkinPixmapKind::Balance {
                continue;
            }

            let mut numbers_fallback = false;
            let mut entry = find_archive_skin_entry(&entries, kind.info().file_stem);
            if entry.is_none() && kind == SkinPixmapKind::Numbers {
                entry = find_archive_skin_entry(&entries, "numbers");
                numbers_fallback = entry.is_some();
            }
            let Some((name, contents)) = entry else {
                continue;
            };

            let mut image = Self::load_skin_image_bytes(
                &format!("{}:{name}", path.display()),
                Path::new(name),
                contents,
            )?;
            if numbers_fallback {
                image = expand_numbers_fallback(image);
            }
            pixmaps.insert(kind, image);
        }

        apply_balance_fallback(&mut pixmaps);
        let vis_colors = load_vis_colors_from_archive(&entries)?;
        let playlist_colors = load_playlist_colors_from_archive(&entries)?;

        Ok(Self {
            pixmaps,
            vis_colors,
            playlist_colors,
        })
    }

    fn find_skin_file(dir: &Path, name: &str) -> Option<PathBuf> {
        let lower = name.to_ascii_lowercase();
        let upper = name.to_ascii_uppercase();
        let mut title = lower.clone();
        if let Some(first) = title.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        let cases = [name, lower.as_str(), upper.as_str(), title.as_str()];
        let exts = [".bmp", ".BMP", ".png", ".PNG", ".xpm", ".XPM"];

        for ext in exts {
            for case in cases {
                let path = dir.join(format!("{case}{ext}"));
                if path.exists() {
                    return Some(path);
                }
            }
        }

        None
    }

    fn load_skin_image(path: &Path) -> io::Result<XpmImage> {
        let contents = fs::read(path)?;
        Self::load_skin_image_bytes(&path.display().to_string(), path, &contents)
    }

    fn load_skin_image_bytes(label: &str, path: &Path, contents: &[u8]) -> io::Result<XpmImage> {
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xpm"))
        {
            let contents = std::str::from_utf8(contents).map_err(|err| {
                io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}"))
            });
            return contents.and_then(|contents| {
                XpmImage::parse(contents).map_err(|err| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}"))
                })
            });
        }

        let decoded = image::load_from_memory(contents)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}")))?;
        let (width, height) = decoded.dimensions();
        let rgba = decoded.to_rgba8();
        let mut argb = Vec::with_capacity((width as usize) * (height as usize));

        for pixel in rgba.pixels() {
            let [r, g, b, mut a] = pixel.0;
            if r == 48 && g == 255 && b == 50 {
                a = 0;
            }
            let pr = ((u16::from(r) * u16::from(a) + 127) / 255) as u32;
            let pg = ((u16::from(g) * u16::from(a) + 127) / 255) as u32;
            let pb = ((u16::from(b) * u16::from(a) + 127) / 255) as u32;
            argb.push((u32::from(a) << 24) | (pr << 16) | (pg << 8) | pb);
        }

        XpmImage::from_argb_pixels(width as usize, height as usize, argb)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {err}")))
    }

    pub fn loaded_pixmap_count(&self) -> usize {
        self.pixmaps.len()
    }

    pub fn get(&self, kind: SkinPixmapKind) -> Option<&XpmImage> {
        self.pixmaps.get(&kind)
    }

    pub fn vis_colors(&self) -> &[[u8; 3]; 24] {
        &self.vis_colors
    }

    pub fn playlist_colors(&self) -> PlaylistColors {
        self.playlist_colors
    }
}

fn archive_entries(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if name.ends_with(".zip") || name.ends_with(".wsz") {
        return zip_archive_entries(path);
    }

    if name.ends_with(".tar") {
        let file = File::open(path)?;
        return tar_archive_entries(file);
    }

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = File::open(path)?;
        return tar_archive_entries(flate2::read::GzDecoder::new(file));
    }

    if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") {
        let file = File::open(path)?;
        return tar_archive_entries(bzip2::read::BzDecoder::new(file));
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("unsupported skin archive format: {}", path.display()),
    ))
}

fn zip_archive_entries(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;
    let mut entries = Vec::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        if entry.is_dir() {
            continue;
        }

        let mut contents = Vec::new();
        entry.read_to_end(&mut contents)?;
        entries.push((entry.name().to_string(), contents));
    }

    Ok(entries)
}

fn tar_archive_entries<R: Read>(reader: R) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let path = entry.path()?.to_string_lossy().into_owned();
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents)?;
        entries.push((path, contents));
    }

    Ok(entries)
}

fn zip_error(err: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

fn find_archive_skin_entry<'a>(
    entries: &'a [(String, Vec<u8>)],
    name: &str,
) -> Option<(&'a str, &'a [u8])> {
    for extension in ["bmp", "png", "xpm"] {
        for (entry_name, contents) in entries {
            let entry_path = Path::new(entry_name);
            let Some(stem) = entry_path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some(entry_extension) = entry_path.extension().and_then(|ext| ext.to_str()) else {
                continue;
            };
            if stem.eq_ignore_ascii_case(name) && entry_extension.eq_ignore_ascii_case(extension) {
                return Some((entry_name, contents));
            }
        }
    }

    None
}

fn load_vis_colors_from_dir(dir: &Path) -> io::Result<[[u8; 3]; 24]> {
    let path = dir.join("viscolor.txt");
    if !path.exists() {
        return Ok(DEFAULT_VIS_COLORS);
    }
    let contents = fs::read_to_string(&path)?;
    Ok(parse_vis_colors(&contents))
}

fn load_vis_colors_from_archive(entries: &[(String, Vec<u8>)]) -> io::Result<[[u8; 3]; 24]> {
    let Some((name, contents)) = entries.iter().find(|(name, _)| {
        Path::new(name)
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.eq_ignore_ascii_case("viscolor.txt"))
    }) else {
        return Ok(DEFAULT_VIS_COLORS);
    };

    let contents = std::str::from_utf8(contents)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {err}")))?;
    Ok(parse_vis_colors(contents))
}

fn load_playlist_colors_from_dir(dir: &Path) -> io::Result<PlaylistColors> {
    let path = dir.join("pledit.txt");
    if !path.exists() {
        return Ok(DEFAULT_PLAYLIST_COLORS);
    }
    let contents = fs::read_to_string(&path)?;
    Ok(parse_playlist_colors(&contents))
}

fn load_playlist_colors_from_archive(entries: &[(String, Vec<u8>)]) -> io::Result<PlaylistColors> {
    let Some((name, contents)) = entries.iter().find(|(name, _)| {
        Path::new(name)
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.eq_ignore_ascii_case("pledit.txt"))
    }) else {
        return Ok(DEFAULT_PLAYLIST_COLORS);
    };

    let contents = std::str::from_utf8(contents)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("{name}: {err}")))?;
    Ok(parse_playlist_colors(contents))
}

fn parse_playlist_colors(contents: &str) -> PlaylistColors {
    let mut colors = DEFAULT_PLAYLIST_COLORS;

    for line in contents.lines() {
        let line = line.trim();
        if let Some(value) = line
            .strip_prefix("Normal=")
            .or_else(|| line.strip_prefix("normal="))
        {
            if let Some(color) = parse_hex_rgb(value) {
                colors.normal = color;
            }
        } else if let Some(value) = line
            .strip_prefix("Current=")
            .or_else(|| line.strip_prefix("current="))
        {
            if let Some(color) = parse_hex_rgb(value) {
                colors.current = color;
            }
        } else if let Some(value) = line
            .strip_prefix("NormalBG=")
            .or_else(|| line.strip_prefix("normalbg="))
        {
            if let Some(color) = parse_hex_rgb(value) {
                colors.normal_bg = color;
            }
        } else if let Some(value) = line
            .strip_prefix("SelectedBG=")
            .or_else(|| line.strip_prefix("selectedbg="))
        {
            if let Some(color) = parse_hex_rgb(value) {
                colors.selected_bg = color;
            }
        }
    }

    colors
}

fn parse_hex_rgb(value: &str) -> Option<[u8; 3]> {
    let value = value.trim();
    let value = value.strip_prefix('#')?;
    if value.len() < 6 {
        return None;
    }

    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some([r, g, b])
}

fn parse_vis_colors(contents: &str) -> [[u8; 3]; 24] {
    let mut colors = DEFAULT_VIS_COLORS;

    for (index, line) in contents.lines().take(24).enumerate() {
        let values: Vec<_> = if line.contains(',') {
            line.split(',').collect()
        } else {
            line.split_whitespace().collect()
        };
        if values.len() < 3 {
            continue;
        }

        let Some(r) = parse_c_int(values[0].trim()) else {
            continue;
        };
        let Some(g) = parse_c_int(values[1].trim()) else {
            continue;
        };
        let Some(b) = parse_c_int(values[2].trim()) else {
            continue;
        };
        colors[index] = [clamp_u8(r), clamp_u8(g), clamp_u8(b)];
    }

    colors
}

fn parse_c_int(value: &str) -> Option<i32> {
    let mut end = 0;
    for (index, ch) in value.char_indices() {
        if index == 0 && (ch == '-' || ch == '+') {
            end = ch.len_utf8();
            continue;
        }
        if !ch.is_ascii_digit() {
            break;
        }
        end = index + ch.len_utf8();
    }

    if end == 0 || value[..end].chars().all(|ch| ch == '-' || ch == '+') {
        return None;
    }

    value[..end].parse().ok()
}

fn clamp_u8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

fn apply_balance_fallback(pixmaps: &mut BTreeMap<SkinPixmapKind, XpmImage>) {
    if !pixmaps.contains_key(&SkinPixmapKind::Balance) {
        if let Some(volume) = pixmaps.get(&SkinPixmapKind::Volume).cloned() {
            pixmaps.insert(SkinPixmapKind::Balance, volume);
        }
    }
}

fn expand_numbers_fallback(image: XpmImage) -> XpmImage {
    if image.width() < 99 || image.height() < 13 {
        return image;
    }

    let mut argb = vec![0; 108 * image.height()];
    for y in 0..13 {
        for x in 0..99 {
            argb[(y * 108) + x] = image.pixel_argb(x, y).unwrap_or(0);
        }
        for x in 99..108 {
            argb[(y * 108) + x] = image.pixel_argb(x - 9, y).unwrap_or(0);
        }
        for x in 101..106 {
            argb[(y * 108) + x] = image.pixel_argb(x - 81, y).unwrap_or(0);
        }
    }

    XpmImage::from_argb_pixels(108, image.height(), argb)
        .expect("expanded numbers fallback dimensions are valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::io::Write;

    const ONE_PIXEL_XPM: &str = r#"
/* XPM */
static char * main_xpm[] = {
"1 1 1 1",
". c #010203",
"."};
"#;

    #[test]
    fn pixmap_info_matches_original_xmms_dimensions() {
        assert_eq!(SkinPixmapKind::Main.info().width, 275);
        assert_eq!(SkinPixmapKind::EqEx.info().height, 50);
        assert_eq!(SkinPixmapKind::Numbers.info().file_stem, "nums_ex");
    }

    #[test]
    fn bundled_default_skin_loads_without_filesystem_lookup() {
        let skin = DefaultSkin::load_bundled().unwrap();
        assert_eq!(skin.loaded_pixmap_count(), SkinPixmapKind::ALL.len());
        assert_eq!(skin.get(SkinPixmapKind::Main).unwrap().width(), 275);
        assert!(skin.get(SkinPixmapKind::Balance).is_some());
        assert_eq!(skin.vis_colors()[0], [9, 34, 53]);
        assert_eq!(skin.playlist_colors(), DEFAULT_PLAYLIST_COLORS);
    }

    #[test]
    fn directory_loader_accepts_png_skin_files() {
        let tmp = std::env::temp_dir().join(format!("xmms-rs-skin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut image = image::RgbaImage::new(1, 1);
        image.put_pixel(0, 0, image::Rgba([48, 255, 50, 255]));
        image.save(tmp.join("main.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.width(), 1);
        assert_eq!(main.height(), 1);
        assert_eq!(main.pixel_argb(0, 0), Some(0));

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_accepts_viscolor_overrides() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-viscolor", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("viscolor.txt"), "300,-1,42\n1 2 3\ninvalid\n").unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        assert_eq!(skin.vis_colors()[0], [255, 0, 42]);
        assert_eq!(skin.vis_colors()[1], [1, 2, 3]);
        assert_eq!(skin.vis_colors()[2], DEFAULT_VIS_COLORS[2]);

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_accepts_playlist_color_overrides() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-pledit", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("pledit.txt"),
            "Normal=#010203\ncurrent=#A0B0C0\nNormalBG=#11223344\nSelectedBG=bad\n",
        )
        .unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        assert_eq!(skin.playlist_colors().normal, [1, 2, 3]);
        assert_eq!(skin.playlist_colors().current, [160, 176, 192]);
        assert_eq!(skin.playlist_colors().normal_bg, [17, 34, 51]);
        assert_eq!(
            skin.playlist_colors().selected_bg,
            DEFAULT_PLAYLIST_COLORS.selected_bg
        );

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_expands_legacy_numbers_fallback() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-numbers", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut image = image::RgbaImage::new(99, 13);
        image.put_pixel(20, 6, image::Rgba([1, 2, 3, 255]));
        image.put_pixel(90, 6, image::Rgba([4, 5, 6, 255]));
        image.save(tmp.join("numbers.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let numbers = skin.get(SkinPixmapKind::Numbers).unwrap();
        assert_eq!(numbers.width(), 108);
        assert_eq!(numbers.height(), 13);
        assert_eq!(numbers.pixel_argb(99, 6), Some(0xff040506));
        assert_eq!(numbers.pixel_argb(101, 6), Some(0xff010203));

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn directory_loader_uses_volume_as_balance_fallback() {
        let tmp =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-balance", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut image = image::RgbaImage::new(1, 1);
        image.put_pixel(0, 0, image::Rgba([10, 20, 30, 255]));
        image.save(tmp.join("volume.png")).unwrap();

        let skin = DefaultSkin::load_from_dir(&tmp).unwrap();
        let balance = skin.get(SkinPixmapKind::Balance).unwrap();
        assert_eq!(balance.width(), 1);
        assert_eq!(balance.height(), 1);
        assert_eq!(balance.pixel_argb(0, 0), Some(0xff0a141e));

        std::fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn path_loader_accepts_wsz_zip_skin_archives() {
        let path =
            std::env::temp_dir().join(format!("xmms-rs-skin-test-{}-zip.wsz", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let file = File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        archive.start_file("Example/Main.xpm", options).unwrap();
        archive.write_all(ONE_PIXEL_XPM.as_bytes()).unwrap();
        archive.start_file("Example/viscolor.txt", options).unwrap();
        archive.write_all(b"4,5,6\n").unwrap();
        archive.start_file("Example/pledit.txt", options).unwrap();
        archive.write_all(b"Normal=#070809\n").unwrap();
        archive.finish().unwrap();

        let skin = DefaultSkin::load_from_path(&path).unwrap();
        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.width(), 1);
        assert_eq!(main.height(), 1);
        assert_eq!(skin.vis_colors()[0], [4, 5, 6]);
        assert_eq!(skin.playlist_colors().normal, [7, 8, 9]);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn path_loader_accepts_tar_gz_skin_archives() {
        let path = std::env::temp_dir().join(format!(
            "xmms-rs-skin-test-{}-tar.tar.gz",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let file = File::create(&path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(ONE_PIXEL_XPM.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, "Example/main.xpm", Cursor::new(ONE_PIXEL_XPM))
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();

        let skin = DefaultSkin::load_from_path(&path).unwrap();
        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.width(), 1);
        assert_eq!(main.height(), 1);

        std::fs::remove_file(path).unwrap();
    }
}
