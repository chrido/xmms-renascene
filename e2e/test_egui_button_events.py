"""Fine-grained egui button event tests.

These tests complement the visual pressed-state tests by clicking each skinned
button and asserting that the egui frontend emits the expected console/store
event for that exact button.
"""

from __future__ import annotations

import subprocess
import time
from collections.abc import Iterator
from importlib import import_module
from pathlib import Path
from typing import Any

from conftest import (
    EGUI_FRONTEND,
    REPO_ROOT,
    assert_app_log_contains,
    command_exists,
    start_gui_process,
    wait_for_main_window_with_log,
)
from gui import (
    EQUALIZER_CONTROL_RECTS,
    PLAYLIST_FOOTER_RECTS,
    PLAYLIST_MENU_RECTS,
    EqualizerControl,
    MainButton,
    MainToggleButton,
    MainWindow,
    PlaylistFooterButton,
    PlaylistMenuButton,
    SkinRect,
    click_skin_rect,
    run_xdotool,
    scaled_skin_point,
    screenshot_window,
    wait_for_visible_window,
    window_geometry,
)

pytest: Any = import_module("pytest")

MAIN_PLAYER_BASE_HEIGHT = 116
PANEL_SHADE_RECT = SkinRect(254, 3, 9, 9)
PANEL_CLOSE_RECT = SkinRect(264, 3, 9, 9)
PLAYLIST_MISC_SORT_ITEM_RECT = SkinRect(98, 166, 25, 18)
PLAYLIST_ADD_FIRST_ITEM_RECT = SkinRect(11, 166, 25, 18)

MAIN_PUSH_BUTTON_EVENTS = [
    (MainButton.MENU, "Menu", "command Ui(SetMainMenuVisible(true))"),
    (MainButton.MINIMIZE, "Minimize", None),
    (MainButton.SHADE, "Shade", "command Panel(ToggleMainShade)"),
    (MainButton.CLOSE, "Close", None),
    (MainButton.PREVIOUS, "Previous", "command Player(PreviousTrack)"),
    (MainButton.PLAY, "Play", "command Player(Play)"),
    (MainButton.PAUSE, "Pause", "command Player(TogglePause)"),
    (MainButton.STOP, "Stop", "command Player(Stop)"),
    (MainButton.NEXT, "Next", "command Player(NextTrack)"),
    (MainButton.EJECT, "Eject", "frontend_effect: egui OpenFileDialog(AddAudioFiles)"),
]

MAIN_TOGGLE_EVENTS = [
    (MainToggleButton.SHUFFLE, "Shuffle", "command Playlist(ToggleShuffle)"),
    (MainToggleButton.REPEAT, "Repeat", "command Playlist(ToggleRepeat)"),
    (MainToggleButton.EQUALIZER, "Equalizer", "command Panel(ToggleEqualizerVisibility)"),
    (MainToggleButton.PLAYLIST, "Playlist", "command Panel(TogglePlaylistVisibility)"),
]

EQUALIZER_CONTROL_EVENTS = [
    (EqualizerControl.ON, "On", "command Equalizer(ToggleActive)"),
    (EqualizerControl.AUTO, "Auto", "command Equalizer(ToggleAuto)"),
    (EqualizerControl.PRESETS, "Presets", None),
]

PANEL_TITLE_EVENTS = [
    ("shade", PANEL_SHADE_RECT, "Shade"),
    ("close", PANEL_CLOSE_RECT, "Close"),
]

PLAYLIST_MENU_EVENTS = [
    (PlaylistMenuButton.ADD, "Add"),
    (PlaylistMenuButton.REMOVE, "Remove"),
    (PlaylistMenuButton.SELECT, "Select"),
    (PlaylistMenuButton.MISC, "Misc"),
    (PlaylistMenuButton.LIST, "List"),
]

PLAYLIST_FOOTER_EVENTS = [
    (PlaylistFooterButton.PREVIOUS, "Previous", "command Player(PreviousTrack)"),
    (PlaylistFooterButton.PLAY, "Play", "command Player(Play)"),
    (PlaylistFooterButton.PAUSE, "Pause", "command Player(TogglePause)"),
    (PlaylistFooterButton.STOP, "Stop", "command Player(Stop)"),
    (PlaylistFooterButton.NEXT, "Next", "command Player(NextTrack)"),
    (PlaylistFooterButton.EJECT, "Eject", "frontend_effect: egui OpenFileDialog(AddAudioFiles)"),
    (PlaylistFooterButton.SCROLL_UP, "ScrollUp", None),
    (PlaylistFooterButton.SCROLL_DOWN, "ScrollDown", None),
]


