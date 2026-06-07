mod core;
mod docked;
mod equalizer;
mod main;
mod playlist;

pub use crate::skin::layout::{
    equalizer_control_spec, equalizer_slider_layout, equalizer_window_height,
    main_push_button_spec, main_slider_layout, main_toggle_button_spec, main_window_height,
    playlist_window_height, EqualizerControl, EqualizerSlider, MainPushButton, MainSlider,
    MainToggleButton, SliderLayout, SpriteSpec, EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH,
    MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH, PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH, PLAYLIST_HEIGHT_BASE, PLAYLIST_MIN_HEIGHT, PLAYLIST_MIN_WIDTH,
    PLAYLIST_WIDTH_STEP,
};
pub use core::*;
pub use docked::*;
pub use equalizer::*;
pub use main::*;
pub use playlist::*;

#[cfg(test)]
mod tests {
    use super::*;
    use cairo::{Context, Format, ImageSurface};

    use crate::skin::xpm::XpmImage;
    use crate::skin::{DefaultSkin, SkinPixmapKind};

    #[test]
    fn creates_cairo_surface_from_xpm_pixels() {
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "2 1 2 1",
            "a c #000000",
            "b c None",
            "ab"
            };"#,
        )
        .unwrap();

        let surface = surface_from_xpm(&image).unwrap();
        assert_eq!(surface.width(), 2);
        assert_eq!(surface.height(), 1);

        let mut surface = surface;
        let data = surface.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(data[0..4].try_into().unwrap()),
            0xff00_0000
        );
        assert_eq!(u32::from_ne_bytes(data[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn blits_source_rect_to_destination() {
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "2 1 2 1",
            "a c #010203",
            "b c #040506",
            "ab"
            };"#,
        )
        .unwrap();
        let source = surface_from_xpm(&image).unwrap();
        let mut dest = ImageSurface::create(Format::ARgb32, 2, 1).unwrap();
        let cr = Context::new(&dest).unwrap();

        assert!(blit_surface_rect(&cr, &source, 1, 0, 0, 0, 1, 1).unwrap());
        drop(cr);
        dest.flush();

        let data = dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(data[0..4].try_into().unwrap()),
            0xff04_0506
        );
        assert_eq!(u32::from_ne_bytes(data[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn blit_clips_negative_source_coordinates_like_c() {
        let image = XpmImage::parse(
            r#"/* XPM */
            static char *x[] = {
            "1 1 1 1",
            "a c #010203",
            "a"
            };"#,
        )
        .unwrap();
        let source = surface_from_xpm(&image).unwrap();
        let mut dest = ImageSurface::create(Format::ARgb32, 2, 1).unwrap();
        let cr = Context::new(&dest).unwrap();

        assert!(blit_surface_rect(&cr, &source, -1, 0, 0, 0, 2, 1).unwrap());
        drop(cr);
        dest.flush();

        let data = dest.data().unwrap();
        assert_eq!(u32::from_ne_bytes(data[0..4].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_ne_bytes(data[4..8].try_into().unwrap()),
            0xff01_0203
        );
    }

    #[test]
    fn scaling_helpers_match_c_rounding_and_clamping() {
        assert_eq!(clamp_scale_factor(0.5), 1.0);
        assert_eq!(clamp_scale_factor(6.0), 5.0);
        assert_eq!(scale_coord(11, 1.5), 17);
        assert_eq!(scale_dim(0, 1.0), 1);
    }

    #[test]
    fn applies_window_scale_from_actual_to_base_size() {
        let surface = ImageSurface::create(Format::ARgb32, 20, 20).unwrap();
        let cr = Context::new(&surface).unwrap();

        assert!(apply_window_scale(&cr, 20, 10, 10, 5));
        let matrix = cr.matrix();
        assert_eq!(matrix.xx(), 2.0);
        assert_eq!(matrix.yy(), 2.0);
        assert!(!apply_window_scale(&cr, 0, 10, 10, 5));
    }

    #[test]
    fn renders_main_titlebar_focused_and_unfocused_rows() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let titlebar = surface_from_xpm(skin.get(SkinPixmapKind::Titlebar).unwrap()).unwrap();

        let mut focused_dest =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let focused_cr = Context::new(&focused_dest).unwrap();
        assert!(render_main_titlebar(&focused_cr, &skin, true, false).unwrap());
        drop(focused_cr);
        focused_dest.flush();

        let mut unfocused_dest =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let unfocused_cr = Context::new(&unfocused_dest).unwrap();
        assert!(render_main_titlebar(&unfocused_cr, &skin, false, false).unwrap());
        drop(unfocused_cr);
        unfocused_dest.flush();

        let mut titlebar = titlebar;
        let titlebar_stride = titlebar.stride() as usize;
        let titlebar_data = titlebar.data().unwrap();
        let focused_source_offset = titlebar_stride * 0 + 27 * 4;
        let unfocused_source_offset = titlebar_stride * 15 + 27 * 4;
        let expected_focused = u32::from_ne_bytes(
            titlebar_data[focused_source_offset..focused_source_offset + 4]
                .try_into()
                .unwrap(),
        );
        let expected_unfocused = u32::from_ne_bytes(
            titlebar_data[unfocused_source_offset..unfocused_source_offset + 4]
                .try_into()
                .unwrap(),
        );

        let focused_data = focused_dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(focused_data[0..4].try_into().unwrap()),
            expected_focused
        );
        drop(focused_data);

        let unfocused_data = unfocused_dest.data().unwrap();
        assert_eq!(
            u32::from_ne_bytes(unfocused_data[0..4].try_into().unwrap()),
            expected_unfocused
        );
    }

    #[test]
    fn renders_normal_and_windowshade_main_player_backgrounds() {
        let skin = DefaultSkin::load_bundled().unwrap();

        let mut normal =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT).unwrap();
        let normal_cr = Context::new(&normal).unwrap();
        assert!(render_main_player(&normal_cr, &skin, true, false).unwrap());
        drop(normal_cr);
        normal.flush();

        let mut shaded =
            ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT).unwrap();
        let shaded_cr = Context::new(&shaded).unwrap();
        assert!(render_main_player(&shaded_cr, &skin, true, true).unwrap());
        drop(shaded_cr);
        shaded.flush();

        assert_eq!(main_window_height(false), MAIN_WINDOW_HEIGHT);
        assert_eq!(main_window_height(true), MAIN_TITLEBAR_HEIGHT);

        let normal_data = normal.data().unwrap();
        let shaded_data = shaded.data().unwrap();
        assert_ne!(u32::from_ne_bytes(normal_data[0..4].try_into().unwrap()), 0);
        assert_ne!(u32::from_ne_bytes(shaded_data[0..4].try_into().unwrap()), 0);
    }

    #[test]
    fn computes_and_renders_docked_panel_composition() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let state = DockedPanelState {
            equalizer_visible: true,
            playlist_visible: true,
            ..DockedPanelState::default()
        };
        let (width, height) = docked_panel_size(state);
        assert_eq!(width, MAIN_WINDOW_WIDTH);
        assert_eq!(
            height,
            MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT + PLAYLIST_DEFAULT_HEIGHT
        );

        let mut surface = ImageSurface::create(Format::ARgb32, width, height).unwrap();
        let cr = Context::new(&surface).unwrap();
        assert!(render_docked_panels(&cr, &skin, state).unwrap());
        drop(cr);
        surface.flush();

        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let main_pixel = u32::from_ne_bytes(data[0..4].try_into().unwrap());
        let playlist_offset = stride * (MAIN_WINDOW_HEIGHT + EQUALIZER_WINDOW_HEIGHT) as usize;
        let playlist_pixel = u32::from_ne_bytes(
            data[playlist_offset..playlist_offset + 4]
                .try_into()
                .unwrap(),
        );
        assert_ne!(main_pixel, 0);
        assert_ne!(playlist_pixel, 0);
    }

    #[test]
    fn docked_panel_size_ignores_detached_panels() {
        let state = DockedPanelState {
            equalizer_visible: true,
            equalizer_detached: true,
            playlist_visible: true,
            playlist_detached: true,
            ..DockedPanelState::default()
        };

        assert_eq!(
            docked_panel_size(state),
            (MAIN_WINDOW_WIDTH, MAIN_WINDOW_HEIGHT)
        );
    }
}
