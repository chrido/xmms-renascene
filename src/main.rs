use xmms_renascene::ui::{self, PreviewOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let preview_options = match parse_preview_options(&args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("xmms-rs: {err}");
            std::process::exit(2);
        }
    };

    if let Some(path) = preview_options.screenshot_path.as_deref() {
        if let Err(err) =
            ui::write_player_screenshot(preview_options.clone(), std::path::Path::new(path))
        {
            eprintln!("xmms-rs: {err}");
            std::process::exit(1);
        }
        return;
    }

    if args.iter().any(|arg| arg == "--gtk-smoke") {
        ui::run_default_skin_preview_smoke(preview_options);
    } else {
        ui::run_default_skin_preview(preview_options);
    }
}

fn parse_preview_options(args: &[String]) -> Result<PreviewOptions, String> {
    let mut options = PreviewOptions::default();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--show-playlist" || arg == "--playlist" {
            options.show_playlist = true;
        } else if arg == "--equalizer" {
            options.show_equalizer = true;
        } else if arg == "--shade" || arg == "--main-shaded" || arg == "--shade-main" {
            options.main_shaded = Some(true);
        } else if arg == "--unshade-main" {
            options.main_shaded = Some(false);
        } else if arg == "--playlist-shaded" || arg == "--shade-playlist" {
            options.show_playlist = true;
            options.playlist_shaded = Some(true);
        } else if arg == "--unshade-playlist" {
            options.playlist_shaded = Some(false);
        } else if arg == "--equalizer-shaded" || arg == "--shade-equalizer" {
            options.show_equalizer = true;
            options.equalizer_shaded = Some(true);
        } else if arg == "--unshade-equalizer" {
            options.equalizer_shaded = Some(false);
        } else if arg == "--playlist-undocked" || arg == "--undock-playlist" {
            options.show_playlist = true;
            options.playlist_detached = Some(true);
        } else if arg == "--playlist-docked" || arg == "--dock-playlist" {
            options.show_playlist = true;
            options.playlist_detached = Some(false);
        } else if arg == "--equalizer-undocked" || arg == "--undock-equalizer" {
            options.show_equalizer = true;
            options.equalizer_detached = Some(true);
        } else if arg == "--equalizer-docked" || arg == "--dock-equalizer" {
            options.show_equalizer = true;
            options.equalizer_detached = Some(false);
        } else if matches!(
            arg.as_str(),
            "--playlist-menu-add"
                | "--playlist-menu-remove"
                | "--playlist-menu-select"
                | "--playlist-menu-misc"
                | "--playlist-menu-list"
        ) {
            options.show_playlist = true;
        } else if arg == "--reset" {
            options.reset = true;
        } else if arg == "--preferences" || arg == "--open-preferences" {
            options.open_preferences = true;
        } else if arg == "--skin-editor" || arg == "--open-skin-editor" {
            options.open_skin_editor = true;
        } else if let Some(value) = arg.strip_prefix("--skin=") {
            options.skin_path = Some(value.to_string());
        } else if arg == "--skin" {
            let Some(value) = iter.next() else {
                return Err("--skin requires PATH".to_string());
            };
            options.skin_path = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--screenshot=") {
            options.screenshot_path = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--scale=") {
            options.scale_factor = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--scale-factor=") {
            options.scale_factor = Some(value.to_string());
        } else if arg == "--screenshot" {
            let Some(value) = iter.next() else {
                return Err("--screenshot requires PATH".to_string());
            };
            options.screenshot_path = Some(value.to_string());
        } else if arg == "--scale" || arg == "--scale-factor" {
            let Some(value) = iter.next() else {
                return Err(format!("{arg} requires SCALE"));
            };
            options.scale_factor = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--playlist-size=") {
            options.playlist_size = Some(parse_playlist_size(value)?);
            options.show_playlist = true;
        } else if arg == "--playlist-size" {
            let Some(value) = iter.next() else {
                return Err("--playlist-size requires WIDTHxHEIGHT".to_string());
            };
            options.playlist_size = Some(parse_playlist_size(value)?);
            options.show_playlist = true;
        }
    }
    Ok(options)
}

fn parse_playlist_size(value: &str) -> Result<(i32, i32), String> {
    let Some((width, height)) = value.split_once('x').or_else(|| value.split_once('X')) else {
        return Err(format!(
            "invalid playlist size '{value}', expected WIDTHxHEIGHT"
        ));
    };
    let width = width
        .parse::<i32>()
        .map_err(|_| format!("invalid playlist width in '{value}'"))?;
    let height = height
        .parse::<i32>()
        .map_err(|_| format!("invalid playlist height in '{value}'"))?;
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(args: &[&str]) -> Vec<String> {
        std::iter::once("xmms-rs")
            .chain(args.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn parses_playlist_size_preview_option() {
        let options = parse_preview_options(&args(&["--gtk", "--playlist-size=325x280"])).unwrap();

        assert_eq!(options.playlist_size, Some((325, 280)));
        assert!(options.show_playlist);
    }

    #[test]
    fn rejects_malformed_playlist_size() {
        assert!(parse_preview_options(&args(&["--gtk", "--playlist-size=bad"])).is_err());
    }

    #[test]
    fn parses_session_style_startup_flags() {
        let options = parse_preview_options(&args(&[
            "--gtk",
            "--playlist",
            "--equalizer",
            "--shade",
            "--playlist-shaded",
            "--equalizer-shaded",
            "--playlist-undocked",
            "--equalizer-undocked",
            "--reset",
            "--open-preferences",
            "--skin-editor",
            "--skin",
            "/tmp/skin.wsz",
            "--screenshot",
            "/tmp/player.png",
        ]))
        .unwrap();

        assert!(options.show_playlist);
        assert!(options.show_equalizer);
        assert_eq!(options.main_shaded, Some(true));
        assert_eq!(options.playlist_shaded, Some(true));
        assert_eq!(options.equalizer_shaded, Some(true));
        assert_eq!(options.playlist_detached, Some(true));
        assert_eq!(options.equalizer_detached, Some(true));
        assert!(options.reset);
        assert!(options.open_preferences);
        assert!(options.open_skin_editor);
        assert_eq!(options.skin_path.as_deref(), Some("/tmp/skin.wsz"));
        assert_eq!(options.screenshot_path.as_deref(), Some("/tmp/player.png"));
        assert_eq!(options.scale_factor, None);
    }

    #[test]
    fn parses_scale_factor_preview_option() {
        let options = parse_preview_options(&args(&["--gtk", "--scale=1.7"])).unwrap();
        assert_eq!(options.scale_factor.as_deref(), Some("1.7"));
    }
}
