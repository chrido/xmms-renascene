//! egui screenshot capture helpers.

use std::path::Path;

use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app_state::AppState;
use crate::render::MainWindowRenderState;
use crate::skin::DefaultSkin;

use super::skin_texture::render_main_player_color_image;

pub fn write_egui_screenshot(options: PreviewOptions, path: &Path) -> Result<(), String> {
    let mut app_state = AppState::default();
    apply_preview_options_to_config(&mut app_state.config, &options)?;
    let skin = match app_state.config.skin.as_deref() {
        Some(path) => DefaultSkin::load_from_path(Path::new(path))
            .map_err(|err| format!("failed to load skin '{}': {err}", path))?,
        None => DefaultSkin::load_bundled().map_err(|err| format!("failed to load bundled skin: {err}"))?,
    };
    let render_state = MainWindowRenderState {
        shaded: app_state.config.main_shaded,
        ..MainWindowRenderState::default()
    };
    let image = render_main_player_color_image(&skin, &render_state)
        .map_err(|err| format!("failed to render egui screenshot: {err}"))?;
    let width = image.size[0] as u32;
    let height = image.size[1] as u32;
    let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
    for pixel in image.pixels {
        bytes.extend_from_slice(&pixel.to_array());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create screenshot directory '{}': {err}", parent.display()))?;
    }
    image::save_buffer(path, &bytes, width, height, image::ColorType::Rgba8)
        .map_err(|err| format!("failed to write egui screenshot '{}': {err}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egui_screenshot_writer_creates_png() {
        let path = std::env::temp_dir().join(format!(
            "xmms-egui-screenshot-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        write_egui_screenshot(PreviewOptions::default(), &path).unwrap();

        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn screenshot_dimensions_match_main_player() {
        assert_eq!(
            (crate::render::MAIN_WINDOW_WIDTH, crate::render::MAIN_WINDOW_HEIGHT),
            (275, 116)
        );
    }
}
