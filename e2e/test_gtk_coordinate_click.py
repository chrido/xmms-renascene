"""GTK frontend coordinate-click smoke tests."""

from __future__ import annotations

import subprocess
from importlib import import_module
from typing import Any

from gui import MainButton, MainWindow, screenshot_tool_available, wait_for_process_exit


pytest: Any = import_module("pytest")

MAIN_PLAYER_BUTTONS = [
    MainButton.MENU,
    MainButton.MINIMIZE,
    MainButton.SHADE,
    MainButton.CLOSE,
    MainButton.PREVIOUS,
    MainButton.PLAY,
    MainButton.PAUSE,
    MainButton.STOP,
    MainButton.NEXT,
    MainButton.EJECT,
]


def test_gtk_main_close_button_accepts_coordinate_click(
    gtk_main_window: MainWindow,
    gtk_app: subprocess.Popen[bytes],
) -> None:
    """Start GTK and click the skinned close button using window coordinates."""
    gtk_main_window.click_main_button(MainButton.CLOSE)

    return_code = wait_for_process_exit(gtk_app)
    assert return_code == 0


@pytest.mark.parametrize("button", MAIN_PLAYER_BUTTONS, ids=[button.value for button in MAIN_PLAYER_BUTTONS])
def test_gtk_main_button_pressed_screenshot(
    gtk_main_window: MainWindow,
    test_output: Any,
    button: MainButton,
) -> None:
    """Hold each skinned main-player button down and capture that pressed state."""
    if not screenshot_tool_available():
        pytest.skip("Install ImageMagick 'import' or xwd to capture E2E screenshots")

    screenshots = gtk_main_window.press_main_button_with_screenshots(
        button,
        test_output.screenshot_path,
    )

    for screenshot in [screenshots.before, screenshots.pressed, screenshots.after]:
        assert screenshot.is_file()
        assert screenshot.stat().st_size > 0
