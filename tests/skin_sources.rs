use std::fs::{self, File};
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use xmms_renascene::skin::{
    DefaultSkin, SkinPixmapKind, DEFAULT_PLAYLIST_COLORS, DEFAULT_VIS_COLORS,
};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct FixtureDir(PathBuf);

impl FixtureDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "xmms-skin-sources-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for FixtureDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

type Assets = Vec<(&'static str, Vec<u8>)>;

fn image_bytes(width: u32, height: u32, color: [u8; 4], format: ImageFormat) -> Vec<u8> {
    encode_image(RgbaImage::from_pixel(width, height, Rgba(color)), format)
}

fn image_pixels_bytes(pixels: &[[u8; 4]], format: ImageFormat) -> Vec<u8> {
    let image = RgbaImage::from_fn(pixels.len() as u32, 1, |x, _| Rgba(pixels[x as usize]));
    encode_image(image, format)
}

fn encode_image(image: RgbaImage, format: ImageFormat) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut output, format)
        .unwrap();
    output.into_inner()
}

fn xpm(color: &str) -> Vec<u8> {
    format!("/* XPM */\nstatic char * skin[] = {{\n\"1 1 1 1\",\n\". c {color}\",\n\".\"}};\n")
        .into_bytes()
}

fn write_directory(path: &Path, assets: &Assets) {
    fs::create_dir_all(path).unwrap();
    for (name, contents) in assets {
        let destination = path.join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, contents).unwrap();
    }
}

fn write_zip(path: &Path, assets: &Assets) -> io::Result<()> {
    let mut zip = zip::ZipWriter::new(File::create(path)?);
    for (name, contents) in assets {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())?;
        zip.write_all(contents)?;
    }
    zip.finish()?;
    Ok(())
}

fn write_tar(path: &Path, assets: &Assets) -> io::Result<()> {
    let mut tar = tar::Builder::new(File::create(path)?);
    for (name, contents) in assets {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, Cursor::new(contents))?;
    }
    tar.finish()
}

fn load_all(assets: &Assets) -> (DefaultSkin, DefaultSkin, DefaultSkin) {
    let fixture = FixtureDir::new();
    let directory = fixture.0.join("directory");
    let zip = fixture.0.join("skin.zip");
    let tar = fixture.0.join("skin.tar");
    write_directory(&directory, assets);
    write_zip(&zip, assets).unwrap();
    write_tar(&tar, assets).unwrap();
    (
        DefaultSkin::load_from_dir(&directory).unwrap(),
        DefaultSkin::load_from_path(&zip).unwrap(),
        DefaultSkin::load_from_path(&tar).unwrap(),
    )
}

fn shared_assets() -> Assets {
    vec![
        ("Skin/MAIN.XPM", xpm("#010203")),
        (
            "Skin/Volume.PNG",
            image_bytes(2, 1, [12, 34, 56, 255], ImageFormat::Png),
        ),
        (
            "Skin/NuMbErS.png",
            image_bytes(99, 13, [7, 8, 9, 255], ImageFormat::Png),
        ),
        (
            "Skin/EQMAIN.png",
            image_bytes(275, 116, [40, 50, 60, 255], ImageFormat::Png),
        ),
        (
            "Skin/Text.png",
            image_bytes(155, 6, [11, 22, 33, 255], ImageFormat::Png),
        ),
        ("Skin/Nested/VISCOLOR.TXT", b"300,-1,42\n1 2 3\n".to_vec()),
        (
            "Skin/Nested/PLEDIT.TXT",
            b"[text]\nNormal=#010203\nCurrent=#040506\n".to_vec(),
        ),
        (
            "Skin/Nested/REGION.TXT",
            b"[Normal]\nNumPoints=2\nPointList=1,2,3,4\n".to_vec(),
        ),
    ]
}

