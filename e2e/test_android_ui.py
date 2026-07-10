"""Android emulator UI and screenshot E2E coverage."""

from __future__ import annotations

from importlib import import_module
from typing import Any

from android import ANDROID_AUTO_PROBE_ACTIVITY, ANDROID_PACKAGE, AndroidDevice
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
    android_device.restart_app(reset_data=True)
    android_device.set_landscape()
    android_device.wait_for_app()
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


def test_android_auto_media_browser_surface(
    android_device: AndroidDevice,
) -> None:
    manifest = android_device.apk_xmltree("AndroidManifest.xml")
    automotive = android_device.apk_xmltree("res/xml/automotive_app_desc.xml")
    assert "com.google.android.gms.car.application" in manifest
    assert "android.media.browse.MediaBrowserService" in manifest
    assert 'A: name="media"' in automotive

    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:42,Android Auto Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/auto.wav\n",
    )
    android_device.clear_logcat()
    android_device.shell(
        "am",
        "start",
        "-n",
        ANDROID_AUTO_PROBE_ACTIVITY,
    )
    android_device.assert_log_contains(
        "connected root=xmms-root",
        "children parent=xmms-root count=1",
        "children parent=xmms-playlist count=1",
        "first title=Android Auto Track",
    )
