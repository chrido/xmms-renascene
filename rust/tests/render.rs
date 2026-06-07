use std::path::PathBuf;

use cairo::{Context, Format, ImageSurface};
use xmms_resuscitated::render::{
    render_equalizer_state, render_main_player_reset, render_playlist_frame, render_playlist_rows,
    render_visualization, surface_from_xpm, EqualizerRenderState, PlaylistRowRenderEntry,
    PlaylistRowsRenderState, VisualizationRenderState, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH,
};
use xmms_resuscitated::skin::widget::{VisAnalyzerMode, VisMode, VisScopeMode};
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

#[test]
fn renders_playlist_rows_with_selected_background() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let mut surface = ImageSurface::create(
        Format::ARgb32,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
    )
    .unwrap();
    let cr = Context::new(&surface).unwrap();
    assert!(render_playlist_frame(
        &cr,
        &skin,
        true,
        false,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
        None,
        None
    )
    .unwrap());
    assert!(render_playlist_rows(
        &cr,
        &skin,
        &PlaylistRowsRenderState {
            entries: vec![
                PlaylistRowRenderEntry {
                    title: "First".to_string(),
                    length_ms: 61_000,
                    selected: true,
                    current: false,
                },
                PlaylistRowRenderEntry {
                    title: "Second".to_string(),
                    length_ms: -1,
                    selected: false,
                    current: true,
                },
            ],
            scroll_offset: 0,
            scrollbar_dragging: false,
            search_query: Some("Beta".to_string()),
            show_numbers: true,
            font_family: "Helvetica".to_string(),
            width: PLAYLIST_DEFAULT_WIDTH,
            height: PLAYLIST_DEFAULT_HEIGHT,
        }
    )
    .unwrap());
    drop(cr);
    surface.flush();

    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let offset = 21 * stride + 150 * 4;
    let selected = skin.playlist_colors().selected_bg;
    assert_eq!(data[offset], selected[2]);
    assert_eq!(data[offset + 1], selected[1]);
    assert_eq!(data[offset + 2], selected[0]);
}

#[test]
fn playlist_search_overlay_stays_inside_row_area() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let mut surface = ImageSurface::create(
        Format::ARgb32,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
    )
    .unwrap();
    let cr = Context::new(&surface).unwrap();
    assert!(render_playlist_frame(
        &cr,
        &skin,
        true,
        false,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
        None,
        None
    )
    .unwrap());
    assert!(render_playlist_rows(
        &cr,
        &skin,
        &PlaylistRowsRenderState {
            entries: vec![],
            scroll_offset: 0,
            scrollbar_dragging: false,
            search_query: Some("needle".to_string()),
            show_numbers: true,
            font_family: "Helvetica".to_string(),
            width: PLAYLIST_DEFAULT_WIDTH,
            height: PLAYLIST_DEFAULT_HEIGHT,
        }
    )
    .unwrap());
    drop(cr);
    surface.flush();

    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let colors = skin.playlist_colors();
    let y = (PLAYLIST_DEFAULT_HEIGHT - 48) as usize;
    let color_at = |x: usize| {
        let offset = y * stride + x * 4;
        [data[offset + 2], data[offset + 1], data[offset]]
    };

    assert_eq!(color_at(20), colors.selected_bg);
    assert_ne!(color_at(10), colors.selected_bg);
    assert_ne!(
        color_at((PLAYLIST_DEFAULT_WIDTH - 18) as usize),
        colors.selected_bg
    );
}

#[test]
fn shaded_playlist_titlebar_does_not_show_skin_separator_pixels() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let width = PLAYLIST_DEFAULT_WIDTH + 14;
    let mut surface = ImageSurface::create(Format::ARgb32, width, 14).unwrap();
    let cr = Context::new(&surface).unwrap();
    assert!(render_playlist_frame(
        &cr,
        &skin,
        true,
        true,
        width,
        PLAYLIST_DEFAULT_HEIGHT,
        Some(""),
        None
    )
    .unwrap());
    drop(cr);
    surface.flush();

    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let transparent_pixels = (0..14_usize)
        .flat_map(|y| (0..width as usize).map(move |x| y * stride + x * 4))
        .filter(|&offset| data[offset + 3] == 0)
        .count();
    let separator_pixels = (0..14_usize)
        .flat_map(|y| (0..width as usize).map(move |x| y * stride + x * 4))
        .filter(|&offset| data[offset] == 0 && data[offset + 1] == 0x39 && data[offset + 2] == 0xff)
        .count();

    assert_eq!(transparent_pixels, 0);
    assert_eq!(separator_pixels, 0);
}

#[test]
fn equalizer_graph_uses_skin_color_ramp() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let mut surface = ImageSurface::create(Format::ARgb32, 275, 116).unwrap();
    let cr = Context::new(&surface).unwrap();
    assert!(render_equalizer_state(&cr, &skin, &EqualizerRenderState::default()).unwrap());
    drop(cr);
    surface.flush();

    let expected = skin
        .get(SkinPixmapKind::EqMain)
        .unwrap()
        .pixel_argb(115, 303)
        .unwrap();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let offset = 26 * stride + 88 * 4;
    let actual = u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap());

    assert_eq!(actual & 0x00ff_ffff, expected & 0x00ff_ffff);
    assert_ne!(actual & 0x00ff_ffff, 0x0000_ff00);
}

