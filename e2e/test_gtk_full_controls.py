"""Full GTK/egui skinned-control E2E coverage with screenshots and console-log assertions."""

from __future__ import annotations

import subprocess
import time
from importlib import import_module
from pathlib import Path
from typing import Any

from conftest import assert_app_log_contains
from gui import (
    EQUALIZER_CONTROL_RECTS,
    PLAYLIST_FOOTER_RECTS,
    PLAYLIST_MENU_RECTS,
    EqualizerControl,
    EqualizerSlider,
    MainButton,
    MainSlider,
    MainToggleButton,
    MainWindow,
    PlaylistFooterButton,
    PlaylistMenuButton,
    SkinRect,
    click_skin_rect,
    drag_playlist_scrollbar_to_bottom,
    drag_skin_rect,
    equalizer_slider_rect,
    run_xdotool,
    screenshot_screen,
    screenshot_tool_available,
    wait_for_visible_window,
)

pytest: Any = import_module("pytest")

EQUALIZER_WINDOW_TITLE = "XMMS Renascene Rust Equalizer"
PLAYLIST_WINDOW_TITLE = "XMMS Renascene Rust Playlist"
MAIN_PLAYER_BASE_HEIGHT = 116
PANEL_SHADE_RECT = SkinRect(254, 3, 9, 9)
PANEL_CLOSE_RECT = SkinRect(264, 3, 9, 9)

EQUALIZER_SLIDERS = [
    EqualizerSlider.PREAMP,
    EqualizerSlider.BAND_0,
    EqualizerSlider.BAND_1,
    EqualizerSlider.BAND_2,
    EqualizerSlider.BAND_3,
    EqualizerSlider.BAND_4,
    EqualizerSlider.BAND_5,
    EqualizerSlider.BAND_6,
    EqualizerSlider.BAND_7,
    EqualizerSlider.BAND_8,
    EqualizerSlider.BAND_9,
]


def assert_screenshot(path: Path) -> None:
    assert path.is_file()
    assert path.stat().st_size > 0


def require_screenshots() -> None:
    if not screenshot_tool_available():
        pytest.skip("Install ImageMagick 'import' or xwd to capture E2E screenshots")


def capture(test_output: Any) -> Path:
    path = screenshot_screen(test_output.screenshot_path())
    assert_screenshot(path)
    return path


def offset_rect(rect: SkinRect, y_offset: int) -> SkinRect:
    return SkinRect(rect.x, rect.y + y_offset, rect.width, rect.height)


def open_panel(main_window: MainWindow, toggle: MainToggleButton, title: str, test_output: Any) -> tuple[str, int]:
    main_window.focus_main_window()
    before_height = main_window.geometry().height
    main_window.click_main_toggle(toggle)
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline:
        separate = run_xdotool("search", "--onlyvisible", "--name", title, check=False)
        windows = [line.strip() for line in separate.stdout.splitlines() if line.strip()]
        if windows:
            time.sleep(0.3)
            capture(test_output)
            return windows[0], 0
        if main_window.geometry().height > before_height:
            time.sleep(0.3)
            capture(test_output)
            return main_window.window_id, MAIN_PLAYER_BASE_HEIGHT
        time.sleep(0.1)
    # Keep the detailed xdotool wait error for the rare detached-window case.
    wait_for_visible_window(title, timeout=0.1)
    raise AssertionError("unreachable: wait_for_visible_window should raise on timeout")


def test_gui_player_transport_toggles_and_sliders_with_tracks_screenshots_and_logs(
    gui_tracked_main_window: MainWindow,
    gui_app_with_tracks: subprocess.Popen[bytes],
    test_output: Any,
) -> None:
    """Click core player controls/sliders on ffmpeg tracks and confirm app logs."""
    require_screenshots()

    capture(test_output)
    gui_tracked_main_window.focus_main_window()

    for toggle in [MainToggleButton.SHUFFLE, MainToggleButton.REPEAT]:
        gui_tracked_main_window.click_main_toggle(toggle)
        time.sleep(0.2)
        capture(test_output)

    for button in [
        MainButton.PLAY,
        MainButton.PAUSE,
        MainButton.STOP,
        MainButton.NEXT,
        MainButton.PREVIOUS,
    ]:
        gui_tracked_main_window.click_main_button(button)
        time.sleep(0.4)
        capture(test_output)

    gui_tracked_main_window.click_main_button(MainButton.PLAY)
    time.sleep(0.8)
    capture(test_output)

    gui_tracked_main_window.drag_main_slider(MainSlider.VOLUME, 0.85)
    time.sleep(0.2)
    capture(test_output)

    gui_tracked_main_window.drag_main_slider(MainSlider.BALANCE, 0.85)
    time.sleep(0.2)
    capture(test_output)

    gui_tracked_main_window.drag_main_slider(MainSlider.POSITION, 0.55)
    time.sleep(0.4)
    capture(test_output)

    assert_app_log_contains(
        gui_app_with_tracks,
        "command Playlist(ToggleShuffle)",
        "command Playlist(ToggleRepeat)",
        "command Player(Play)",
        "StartPlaybackUri",
        "xmms-e2e-track-",
        "command Player(TogglePause)",
        "command Player(Stop)",
        "command Player(NextTrack)",
        "command Player(PreviousTrack)",
        "command Audio(SetVolume",
        "command Audio(SetBalance",
        "player: slider changed, slider_name=Volume",
        "player: slider changed, slider_name=Balance",
        "player: slider changed, slider_name=Position",
    )


