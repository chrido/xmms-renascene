//! Pure Rust image to egui texture helpers.

use crate::render::{
    equalizer_window_height, playlist_window_height, render_equalizer_state,
    render_main_player_state, render_playlist_frame, render_playlist_menu, render_playlist_rows,
    render_scaled, render_transport_buttons, Context, EqualizerRenderState, Format, ImageSurface,
    MainWindowRenderState, PlaylistMenuRenderState, PlaylistRowsRenderState, RenderError,
    EQUALIZER_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
};
use crate::skin::layout::MainPushButton;
use crate::skin::DefaultSkin;

pub const TRANSPORT_BUTTONS_WIDTH: i32 = 114;
pub const TRANSPORT_BUTTONS_HEIGHT: i32 = 18;
pub const PLAYER_INFO_X: usize = 104;
pub const PLAYER_INFO_Y: usize = 20;
pub const PLAYER_INFO_WIDTH: usize = 164;
pub const PLAYER_INFO_HEIGHT: usize = 37;

pub fn player_info_render_state(
    title: &str,
    bitrate: i32,
    frequency: i32,
    channels: i32,
    title_offset_px: i32,
) -> MainWindowRenderState {
    MainWindowRenderState {
        title: if title.is_empty() {
            "XMMS Renascene".to_string()
        } else {
            title.to_string()
        },
        bitrate_text: (bitrate > 0)
            .then(|| bitrate.to_string())
            .unwrap_or_default(),
        frequency_text: (frequency > 0)
            .then(|| frequency.to_string())
            .unwrap_or_default(),
        channels: channels.max(0),
        title_offset_px: title_offset_px.max(0),
        ..MainWindowRenderState::default()
    }
}

pub fn argb_to_egui_rgba(argb: u32) -> [u8; 4] {
    let [b, g, r, a] = argb.to_ne_bytes();
    [r, g, b, a]
}

pub fn surface_to_color_image(surface: &mut ImageSurface) -> Result<egui::ColorImage, RenderError> {
    let width = surface.width() as usize;
    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data()?;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = &data[y * stride..y * stride + width * 4];
        for pixel in row.chunks_exact(4) {
            let [r, g, b, a] =
                argb_to_egui_rgba(u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]));
            rgba.extend_from_slice(&[r, g, b, a]);
        }
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
        &rgba,
    ))
}

pub fn render_main_player_color_image(
    skin: &DefaultSkin,
    state: &MainWindowRenderState,
) -> Result<egui::ColorImage, RenderError> {
    let height = if state.shaded {
        MAIN_TITLEBAR_HEIGHT
    } else {
        MAIN_WINDOW_HEIGHT
    };
    let mut surface = ImageSurface::create(Format::ARgb32, MAIN_WINDOW_WIDTH, height)?;
    let cr = Context::new(&surface)?;
    render_main_player_state(&cr, skin, state)?;
    drop(cr);
    surface_to_color_image(&mut surface)
}

pub fn render_transport_buttons_color_image(
    skin: &DefaultSkin,
    pressed: Option<MainPushButton>,
) -> Result<egui::ColorImage, RenderError> {
    let mut surface = ImageSurface::create(
        Format::ARgb32,
        TRANSPORT_BUTTONS_WIDTH,
        TRANSPORT_BUTTONS_HEIGHT,
    )?;
    let cr = Context::new(&surface)?;
    render_transport_buttons(&cr, skin, pressed)?;
    drop(cr);
    surface_to_color_image(&mut surface)
}

