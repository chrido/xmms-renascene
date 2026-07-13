"""Android emulator UI and screenshot E2E coverage."""

from __future__ import annotations

from io import BytesIO
from importlib import import_module
from pathlib import Path
from typing import Any
import time
import wave
import zipfile

from PIL import Image

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


def test_android_preferences_use_touch_layout_and_back_navigation(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    original_pid = android_device.app_pid()

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    categories = android_device.screenshot(test_output.screenshot_path())

    android_device.tap_usable_fraction(0.5, 0.22)
    player_page = android_device.screenshot(test_output.screenshot_path())
    android_device.swipe_usable_fraction(0.20, 0.5, 0.70, 0.5)
    categories_after_back = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_usable_fraction(0.5, 0.463)
    skins_page = android_device.screenshot(test_output.screenshot_path())
    android_device.swipe_usable_fraction(0.20, 0.5, 0.70, 0.5)
    categories_after_skin_back = android_device.screenshot(
        test_output.screenshot_path()
    )
    android_device.tap_usable_fraction(0.13, 0.045)
    closed = android_device.screenshot(test_output.screenshot_path())

    android_device.assert_log_contains(
        "command Ui(SetPreferencesVisible(true))",
        "command Ui(SetPreferencesVisible(false))",
    )
    assert android_device.app_pid() == original_pid
    assert categories.read_bytes() != player_page.read_bytes()
    assert categories.read_bytes() == categories_after_back.read_bytes()
    assert categories.read_bytes() != skins_page.read_bytes()
    assert categories.read_bytes() == categories_after_skin_back.read_bytes()
    assert categories.read_bytes() != closed.read_bytes()


def test_android_equalizer_presets_menu_saves_winamp_eqf(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    output_path = "/sdcard/Download/preset.eqf"
    android_device.shell("rm", "-f", output_path, check=False)
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(
        MAIN_TOGGLE_RECTS[MainToggleButton.EQUALIZER],
        player_bounds,
    )
    player_bounds = android_device.main_player_bounds()
    android_device.tap_skin_rect(
        offset_rect(
            EQUALIZER_CONTROL_RECTS[EqualizerControl.PRESETS],
            MAIN_PLAYER_BASE_HEIGHT,
        ),
        player_bounds,
    )
    menu = android_device.screenshot(test_output.screenshot_path())

    android_device.tap_usable_fraction(0.477, 0.405)
    android_device.wait_for_focus("com.google.android.documentsui")
    picker = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_ui_text("Save")
    android_device.wait_for_focus(ANDROID_PACKAGE)
    android_device.wait_for_external_file(output_path)
    header = android_device.command(
        "exec-out",
        "head",
        "-c",
        "31",
        output_path,
    ).stdout

    assert menu.read_bytes() != picker.read_bytes()
    assert header == "Winamp EQ library file v1.1\x1a!--"
    android_device.shell("rm", "-f", output_path, check=False)


def test_android_menu_button_opens_preferences_directly(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player_bounds = android_device.main_player_bounds()

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU], player_bounds)
    preferences = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_usable_fraction(0.13, 0.045)
    player = android_device.screenshot(test_output.screenshot_path())

    android_device.assert_log_contains(
        "command Ui(SetPreferencesVisible(true))",
        "command Ui(SetPreferencesVisible(false))",
    )
    assert preferences.read_bytes() != player.read_bytes()


def test_android_landscape_uses_full_height_and_accepts_skin_taps(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.restart_app(reset_data=True)
    android_device.set_landscape()
    android_device.force_stop()
    android_device.start_activity()
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


def test_android_managed_playlists_save_load_and_delete_from_settings(
    android_device: AndroidDevice,
) -> None:
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    managed_path = "files/config/playlists/playlist"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:42,Managed Original\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/original.wav\n",
    )
    android_device.start_activity()

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    android_device.tap_usable_fraction(0.5, 0.544)
    android_device.tap_usable_fraction(0.5, 0.30)
    android_device.wait_for_private_file_contains(managed_path, "Managed Original")
    assert not android_device.private_file_exists(f"{managed_path}.m3u8")

    android_device.write_private_file(
        managed_path,
        "#EXTM3U\n"
        "#EXTINF:42,Managed Loaded\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/loaded.wav\n",
    )
    android_device.tap_usable_fraction(0.32, 0.74)
    android_device.close_activity()
    android_device.wait_for_private_file_contains(playlist_path, "Managed Loaded")
    android_device.start_activity()

    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    android_device.tap_usable_fraction(0.5, 0.544)
    android_device.tap_usable_fraction(0.84, 0.74)
    android_device.wait_for_private_file_absent(managed_path)


def test_android_playlist_save_and_load_menu_items_open_managed_dialog(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.restart_app(reset_data=True)
    player = android_device.screenshot(test_output.screenshot_path())
    left, top, right, bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    list_x = round(left + 240.5 * scale)

    def tap_list_y(offset_from_bottom: float) -> None:
        android_device.shell(
            "input",
            "tap",
            str(list_x),
            str(round(bottom - offset_from_bottom * scale)),
        )
        time.sleep(0.4)

    tap_list_y(20)
    tap_list_y(39)
    save_dialog = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_usable_fraction(0.13, 0.043)

    tap_list_y(20)
    tap_list_y(21)
    load_dialog = android_device.screenshot(test_output.screenshot_path())

    def image_pixels(path: Path) -> bytes:
        with Image.open(path) as image:
            return image.convert("RGB").tobytes()

    assert image_pixels(player) != image_pixels(save_dialog)
    assert image_pixels(player) != image_pixels(load_dialog)


def test_android_playlist_swipes_select_right_and_deselect_left(
    android_device: AndroidDevice,
) -> None:
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:42,Swipe First\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/first.wav\n"
        "#EXTINF:42,Swipe Second\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/second.wav\n",
    )
    android_device.start_activity()
    left, top, right, _bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    playlist_top = top + round(116 * scale)
    row_top = playlist_top + round(20 * scale)
    row_bottom = row_top + round(11 * scale)
    row_y = (row_top + row_bottom) // 2
    start_x = left + round(40 * scale)
    end_x = left + round(200 * scale)

    def row_pixels() -> bytes:
        with Image.open(BytesIO(android_device.framebuffer_png())) as screenshot:
            return screenshot.convert("RGB").crop(
                (left, row_top, right, row_bottom)
            ).tobytes()

    def wait_for_row_change(previous: bytes) -> bytes:
        deadline = time.monotonic() + 5.0
        current = previous
        while time.monotonic() < deadline:
            current = row_pixels()
            if current != previous:
                return current
            time.sleep(0.2)
        raise AssertionError("playlist row did not redraw after swipe selection")

    before = row_pixels()
    android_device.clear_logcat()
    android_device.shell(
        "input",
        "swipe",
        str(start_x),
        str(row_y),
        str(end_x),
        str(row_y),
        "300",
    )
    android_device.assert_log_contains(
        "playlist: swipe selection applied, swiped_index=0, selected=true"
    )
    selected = wait_for_row_change(before)
    android_device.clear_logcat()
    android_device.shell(
        "input",
        "swipe",
        str(end_x),
        str(row_y),
        str(start_x),
        str(row_y),
        "300",
    )
    android_device.assert_log_contains(
        "playlist: swipe selection applied, swiped_index=0, selected=false"
    )
    deselected = wait_for_row_change(selected)

    assert before != selected
    assert selected != deselected


def test_android_misc_popup_dismisses_outside_and_file_info_uses_full_screen(
    android_device: AndroidDevice,
    test_output: Any,
) -> None:
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()
    android_device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n"
        "#EXTINF:42,File Info Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/info.wav\n",
    )
    android_device.start_activity()
    player = android_device.screenshot(test_output.screenshot_path())
    left, _top, right, bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    misc_x = round(left + 111.5 * scale)
    menu_y = round(bottom - 20 * scale)

    def open_misc() -> None:
        android_device.shell("input", "tap", str(misc_x), str(menu_y))
        time.sleep(0.5)

    open_misc()
    popup = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_usable_fraction(0.1, 0.5)
    dismissed = android_device.screenshot(test_output.screenshot_path())

    open_misc()
    android_device.tap_usable_fraction(0.67, 0.85)
    file_info = android_device.screenshot(test_output.screenshot_path())
    android_device.tap_usable_fraction(0.13, 0.043)
    returned = android_device.screenshot(test_output.screenshot_path())

    def image_pixels(path: Path) -> bytes:
        with Image.open(path) as image:
            return image.convert("RGB").tobytes()

    assert image_pixels(player) != image_pixels(popup)
    assert image_pixels(popup) != image_pixels(dismissed)
    assert image_pixels(dismissed) != image_pixels(file_info)
    assert image_pixels(file_info) != image_pixels(returned)