def test_gui_equalizer_controls_and_sliders_screenshots_and_logs(
    gui_tracked_main_window: MainWindow,
    gui_app_with_tracks: subprocess.Popen[bytes],
    test_output: Any,
) -> None:
    """Click every equalizer button and every equalizer slider."""
    require_screenshots()

    equalizer_window, equalizer_y = open_panel(
        gui_tracked_main_window,
        MainToggleButton.EQUALIZER,
        EQUALIZER_WINDOW_TITLE,
        test_output,
    )

    for control in [EqualizerControl.ON, EqualizerControl.AUTO, EqualizerControl.PRESETS]:
        click_skin_rect(equalizer_window, offset_rect(EQUALIZER_CONTROL_RECTS[control], equalizer_y))
        time.sleep(0.25)
        capture(test_output)
        if control is EqualizerControl.PRESETS:
            run_xdotool("key", "Escape", check=False)
            time.sleep(0.1)

    for index, slider in enumerate(EQUALIZER_SLIDERS):
        drag_skin_rect(
            equalizer_window,
            offset_rect(equalizer_slider_rect(slider), equalizer_y),
            end_fraction=0.2 if index % 2 == 0 else 0.8,
            horizontal=False,
        )
        time.sleep(0.15)
        capture(test_output)

    click_skin_rect(equalizer_window, offset_rect(PANEL_SHADE_RECT, equalizer_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(equalizer_window, offset_rect(PANEL_SHADE_RECT, equalizer_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(equalizer_window, offset_rect(PANEL_CLOSE_RECT, equalizer_y))
    time.sleep(0.25)
    capture(test_output)

    assert_app_log_contains(
        gui_app_with_tracks,
        "equalizer: control activated, control_name=On",
        "equalizer: control activated, control_name=Auto",
        "equalizer: control activated, control_name=Presets",
        "command Equalizer(SetPreamp",
        "command Equalizer(SetBand { band: 0",
        "command Equalizer(SetBand { band: 9",
        "command Panel(ToggleEqualizerShade)",
        "command Panel(SetEqualizerVisibility(false))",
    )


def test_gui_playlist_buttons_menus_and_scrollbar_screenshots_and_logs(
    gui_tracked_main_window: MainWindow,
    gui_app_with_tracks: subprocess.Popen[bytes],
    test_output: Any,
) -> None:
    """Click every playlist bottom/menu button and drag the playlist scrollbar."""
    require_screenshots()

    playlist_window, playlist_y = open_panel(
        gui_tracked_main_window,
        MainToggleButton.PLAYLIST,
        PLAYLIST_WINDOW_TITLE,
        test_output,
    )

    for menu in [
        PlaylistMenuButton.ADD,
        PlaylistMenuButton.REMOVE,
        PlaylistMenuButton.SELECT,
        PlaylistMenuButton.MISC,
        PlaylistMenuButton.LIST,
    ]:
        click_skin_rect(playlist_window, offset_rect(PLAYLIST_MENU_RECTS[menu], playlist_y))
        time.sleep(0.25)
        capture(test_output)
        run_xdotool("key", "Escape", check=False)
        time.sleep(0.1)

    for button in [
        PlaylistFooterButton.PREVIOUS,
        PlaylistFooterButton.PLAY,
        PlaylistFooterButton.PAUSE,
        PlaylistFooterButton.STOP,
        PlaylistFooterButton.NEXT,
        PlaylistFooterButton.SCROLL_DOWN,
        PlaylistFooterButton.SCROLL_UP,
        PlaylistFooterButton.EJECT,
    ]:
        click_skin_rect(playlist_window, offset_rect(PLAYLIST_FOOTER_RECTS[button], playlist_y))
        time.sleep(0.3)
        capture(test_output)
        if button is PlaylistFooterButton.EJECT:
            run_xdotool("key", "Escape", check=False)
            time.sleep(0.2)

    drag_playlist_scrollbar_to_bottom(playlist_window, playlist_y)
    time.sleep(0.3)
    capture(test_output)

    click_skin_rect(playlist_window, offset_rect(PANEL_SHADE_RECT, playlist_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(playlist_window, offset_rect(PANEL_SHADE_RECT, playlist_y))
    time.sleep(0.25)
    capture(test_output)
    click_skin_rect(playlist_window, offset_rect(PANEL_CLOSE_RECT, playlist_y))
    time.sleep(0.25)
    capture(test_output)

    assert_app_log_contains(
        gui_app_with_tracks,
        "playlist: menu opened, menu_name=Add",
        "playlist: menu opened, menu_name=Remove",
        "playlist: menu opened, menu_name=Select",
        "playlist: menu opened, menu_name=Misc",
        "playlist: menu opened, menu_name=List",
        "playlist: footer button, button_name=Previous",
        "playlist: footer button, button_name=Play",
        "playlist: footer button, button_name=Pause",
        "playlist: footer button, button_name=Stop",
        "playlist: footer button, button_name=Next",
        "playlist: footer button, button_name=ScrollDown",
        "playlist: footer button, button_name=ScrollUp",
        "playlist: footer button, button_name=Eject",
        "command Player(Play)",
        "command Player(TogglePause)",
        "command Player(Stop)",
        "command Player(NextTrack)",
        "command Player(PreviousTrack)",
        "command Panel(TogglePlaylistShade)",
        "command Panel(SetPlaylistVisibility(false))",
    )
