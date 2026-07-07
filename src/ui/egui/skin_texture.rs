//! Cairo/image to egui texture helpers.

use cairo::{Context, Format, ImageSurface};

use crate::render::{
    equalizer_window_height, playlist_window_height, render_equalizer_state,
    render_main_player_state, render_playlist_frame, render_playlist_menu, render_playlist_rows,
    render_scaled, EqualizerRenderState, MainWindowRenderState, PlaylistMenuRenderState,
    PlaylistRowsRenderState, RenderError, EQUALIZER_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT,
    MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
};
use crate::skin::DefaultSkin;

pub fn cairo_argb_to_egui_rgba(argb: u32) -> [u8; 4] {
    let [b, g, r, a] = argb.to_ne_bytes();
    [r, g, b, a]
}

pub fn cairo_surface_to_color_image(
    surface: &mut ImageSurface,
) -> Result<egui::ColorImage, RenderError> {
    let width = surface.width() as usize;
    let height = surface.height() as usize;
    let stride = surface.stride() as usize;
    let data = surface.data()?;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = &data[y * stride..y * stride + width * 4];
        for pixel in row.chunks_exact(4) {
            let [r, g, b, a] = cairo_argb_to_egui_rgba(u32::from_ne_bytes([
                pixel[0], pixel[1], pixel[2], pixel[3],
            ]));
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
    cairo_surface_to_color_image(&mut surface)
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
    cairo_surface_to_color_image(&mut surface)
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
    cairo_surface_to_color_image(&mut surface)
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
    cairo_surface_to_color_image(&mut surface)
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
    fn cairo_argb_conversion_outputs_rgba() {
        let argb = u32::from_ne_bytes([3, 2, 1, 4]);
        assert_eq!(cairo_argb_to_egui_rgba(argb), [1, 2, 3, 4]);
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

        // At 2x zoom the cairo image must be produced at the full device
        // resolution so the vector song-name font is rasterised crisply instead
        // of being upscaled (blurred) by egui.
        let scaled = render_playlist_color_image(
            &skin, true, false, 275, 232, None, &rows, None, None, None, 2.0,
        )
        .unwrap();
        assert_eq!(scaled.size, [550, 464]);
    }
}