@pytest.fixture(scope="module")
def egui_event_tracks(tmp_path_factory: Any) -> list[Path]:
    """Small generated playlist used by per-button playback event tests."""
    if not command_exists("ffmpeg"):
        pytest.skip("ffmpeg is required to create E2E audio tracks")
    tracks_dir = tmp_path_factory.mktemp("egui-button-event-tracks")
    tracks: list[Path] = []
    for index in range(3):
        path = tracks_dir / f"egui-button-event-track-{index:02}.wav"
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                f"sine=frequency={523 + index * 41}:duration=1.0",
                "-ac",
                "2",
                "-ar",
                "44100",
                str(path),
            ],
            cwd=REPO_ROOT,
            check=True,
        )
        if not path.is_file() or path.stat().st_size == 0:
            raise AssertionError(f"ffmpeg did not create {path}")
        tracks.append(path)
    return tracks


@pytest.fixture
def egui_app_with_event_tracks(
    tmp_path: Path,
    egui_event_tracks: list[Path],
) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(
        tmp_path,
        EGUI_FRONTEND,
        [str(track) for track in egui_event_tracks],
        log_name="xmms-egui-button-events.log",
    )


@pytest.fixture
def egui_detached_app_with_event_tracks(
    tmp_path: Path,
    egui_event_tracks: list[Path],
) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(
        tmp_path,
        EGUI_FRONTEND,
        [
            "--equalizer-undocked",
            "--playlist-undocked",
            *(str(track) for track in egui_event_tracks),
        ],
        log_name="xmms-egui-detached-button-events.log",
    )


@pytest.fixture
def egui_main_window(egui_app_with_event_tracks: subprocess.Popen[bytes]) -> MainWindow:
    return wait_for_main_window_with_log(egui_app_with_event_tracks)


@pytest.fixture
def egui_detached_main_window(
    egui_detached_app_with_event_tracks: subprocess.Popen[bytes],
) -> MainWindow:
    return wait_for_main_window_with_log(egui_detached_app_with_event_tracks)


def offset_rect(rect: SkinRect, y_offset: int) -> SkinRect:
    return SkinRect(rect.x, rect.y + y_offset, rect.width, rect.height)


def open_docked_panel(main_window: MainWindow, toggle: MainToggleButton) -> int:
    main_window.focus_main_window()
    before_height = main_window.geometry().height
    main_window.click_main_toggle(toggle)
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline:
        if main_window.geometry().height > before_height:
            time.sleep(0.25)
            return MAIN_PLAYER_BASE_HEIGHT
        time.sleep(0.1)
    raise AssertionError(f"egui panel for {toggle.value} did not open")


def visible_windows(title: str) -> list[str]:
    result = run_xdotool("search", "--onlyvisible", "--name", title, check=False)
    if result.returncode != 0:
        return []
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def wait_for_detached_panel_window(
    process: subprocess.Popen[bytes],
    title: str,
) -> str:
    return wait_for_visible_window(title, process=process, timeout=5.0)


def click_detached_skin_rect_without_activation_click(
    main_window: MainWindow,
    detached_window_id: str,
    rect: SkinRect,
) -> None:
    x, y = scaled_skin_point(detached_window_id, rect)
    # Xvfb often runs without a window manager, so egui native windows all start
    # overlapped at 0,0. Put the detached target somewhere non-overlapping and
    # focus its X window for event delivery, then send exactly one button click.
    # No separate activation click is used.
    run_xdotool("windowmove", detached_window_id, "320", "280", check=False)
    time.sleep(0.1)
    main_window.focus_main_window()
    run_xdotool("windowfocus", detached_window_id, check=False)
    # focus_main_window() raises the main window to the top of the (WM-less) X11
    # stack, and the main window still overlaps the moved detached window. Raise
    # the detached target back above it so the coordinate click lands on the
    # detached window rather than the overlapping main window. Raising only
    # changes stacking order; it is not a pointer activation click.
    run_xdotool("windowraise", detached_window_id, check=False)
    time.sleep(0.1)
    geometry = window_geometry(detached_window_id)
    # Move the pointer to the target first (as its own event) and let the egui
    # viewport process the CursorMoved / establish hover before the press. When
    # the move and the button press are sent in a single xdotool invocation the
    # press can be processed before the hover position updates, so egui does not
    # attribute the press to the hovered widget and the click is dropped.
    run_xdotool("mousemove", str(geometry.x + x), str(geometry.y + y))
    time.sleep(0.1)
    run_xdotool("mousedown", "1")
    time.sleep(0.05)
    run_xdotool("mouseup", "1")


