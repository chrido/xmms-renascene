use std::process::Command;

#[cfg(all(feature = "gtk-ui", feature = "egui-ui"))]
use xmms_renascene::app::preview::PreviewOptions;
use xmms_renascene::app::screenshot_scenarios::ScreenshotScenario;

#[test]
fn screenshot_scenarios_cover_first_egui_milestone_states() {
    let names: Vec<_> = ScreenshotScenario::all()
        .iter()
        .map(|scenario| scenario.name())
        .collect();

    assert!(names.contains(&"main-player-default"));
    assert!(names.contains(&"main-player-shaded"));
    assert!(names.contains(&"playlist-with-selection"));
    assert!(names.contains(&"equalizer-non-default"));
    assert!(names.contains(&"preferences-default"));
}

#[test]
fn repo_tool_screenshot_diff_self_test_passes() {
    let output = match Command::new("./repo")
        .arg("frontend-screenshot-diff-self-test")
        .output()
    {
        Ok(output) => output,
        Err(err) => panic!("repo tool should run: {err}"),
    };

    assert!(
        output.status.success(),
        "repo frontend screenshot diff self-test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(all(feature = "gtk-ui", feature = "egui-ui"))]
#[test]
fn offscreen_gtk_and_egui_docked_screenshots_match_pixel_for_pixel() {
    let output_dir = std::env::temp_dir().join(format!(
        "xmms-rs-frontend-docking-parity-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&output_dir);
    if let Err(err) = std::fs::create_dir_all(&output_dir) {
        panic!("failed to create screenshot parity directory: {err}");
    }

    let cases = [
        (
            "main-player-default",
            PreviewOptions {
                screenshot_scenario: Some(ScreenshotScenario::MainPlayerDefault),
                ..PreviewOptions::default()
            },
        ),
        (
            "main-player-shaded",
            PreviewOptions {
                main_shaded: Some(true),
                screenshot_scenario: Some(ScreenshotScenario::MainPlayerShaded),
                ..PreviewOptions::default()
            },
        ),
        (
            "docked-playlist",
            PreviewOptions {
                show_playlist: true,
                screenshot_scenario: Some(ScreenshotScenario::PlaylistWithSelection),
                ..PreviewOptions::default()
            },
        ),
        (
            "resized-docked-playlist",
            PreviewOptions {
                show_playlist: true,
                playlist_size: Some((325, 290)),
                screenshot_scenario: Some(ScreenshotScenario::PlaylistWithSelection),
                ..PreviewOptions::default()
            },
        ),
        (
            "docked-equalizer",
            PreviewOptions {
                show_equalizer: true,
                screenshot_scenario: Some(ScreenshotScenario::EqualizerNonDefault),
                ..PreviewOptions::default()
            },
        ),
        (
            "docked-equalizer-and-playlist",
            PreviewOptions {
                show_equalizer: true,
                show_playlist: true,
                screenshot_scenario: Some(ScreenshotScenario::PlaylistWithSelection),
                ..PreviewOptions::default()
            },
        ),
    ];

    for (name, options) in cases {
        let gtk_path = output_dir.join(format!("gtk-{name}.png"));
        let egui_path = output_dir.join(format!("egui-{name}.png"));
        if let Err(err) = xmms_renascene::ui::write_player_screenshot(options.clone(), &gtk_path) {
            panic!("failed to write GTK screenshot for {name}: {err}");
        }
        if let Err(err) =
            xmms_renascene::egui_frontend::screenshots::write_egui_screenshot(options, &egui_path)
        {
            panic!("failed to write egui screenshot for {name}: {err}");
        }

        let gtk = match image::open(&gtk_path) {
            Ok(image) => image.into_rgba8(),
            Err(err) => panic!("failed to read GTK screenshot for {name}: {err}"),
        };
        let egui = match image::open(&egui_path) {
            Ok(image) => image.into_rgba8(),
            Err(err) => panic!("failed to read egui screenshot for {name}: {err}"),
        };
        assert_eq!(
            gtk.dimensions(),
            egui.dimensions(),
            "{name} dimensions differ"
        );
        assert_eq!(gtk.as_raw(), egui.as_raw(), "{name} pixels differ");
    }

    let _ = std::fs::remove_dir_all(&output_dir);
}
