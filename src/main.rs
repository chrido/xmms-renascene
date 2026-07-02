use xmms_renascene::app::preview::{FrontendKind, PreviewOptions};
use xmms_renascene::app::screenshot_scenarios::ScreenshotScenario;
#[cfg(feature = "egui-ui")]
use xmms_renascene::egui_frontend;
#[cfg(feature = "gtk-ui")]
use xmms_renascene::ui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return;
    }

    let preview_options = match parse_preview_options(&args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("xmms-rs: {err}");
            std::process::exit(2);
        }
    };

    match preview_options.frontend {
        FrontendKind::Gtk => run_gtk_frontend(preview_options, &args),
        FrontendKind::Egui => run_egui_frontend(preview_options),
    }
}

#[cfg(feature = "gtk-ui")]
fn run_gtk_frontend(preview_options: PreviewOptions, args: &[String]) {
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

#[cfg(not(feature = "gtk-ui"))]
fn run_gtk_frontend(_preview_options: PreviewOptions, _args: &[String]) {
    eprintln!("xmms-rs: this binary was built without the gtk-ui frontend");
    std::process::exit(2);
}

#[cfg(feature = "egui-ui")]
fn run_egui_frontend(preview_options: PreviewOptions) {
    if let Some(path) = preview_options.screenshot_path.as_deref() {
        if let Err(err) = egui_frontend::screenshots::write_egui_screenshot(
            preview_options.clone(),
            std::path::Path::new(path),
        ) {
            eprintln!("xmms-rs: {err}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(err) = egui_frontend::app::run_egui_frontend(preview_options) {
        eprintln!("xmms-rs: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "egui-ui"))]
fn run_egui_frontend(_preview_options: PreviewOptions) {
    eprintln!("xmms-rs: this binary was built without the egui-ui frontend");
    std::process::exit(2);
}

fn print_help() {
    println!("Usage: xmms-rs [--frontend gtk|egui] [--socket PORT] [preview options]");
    println!("If --frontend is omitted, gtk is used for compatibility.");
    println!("--socket PORT starts a JSON-lines TCP control socket on 127.0.0.1:PORT.");
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_preview_options(args: &[String]) -> Result<PreviewOptions, String> {
    let mut options = PreviewOptions::default();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--frontend=") {
            options.frontend = FrontendKind::parse(value)?;
        } else if arg == "--frontend" {
            let Some(value) = iter.next() else {
                return Err("--frontend requires gtk or egui".to_string());
            };
            options.frontend = FrontendKind::parse(value)?;
        } else if arg == "--show-playlist" || arg == "--playlist" {
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
        } else if let Some(value) = arg.strip_prefix("--screenshot-scenario=") {
            options.screenshot_scenario = Some(ScreenshotScenario::parse(value)?);
        } else if arg == "--screenshot-scenario" {
            let Some(value) = iter.next() else {
                return Err("--screenshot-scenario requires NAME".to_string());
            };
            options.screenshot_scenario = Some(ScreenshotScenario::parse(value)?);
        } else if arg == "--scale" || arg == "--scale-factor" {
            let Some(value) = iter.next() else {
                return Err(format!("{arg} requires SCALE"));
            };
            options.scale_factor = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--socket=") {
            options.socket_port = Some(parse_socket_port(value)?);
        } else if arg == "--socket" {
            let Some(value) = iter.next() else {
                return Err("--socket requires PORT".to_string());
            };
            options.socket_port = Some(parse_socket_port(value)?);
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

fn parse_socket_port(value: &str) -> Result<u16, String> {
    let port = value
        .parse::<u16>()
        .map_err(|_| format!("invalid socket port '{value}'"))?;
    if port == 0 {
        return Err("--socket requires a non-zero TCP port".to_string());
    }
    Ok(port)
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

    #[test]
    fn parses_socket_preview_option() {
        let options = parse_preview_options(&args(&["--socket", "48155"])).unwrap();
        assert_eq!(options.socket_port, Some(48155));
    }

    #[test]
    fn rejects_zero_socket_port() {
        assert!(parse_preview_options(&args(&["--socket=0"])).is_err());
    }

    #[test]
    fn unspecified_frontend_defaults_to_gtk() {
        let options = parse_preview_options(&args(&[])).unwrap();
        assert_eq!(options.frontend, FrontendKind::Gtk);
    }

    #[test]
    fn parses_explicit_gtk_frontend() {
        let options = parse_preview_options(&args(&["--frontend", "gtk"])).unwrap();
        assert_eq!(options.frontend, FrontendKind::Gtk);
    }

    #[test]
    fn parses_explicit_egui_frontend() {
        let options = parse_preview_options(&args(&["--frontend=egui"])).unwrap();
        assert_eq!(options.frontend, FrontendKind::Egui);
    }

    #[test]
    fn rejects_unknown_frontend() {
        assert!(parse_preview_options(&args(&["--frontend", "qt"])).is_err());
    }
}
