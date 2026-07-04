//! egui screenshot capture helpers.

use std::path::Path;

use cairo::{Context, Format, ImageSurface};

use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app::view_model::{
    balance_to_eq_shaded_position, balance_to_position, volume_to_eq_shaded_position,
    volume_to_position,
};
use crate::app_state::AppState;
use crate::render::{
    docked_panel_size, equalizer_window_height, main_window_height, render_equalizer_state,
    render_main_player_state, render_playlist_frame, render_playlist_rows, DockedPanelState,
    EqualizerRenderState, MainWindowRenderState, PlaylistRowsRenderState, RenderPass,
    PLAYLIST_DEFAULT_HEIGHT, PLAYLIST_DEFAULT_WIDTH,
};
use crate::skin::DefaultSkin;

use super::skin_texture::cairo_surface_to_color_image;

pub fn write_egui_screenshot(options: PreviewOptions, path: &Path) -> Result<(), String> {
    let (skin, app_state, docked_state) = screenshot_state(options)?;
    let mut surface = render_docked_screenshot_surface(&skin, &app_state, docked_state)
        .map_err(|err| format!("failed to render egui screenshot: {err}"))?;
    let image = cairo_surface_to_color_image(&mut surface)
        .map_err(|err| format!("failed to convert egui screenshot: {err}"))?;
    let width = image.size[0] as u32;
    let height = image.size[1] as u32;
    let mut bytes = Vec::with_capacity(image.pixels.len() * 4);
    for pixel in image.pixels {
        bytes.extend_from_slice(&pixel.to_array());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create screenshot directory '{}': {err}",
                parent.display()
            )
        })?;
    }
    image::save_buffer(path, &bytes, width, height, image::ColorType::Rgba8).map_err(|err| {
        format!(
            "failed to write egui screenshot '{}': {err}",
            path.display()
        )
    })?;
    Ok(())
}

fn screenshot_state(
    options: PreviewOptions,
) -> Result<(DefaultSkin, AppState, DockedPanelState), String> {
    let mut app_state = AppState::default();
    apply_preview_options_to_config(&mut app_state.config, &options)?;
    if let Some(scenario) = options.screenshot_scenario {
        scenario.apply_to_app_state(&mut app_state);
    }
    let skin = match app_state.config.skin.as_deref() {
        Some(path) => DefaultSkin::load_from_path(Path::new(path))
            .map_err(|err| format!("failed to load skin '{}': {err}", path))?,
        None => DefaultSkin::load_bundled()
            .map_err(|err| format!("failed to load bundled skin: {err}"))?,
    };
    let (playlist_width, playlist_height) = options
        .playlist_size
        .unwrap_or((PLAYLIST_DEFAULT_WIDTH, PLAYLIST_DEFAULT_HEIGHT));
    let docked_state = DockedPanelState {
        main_focused: true,
        main_shaded: app_state.config.main_shaded,
        equalizer_visible: app_state.config.equalizer_visible,
        equalizer_detached: app_state.config.equalizer_detached,
        equalizer_focused: true,
        equalizer_shaded: app_state.config.equalizer_shaded,
        playlist_visible: app_state.config.playlist_visible,
        playlist_detached: app_state.config.playlist_detached,
        playlist_focused: true,
        playlist_shaded: app_state.config.playlist_shaded,
        playlist_width,
        playlist_height,
    };
    Ok((skin, app_state, docked_state))
}

fn render_docked_screenshot_surface(
    skin: &DefaultSkin,
    app_state: &AppState,
    docked_state: DockedPanelState,
) -> Result<ImageSurface, crate::render::RenderError> {
    let (width, height) = docked_panel_size(docked_state);
    let surface = ImageSurface::create(Format::ARgb32, width, height)?;
    let cr = Context::new(&surface)?;
    render_docked_screenshot_pass(&cr, skin, app_state, docked_state, RenderPass::Bitmap)?;
    render_docked_screenshot_pass(&cr, skin, app_state, docked_state, RenderPass::Text)?;
    drop(cr);
    Ok(surface)
}