def test_android_clear_list_stops_playback_and_resets_playlist(
    android_device: AndroidDevice,
) -> None:
    config_path = "files/config/xmms-renascene/config"
    playlist_path = "files/config/xmms-renascene/playlist.m3u"
    audio_path = "files/imports/clear.wav"
    android_device.set_portrait()
    android_device.force_stop()
    android_device.shell("pm", "clear", ANDROID_PACKAGE)
    android_device.grant_runtime_permissions()

    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 8)
    android_device.write_private_bytes(audio_path, audio.getvalue())
    android_device.write_private_file(
        playlist_path,
        "#EXTM3U\n"
        "#EXTINF:8,Clear Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/clear.wav\n",
    )
    android_device.start_activity()
    android_device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    android_device.wait_for_service("XmmsPlaybackService")

    left, _top, right, bottom = android_device.main_player_bounds()
    scale = (right - left) / 275
    list_x = round(left + 240.5 * scale)
    android_device.shell(
        "input",
        "tap",
        str(list_x),
        str(round(bottom - 20 * scale)),
    )
    time.sleep(0.4)
    android_device.shell(
        "input",
        "tap",
        str(list_x),
        str(round(bottom - 57 * scale)),
    )

    android_device.wait_for_service_absent("XmmsPlaybackService")
    android_device.wait_for_private_file_not_contains(playlist_path, "Clear Track")
    android_device.wait_for_private_file_contains(config_path, "playback_position_ms=0")


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


def test_android_player_widget_is_packaged(
    android_device: AndroidDevice,
) -> None:
    manifest = android_device.apk_xmltree("AndroidManifest.xml")
    widget_info = android_device.apk_xmltree("res/xml/player_widget_info.xml")
    widget_layout = android_device.apk_xmltree("res/layout/widget_player.xml")

    assert ".XmmsPlayerWidget" in manifest
    assert "android.appwidget.action.APPWIDGET_UPDATE" in manifest
    assert "android:initialLayout" in widget_info
    assert "E: ImageView" in widget_layout
    assert widget_layout.count("E: ImageButton") == 5

    apk = Path(__file__).resolve().parents[1] / "target/debug/apk/xmms-renascene.apk"
    with zipfile.ZipFile(apk) as package:
        native_library = next(
            name
            for name in package.namelist()
            if name.startswith("lib/") and name.endswith("/libxmms_renascene.so")
        )
        library_bytes = package.read(native_library)
    assert (
        b"Java_org_xmms_renascene_XmmsPlayerWidget_nativeRenderPlayerWidget"
        in library_bytes
    )