#[test]
fn directory_zip_and_tar_assemble_the_same_partial_skin() {
    let (directory, zip, tar) = load_all(&shared_assets());
    assert_eq!(directory, zip);
    assert_eq!(directory, tar);
    assert_eq!(directory.loaded_pixmap_count(), SkinPixmapKind::ALL.len());
    assert_eq!(
        directory
            .get(SkinPixmapKind::Main)
            .unwrap()
            .pixel_argb(0, 0),
        Some(0xff010203)
    );
    assert_eq!(
        directory.get(SkinPixmapKind::Balance),
        directory.get(SkinPixmapKind::Volume)
    );
    let numbers = directory.get(SkinPixmapKind::Numbers).unwrap();
    assert_eq!(numbers.width(), 108);
    assert_eq!(numbers.pixel_argb(101, 0), Some(0xff070809));
    let eq_ex = directory.get(SkinPixmapKind::EqEx).unwrap();
    assert_eq!(eq_ex.pixel_argb(0, 0), Some(0xff28323c));
    assert_eq!(eq_ex.pixel_argb(0, 15), Some(0xff28323c));
    assert_eq!(
        eq_ex.pixel_argb(0, 30),
        DefaultSkin::load_bundled()
            .unwrap()
            .get(SkinPixmapKind::EqEx)
            .unwrap()
            .pixel_argb(0, 30)
    );
    assert_eq!(directory.vis_colors()[0], [255, 0, 42]);
    assert_eq!(directory.vis_colors()[2], DEFAULT_VIS_COLORS[2]);
    assert_eq!(directory.playlist_colors().normal, [1, 2, 3]);
    assert_eq!(
        directory.playlist_colors().selected_bg,
        DEFAULT_PLAYLIST_COLORS.selected_bg
    );
    assert_eq!(
        directory.region_masks().normal.as_ref().unwrap().polygons(),
        &[vec![[1, 2], [3, 4]]]
    );
    assert_eq!(directory.text_colors().background[0], [11, 22, 33]);
}

#[test]
fn png_pixels_round_and_chroma_key_identically_in_directory_zip_and_tar() {
    let png = image_pixels_bytes(
        &[
            [48, 255, 50, 255], // opaque chroma key
            [48, 255, 50, 128], // keyed even with partial alpha
            [1, 1, 1, 127],     // rounds down
            [1, 1, 1, 128],     // rounds up
            [255, 128, 64, 128],
            [10, 20, 30, 255],
            [10, 20, 30, 0],
        ],
        ImageFormat::Png,
    );
    let (directory, zip, tar) = load_all(&vec![("Skin/Main.png", png)]);
    for skin in [&directory, &zip, &tar] {
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixels_argb(),
            &[0, 0, 0x7f00_0000, 0x8001_0101, 0x8080_4020, 0xff0a_141e, 0]
        );
    }
}

#[test]
fn bmp_chroma_key_is_applied_in_directory_zip_and_tar() {
    let bmp = image_pixels_bytes(&[[48, 255, 50, 255], [17, 34, 51, 255]], ImageFormat::Bmp);
    let (directory, zip, tar) = load_all(&vec![("Skin/Main.bmp", bmp)]);
    for skin in [&directory, &zip, &tar] {
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixels_argb(),
            &[0, 0xff11_2233]
        );
    }
}

#[test]
fn missing_or_non_directory_roots_are_errors() {
    let fixture = FixtureDir::new();
    let missing = fixture.0.join("missing");
    for result in [
        DefaultSkin::load_from_dir(&missing),
        DefaultSkin::load_from_path(&missing),
    ] {
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(err.to_string().contains(&missing.display().to_string()));
    }

    let file = fixture.0.join("regular-file");
    fs::write(&file, b"not a directory").unwrap();
    let err = DefaultSkin::load_from_dir(&file).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotADirectory);
    assert!(err
        .to_string()
        .contains(&format!("read_dir {}", file.display())));
}

#[test]
fn unicode_playlist_colors_fall_back_to_defaults_in_all_sources() {
    let assets: Assets = vec![(
        "Skin/PLEDIT.TXT",
        "[text]\nNormal=#€123\nCurrent=12€34\nNormalBG=1234€\nSelectedBG=a€123\n"
            .as_bytes()
            .to_vec(),
    )];
    let (directory, zip, tar) = load_all(&assets);
    for skin in [&directory, &zip, &tar] {
        assert_eq!(skin.playlist_colors(), DEFAULT_PLAYLIST_COLORS);
    }
}