def assert_event_log(
    process: subprocess.Popen[bytes],
    primary_event: str,
    extra_event: str | None = None,
) -> None:
    events = [primary_event]
    if extra_event is not None:
        events.append(extra_event)
    assert_app_log_contains(process, *events)


@pytest.mark.parametrize(
    ("button", "button_name", "expected_event"),
    MAIN_PUSH_BUTTON_EVENTS,
    ids=[button.value for button, _, _ in MAIN_PUSH_BUTTON_EVENTS],
)
def test_egui_main_push_button_emits_event(
    egui_main_window: MainWindow,
    egui_app_with_event_tracks: subprocess.Popen[bytes],
    button: MainButton,
    button_name: str,
    expected_event: str | None,
) -> None:
    egui_main_window.click_main_button(button)

    assert_event_log(
        egui_app_with_event_tracks,
        f"player: button activated, button_name={button_name}",
        expected_event,
    )


@pytest.mark.parametrize(
    ("toggle", "toggle_name", "expected_event"),
    MAIN_TOGGLE_EVENTS,
    ids=[toggle.value for toggle, _, _ in MAIN_TOGGLE_EVENTS],
)
def test_egui_main_toggle_button_emits_event(
    egui_main_window: MainWindow,
    egui_app_with_event_tracks: subprocess.Popen[bytes],
    toggle: MainToggleButton,
    toggle_name: str,
    expected_event: str,
) -> None:
    egui_main_window.click_main_toggle(toggle)

    assert_event_log(
        egui_app_with_event_tracks,
        f"player: toggle activated, toggle_name={toggle_name}",
        expected_event,
    )


@pytest.mark.parametrize(
    ("control", "control_name", "expected_event"),
    EQUALIZER_CONTROL_EVENTS,
    ids=[control.value for control, _, _ in EQUALIZER_CONTROL_EVENTS],
)
def test_egui_equalizer_control_button_emits_event(
    egui_main_window: MainWindow,
    egui_app_with_event_tracks: subprocess.Popen[bytes],
    control: EqualizerControl,
    control_name: str,
    expected_event: str | None,
) -> None:
    equalizer_y = open_docked_panel(egui_main_window, MainToggleButton.EQUALIZER)
    click_skin_rect(
        egui_main_window.window_id,
        offset_rect(EQUALIZER_CONTROL_RECTS[control], equalizer_y),
    )

    assert_event_log(
        egui_app_with_event_tracks,
        f"equalizer: control activated, control_name={control_name}",
        expected_event,
    )
    if control is EqualizerControl.PRESETS:
        time.sleep(0.2)
        assert visible_windows("Equalizer Presets") == []
        run_xdotool("key", "Escape", check=False)


@pytest.mark.parametrize(
    ("button_id", "rect", "button_name"),
    PANEL_TITLE_EVENTS,
    ids=[button_id for button_id, _, _ in PANEL_TITLE_EVENTS],
)
def test_egui_equalizer_title_button_emits_event(
    egui_main_window: MainWindow,
    egui_app_with_event_tracks: subprocess.Popen[bytes],
    button_id: str,
    rect: SkinRect,
    button_name: str,
) -> None:
    equalizer_y = open_docked_panel(egui_main_window, MainToggleButton.EQUALIZER)
    click_skin_rect(egui_main_window.window_id, offset_rect(rect, equalizer_y))

    expected_command = (
        "command Panel(ToggleEqualizerShade)"
        if button_id == "shade"
        else "command Panel(SetEqualizerVisibility(false))"
    )
    assert_event_log(
        egui_app_with_event_tracks,
        f"equalizer: title button, button_name={button_name}",
        expected_command,
    )


