"""GTK/egui zoom-level click tests.

The click helpers store base skin rectangles and calculate runtime click points
from the actual X11 window geometry. These tests start both frontends at several
zoom levels and verify controls still emit the expected events at each scale.
"""

from __future__ import annotations

import subprocess
import time
from collections.abc import Iterator
from importlib import import_module
from pathlib import Path
from typing import Any

from conftest import (
    GUI_FRONTENDS,
    GuiFrontend,
    assert_app_log_contains,
    start_gui_process,
    wait_for_main_window_with_log,
)
from gui import (
    BASE_MAIN_WIDTH,
    EQUALIZER_CONTROL_RECTS,
    MAIN_BUTTON_RECTS,
    PLAYLIST_FOOTER_RECTS,
    PLAYLIST_MENU_RECTS,
    EqualizerControl,
    MAIN_PLAYER_BASE_HEIGHT,
    MainButton,
    MainToggleButton,
    MainWindow,
    PlaylistFooterButton,
    PlaylistMenuButton,
    click_skin_rect,
    offset_rect,
    open_panel,
    run_xdotool,
)

pytest: Any = import_module("pytest")

EQUALIZER_WINDOW_TITLE = "XMMS Renascene Rust Equalizer"
PLAYLIST_WINDOW_TITLE = "XMMS Renascene Rust Playlist"
ZOOM_LEVELS = [1.0, 1.5, 2.0]
GUI_ZOOM_CASES = [
    pytest.param((frontend, zoom), id=f"{frontend.name}-{zoom:g}x")
    for frontend in GUI_FRONTENDS
    for zoom in ZOOM_LEVELS
]


@pytest.fixture(params=GUI_ZOOM_CASES)
def zoom_case(request: Any) -> tuple[GuiFrontend, float]:
    return request.param


@pytest.fixture
def zoomed_gui_app(
    tmp_path: Path,
    zoom_case: tuple[GuiFrontend, float],
) -> Iterator[subprocess.Popen[bytes]]:
    frontend, zoom = zoom_case
    yield from start_gui_process(
        tmp_path,
        frontend,
        ["--scale-factor", f"{zoom:g}"],
        log_name=f"xmms-{frontend.name}-{zoom:g}x.log",
    )


@pytest.fixture
def zoomed_main_window(zoomed_gui_app: subprocess.Popen[bytes]) -> MainWindow:
    return wait_for_main_window_with_log(zoomed_gui_app)


def scale_dim(value: int, zoom: float) -> int:
    return max(1, round(value * max(1.0, min(5.0, zoom))))


def assert_initial_geometry_uses_zoom(main_window: MainWindow, zoom: float) -> float:
    geometry = main_window.geometry()
    expected_width = scale_dim(BASE_MAIN_WIDTH, zoom)
    expected_height = scale_dim(MAIN_PLAYER_BASE_HEIGHT, zoom)
    assert abs(geometry.width - expected_width) <= 2
    assert abs(geometry.height - expected_height) <= 2
    dynamic_scale = geometry.width / BASE_MAIN_WIDTH
    assert abs(dynamic_scale - zoom) <= 0.02
    return dynamic_scale


def assert_main_button_point_uses_dynamic_geometry(main_window: MainWindow, button: MainButton) -> None:
    geometry = main_window.geometry()
    dynamic_scale = geometry.width / BASE_MAIN_WIDTH
    center_x, center_y = MAIN_BUTTON_RECTS[button].center()
    expected_point = (round(center_x * dynamic_scale), round(center_y * dynamic_scale))
    assert main_window.main_button_point(button) == expected_point


@pytest.mark.parametrize("button", [MainButton.PLAY], ids=["play"])
def test_gui_zoomed_main_button_click_uses_dynamic_geometry(
    zoomed_main_window: MainWindow,
    zoomed_gui_app: subprocess.Popen[bytes],
    zoom_case: tuple[GuiFrontend, float],
    button: MainButton,
) -> None:
    _frontend, zoom = zoom_case
    assert_initial_geometry_uses_zoom(zoomed_main_window, zoom)
    assert_main_button_point_uses_dynamic_geometry(zoomed_main_window, button)

    zoomed_main_window.click_main_button(button)

    assert_app_log_contains(zoomed_gui_app, "command Player(Play)")


def test_gui_zoomed_equalizer_button_click_uses_dynamic_panel_geometry(
    zoomed_main_window: MainWindow,
    zoomed_gui_app: subprocess.Popen[bytes],
    zoom_case: tuple[GuiFrontend, float],
) -> None:
    frontend, zoom = zoom_case
    assert_initial_geometry_uses_zoom(zoomed_main_window, zoom)

    equalizer_window, equalizer_y = open_panel(
        zoomed_main_window,
        MainToggleButton.EQUALIZER,
        EQUALIZER_WINDOW_TITLE,
    )
    click_skin_rect(
        equalizer_window,
        offset_rect(EQUALIZER_CONTROL_RECTS[EqualizerControl.ON], equalizer_y),
    )

    expected_events = [
        "command Panel(ToggleEqualizerVisibility)",
        "equalizer: control activated, control_name=On",
    ]
    if frontend.name == "egui":
        expected_events.append("command Equalizer(ToggleActive)")
    assert_app_log_contains(zoomed_gui_app, *expected_events)


def test_gui_zoomed_playlist_buttons_click_using_dynamic_panel_geometry(
    zoomed_main_window: MainWindow,
    zoomed_gui_app: subprocess.Popen[bytes],
    zoom_case: tuple[GuiFrontend, float],
) -> None:
    _frontend, zoom = zoom_case
    assert_initial_geometry_uses_zoom(zoomed_main_window, zoom)

    playlist_window, playlist_y = open_panel(
        zoomed_main_window,
        MainToggleButton.PLAYLIST,
        PLAYLIST_WINDOW_TITLE,
    )
    click_skin_rect(
        playlist_window,
        offset_rect(PLAYLIST_MENU_RECTS[PlaylistMenuButton.LIST], playlist_y),
    )
    time.sleep(0.1)
    run_xdotool("key", "Escape", check=False)
    click_skin_rect(
        playlist_window,
        offset_rect(PLAYLIST_FOOTER_RECTS[PlaylistFooterButton.NEXT], playlist_y),
    )

    assert_app_log_contains(
        zoomed_gui_app,
        "command Panel(TogglePlaylistVisibility)",
        "playlist: menu opened, menu_name=List",
        "playlist: footer button, button_name=Next",
        "command Player(NextTrack)",
    )
