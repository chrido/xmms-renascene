"""Android emulator UI and screenshot E2E coverage."""

from __future__ import annotations

from io import BytesIO
from importlib import import_module
from typing import Any
import wave

from android import ANDROID_AUTO_PROBE_ACTIVITY, ANDROID_PACKAGE, AndroidDevice
from gui import (
    EQUALIZER_CONTROL_RECTS,
    MAIN_BUTTON_RECTS,
    MAIN_PLAYER_BASE_HEIGHT,
    MAIN_TOGGLE_RECTS,
    EqualizerControl,
    MainButton,
    MainToggleButton,
    offset_rect,
)

pytest: Any = import_module("pytest")

pytestmark = pytest.mark.android


def test_android_checkpoints_background_playback_position(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    audio_path = "files/imports/checkpoint.wav"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()

    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 20)
    android_device.write_private_bytes(audio_path, audio.getvalue())
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:20,Position Checkpoint\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/checkpoint.wav\n",
    )

    android_device.start_activity()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    android_device.assert_log_contains(
        "player: button activated, button_name=Play",
    )
    android_device.wait_for_service("XmmsPlaybackService")
    android_device.shell("input", "keyevent", "3")
    position_ms = android_device.wait_for_private_file_int_at_least(
        config_path,
        "playback_position_ms",
        8_000,
        timeout=15.0,
    )
    assert position_ms < 20_000


def test_android_shows_playlist_by_default(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    geometry = android_device.display_geometry()
    player_bounds = android_device.main_player_bounds()
    assert player_bounds[1] <= geometry.top_inset + 4
    assert player_bounds[3] <= geometry.height - geometry.bottom_inset + 1
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST],
        player_bounds,
    )

    android_device.assert_log_contains(
        "player: toggle activated, toggle_name=Playlist",
    )
    android_device.wait_for_private_file_contains(
        config_path,
        "playlist_visible=false",
    )

    android_device.restart_app()
    android_device.tap_skin_rect(MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST])
    android_device.wait_for_private_file_contains(
        config_path,
        "playlist_visible=true",
    )


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


def test_android_restores_panels_settings_and_playlist_after_relaunch(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:42,Restored Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/restored.wav\n",
    )
    android_device.start_activity()
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.REPEAT],
        player_bounds,
    )
    android_device.tap_skin_rect(
        offset_rect(
            EQUALIZER_CONTROL_RECTS[EqualizerControl.ON],
            MAIN_PLAYER_BASE_HEIGHT,
        ),
        player_bounds,
    )
    for setting in (
        "playlist_visible=true",
        "equalizer_visible=true",
        "equalizer_active=false",
        "shuffle=true",
        "repeat=true",
    ):
        android_device.wait_for_private_file_contains(config_path, setting)

    original_pid = android_device.close_activity()
    android_device.start_activity()
    assert android_device.app_pid() == original_pid
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(
        offset_rect(
            EQUALIZER_CONTROL_RECTS[EqualizerControl.ON],
            MAIN_PLAYER_BASE_HEIGHT,
        ),
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.PLAYLIST],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.SHUFFLE],
        player_bounds,
    )
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.REPEAT],
        player_bounds,
    )
    for setting in (
        "playlist_visible=false",
        "equalizer_visible=false",
        "equalizer_active=true",
        "shuffle=false",
        "repeat=false",
    ):
        android_device.wait_for_private_file_contains(config_path, setting)
    android_device.wait_for_private_file_contains(playlist_path, "Restored Track")


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