@pytest.mark.parametrize(
    ("menu", "menu_name"),
    PLAYLIST_MENU_EVENTS,
    ids=[menu.value for menu, _ in PLAYLIST_MENU_EVENTS],
)
def test_egui_playlist_menu_button_emits_event(
    egui_main_window: MainWindow,
    egui_app_with_event_tracks: subprocess.Popen[bytes],
    menu: PlaylistMenuButton,
    menu_name: str,
) -> None:
    playlist_y = open_docked_panel(egui_main_window, MainToggleButton.PLAYLIST)
    click_skin_rect(
        egui_main_window.window_id,
        offset_rect(PLAYLIST_MENU_RECTS[menu], playlist_y),
    )

    assert_event_log(
        egui_app_with_event_tracks,
        f"playlist: menu opened, menu_name={menu_name}",
    )
    if menu is PlaylistMenuButton.MISC:
        click_skin_rect(
            egui_main_window.window_id,
            offset_rect(PLAYLIST_MISC_SORT_ITEM_RECT, playlist_y),
        )
        time.sleep(0.2)
        assert visible_windows("Playlist Sort") == []
        run_xdotool("key", "Escape", check=False)


@pytest.mark.parametrize(
    ("button", "button_name", "expected_event"),
    PLAYLIST_FOOTER_EVENTS,
    ids=[button.value for button, _, _ in PLAYLIST_FOOTER_EVENTS],
)
def test_egui_playlist_footer_button_emits_event(
    egui_main_window: MainWindow,
    egui_app_with_event_tracks: subprocess.Popen[bytes],
    button: PlaylistFooterButton,
    button_name: str,
    expected_event: str | None,
) -> None:
    playlist_y = open_docked_panel(egui_main_window, MainToggleButton.PLAYLIST)
    click_skin_rect(
        egui_main_window.window_id,
        offset_rect(PLAYLIST_FOOTER_RECTS[button], playlist_y),
    )

    assert_event_log(
        egui_app_with_event_tracks,
        f"playlist: footer button, button_name={button_name}",
        expected_event,
    )
    if button is PlaylistFooterButton.EJECT:
        run_xdotool("key", "Escape", check=False)


@pytest.mark.parametrize(
    ("button_id", "rect", "button_name"),
    PANEL_TITLE_EVENTS,
    ids=[button_id for button_id, _, _ in PANEL_TITLE_EVENTS],
)
def test_egui_playlist_title_button_emits_event(
    egui_main_window: MainWindow,
    egui_app_with_event_tracks: subprocess.Popen[bytes],
    button_id: str,
    rect: SkinRect,
    button_name: str,
) -> None:
    playlist_y = open_docked_panel(egui_main_window, MainToggleButton.PLAYLIST)
    click_skin_rect(egui_main_window.window_id, offset_rect(rect, playlist_y))

    expected_command = (
        "command Panel(TogglePlaylistShade)"
        if button_id == "shade"
        else "command Panel(SetPlaylistVisibility(false))"
    )
    assert_event_log(
        egui_app_with_event_tracks,
        f"playlist: title button, button_name={button_name}",
        expected_command,
    )


@pytest.mark.parametrize(
    ("control", "control_name", "expected_event"),
    [event for event in EQUALIZER_CONTROL_EVENTS if event[0] is not EqualizerControl.PRESETS],
    ids=[event[0].value for event in EQUALIZER_CONTROL_EVENTS if event[0] is not EqualizerControl.PRESETS],
)
def test_egui_detached_equalizer_control_button_emits_event_without_activation_click(
    egui_detached_main_window: MainWindow,
    egui_detached_app_with_event_tracks: subprocess.Popen[bytes],
    control: EqualizerControl,
    control_name: str,
    expected_event: str | None,
) -> None:
    equalizer_window = wait_for_detached_panel_window(
        egui_detached_app_with_event_tracks,
        "Equalizer",
    )

    click_detached_skin_rect_without_activation_click(
        egui_detached_main_window,
        equalizer_window,
        EQUALIZER_CONTROL_RECTS[control],
    )

    assert_event_log(
        egui_detached_app_with_event_tracks,
        f"equalizer: control activated, control_name={control_name}",
        expected_event,
    )


def test_egui_detached_equalizer_title_shade_button_emits_event_without_activation_click(
    egui_detached_main_window: MainWindow,
    egui_detached_app_with_event_tracks: subprocess.Popen[bytes],
) -> None:
    equalizer_window = wait_for_detached_panel_window(
        egui_detached_app_with_event_tracks,
        "Equalizer",
    )

    click_detached_skin_rect_without_activation_click(
        egui_detached_main_window,
        equalizer_window,
        PANEL_SHADE_RECT,
    )

    assert_event_log(
        egui_detached_app_with_event_tracks,
        "command Panel(ToggleEqualizerShade)",
    )


