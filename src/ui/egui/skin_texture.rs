//! Cairo/image to egui texture helpers.

use cairo::{Context, Format, ImageSurface};

use crate::render::{
    equalizer_window_height, playlist_window_height, render_equalizer_state,
    render_main_player_state, render_playlist_frame, render_playlist_menu, render_playlist_rows,
    EqualizerRenderState, MainWindowRenderState, PlaylistMenuRenderState, PlaylistRowsRenderState,
    RenderError, RenderPass, EQUALIZER_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT, MAIN_WINDOW_HEIGHT,
    MAIN_WINDOW_WIDTH,
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

pub fn render_playlist_color_image(
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
    width: i32,
    height: i32,
    rows: &PlaylistRowsRenderState,
    footer_info: Option<&str>,
    footer_time_minutes: Option<&str>,
    footer_time_seconds: Option<&str>,
) -> Result<egui::ColorImage, RenderError> {
    let render_height = playlist_window_height(shaded, height);
    let mut surface = ImageSurface::create(Format::ARgb32, width, render_height)?;
    let cr = Context::new(&surface)?;
    render_playlist_frame(
        &cr,
        skin,
        focused,
        shaded,
        width,
        height,
        None,
        footer_info,
        footer_time_minutes,
        footer_time_seconds,
    )?;
    if !shaded {
        render_playlist_rows(&cr, skin, rows, RenderPass::Bitmap)?;
        render_playlist_rows(&cr, skin, rows, RenderPass::Text)?;
    }
    drop(cr);
    cairo_surface_to_color_image(&mut surface)
}

pub fn render_playlist_menu_color_image(
    skin: &DefaultSkin,
    state: PlaylistMenuRenderState,
    width: i32,
    height: i32,
) -> Result<egui::ColorImage, RenderError> {
    let mut surface = ImageSurface::create(Format::ARgb32, width, height)?;
    let cr = Context::new(&surface)?;
    render_playlist_menu(&cr, skin, state)?;
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
        )
        .unwrap();

        assert_eq!(image.size, [25, 54]);
    }
}
