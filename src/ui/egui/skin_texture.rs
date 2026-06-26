//! Cairo/image to egui texture helpers.

use cairo::{Context, Format, ImageSurface};

use crate::render::{
    render_main_player_state, MainWindowRenderState, RenderError, MAIN_TITLEBAR_HEIGHT,
    MAIN_WINDOW_HEIGHT, MAIN_WINDOW_WIDTH,
};
use crate::skin::DefaultSkin;

pub fn cairo_argb_to_egui_rgba(argb: u32) -> [u8; 4] {
    let [b, g, r, a] = argb.to_ne_bytes();
    [r, g, b, a]
}

pub fn cairo_surface_to_color_image(surface: &mut ImageSurface) -> Result<egui::ColorImage, RenderError> {
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
    Ok(egui::ColorImage::from_rgba_unmultiplied([width, height], &rgba))
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
        let image = render_main_player_color_image(&skin, &MainWindowRenderState::default()).unwrap();

        assert_eq!(image.size, [MAIN_WINDOW_WIDTH as usize, MAIN_WINDOW_HEIGHT as usize]);
    }
}
