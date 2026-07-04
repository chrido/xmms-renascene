use std::process::Command;

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
    let output = Command::new("./repo")
        .arg("frontend-screenshot-diff-self-test")
        .output()
        .expect("repo tool should run");

    assert!(
        output.status.success(),
        "repo frontend screenshot diff self-test failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
