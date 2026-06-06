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
}

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
        Ok(Self { pixmaps })
    }

    pub fn load_from_dir(dir: &Path) -> io::Result<Self> {
        let mut pixmaps = BTreeMap::new();

        for kind in SkinPixmapKind::ALL {
            if kind == SkinPixmapKind::Balance {
                continue;
            }

            let Some(path) = Self::find_skin_file(dir, kind.info().file_stem) else {
                continue;
            };

            let image = Self::load_skin_image(&path)?;
            pixmaps.insert(kind, image);
        }

        apply_balance_fallback(&mut pixmaps);

        Ok(Self { pixmaps })
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

            let Some((name, contents)) = find_archive_skin_entry(&entries, kind.info().file_stem)
            else {
                continue;
            };

            let image = Self::load_skin_image_bytes(
                &format!("{}:{name}", path.display()),
                Path::new(name),
                contents,
            )?;
            pixmaps.insert(kind, image);
        }

        apply_balance_fallback(&mut pixmaps);

        Ok(Self { pixmaps })
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

fn apply_balance_fallback(pixmaps: &mut BTreeMap<SkinPixmapKind, XpmImage>) {
    if !pixmaps.contains_key(&SkinPixmapKind::Balance) {
        if let Some(volume) = pixmaps.get(&SkinPixmapKind::Volume).cloned() {
            pixmaps.insert(SkinPixmapKind::Balance, volume);
        }
    }
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
        archive.finish().unwrap();

        let skin = DefaultSkin::load_from_path(&path).unwrap();
        let main = skin.get(SkinPixmapKind::Main).unwrap();
        assert_eq!(main.width(), 1);
        assert_eq!(main.height(), 1);

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