pub fn crop_color_image(
    image: &egui::ColorImage,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> egui::ColorImage {
    assert!(x + width <= image.size[0]);
    assert!(y + height <= image.size[1]);
    let mut pixels = Vec::with_capacity(width * height);
    for row in y..y + height {
        let start = row * image.size[0] + x;
        pixels.extend_from_slice(&image.pixels[start..start + width]);
    }
    egui::ColorImage {
        size: [width, height],
        pixels,
        source_size: egui::vec2(width as f32, height as f32),
    }
}

fn flatten_color_image_onto_black(mut image: egui::ColorImage) -> egui::ColorImage {
    // Imported MAIN images can contain keyed transparency; widgets need an opaque rectangle.
    for pixel in &mut image.pixels {
        let [red, green, blue, _] = pixel.to_array();
        *pixel = egui::Color32::from_rgb(red, green, blue);
    }
    image
}

pub fn render_player_info_color_image(
    skin: &DefaultSkin,
    state: &MainWindowRenderState,
) -> Result<egui::ColorImage, RenderError> {
    let image = render_main_player_color_image(skin, state)?;
    Ok(flatten_color_image_onto_black(crop_color_image(
        &image,
        PLAYER_INFO_X,
        PLAYER_INFO_Y,
        PLAYER_INFO_WIDTH,
        PLAYER_INFO_HEIGHT,
    )))
}

pub fn render_equalizer_color_image(
    skin: &DefaultSkin,
    state: &EqualizerRenderState,
) -> Result<egui::ColorImage, RenderError> {
    let height = equalizer_window_height(state.shaded);
    let mut surface = ImageSurface::create(Format::ARgb32, EQUALIZER_WINDOW_WIDTH, height)?;
    let cr = Context::new(&surface)?;
    render_equalizer_state(&cr, skin, state)?;
    drop(cr);
    surface_to_color_image(&mut surface)
}

#[allow(clippy::too_many_arguments)]
pub fn render_playlist_color_image(
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
    width: i32,
    height: i32,
    shaded_info: Option<&str>,
    rows: &PlaylistRowsRenderState,
    footer_info: Option<&str>,
    footer_time_minutes: Option<&str>,
    footer_time_seconds: Option<&str>,
    scale: f64,
) -> Result<egui::ColorImage, RenderError> {
    let render_height = playlist_window_height(shaded, height);
    // Render at the final device resolution so the vector song-name font is
    // rasterised crisply (like GTK), instead of rendering at 1x and letting
    // egui upscale the anti-aliased text into a blurry texture. render_scaled
    // nearest-scales the skin bitmap layer and draws the text pass at device
    // resolution, matching the GTK playlist draw path exactly.
    let scale = scale.max(1.0);
    let device_width = ((width as f64) * scale).round().max(1.0) as i32;
    let device_height = ((render_height as f64) * scale).round().max(1.0) as i32;
    let mut surface = ImageSurface::create(Format::ARgb32, device_width, device_height)?;
    let cr = Context::new(&surface)?;
    render_scaled(
        &cr,
        device_width,
        device_height,
        width,
        render_height,
        |cr, pass| {
            if pass.is_bitmap() {
                render_playlist_frame(
                    cr,
                    skin,
                    focused,
                    shaded,
                    width,
                    height,
                    shaded_info,
                    footer_info,
                    footer_time_minutes,
                    footer_time_seconds,
                )?;
            }
            if !shaded {
                render_playlist_rows(cr, skin, rows, pass)?;
            }
            Ok(())
        },
    )?;
    drop(cr);
    surface_to_color_image(&mut surface)
}

pub fn render_playlist_menu_color_image(
    skin: &DefaultSkin,
    state: PlaylistMenuRenderState,
    width: i32,
    height: i32,
    scale: f64,
) -> Result<egui::ColorImage, RenderError> {
    let scale = scale.max(1.0);
    let device_width = ((width as f64) * scale).round().max(1.0) as i32;
    let device_height = ((height as f64) * scale).round().max(1.0) as i32;
    let mut surface = ImageSurface::create(Format::ARgb32, device_width, device_height)?;
    let cr = Context::new(&surface)?;
    render_scaled(
        &cr,
        device_width,
        device_height,
        width,
        height,
        |cr, pass| {
            if pass.is_bitmap() {
                render_playlist_menu(cr, skin, state)?;
            }
            Ok(())
        },
    )?;
    drop(cr);
    surface_to_color_image(&mut surface)
}

pub fn upload_color_image(
    ctx: &egui::Context,
    name: impl Into<String>,
    image: egui::ColorImage,
) -> egui::TextureHandle {
    ctx.load_texture(name, image, egui::TextureOptions::NEAREST)
}

/// Snap a rectangle to the physical pixel grid so a NEAREST skin texture maps
/// one texel to a whole number of device pixels. Without this, a fractional
/// `pixels_per_point` (or a sub-pixel layout origin) resamples the bitmap font
/// and it looks blurry.
pub fn pixel_snapped_rect(ctx: &egui::Context, rect: egui::Rect) -> egui::Rect {
    let ppp = ctx.pixels_per_point();
    if ppp <= 0.0 {
        return rect;
    }
    let snap = |value: f32| (value * ppp).round() / ppp;
    egui::Rect::from_min_max(
        egui::pos2(snap(rect.min.x), snap(rect.min.y)),
        egui::pos2(snap(rect.max.x), snap(rect.max.y)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_conversion_outputs_rgba() {
        let argb = u32::from_ne_bytes([3, 2, 1, 4]);
        assert_eq!(argb_to_egui_rgba(argb), [1, 2, 3, 4]);
    }

    #[test]
    fn renders_main_player_to_egui_color_image() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let image =
            render_main_player_color_image(&skin, &MainWindowRenderState::default()).unwrap();

        assert_eq!(
            image.size,
            [MAIN_WINDOW_WIDTH as usize, MAIN_WINDOW_HEIGHT as usize]
        );
    }

    #[test]
    fn transport_image_only_changes_the_pressed_button() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let normal = render_transport_buttons_color_image(&skin, None).unwrap();
        let buttons = [
            (MainPushButton::Previous, 0, 23),
            (MainPushButton::Play, 23, 46),
            (MainPushButton::Pause, 46, 69),
            (MainPushButton::Stop, 69, 92),
            (MainPushButton::Next, 92, 114),
        ];

        for (button, start_x, end_x) in buttons {
            let pressed = render_transport_buttons_color_image(&skin, Some(button)).unwrap();
            assert_eq!(pressed.size, [114, 18]);
            let changed: Vec<_> = normal
                .pixels
                .iter()
                .zip(&pressed.pixels)
                .enumerate()
                .filter_map(|(index, (normal, pressed))| (normal != pressed).then_some(index))
                .collect();
            assert!(!changed.is_empty(), "{button:?} has no pressed feedback");
            assert!(changed
                .iter()
                .all(|index| (start_x..end_x).contains(&(index % 114))));
        }
    }

    #[test]
    fn player_info_image_is_native_information_rectangle() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let state = player_info_render_state("Widget title", 192, 44, 2, 0);
        let full = render_main_player_color_image(&skin, &state).unwrap();
        let info = render_player_info_color_image(&skin, &state).unwrap();

        assert_eq!(PLAYER_INFO_X, 104);
        assert_eq!(PLAYER_INFO_Y, 20);
        assert_eq!(PLAYER_INFO_X + PLAYER_INFO_WIDTH, 268);
        assert_eq!(PLAYER_INFO_Y + PLAYER_INFO_HEIGHT, 57);
        assert_eq!(info.size, [PLAYER_INFO_WIDTH, PLAYER_INFO_HEIGHT]);
        for y in 0..PLAYER_INFO_HEIGHT {
            let full_start = (PLAYER_INFO_Y + y) * MAIN_WINDOW_WIDTH as usize + PLAYER_INFO_X;
            let info_start = y * PLAYER_INFO_WIDTH;
            assert_eq!(
                &info.pixels[info_start..info_start + PLAYER_INFO_WIDTH],
                &full.pixels[full_start..full_start + PLAYER_INFO_WIDTH]
            );
        }
    }

    #[test]
    fn player_info_image_has_no_transparent_corner_or_skin_pixels() {
        let transparent = egui::ColorImage::from_rgba_unmultiplied(
            [2, 1],
            &[255, 255, 255, 0, 200, 100, 50, 128],
        );
        let flattened = flatten_color_image_onto_black(transparent);
        assert!(flattened.pixels.iter().all(|pixel| pixel.a() == 255));
        assert_eq!(flattened.pixels[0], egui::Color32::BLACK);

        let skin = DefaultSkin::load_bundled().unwrap();
        let image = render_player_info_color_image(
            &skin,
            &player_info_render_state("Square widget", 192, 44, 2, 0),
        )
        .unwrap();
        assert!(image.pixels.iter().all(|pixel| pixel.a() == 255));
        for index in [
            0,
            PLAYER_INFO_WIDTH - 1,
            (PLAYER_INFO_HEIGHT - 1) * PLAYER_INFO_WIDTH,
            PLAYER_INFO_HEIGHT * PLAYER_INFO_WIDTH - 1,
        ] {
            assert_eq!(image.pixels[index].a(), 255);
        }
    }

    #[test]
    fn player_info_state_maps_display_fields() {
        let stereo = player_info_render_state("Track title", 192, 44, 2, 7);
        assert_eq!(stereo.title, "Track title");
        assert_eq!(stereo.bitrate_text, "192");
        assert_eq!(stereo.frequency_text, "44");
        assert_eq!(stereo.channels, 2);
        assert_eq!(stereo.title_offset_px, 7);

        let mono = player_info_render_state("", -1, 0, 1, -1);
        assert_eq!(mono.title, "XMMS Renascene");
        assert!(mono.bitrate_text.is_empty());
        assert!(mono.frequency_text.is_empty());
        assert_eq!(mono.channels, 1);
        assert_eq!(mono.title_offset_px, 0);

        let unknown = player_info_render_state("Unknown channels", 0, -1, -2, 0);
        assert_eq!(unknown.channels, 0);
    }

    #[test]
    fn player_info_image_reflects_each_display_field() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let render = |title, bitrate, frequency, channels| {
            render_player_info_color_image(
                &skin,
                &player_info_render_state(title, bitrate, frequency, channels, 0),
            )
            .unwrap()
            .pixels
        };
        let empty = render("", 0, 0, 0);

        assert_ne!(empty, render("Track title", 0, 0, 0));
        assert_ne!(empty, render("", 192, 0, 0));
        assert_ne!(empty, render("", 0, 44, 0));
        assert_ne!(empty, render("", 0, 0, 1));
        assert_ne!(empty, render("", 0, 0, 2));
        assert_ne!(render("", 0, 0, 1), render("", 0, 0, 2));
    }

    #[test]
    fn player_info_image_uses_bitmap_title_marquee_offset() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let at_start = render_player_info_color_image(
            &skin,
            &player_info_render_state("A title long enough to overflow", 192, 44, 2, 0),
        )
        .unwrap();
        let scrolled = render_player_info_color_image(
            &skin,
            &player_info_render_state("A title long enough to overflow", 192, 44, 2, 10),
        )
        .unwrap();

        assert_ne!(at_start.pixels, scrolled.pixels);
    }

    #[test]
    fn renders_playlist_menu_to_egui_color_image() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let image = render_playlist_menu_color_image(
            &skin,
            PlaylistMenuRenderState {
                kind: crate::playlist::PlaylistMenuKind::Add,
                hover: Some(1),
            },
            25,
            54,
            1.0,
        )
        .unwrap();

        assert_eq!(image.size, [25, 54]);
    }

    #[test]
    fn playlist_image_renders_at_device_resolution_when_scaled() {
        let skin = DefaultSkin::load_bundled().unwrap();
        let rows = PlaylistRowsRenderState {
            entries: Vec::new(),
            scroll_offset: 0,
            scrollbar_dragging: false,
            search_query: None,
            show_numbers: false,
            font_family: "Helvetica".to_string(),
            width: 275,
            height: 232,
        };
        let base = render_playlist_color_image(
            &skin, true, false, 275, 232, None, &rows, None, None, None, 1.0,
        )
        .unwrap();
        assert_eq!(base.size, [275, 232]);

        // At 2x zoom the image must be produced at the full device
        // resolution so the vector song-name font is rasterised crisply instead
        // of being upscaled (blurred) by egui.
        let scaled = render_playlist_color_image(
            &skin, true, false, 275, 232, None, &rows, None, None, None, 2.0,
        )
        .unwrap();
        assert_eq!(scaled.size, [550, 464]);
    }
}
