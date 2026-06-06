use std::path::PathBuf;

use xmms_resuscitated::render::surface_from_xpm;
use xmms_resuscitated::skin::{DefaultSkin, SkinPixmapKind};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn renders_main_default_skin_to_cairo_surface() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let main = skin.get(SkinPixmapKind::Main).unwrap();
    let surface = surface_from_xpm(main).unwrap();

    assert_eq!(surface.width(), 275);
    assert_eq!(surface.height(), 116);
    assert!(surface.stride() >= 275 * 4);
}
