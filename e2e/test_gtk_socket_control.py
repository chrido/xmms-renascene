"""GTK frontend E2E tests driven through the JSON-lines control socket."""

from __future__ import annotations

import time
from importlib import import_module
from typing import Any

from gui import screenshot_screen, screenshot_tool_available, wait_for_no_visible_window, wait_for_visible_window


pytest: Any = import_module("pytest")


def assert_screenshot(path: Any) -> None:
    assert path.is_file()
    assert path.stat().st_size > 0


def test_socket_opens_and_closes_playlist_and_equalizer(
    control_client: Any,
    gtk_socket_main_window: Any,
    test_output: Any,
) -> None:
    """Drive panel visibility via socket commands and screenshot each state."""
    if not screenshot_tool_available():
        pytest.skip("Install ImageMagick 'import' or xwd to capture E2E screenshots")

    gtk_socket_main_window.focus_main_window()
    before = screenshot_screen(test_output.screenshot_path())

    control_client.command("show_playlist")
    time.sleep(0.3)
    playlist_open = screenshot_screen(test_output.screenshot_path())

    control_client.command("show_equalizer")
    time.sleep(0.3)
    both_open = screenshot_screen(test_output.screenshot_path())

    control_client.command("hide_playlist")
    control_client.command("hide_equalizer")
    time.sleep(0.3)
    closed = screenshot_screen(test_output.screenshot_path())

    for screenshot in [before, playlist_open, both_open, closed]:
        assert_screenshot(screenshot)


def test_socket_opens_and_closes_preferences(
    control_client: Any,
    gtk_socket_main_window: Any,
    test_output: Any,
) -> None:
    """Open and close Preferences via socket commands and screenshot each state."""
    if not screenshot_tool_available():
        pytest.skip("Install ImageMagick 'import' or xwd to capture E2E screenshots")

    gtk_socket_main_window.focus_main_window()
    before = screenshot_screen(test_output.screenshot_path())

    control_client.command("show_preferences")
    wait_for_visible_window("Preferences", timeout=3.0)
    time.sleep(0.3)
    opened = screenshot_screen(test_output.screenshot_path())

    control_client.command("hide_preferences")
    wait_for_no_visible_window("Preferences", timeout=3.0)
    gtk_socket_main_window.focus_main_window()
    closed = screenshot_screen(test_output.screenshot_path())

    for screenshot in [before, opened, closed]:
        assert_screenshot(screenshot)


def test_socket_shades_player_and_opens_menu(
    control_client: Any,
    gtk_socket_main_window: Any,
    test_output: Any,
) -> None:
    """Drive main-window UI state through socket commands and screenshot transitions."""
    if not screenshot_tool_available():
        pytest.skip("Install ImageMagick 'import' or xwd to capture E2E screenshots")

    gtk_socket_main_window.focus_main_window()
    before = screenshot_screen(test_output.screenshot_path())

    control_client.command("shade_main")
    time.sleep(0.3)
    shaded = screenshot_screen(test_output.screenshot_path())

    control_client.command("unshade_main")
    time.sleep(0.3)
    unshaded = screenshot_screen(test_output.screenshot_path())

    control_client.command("show_menu")
    time.sleep(0.3)
    menu_open = screenshot_screen(test_output.screenshot_path())

    control_client.command("hide_menu")
    time.sleep(0.3)
    menu_closed = screenshot_screen(test_output.screenshot_path())

    for screenshot in [before, shaded, unshaded, menu_open, menu_closed]:
        assert_screenshot(screenshot)