#[test]
fn shaded_equalizer_draws_volume_and_balance_slider_handles() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let mut surface = ImageSurface::create(Format::ARgb32, 275, 14).unwrap();
    let cr = Context::new(&surface).unwrap();
    assert!(render_equalizer_state(
        &cr,
        &skin,
        &EqualizerRenderState {
            shaded: true,
            volume_position: 45,
            balance_position: 30,
            ..EqualizerRenderState::default()
        }
    )
    .unwrap());
    drop(cr);
    surface.flush();

    let eq_ex = skin.get(SkinPixmapKind::EqEx).unwrap();
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    for (source_x, dest_x) in [(4_usize, 61_usize + 45), (17_usize, 164_usize + 30)] {
        let (dx, dy, expected) = (0..7_usize)
            .flat_map(|y| (0..3_usize).map(move |x| (x, y)))
            .find_map(|(x, y)| {
                eq_ex
                    .pixel_argb(source_x + x, 30 + y)
                    .filter(|pixel| *pixel != 0)
                    .map(|pixel| (x, y, pixel))
            })
            .expect("default eq_ex skin should contain visible shaded slider knob pixels");
        let offset = (4 + dy) * stride + (dest_x + dx) * 4;
        let actual = u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap());
        assert_eq!(actual, expected);
    }
}

#[test]
fn shaded_playlist_title_uses_skin_bitmap_text() {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let mut surface = ImageSurface::create(Format::ARgb32, PLAYLIST_DEFAULT_WIDTH, 14).unwrap();
    let cr = Context::new(&surface).unwrap();
    assert!(render_playlist_frame(
        &cr,
        &skin,
        true,
        true,
        PLAYLIST_DEFAULT_WIDTH,
        PLAYLIST_DEFAULT_HEIGHT,
        Some("A"),
        None
    )
    .unwrap());
    drop(cr);
    surface.flush();

    let text = skin.get(SkinPixmapKind::Text).unwrap();
    let expected = (0..6_usize)
        .flat_map(|y| (0..5_usize).map(move |x| (x, y)))
        .find_map(|(x, y)| {
            text.pixel_argb(x, y)
                .filter(|pixel| *pixel != 0)
                .map(|pixel| (x, y, pixel))
        })
        .expect("default text skin should contain visible A pixels");
    let stride = surface.stride() as usize;
    let data = surface.data().unwrap();
    let offset = (4 + expected.1) * stride + (4 + expected.0) * 4;
    let actual = u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap());

    assert_eq!(actual, expected.2);
}

fn rendered_visualization_bytes(state: VisualizationRenderState) -> Vec<u8> {
    let skin = DefaultSkin::load_from_dir(&repo_root().join("data").join("defskin")).unwrap();
    let mut surface = ImageSurface::create(Format::ARgb32, 76, 16).unwrap();
    let cr = Context::new(&surface).unwrap();
    render_visualization(&cr, &skin, 0, 0, 76, &state).unwrap();
    drop(cr);
    surface.flush();
    let bytes = surface.data().unwrap().to_vec();
    bytes
}

fn sample_visualization_state(mode: VisMode) -> VisualizationRenderState {
    let mut state = VisualizationRenderState {
        mode,
        ..VisualizationRenderState::default()
    };
    for (index, value) in state.data.iter_mut().enumerate() {
        *value = ((index % 16) as f32 + 1.0) / 16.0;
    }
    state.peak = state.data;
    state
}

#[test]
fn renders_distinct_visualization_modes_for_analyzer_and_scope() {
    let analyzer = rendered_visualization_bytes(sample_visualization_state(VisMode::Analyzer));
    let scope = rendered_visualization_bytes(sample_visualization_state(VisMode::Scope));
    let off = rendered_visualization_bytes(sample_visualization_state(VisMode::Off));

    assert_ne!(analyzer, off);
    assert_ne!(scope, off);
    assert_ne!(analyzer, scope);
}

#[test]
fn analyzer_and_scope_submodes_change_rendered_output() {
    let mut normal = sample_visualization_state(VisMode::Analyzer);
    normal.analyzer_mode = VisAnalyzerMode::Normal;
    let mut fire = normal.clone();
    fire.analyzer_mode = VisAnalyzerMode::Fire;

    let mut dot = sample_visualization_state(VisMode::Scope);
    dot.scope_mode = VisScopeMode::Dot;
    let mut solid = dot.clone();
    solid.scope_mode = VisScopeMode::Solid;

    assert_ne!(
        rendered_visualization_bytes(normal),
        rendered_visualization_bytes(fire)
    );
    assert_ne!(
        rendered_visualization_bytes(dot),
        rendered_visualization_bytes(solid)
    );
}
