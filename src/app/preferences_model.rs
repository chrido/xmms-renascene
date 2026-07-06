//! Frontend-neutral preferences helpers.

use crate::config::Config;

pub fn normalize_preferences_config(config: &mut Config) {
    config.scale_factor = clamped_scale_factor(config.scale_factor);
    config.doublesize = config.scale_factor > 1.0;
}

pub fn clamped_scale_factor(scale_factor: f64) -> f64 {
    scale_factor.clamp(1.0, 5.0)
}

pub fn set_scale_factor(config: &mut Config, scale_factor: f64) {
    config.scale_factor = clamped_scale_factor(scale_factor);
    config.doublesize = config.scale_factor > 1.0;
}

pub fn normalize_title_format(format: &str) -> String {
    if format.trim().is_empty() {
        "%p - %t".to_string()
    } else {
        format.to_string()
    }
}

pub fn title_format_preview(config: &Config) -> String {
    crate::app::view_model::format_title_for_preferences(
        &config.title_format,
        "file:///tmp/Example_Artist%20-%20Example_Title.mp3",
        "Example Artist - Example Title",
        config,
    )
}

pub fn mirror_live_preferences_fields(target: &mut Config, live: &Config) {
    target.playlist_visible = live.playlist_visible;
    target.equalizer_visible = live.equalizer_visible;
    target.playlist_detached = live.playlist_detached;
    target.equalizer_detached = live.equalizer_detached;
    target.playlist_shaded = live.playlist_shaded;
    target.equalizer_shaded = live.equalizer_shaded;
    target.repeat = live.repeat;
    target.shuffle = live.shuffle;
    target.volume = live.volume;
    target.balance = live.balance;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_config_clamps_scale_and_updates_doublesize() {
        let mut config = Config {
            scale_factor: 0.25,
            doublesize: true,
            ..Config::default()
        };
        normalize_preferences_config(&mut config);
        assert_eq!(config.scale_factor, 1.0);
        assert!(!config.doublesize);

        set_scale_factor(&mut config, 1.25);
        assert_eq!(config.scale_factor, 1.25);
        assert!(config.doublesize);
    }

    #[test]
    fn preferences_title_preview_uses_shared_title_formatter() {
        let config = Config {
            title_format: "%t (%p)".to_string(),
            ..Config::default()
        };
        assert_eq!(
            title_format_preview(&config),
            "Example Title (Example Artist)"
        );
        assert_eq!(normalize_title_format(""), "%p - %t");
        assert_eq!(normalize_title_format("%t"), "%t");
    }

    #[test]
    fn mirrors_live_panel_and_playback_fields() {
        let mut target = Config::default();
        let live = Config {
            playlist_visible: true,
            equalizer_visible: true,
            playlist_detached: true,
            equalizer_detached: true,
            playlist_shaded: true,
            equalizer_shaded: true,
            repeat: true,
            shuffle: true,
            volume: 42,
            balance: -12,
            title_format: "%t".to_string(),
            ..Config::default()
        };

        mirror_live_preferences_fields(&mut target, &live);

        assert!(target.playlist_visible);
        assert!(target.equalizer_visible);
        assert!(target.playlist_detached);
        assert!(target.equalizer_detached);
        assert!(target.playlist_shaded);
        assert!(target.equalizer_shaded);
        assert!(target.repeat);
        assert!(target.shuffle);
        assert_eq!(target.volume, 42);
        assert_eq!(target.balance, -12);
        assert_ne!(target.title_format, live.title_format);
    }
}