fn render_docked_screenshot_pass(
    cr: &Context,
    skin: &DefaultSkin,
    app_state: &AppState,
    docked_state: DockedPanelState,
    pass: RenderPass,
) -> Result<bool, crate::render::RenderError> {
    let mut rendered = false;
    let mut y = 0;
    if pass.is_bitmap() {
        rendered |= render_main_player_state(
            cr,
            skin,
            &MainWindowRenderState {
                focused: docked_state.main_focused,
                shaded: docked_state.main_shaded,
                volume_position: volume_to_position(app_state.player.volume()),
                balance_position: balance_to_position(app_state.player.balance()),
                equalizer_selected: docked_state.equalizer_visible,
                playlist_selected: docked_state.playlist_visible,
                ..MainWindowRenderState::default()
            },
        )?;
    }
    y += main_window_height(docked_state.main_shaded);

    if docked_state.equalizer_visible && !docked_state.equalizer_detached {
        if pass.is_bitmap() {
            cr.save()?;
            cr.translate(0.0, f64::from(y));
            rendered |= render_equalizer_state(
                cr,
                skin,
                &EqualizerRenderState {
                    focused: docked_state.equalizer_focused,
                    shaded: docked_state.equalizer_shaded,
                    active: app_state.config.equalizer_active,
                    automatic: app_state.config.equalizer_auto,
                    preamp_position: app_state.config.equalizer_preamp_pos,
                    band_positions: app_state.config.equalizer_band_pos,
                    volume_position: volume_to_eq_shaded_position(app_state.player.volume()),
                    balance_position: balance_to_eq_shaded_position(app_state.player.balance()),
                    ..EqualizerRenderState::default()
                },
            )?;
            cr.restore()?;
        }
        y += equalizer_window_height(docked_state.equalizer_shaded);
    }

    if docked_state.playlist_visible && !docked_state.playlist_detached {
        cr.save()?;
        cr.translate(0.0, f64::from(y));
        if pass.is_bitmap() {
            rendered |= render_playlist_frame(
                cr,
                skin,
                docked_state.playlist_focused,
                docked_state.playlist_shaded,
                docked_state.playlist_width,
                docked_state.playlist_height,
                None,
                None,
                None,
                None,
            )?;
        }
        if !docked_state.playlist_shaded {
            rendered |= render_playlist_rows(
                cr,
                skin,
                &PlaylistRowsRenderState {
                    entries: app_state
                        .playlist
                        .entries()
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| crate::render::PlaylistRowRenderEntry {
                            title: entry.title.clone(),
                            length_ms: entry.length_ms,
                            selected: entry.selected,
                            current: app_state.playlist.position() == Some(index),
                        })
                        .collect(),
                    scroll_offset: 0,
                    scrollbar_dragging: false,
                    search_query: None,
                    show_numbers: app_state.config.show_numbers_in_pl,
                    font_family: app_state.config.playlist_font.clone(),
                    width: docked_state.playlist_width,
                    height: docked_state.playlist_height,
                },
                pass,
            )?;
        }
        cr.restore()?;
    }

    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egui_screenshot_writer_creates_png() {
        let path =
            std::env::temp_dir().join(format!("xmms-egui-screenshot-{}.png", std::process::id()));
        let _ = std::fs::remove_file(&path);

        write_egui_screenshot(PreviewOptions::default(), &path).unwrap();

        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().len() > 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn screenshot_dimensions_match_main_player() {
        let (skin, app_state, docked_state) = screenshot_state(PreviewOptions::default()).unwrap();
        let surface = render_docked_screenshot_surface(&skin, &app_state, docked_state).unwrap();
        assert_eq!(
            (surface.width(), surface.height()),
            (
                crate::render::MAIN_WINDOW_WIDTH,
                crate::render::MAIN_WINDOW_HEIGHT
            )
        );
    }

    #[test]
    fn screenshot_dimensions_include_docked_playlist_and_equalizer() {
        let options = PreviewOptions {
            show_playlist: true,
            show_equalizer: true,
            ..PreviewOptions::default()
        };
        let (skin, app_state, docked_state) = screenshot_state(options).unwrap();
        let surface = render_docked_screenshot_surface(&skin, &app_state, docked_state).unwrap();
        assert_eq!(
            (surface.width(), surface.height()),
            (
                crate::render::MAIN_WINDOW_WIDTH,
                crate::render::MAIN_WINDOW_HEIGHT
                    + crate::render::EQUALIZER_WINDOW_HEIGHT
                    + PLAYLIST_DEFAULT_HEIGHT,
            )
        );
    }
}
