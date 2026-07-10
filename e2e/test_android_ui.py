"""Android emulator UI and screenshot E2E coverage."""

from __future__ import annotations

from importlib import import_module
from typing import Any

from android import AndroidDevice
from gui import MAIN_BUTTON_RECTS, MAIN_TOGGLE_RECTS, MainButton, MainToggleButton

pytest: Any = import_module("pytest")

pytestmark = pytest.mark.android


def test_android_portrait_player_controls_and_panels(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    initial = android_device.screenshot(test_output.screenshot_path())

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER])
    equalizer = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST])
    playlist = android_device.screenshot(test_output.screenshot_path())

    android_device.assert_log_contains(
        "player: button activated, button_name=Play",
        "player: toggle activated, toggle_name=Equalizer",
        "player: toggle activated, toggle_name=Playlist",
    )
    assert initial.read_bytes() != equalizer.read_bytes()
    assert equalizer.read_bytes() != playlist.read_bytes()


def test_android_touching_player_closes_main_menu(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU], player_bounds)
    menu_open = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    menu_closed = android_device.screenshot(test_output.screenshot_path())

    android_device.assert_log_contains(
        "command Ui(SetMainMenuVisible(true))",
        "player: toggle activated, toggle_name=Shuffle",
        "command Ui(SetMainMenuVisible(false))",
    )
    assert menu_open.read_bytes() != menu_closed.read_bytes()


def test_android_landscape_uses_full_height_and_accepts_skin_taps(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_landscape()
    android_device.restart_app(reset_data=True)
    geometry = android_device.display_geometry()
    scale = android_device.main_player_scale()

    assert geometry.width > geometry.height
    default_player_column_height = 116 * 2
    assert scale * default_player_column_height >= geometry.usable_height * 0.85

    before = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.REPEAT])
    after = android_device.screenshot(test_output.screenshot_path())

    android_device.assert_log_contains(
        "player: toggle activated, toggle_name=Repeat",
    )
    assert before.read_bytes() != after.read_bytes()


def test_android_persists_player_configuration(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    android_device.wait_for_private_file_contains(config_path, "shuffle=true")

    android_device.restart_app()
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )

    android_device.assert_log_contains(
        "player: toggle activated, toggle_name=Shuffle",
    )
    android_device.wait_for_private_file_contains(config_path, "shuffle=false")
