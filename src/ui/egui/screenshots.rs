//! egui screenshot capture helpers.

use std::path::Path;

use cairo::{Context, Format, ImageSurface};

use crate::app::preview::{apply_preview_options_to_config, PreviewOptions};
use crate::app::view_model::{
    balance_to_eq_shaded_position, balance_to_position, ellipsize_chars, format_duration,
    formatted_current_title as shared_formatted_current_title, formatted_playlist_entry_title,
    playlist_footer_info as shared_playlist_footer_info,
    playlist_rows_render_state as shared_playlist_rows_render_state, volume_to_eq_shaded_position,
    volume_to_position,
};
use crate::app_state::AppState;
use crate::render::{
    docked_panel_size, equalizer_window_height, main_window_height, render_equalizer_state,
    render_main_player_state, render_playlist_frame, render_playlist_rows, DockedPanelState,
    EqualizerRenderState, MainWindowRenderState, RenderPass, PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH,
};
use crate::skin::widget::PlayStatusValue;
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
        equalizer_focused: false,
        equalizer_shaded: app_state.config.equalizer_shaded,
        playlist_visible: app_state.config.playlist_visible,
        playlist_detached: app_state.config.playlist_detached,
        playlist_focused: false,
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
                title: screenshot_current_title(app_state),
                bitrate_text: screenshot_bitrate_text(app_state),
                frequency_text: screenshot_frequency_text(app_state),
                play_status: match app_state.player.state() {
                    crate::player::PlayerState::Playing => PlayStatusValue::Playing,
                    crate::player::PlayerState::Paused => PlayStatusValue::Paused,
                    crate::player::PlayerState::Stopped => PlayStatusValue::Stopped,
                },
                channels: app_state.player.channels(),
                volume_position: volume_to_position(app_state.player.volume()),
                balance_position: balance_to_position(app_state.player.balance()),
                equalizer_selected: docked_state.equalizer_visible,
                playlist_selected: docked_state.playlist_visible,
                shuffle_selected: app_state.playlist.shuffle(),
                repeat_selected: app_state.playlist.repeat(),
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
            let shaded_info =
                screenshot_shaded_playlist_info(app_state, docked_state.playlist_width);
            let footer_info = shared_playlist_footer_info(app_state);
            let (footer_min, footer_sec) = screenshot_playlist_footer_time_parts(app_state);
            rendered |= render_playlist_frame(
                cr,
                skin,
                docked_state.playlist_focused,
                docked_state.playlist_shaded,
                docked_state.playlist_width,
                docked_state.playlist_height,
                Some(&shaded_info),
                Some(&footer_info),
                Some(&footer_min),
                Some(&footer_sec),
            )?;
        }
        if !docked_state.playlist_shaded {
            let rows = shared_playlist_rows_render_state(
                app_state,
                0,
                false,
                None,
                docked_state.playlist_width,
                docked_state.playlist_height,
            );
            rendered |= render_playlist_rows(cr, skin, &rows, pass)?;
        }
        cr.restore()?;
    }

    Ok(rendered)
}

fn screenshot_current_title(app_state: &AppState) -> String {
    shared_formatted_current_title(app_state)
}

fn screenshot_bitrate_text(app_state: &AppState) -> String {
    let bitrate = app_state.player.bitrate();
    if bitrate <= 0 {
        return "   ".to_string();
    }
    if bitrate < 1000 {
        format!("{bitrate:>3}")
    } else {
        format!("{:>2}H", bitrate / 100)
    }
}

fn screenshot_frequency_text(app_state: &AppState) -> String {
    let frequency = app_state.player.frequency();
    if frequency <= 0 {
        return "  ".to_string();
    }
    let khz = if frequency >= 1000 {
        (frequency + 500) / 1000
    } else {
        frequency
    };
    format!("{khz:>2}")
}

fn screenshot_playlist_footer_time_parts(app_state: &AppState) -> (String, String) {
    if app_state.player.state() == crate::player::PlayerState::Stopped {
        return ("   ".to_string(), "  ".to_string());
    }
    let total_seconds = app_state.config.playback_position_ms.max(0) / 1_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    (format!("{minutes:>3}"), format!("{seconds:02}"))
}

fn screenshot_shaded_playlist_info(app_state: &AppState, width: i32) -> String {
    let Some(position) = app_state.playlist.position() else {
        return String::new();
    };
    let Some(entry) = app_state.playlist.entries().get(position) else {
        return String::new();
    };

    let title = formatted_playlist_entry_title(app_state, entry);
    let prefix = if app_state.config.show_numbers_in_pl {
        format!("{}. ", position + 1)
    } else {
        String::new()
    };
    let suffix = if entry.length_ms >= 0 {
        format!(" {}", format_duration(entry.length_ms))
    } else {
        String::new()
    };
    let max_len = ((width - 35) / 5)
        .saturating_sub(prefix.len() as i32)
        .saturating_sub(suffix.len() as i32)
        .max(0) as usize;
    let title = ellipsize_chars(&title, max_len);
    format!("{prefix}{title:<max_len$}{suffix}")
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
