use std::path::PathBuf;

use cairo::{Context, Format, ImageSurface};
use xmms_resuscitated::render::{
    render_main_player_reset, surface_from_xpm, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
};
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

#[test]
fn renders_reset_state_widgets_over_main_skin() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let main = skin.get(SkinPixmapKind::Main).unwrap();
    let mut main_surface = surface_from_xpm(main).unwrap();
    let mut reset_surface =
        ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT).unwrap();
    let cr = Context::new(&reset_surface).unwrap();

    assert!(render_main_player_reset(&cr, &skin).unwrap());
    drop(cr);

    let main_stride = main_surface.stride() as usize;
    let reset_stride = reset_surface.stride() as usize;
    let main_data = main_surface.data().unwrap();
    let reset_data = reset_surface.data().unwrap();
    let changed_pixels = (0..MAIN_WINDOW_HEIGHT as usize)
        .flat_map(|y| (0..MAIN_WINDOW_WIDTH as usize).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let main_offset = y * main_stride + x * 4;
            let reset_offset = y * reset_stride + x * 4;
            main_data[main_offset..main_offset + 4] != reset_data[reset_offset..reset_offset + 4]
        })
        .count();

    assert!(changed_pixels > 1_000);
}