#[test]
fn malformed_metadata_reports_invalid_data_and_its_asset_path() {
    let fixture = FixtureDir::new();
    let directory = fixture.0.join("directory");
    for name in ["viscolor.txt", "pledit.txt", "region.txt"] {
        let path = directory.join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, b"\xff\xfe").unwrap();
        let err = DefaultSkin::load_from_dir(&directory).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("decode text asset"));
        assert!(err.to_string().contains(&path.display().to_string()));
        fs::remove_file(path).unwrap();
    }

    let assets: Assets = vec![("Nested/viscolor.txt", b"\xff\xfe".to_vec())];
    for (archive, write) in [
        (
            fixture.0.join("skin.zip"),
            write_zip as fn(&Path, &Assets) -> io::Result<()>,
        ),
        (fixture.0.join("skin.tar"), write_tar),
    ] {
        write(&archive, &assets).unwrap();
        let err = DefaultSkin::load_from_path(&archive).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("decode text asset"));
        assert!(err
            .to_string()
            .contains(&format!("{}:Nested/viscolor.txt", archive.display())));
    }
}

#[cfg(unix)]
#[test]
fn root_symlinks_still_dispatch_to_directories_and_archives() {
    use std::os::unix::fs::symlink;

    let fixture = FixtureDir::new();
    let directory = fixture.0.join("directory");
    let archive = fixture.0.join("skin.zip");
    let assets: Assets = vec![("Main.xpm", xpm("#010203"))];
    write_directory(&directory, &assets);
    write_zip(&archive, &assets).unwrap();
    let directory_link = fixture.0.join("dir-link");
    let archive_link = fixture.0.join("zip-link.zip");
    symlink(&directory, &directory_link).unwrap();
    symlink(&archive, &archive_link).unwrap();
    assert_eq!(
        DefaultSkin::load_from_path(&directory_link).unwrap(),
        DefaultSkin::load_from_dir(&directory).unwrap()
    );
    assert_eq!(
        DefaultSkin::load_from_path(&archive_link).unwrap(),
        DefaultSkin::load_from_path(&archive).unwrap()
    );
}

#[test]
fn missing_optional_assets_use_bundled_defaults_in_all_sources() {
    let (directory, zip, tar) = load_all(&Vec::new());
    let bundled = DefaultSkin::load_bundled().unwrap();
    assert_eq!(directory, bundled);
    assert_eq!(zip, bundled);
    assert_eq!(tar, bundled);
}

#[test]
fn explicit_numbers_balance_and_eq_ex_override_compatibility_fallbacks() {
    let mut assets = shared_assets();
    assets.extend([
        ("Skin/nums_ex.xpm", xpm("#aabbcc")),
        ("Skin/balance.xpm", xpm("#112233")),
        ("Skin/eq_ex.xpm", xpm("#445566")),
    ]);
    let (directory, zip, tar) = load_all(&assets);
    assert_eq!(directory, zip);
    assert_eq!(directory, tar);
    for (kind, color) in [
        (SkinPixmapKind::Numbers, 0xffaabbcc),
        (SkinPixmapKind::Balance, 0xff112233),
        (SkinPixmapKind::EqEx, 0xff445566),
    ] {
        assert_eq!(directory.get(kind).unwrap().pixel_argb(0, 0), Some(color));
        assert_eq!(directory.get(kind).unwrap().width(), 1);
    }
}

#[test]
fn mixed_case_duplicates_keep_filesystem_and_archive_entry_order() {
    let assets: Assets = vec![
        (
            "mAiN.BMP",
            image_bytes(1, 1, [1, 2, 3, 255], ImageFormat::Bmp),
        ),
        (
            "MAIN.bmp",
            image_bytes(1, 1, [4, 5, 6, 255], ImageFormat::Bmp),
        ),
        ("VisColor.TXT", b"7,8,9\n".to_vec()),
        ("VISCOLOR.txt", b"10,11,12\n".to_vec()),
    ];
    let fixture = FixtureDir::new();
    let directory = fixture.0.join("directory");
    let zip_path = fixture.0.join("skin.zip");
    let tar_path = fixture.0.join("skin.tar");
    write_directory(&directory, &assets);
    write_zip(&zip_path, &assets).unwrap();
    write_tar(&tar_path, &assets).unwrap();

    // Derive expected directory ties from the actual, unsorted read_dir order.
    let names: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    let first_image = names
        .iter()
        .find(|name| name.eq_ignore_ascii_case("main.bmp"))
        .unwrap();
    let first_text = names
        .iter()
        .find(|name| name.eq_ignore_ascii_case("viscolor.txt"))
        .unwrap();
    let skin = DefaultSkin::load_from_dir(&directory).unwrap();
    let dir_pixel = if first_image == "mAiN.BMP" {
        0xff010203
    } else {
        0xff040506
    };
    let dir_vis = if first_text == "VisColor.TXT" {
        [7, 8, 9]
    } else {
        [10, 11, 12]
    };
    assert_eq!(
        skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
        Some(dir_pixel)
    );
    assert_eq!(skin.vis_colors()[0], dir_vis);

    for path in [&zip_path, &tar_path] {
        let skin = DefaultSkin::load_from_path(path).unwrap();
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff010203)
        );
        assert_eq!(skin.vis_colors()[0], [7, 8, 9]);
    }
}

