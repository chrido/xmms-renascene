"""Detached egui playlist interaction parity tests."""

from __future__ import annotations

import contextlib
import subprocess
import time
from pathlib import Path
from typing import Any

from conftest import (
    EGUI_FRONTEND,
    assert_app_log_contains,
    start_gui_process,
    wait_for_main_window_with_log,
)
from gui import (
    PLAYLIST_DEFAULT_HEIGHT,
    PLAYLIST_DEFAULT_WIDTH,
    MainWindow,
    SkinRect,
    run_xdotool,
    scaled_skin_point,
    screenshot_window,
    wait_for_visible_window,
    window_geometry,
)

EGUI_PLAYLIST_TITLE = "Playlist"
PLAYLIST_FIRST_ROW_RECT = SkinRect(12, 20, PLAYLIST_DEFAULT_WIDTH - 31, 11)


def focus_detached_window_for_xvfb(main_window: MainWindow, window_id: str) -> None:
    # Xvfb usually has no window manager; move detached egui windows away from
    # the root window and focus the X window so synthetic events are delivered.
    run_xdotool("windowmove", window_id, "320", "280", check=False)
    time.sleep(0.1)
    main_window.focus_main_window()
    run_xdotool("windowfocus", window_id, check=False)
    time.sleep(0.4)


def double_click_detached_rect(window_id: str, rect: SkinRect) -> None:
    x, y = scaled_skin_point(window_id, rect, base_width=PLAYLIST_DEFAULT_WIDTH)
    geometry = window_geometry(window_id)
    run_xdotool("mousemove", str(geometry.x + x), str(geometry.y + y))
    for _ in range(2):
        run_xdotool("mousedown", "1")
        time.sleep(0.04)
        run_xdotool("mouseup", "1")
        time.sleep(0.08)


def drag_detached_playlist_scrollbar_to_bottom(window_id: str) -> None:
    geometry = window_geometry(window_id)
    scale = geometry.width / PLAYLIST_DEFAULT_WIDTH
    x = round((PLAYLIST_DEFAULT_WIDTH - 12) * scale)
    start_y = round(20 * scale)
    end_y = round((PLAYLIST_DEFAULT_HEIGHT - 39) * scale)
    run_xdotool("mousemove", "--window", window_id, str(x), str(start_y))
    run_xdotool("mousedown", "1")
    try:
        time.sleep(0.1)
        for step in range(1, 9):
            y = round(start_y + (end_y - start_y) * step / 8)
            run_xdotool("mousemove", "--window", window_id, str(x), str(y))
            time.sleep(0.05)
    finally:
        run_xdotool("mouseup", "1", check=False)


@contextlib.contextmanager
def start_detached_playlist_app(
    tmp_path: Path,
    generated_tracks: list[Path],
) -> Any:
    yield from start_gui_process(
        tmp_path,
        EGUI_FRONTEND,
        ["--playlist", "--playlist-undocked", *(str(track) for track in generated_tracks)],
        log_name="xmms-egui-detached-playlist-interactions.log",
    )


def test_egui_detached_playlist_double_click_starts_selected_track(
    tmp_path: Path,
    generated_tracks: list[Path],
) -> None:
    with start_detached_playlist_app(tmp_path, generated_tracks) as process:
        assert isinstance(process, subprocess.Popen)
        _main_window: MainWindow = wait_for_main_window_with_log(process)
        playlist_window = wait_for_visible_window(EGUI_PLAYLIST_TITLE, process=process)
        focus_detached_window_for_xvfb(_main_window, playlist_window)

        double_click_detached_rect(playlist_window, PLAYLIST_FIRST_ROW_RECT)

        assert_app_log_contains(
            process,
            "command Playlist(SetPosition(0))",
            "command Player(StartCurrentTrack)",
        )


def test_egui_detached_playlist_scrollbar_drag_updates_visible_rows(
    tmp_path: Path,
    generated_tracks: list[Path],
    test_output: Any,
) -> None:
    with start_detached_playlist_app(tmp_path, generated_tracks) as process:
        assert isinstance(process, subprocess.Popen)
        _main_window: MainWindow = wait_for_main_window_with_log(process)
        playlist_window = wait_for_visible_window(EGUI_PLAYLIST_TITLE, process=process)
        focus_detached_window_for_xvfb(_main_window, playlist_window)

        before = test_output.screenshot_path()
        screenshot_window(playlist_window, before)
        drag_detached_playlist_scrollbar_to_bottom(playlist_window)
        time.sleep(0.4)
        after = test_output.screenshot_path()
        screenshot_window(playlist_window, after)

        assert before.read_bytes() != after.read_bytes()
