"""GTK frontend coordinate-click smoke tests."""

from __future__ import annotations

import subprocess
from importlib import import_module
from pathlib import Path
from typing import Any

from gui import MainButton, MainWindow, screenshot_tool_available, wait_for_process_exit


pytest: Any = import_module("pytest")


def test_gtk_main_close_button_accepts_coordinate_click(
    gtk_main_window: MainWindow,
    gtk_app: subprocess.Popen[bytes],
) -> None:
    """Start GTK and click the skinned close button using window coordinates."""
    gtk_main_window.click_main_button(MainButton.CLOSE)

    return_code = wait_for_process_exit(gtk_app)
    assert return_code == 0


def test_gtk_main_pause_button_pressed_screenshot(
    gtk_main_window: MainWindow,
    e2e_screenshot_dir: Path,
    gtk_app: subprocess.Popen[bytes],
) -> None:
    """Hold the skinned Pause button down and capture that pressed state."""
    if not screenshot_tool_available():
        pytest.skip("Install ImageMagick 'import' or xwd to capture E2E screenshots")

    screenshot = gtk_main_window.press_main_button_and_screenshot(
        MainButton.PAUSE,
        e2e_screenshot_dir / "gtk-main-pause-pressed.png",
    )

    assert screenshot.is_file()
    assert screenshot.stat().st_size > 0

    gtk_main_window.click_main_button(MainButton.CLOSE)
    return_code = wait_for_process_exit(gtk_app)
    assert return_code == 0
