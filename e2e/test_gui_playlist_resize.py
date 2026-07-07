"""Live GTK/egui playlist resize regression tests."""

from __future__ import annotations

import contextlib
import subprocess
import time
from importlib import import_module
from pathlib import Path
from typing import Any

from conftest import (
    EGUI_FRONTEND,
    GTK_FRONTEND,
    GUI_FRONTENDS,
    GuiFrontend,
    start_gui_process,
    wait_for_main_window_with_log,
)
from gui import (
    MAIN_PLAYER_BASE_HEIGHT,
    MainWindow,
    drag_playlist_resize_handle,
    wait_for_visible_window,
    window_geometry,
)

pytest: Any = import_module("pytest")

GTK_PLAYLIST_TITLE = "XMMS Renascene Rust Playlist"
EGUI_PLAYLIST_TITLE = "Playlist"


@pytest.fixture(params=GUI_FRONTENDS, ids=[frontend.name for frontend in GUI_FRONTENDS])
def frontend(request: Any) -> GuiFrontend:
    return request.param


def wait_for_geometry(
    window_id: str,
    predicate: Any,
    *,
    timeout: float = 5.0,
) -> None:
    deadline = time.monotonic() + timeout
    last = window_geometry(window_id)
    while time.monotonic() < deadline:
        last = window_geometry(window_id)
        if predicate(last):
            return
        time.sleep(0.1)
    raise AssertionError(f"window geometry did not satisfy predicate; latest={last}")


def detached_playlist_title(frontend: GuiFrontend) -> str:
    if frontend is EGUI_FRONTEND:
        return EGUI_PLAYLIST_TITLE
    if frontend is GTK_FRONTEND:
        return GTK_PLAYLIST_TITLE
    raise AssertionError(f"unknown frontend: {frontend}")


def test_docked_playlist_resize_handle_changes_height(
    tmp_path: Path,
    frontend: GuiFrontend,
) -> None:
    with contextlib.contextmanager(start_gui_process)(
        tmp_path,
        frontend,
        ["--playlist"],
        log_name=f"xmms-{frontend.name}-docked-playlist-resize.log",
    ) as process:
        assert isinstance(process, subprocess.Popen)
        main_window = wait_for_main_window_with_log(process)
        before = main_window.geometry()

        drag_playlist_resize_handle(
            main_window.window_id,
            base_y=MAIN_PLAYER_BASE_HEIGHT,
            delta_y=58,
        )

        wait_for_geometry(
            main_window.window_id,
            lambda geometry: geometry.height >= before.height + 50,
        )


def test_detached_playlist_resize_handle_changes_size(
    tmp_path: Path,
    frontend: GuiFrontend,
) -> None:
    with contextlib.contextmanager(start_gui_process)(
        tmp_path,
        frontend,
        ["--playlist", "--playlist-undocked"],
        log_name=f"xmms-{frontend.name}-detached-playlist-resize.log",
    ) as process:
        assert isinstance(process, subprocess.Popen)
        _main_window: MainWindow = wait_for_main_window_with_log(process)
        playlist_window = wait_for_visible_window(
            detached_playlist_title(frontend),
            process=process,
        )
        before = window_geometry(playlist_window)

        drag_playlist_resize_handle(
            playlist_window,
            delta_x=50,
            delta_y=58,
        )

        wait_for_geometry(
            playlist_window,
            lambda geometry: geometry.width >= before.width + 40
            and geometry.height >= before.height + 50,
        )
