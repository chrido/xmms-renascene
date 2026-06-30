"""Small X11/skin abstractions for Python GUI E2E tests."""

from __future__ import annotations

import shutil
import subprocess
import time
from collections.abc import Callable
from dataclasses import dataclass
from enum import Enum
from pathlib import Path


BASE_MAIN_WIDTH = 275
BASE_MAIN_HEIGHT = 116
BASE_MAIN_SHADED_HEIGHT = 14


class MainButton(str, Enum):
    """Skinned main-window push buttons addressed by base-skin geometry."""

    MENU = "menu"
    MINIMIZE = "minimize"
    SHADE = "shade"
    CLOSE = "close"
    PREVIOUS = "previous"
    PLAY = "play"
    PAUSE = "pause"
    STOP = "stop"
    NEXT = "next"
    EJECT = "eject"


@dataclass(frozen=True)
class SkinRect:
    x: int
    y: int
    width: int
    height: int

    def center(self) -> tuple[float, float]:
        return (self.x + self.width / 2, self.y + self.height / 2)


@dataclass(frozen=True)
class WindowGeometry:
    x: int
    y: int
    width: int
    height: int


MAIN_BUTTON_RECTS: dict[MainButton, SkinRect] = {
    MainButton.MENU: SkinRect(6, 3, 9, 9),
    MainButton.MINIMIZE: SkinRect(244, 3, 9, 9),
    MainButton.SHADE: SkinRect(254, 3, 9, 9),
    MainButton.CLOSE: SkinRect(264, 3, 9, 9),
    MainButton.PREVIOUS: SkinRect(16, 88, 23, 18),
    MainButton.PLAY: SkinRect(39, 88, 23, 18),
    MainButton.PAUSE: SkinRect(62, 88, 23, 18),
    MainButton.STOP: SkinRect(85, 88, 23, 18),
    MainButton.NEXT: SkinRect(108, 88, 22, 18),
    MainButton.EJECT: SkinRect(136, 89, 22, 16),
}

MAIN_SHADED_BUTTON_RECTS: dict[MainButton, SkinRect] = {
    **MAIN_BUTTON_RECTS,
    MainButton.PREVIOUS: SkinRect(169, 4, 8, 7),
    MainButton.PLAY: SkinRect(177, 4, 10, 7),
    MainButton.PAUSE: SkinRect(187, 4, 10, 7),
    MainButton.STOP: SkinRect(197, 4, 9, 7),
    MainButton.NEXT: SkinRect(206, 4, 8, 7),
    MainButton.EJECT: SkinRect(216, 4, 9, 7),
}


class ScreenshotToolUnavailable(RuntimeError):
    """Raised when no supported X11 screenshot tool is installed."""


@dataclass(frozen=True)
class ButtonPressScreenshots:
    """Screenshot triplet for a visual button-press interaction."""

    before: Path
    pressed: Path
    after: Path


def command_exists(name: str) -> bool:
    return shutil.which(name) is not None


def screenshot_tool_available() -> bool:
    return command_exists("import") or command_exists("xwd")


def run_xdotool(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["xdotool", *args],
        text=True,
        capture_output=True,
        check=check,
    )


def wait_for_window(title: str, process: subprocess.Popen[bytes], timeout: float = 10.0) -> str:
    deadline = time.monotonic() + timeout
    last_error = ""
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise AssertionError(f"application exited before window appeared: {process.returncode}")
        result = run_xdotool("search", "--onlyvisible", "--name", title, check=False)
        if result.returncode == 0:
            windows = [line.strip() for line in result.stdout.splitlines() if line.strip()]
            if windows:
                return windows[0]
        last_error = (result.stderr or result.stdout).strip()
        time.sleep(0.1)
    raise TimeoutError(f"window named {title!r} did not appear; last xdotool output: {last_error}")


def parse_xdotool_int(value: str, source_line: str) -> int:
    try:
        return int(value)
    except ValueError as exc:
        raise AssertionError(f"invalid integer value in xdotool geometry: {source_line!r}") from exc


def window_geometry(window_id: str) -> WindowGeometry:
    result = run_xdotool("getwindowgeometry", "--shell", window_id)
    values: dict[str, int] = {}
    for line in result.stdout.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        if key in {"X", "Y", "WIDTH", "HEIGHT"}:
            values[key] = parse_xdotool_int(value, line)
    missing = {"X", "Y", "WIDTH", "HEIGHT"} - set(values)
    if missing:
        raise AssertionError(f"missing geometry keys {missing} from: {result.stdout!r}")
    return WindowGeometry(values["X"], values["Y"], values["WIDTH"], values["HEIGHT"])


