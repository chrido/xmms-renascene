//! Frontend-neutral preview/startup option handling.
//!
//! Preview options are shared by the CLI/session layer and concrete frontends.

use crate::app::screenshot_scenarios::ScreenshotScenario;
use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrontendKind {
    #[default]
    Gtk,
    Egui,
}

impl FrontendKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "gtk" => Ok(Self::Gtk),
            "egui" => Ok(Self::Egui),
            other => Err(format!(
                "unknown frontend '{other}', expected 'gtk' or 'egui'"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreviewOptions {
    pub show_playlist: bool,
    pub show_equalizer: bool,
    pub main_shaded: Option<bool>,
    pub playlist_shaded: Option<bool>,
    pub equalizer_shaded: Option<bool>,
    pub playlist_detached: Option<bool>,
    pub equalizer_detached: Option<bool>,
    pub playlist_size: Option<(i32, i32)>,
    pub reset: bool,
    pub open_preferences: bool,
    pub open_skin_editor: bool,
    pub skin_path: Option<String>,
    pub screenshot_path: Option<String>,
    pub screenshot_scenario: Option<ScreenshotScenario>,
    pub scale_factor: Option<String>,
    pub socket_port: Option<u16>,
    pub positional_paths: Vec<String>,
    pub frontend: FrontendKind,
}

pub fn apply_preview_options_to_config(
    config: &mut Config,
    options: &PreviewOptions,
) -> Result<(), String> {
    if options.show_playlist || options.playlist_size.is_some() {
        config.playlist_visible = true;
    }
    if options.show_equalizer {
        config.equalizer_visible = true;
    }
    if let Some(shaded) = options.main_shaded {
        config.main_shaded = shaded;
    }
    if let Some(shaded) = options.playlist_shaded {
        config.playlist_shaded = shaded;
    }
    if let Some(shaded) = options.equalizer_shaded {
        config.equalizer_shaded = shaded;
    }
    if let Some(detached) = options.playlist_detached {
        config.playlist_detached = detached;
    }
    if let Some(detached) = options.equalizer_detached {
        config.equalizer_detached = detached;
    }
    if let Some(skin_path) = options.skin_path.as_ref() {
        config.skin = Some(skin_path.clone());
    }
    if let Some(scale_factor) = options.scale_factor.as_ref() {
        config.scale_factor = scale_factor
            .parse::<f64>()
            .map_err(|_| format!("invalid scale factor '{scale_factor}'"))?
            .clamp(1.0, 5.0);
        config.doublesize = config.scale_factor > 1.0;
    }
    Ok(())
}
