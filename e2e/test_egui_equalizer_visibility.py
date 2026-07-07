"""egui equalizer startup visibility regression tests."""

from __future__ import annotations

import subprocess
import time
from collections.abc import Iterator
from importlib import import_module
from pathlib import Path
from typing import Any

from conftest import EGUI_FRONTEND, assert_app_log_contains, start_gui_process, wait_for_main_window_with_log
from gui import (
    BASE_MAIN_WIDTH,
    EQUALIZER_CONTROL_RECTS,
    EQUALIZER_WINDOW_HEIGHT,
    MAIN_PLAYER_BASE_HEIGHT,
    EqualizerControl,
    MainWindow,
    click_skin_rect,
    offset_rect,
    wait_for_visible_window,
    window_geometry,
)

pytest: Any = import_module("pytest")

EQUALIZER_DETACHED_TITLE = "Equalizer"


@pytest.fixture
def egui_app_with_equalizer(tmp_path: Path) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(
        tmp_path,
        EGUI_FRONTEND,
        ["--equalizer"],
        log_name="xmms-egui-equalizer-enabled.log",
    )


@pytest.fixture
def egui_app_with_detached_equalizer(tmp_path: Path) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(
        tmp_path,
        EGUI_FRONTEND,
        ["--equalizer", "--equalizer-undocked"],
        log_name="xmms-egui-detached-equalizer-enabled.log",
    )


def wait_for_minimum_height(main_window: MainWindow, minimum_height: int) -> None:
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline:
        if main_window.geometry().height >= minimum_height:
            return
        time.sleep(0.1)
    raise AssertionError(
        f"main window height stayed below {minimum_height}; latest geometry={main_window.geometry()}"
    )


def test_egui_equalizer_is_visible_when_started_enabled(
    egui_app_with_equalizer: subprocess.Popen[bytes],
) -> None:
    main_window = wait_for_main_window_with_log(egui_app_with_equalizer)
    wait_for_minimum_height(main_window, MAIN_PLAYER_BASE_HEIGHT + EQUALIZER_WINDOW_HEIGHT)

    click_skin_rect(
        main_window.window_id,
        offset_rect(EQUALIZER_CONTROL_RECTS[EqualizerControl.ON], MAIN_PLAYER_BASE_HEIGHT),
    )

    assert_app_log_contains(
        egui_app_with_equalizer,
        "equalizer: control activated, control_name=On",
        "command Equalizer(ToggleActive)",
    )


def test_egui_detached_equalizer_window_is_visible_when_started_enabled(
    egui_app_with_detached_equalizer: subprocess.Popen[bytes],
) -> None:
    main_window = wait_for_main_window_with_log(egui_app_with_detached_equalizer)
    main_geometry = main_window.geometry()
    dynamic_scale = main_geometry.width / BASE_MAIN_WIDTH
    expected_main_height = round(MAIN_PLAYER_BASE_HEIGHT * dynamic_scale)
    assert abs(main_geometry.height - expected_main_height) <= 4

    equalizer_window = wait_for_visible_window(
        EQUALIZER_DETACHED_TITLE,
        timeout=5.0,
        process=egui_app_with_detached_equalizer,
    )
    geometry = window_geometry(equalizer_window)
    assert geometry.width >= 275
    assert geometry.height >= EQUALIZER_WINDOW_HEIGHT
