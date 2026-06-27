"""GTK frontend coordinate-click smoke tests."""

from __future__ import annotations

import subprocess

from conftest import (
    BASE_MAIN_WIDTH,
    MAIN_WINDOW_TITLE,
    click_window_coordinate,
    wait_for_process_exit,
    wait_for_window,
    window_geometry,
)


def test_gtk_main_close_button_accepts_coordinate_click(
    gtk_app: subprocess.Popen[bytes],
) -> None:
    """Start GTK and click the skinned close button using window coordinates."""
    window_id = wait_for_window(MAIN_WINDOW_TITLE, gtk_app)
    geometry = window_geometry(window_id)
    scale = geometry["WIDTH"] / BASE_MAIN_WIDTH

    # Base-skin center of MainPushButton::Close is approximately (268.5, 7.5).
    close_x = round(269 * scale)
    close_y = round(8 * scale)
    click_window_coordinate(window_id, close_x, close_y)

    return_code = wait_for_process_exit(gtk_app)
    assert return_code == 0