def click_window_coordinate(window_id: str, x: int, y: int) -> None:
    # Some Xvfb setups have no window manager, so activation may fail; the click
    # itself uses coordinates relative to the target window and is the important part.
    run_xdotool("windowactivate", "--sync", window_id, check=False)
    run_xdotool("mousemove", "--window", window_id, str(x), str(y), "click", "1")


def screenshot_window(window_id: str, requested_path: Path) -> Path:
    requested_path.parent.mkdir(parents=True, exist_ok=True)
    if command_exists("import"):
        subprocess.run(["import", "-window", window_id, str(requested_path)], check=True)
        if requested_path.is_file() and requested_path.stat().st_size > 0:
            return requested_path
        raise AssertionError(f"screenshot command did not create {requested_path}")

    if command_exists("xwd"):
        actual_path = requested_path if requested_path.suffix == ".xwd" else requested_path.with_suffix(".xwd")
        subprocess.run(["xwd", "-silent", "-id", window_id, "-out", str(actual_path)], check=True)
        if actual_path.is_file() and actual_path.stat().st_size > 0:
            return actual_path
        raise AssertionError(f"screenshot command did not create {actual_path}")

    raise ScreenshotToolUnavailable("Install ImageMagick 'import' or xwd to capture E2E screenshots")


@dataclass(frozen=True)
class MainWindow:
    """Coordinate driver for the skinned main player window."""

    window_id: str

    @classmethod
    def wait(cls, title: str, process: subprocess.Popen[bytes], timeout: float = 10.0) -> MainWindow:
        return cls(wait_for_window(title, process, timeout))

    def geometry(self) -> WindowGeometry:
        return window_geometry(self.window_id)

    def main_button_point(self, button: MainButton, shaded: bool = False) -> tuple[int, int]:
        rects = MAIN_SHADED_BUTTON_RECTS if shaded else MAIN_BUTTON_RECTS
        base_height = BASE_MAIN_SHADED_HEIGHT if shaded else BASE_MAIN_HEIGHT
        rect = rects[button]
        center_x, center_y = rect.center()
        geometry = self.geometry()
        return (
            round(center_x * geometry.width / BASE_MAIN_WIDTH),
            round(center_y * geometry.height / base_height),
        )

    def click_main_button(self, button: MainButton, shaded: bool = False) -> None:
        x, y = self.main_button_point(button, shaded)
        click_window_coordinate(self.window_id, x, y)

    def screenshot(self, path: Path) -> Path:
        return screenshot_window(self.window_id, path)

    def press_main_button_and_screenshot(
        self,
        button: MainButton,
        path: Path,
        *,
        shaded: bool = False,
        settle_delay: float = 0.1,
    ) -> Path:
        """Hold a skinned button down, capture the window, then release it."""
        x, y = self.main_button_point(button, shaded)
        run_xdotool("windowactivate", "--sync", self.window_id, check=False)
        run_xdotool("mousemove", "--window", self.window_id, str(x), str(y))
        run_xdotool("mousedown", "1")
        try:
            time.sleep(settle_delay)
            return self.screenshot(path)
        finally:
            run_xdotool("mouseup", "1", check=False)

    def press_main_button_with_screenshots(
        self,
        button: MainButton,
        next_screenshot_path: Callable[[], Path],
        *,
        shaded: bool = False,
        settle_delay: float = 0.1,
        after_delay: float = 0.1,
    ) -> ButtonPressScreenshots:
        """Capture before, pressed, and after screenshots for a main button.

        The mouse is released outside the button so visual-state tests can cover
        destructive or disruptive controls such as Close, Minimize, Shade, and
        Eject without closing, hiding, resizing, or opening dialogs.
        """
        before = self.screenshot(next_screenshot_path())
        x, y = self.main_button_point(button, shaded)
        run_xdotool("windowactivate", "--sync", self.window_id, check=False)
        run_xdotool("mousemove", "--window", self.window_id, str(x), str(y))
        run_xdotool("mousedown", "1")
        try:
            time.sleep(settle_delay)
            pressed = self.screenshot(next_screenshot_path())
        finally:
            run_xdotool("mousemove", "--window", self.window_id, "0", "0", check=False)
            run_xdotool("mouseup", "1", check=False)
        time.sleep(after_delay)
        after = self.screenshot(next_screenshot_path())
        return ButtonPressScreenshots(before=before, pressed=pressed, after=after)


def wait_for_process_exit(process: subprocess.Popen[bytes], timeout: float = 5.0) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        return_code = process.poll()
        if return_code is not None:
            return return_code
        time.sleep(0.05)
    raise TimeoutError("application did not exit after coordinate click")