#[test]
fn nested_extension_priority_and_files_before_children_survive_indexing() {
    let assets: Assets = vec![
        (
            "First/Main.png",
            image_bytes(1, 1, [1, 2, 3, 255], ImageFormat::Png),
        ),
        (
            "Second/Main.bmp",
            image_bytes(1, 1, [4, 5, 6, 255], ImageFormat::Bmp),
        ),
        ("First/Main.xpm", xpm("#070809")),
    ];
    let (directory, zip, tar) = load_all(&assets);
    for skin in [&directory, &zip, &tar] {
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff040506)
        );
    }

    let assets: Assets = vec![
        (
            "Outer/Child/Main.png",
            image_bytes(1, 1, [1, 2, 3, 255], ImageFormat::Png),
        ),
        ("Outer/Child/VisColor.txt", b"1,2,3\n".to_vec()),
        (
            "Outer/Main.png",
            image_bytes(1, 1, [4, 5, 6, 255], ImageFormat::Png),
        ),
        ("Outer/viscolor.txt", b"4,5,6\n".to_vec()),
    ];
    let (directory, zip, tar) = load_all(&assets);
    assert_eq!(
        directory
            .get(SkinPixmapKind::Main)
            .unwrap()
            .pixel_argb(0, 0),
        Some(0xff040506)
    );
    assert_eq!(directory.vis_colors()[0], [4, 5, 6]);
    for skin in [&zip, &tar] {
        assert_eq!(
            skin.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff010203)
        );
        assert_eq!(skin.vis_colors()[0], [1, 2, 3]);
    }
}

#[test]
fn directory_prefers_root_then_extension_while_archives_prefer_extension_then_entry_order() {
    let assets = vec![
        (
            "Nested/Main.bmp",
            image_bytes(1, 1, [1, 2, 3, 255], ImageFormat::Bmp),
        ),
        (
            "Main.png",
            image_bytes(1, 1, [7, 8, 9, 255], ImageFormat::Png),
        ),
        ("Main.xpm", xpm("#0a0b0c")),
        ("Nested/VISCOLOR.TXT", b"1,2,3\n".to_vec()),
        ("viscolor.txt", b"4,5,6\n".to_vec()),
    ];
    let (directory, zip, tar) = load_all(&assets);
    assert_eq!(
        directory
            .get(SkinPixmapKind::Main)
            .unwrap()
            .pixel_argb(0, 0),
        Some(0xff070809)
    );
    for archive in [&zip, &tar] {
        assert_eq!(
            archive.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff010203)
        );
        assert_eq!(archive.vis_colors()[0], [1, 2, 3]);
    }
    assert_eq!(directory.vis_colors()[0], [4, 5, 6]);

    let assets: Assets = vec![
        (
            "Nested/Main.bmp",
            image_bytes(1, 1, [1, 2, 3, 255], ImageFormat::Bmp),
        ),
        (
            "Main.bmp",
            image_bytes(1, 1, [4, 5, 6, 255], ImageFormat::Bmp),
        ),
        (
            "Main.png",
            image_bytes(1, 1, [7, 8, 9, 255], ImageFormat::Png),
        ),
        ("Main.xpm", xpm("#0a0b0c")),
    ];
    let (directory, zip, tar) = load_all(&assets);
    assert_eq!(
        directory
            .get(SkinPixmapKind::Main)
            .unwrap()
            .pixel_argb(0, 0),
        Some(0xff040506)
    );
    for archive in [&zip, &tar] {
        assert_eq!(
            archive.get(SkinPixmapKind::Main).unwrap().pixel_argb(0, 0),
            Some(0xff010203)
        );
    }
}