@pytest.mark.parametrize(
    ("button", "button_name", "expected_event"),
    [event for event in PLAYLIST_FOOTER_EVENTS if event[0] is not PlaylistFooterButton.EJECT],
    ids=[event[0].value for event in PLAYLIST_FOOTER_EVENTS if event[0] is not PlaylistFooterButton.EJECT],
)
def test_egui_detached_playlist_footer_button_emits_event_without_activation_click(
    egui_detached_main_window: MainWindow,
    egui_detached_app_with_event_tracks: subprocess.Popen[bytes],
    button: PlaylistFooterButton,
    button_name: str,
    expected_event: str | None,
) -> None:
    playlist_window = wait_for_detached_panel_window(
        egui_detached_app_with_event_tracks,
        "Playlist",
    )

    click_detached_skin_rect_without_activation_click(
        egui_detached_main_window,
        playlist_window,
        PLAYLIST_FOOTER_RECTS[button],
    )

    assert_event_log(
        egui_detached_app_with_event_tracks,
        f"playlist: footer button, button_name={button_name}",
        expected_event,
    )


@pytest.mark.parametrize(
    ("menu", "menu_name"),
    PLAYLIST_MENU_EVENTS,
    ids=[menu.value for menu, _ in PLAYLIST_MENU_EVENTS],
)
def test_egui_detached_playlist_menu_button_emits_event_without_activation_click(
    egui_detached_main_window: MainWindow,
    egui_detached_app_with_event_tracks: subprocess.Popen[bytes],
    menu: PlaylistMenuButton,
    menu_name: str,
) -> None:
    playlist_window = wait_for_detached_panel_window(
        egui_detached_app_with_event_tracks,
        "Playlist",
    )

    click_detached_skin_rect_without_activation_click(
        egui_detached_main_window,
        playlist_window,
        PLAYLIST_MENU_RECTS[menu],
    )

    assert_event_log(
        egui_detached_app_with_event_tracks,
        f"playlist: menu opened, menu_name={menu_name}",
    )


def test_egui_detached_playlist_menu_button_changes_detached_window_image(
    egui_detached_main_window: MainWindow,
    egui_detached_app_with_event_tracks: subprocess.Popen[bytes],
    test_output: Any,
) -> None:
    playlist_window = wait_for_detached_panel_window(
        egui_detached_app_with_event_tracks,
        "Playlist",
    )
    run_xdotool("windowmove", playlist_window, "320", "280", check=False)
    run_xdotool("windowfocus", playlist_window, check=False)
    time.sleep(0.2)
    before = test_output.screenshot_path()
    screenshot_window(playlist_window, before)

    click_detached_skin_rect_without_activation_click(
        egui_detached_main_window,
        playlist_window,
        PLAYLIST_MENU_RECTS[PlaylistMenuButton.ADD],
    )
    assert_event_log(
        egui_detached_app_with_event_tracks,
        "playlist: menu opened, menu_name=Add",
    )
    time.sleep(0.2)
    after = test_output.screenshot_path()
    screenshot_window(playlist_window, after)

    assert before.read_bytes() != after.read_bytes()

    hover_x, hover_y = scaled_skin_point(playlist_window, PLAYLIST_ADD_FIRST_ITEM_RECT)
    geometry = window_geometry(playlist_window)
    run_xdotool("mousemove", str(geometry.x + hover_x), str(geometry.y + hover_y))
    time.sleep(0.3)
    hover = test_output.screenshot_path()
    screenshot_window(playlist_window, hover)

    assert hover.read_bytes() != after.read_bytes()


def test_egui_detached_playlist_title_shade_button_emits_event_without_activation_click(
    egui_detached_main_window: MainWindow,
    egui_detached_app_with_event_tracks: subprocess.Popen[bytes],
) -> None:
    playlist_window = wait_for_detached_panel_window(
        egui_detached_app_with_event_tracks,
        "Playlist",
    )

    click_detached_skin_rect_without_activation_click(
        egui_detached_main_window,
        playlist_window,
        PANEL_SHADE_RECT,
    )

    assert_event_log(
        egui_detached_app_with_event_tracks,
        "command Panel(TogglePlaylistShade)",
    )
