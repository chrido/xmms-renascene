use std::path::PathBuf;

use xmms_renascene::skin::layout::{MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH};
use xmms_renascene::skin::xpm::XpmImage;
use xmms_renascene::skin::{DefaultSkin, SkinPixmapKind};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn parses_all_bundled_default_xpm_files() {
    let skin = DefaultSkin::load_bundled().unwrap();

    assert_eq!(skin.loaded_pixmap_count(), SkinPixmapKind::ALL.len());

    for kind in SkinPixmapKind::ALL {
        let image = skin.get(kind).unwrap();
        if kind == SkinPixmapKind::Balance {
            assert_eq!(image.width(), SkinPixmapKind::Volume.info().width);
            assert_eq!(
                image.height(),
                skin.get(SkinPixmapKind::Volume).unwrap().height()
            );
        }

        assert_eq!(image.pixels_argb().len(), image.width() * image.height());
    }

    assert_eq!(
        skin.get(SkinPixmapKind::Main).unwrap().width(),
        MAIN_WINDOW_WIDTH as usize
    );
    assert_eq!(
        skin.get(SkinPixmapKind::Main).unwrap().height(),
        MAIN_WINDOW_HEIGHT as usize
    );
    assert_eq!(skin.get(SkinPixmapKind::Titlebar).unwrap().width(), 344);
    assert_eq!(skin.get(SkinPixmapKind::EqMain).unwrap().height(), 315);
}

#[test]
fn directory_default_skin_loader_matches_bundled_loader() {
    let skin_dir = repo_root().join("data").join("defskin");
    let from_dir = DefaultSkin::load_from_dir(&skin_dir).unwrap();
    let bundled = DefaultSkin::load_bundled().unwrap();

    for kind in SkinPixmapKind::ALL {
        assert_eq!(
            from_dir.get(kind).unwrap().pixels_argb(),
            bundled.get(kind).unwrap().pixels_argb(),
            "{kind:?}"
        );
    }
}

#[test]
fn bundled_main_skin_has_expected_dimensions_and_pixels() {
    let path = repo_root().join("data").join("defskin").join("main.xpm");
    let contents = std::fs::read_to_string(path).unwrap();
    let image = XpmImage::parse(&contents).unwrap();

    assert_eq!(image.width(), MAIN_WINDOW_WIDTH as usize);
    assert_eq!(image.height(), MAIN_WINDOW_HEIGHT as usize);
    assert_eq!(
        image.pixels_argb().len(),
        (MAIN_WINDOW_WIDTH * MAIN_WINDOW_HEIGHT) as usize
    );
    assert_eq!(image.pixel_argb(0, 0), Some(0xff00_0000));
}
