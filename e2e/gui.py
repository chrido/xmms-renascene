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


class MainToggleButton(str, Enum):
    """Skinned main-window toggle buttons addressed by base-skin geometry."""

    SHUFFLE = "shuffle"
    REPEAT = "repeat"
    EQUALIZER = "equalizer"
    PLAYLIST = "playlist"


class MainSlider(str, Enum):
    """Skinned main-window sliders addressed by base-skin geometry."""

    VOLUME = "volume"
    BALANCE = "balance"
    POSITION = "position"


class EqualizerControl(str, Enum):
    """Skinned equalizer buttons addressed by base-skin geometry."""

    ON = "on"
    AUTO = "auto"
    PRESETS = "presets"


class EqualizerSlider(str, Enum):
    """Skinned equalizer sliders addressed by base-skin geometry."""

    PREAMP = "preamp"
    BAND_0 = "band_0"
    BAND_1 = "band_1"
    BAND_2 = "band_2"
    BAND_3 = "band_3"
    BAND_4 = "band_4"
    BAND_5 = "band_5"
    BAND_6 = "band_6"
    BAND_7 = "band_7"
    BAND_8 = "band_8"
    BAND_9 = "band_9"


class PlaylistFooterButton(str, Enum):
    """Skinned playlist footer transport/scroll buttons."""

    PREVIOUS = "previous"
    PLAY = "play"
    PAUSE = "pause"
    STOP = "stop"
    NEXT = "next"
    EJECT = "eject"
    SCROLL_UP = "scroll_up"
    SCROLL_DOWN = "scroll_down"


class PlaylistMenuButton(str, Enum):
    """Skinned playlist bottom menu buttons."""

    ADD = "add"
    REMOVE = "remove"
    SELECT = "select"
    MISC = "misc"
    LIST = "list"


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

MAIN_TOGGLE_RECTS: dict[MainToggleButton, SkinRect] = {
    MainToggleButton.SHUFFLE: SkinRect(164, 89, 46, 15),
    MainToggleButton.REPEAT: SkinRect(210, 89, 28, 15),
    MainToggleButton.EQUALIZER: SkinRect(219, 58, 23, 12),
    MainToggleButton.PLAYLIST: SkinRect(242, 58, 23, 12),
}

MAIN_SLIDER_RECTS: dict[MainSlider, SkinRect] = {
    MainSlider.VOLUME: SkinRect(107, 57, 68, 13),
    MainSlider.BALANCE: SkinRect(177, 57, 38, 13),
    MainSlider.POSITION: SkinRect(16, 72, 248, 10),
}

EQUALIZER_CONTROL_RECTS: dict[EqualizerControl, SkinRect] = {
    EqualizerControl.ON: SkinRect(14, 18, 25, 12),
    EqualizerControl.AUTO: SkinRect(39, 18, 33, 12),
    EqualizerControl.PRESETS: SkinRect(217, 18, 44, 12),
}

PLAYLIST_DEFAULT_WIDTH = 275
PLAYLIST_DEFAULT_HEIGHT = 232

PLAYLIST_FOOTER_RECTS: dict[PlaylistFooterButton, SkinRect] = {
    PlaylistFooterButton.PREVIOUS: SkinRect(PLAYLIST_DEFAULT_WIDTH - 144, PLAYLIST_DEFAULT_HEIGHT - 16, 8, 7),
    PlaylistFooterButton.PLAY: SkinRect(PLAYLIST_DEFAULT_WIDTH - 138, PLAYLIST_DEFAULT_HEIGHT - 16, 10, 7),
    PlaylistFooterButton.PAUSE: SkinRect(PLAYLIST_DEFAULT_WIDTH - 128, PLAYLIST_DEFAULT_HEIGHT - 16, 10, 7),
    PlaylistFooterButton.STOP: SkinRect(PLAYLIST_DEFAULT_WIDTH - 118, PLAYLIST_DEFAULT_HEIGHT - 16, 9, 7),
    PlaylistFooterButton.NEXT: SkinRect(PLAYLIST_DEFAULT_WIDTH - 109, PLAYLIST_DEFAULT_HEIGHT - 16, 8, 7),
    PlaylistFooterButton.EJECT: SkinRect(PLAYLIST_DEFAULT_WIDTH - 100, PLAYLIST_DEFAULT_HEIGHT - 16, 9, 7),
    PlaylistFooterButton.SCROLL_UP: SkinRect(PLAYLIST_DEFAULT_WIDTH - 14, PLAYLIST_DEFAULT_HEIGHT - 35, 8, 5),
    PlaylistFooterButton.SCROLL_DOWN: SkinRect(PLAYLIST_DEFAULT_WIDTH - 14, PLAYLIST_DEFAULT_HEIGHT - 30, 8, 5),
}

PLAYLIST_MENU_RECTS: dict[PlaylistMenuButton, SkinRect] = {
    PlaylistMenuButton.ADD: SkinRect(12, PLAYLIST_DEFAULT_HEIGHT - 29, 25, 18),
    PlaylistMenuButton.REMOVE: SkinRect(41, PLAYLIST_DEFAULT_HEIGHT - 29, 25, 18),
    PlaylistMenuButton.SELECT: SkinRect(70, PLAYLIST_DEFAULT_HEIGHT - 29, 25, 18),
    PlaylistMenuButton.MISC: SkinRect(99, PLAYLIST_DEFAULT_HEIGHT - 29, 25, 18),
    PlaylistMenuButton.LIST: SkinRect(PLAYLIST_DEFAULT_WIDTH - 46, PLAYLIST_DEFAULT_HEIGHT - 29, 23, 18),
}


def equalizer_slider_rect(slider: EqualizerSlider) -> SkinRect:
    if slider is EqualizerSlider.PREAMP:
        return SkinRect(21, 38, 14, 63)
    try:
        band = int(slider.value.split("_", 1)[1])
    except (IndexError, ValueError) as exc:
        raise AssertionError(f"invalid equalizer slider value: {slider.value!r}") from exc
    return SkinRect(78 + band * 18, 38, 14, 63)


class ScreenshotToolUnavailable(RuntimeError):
    """Raised when no supported X11 screenshot tool is installed."""


@dataclass(frozen=True)
class ButtonPressScreenshots:
    """Screenshot triplet for a visual button-press interaction."""

    before: Path
    pressed: Path
    after: Path


@dataclass(frozen=True)
class ToggleOpenCloseScreenshots:
    """Screenshots for opening and closing a panel through a toggle."""

    before: Path
    opening_pressed: Path
    opened: Path
    closing_pressed: Path
    closed: Path


@dataclass(frozen=True)
class WindowOpenCloseScreenshots:
    """Screenshots for a top-level window open/close interaction."""

    before: Path
    opened: Path
    closed: Path


@dataclass(frozen=True)
class MenuWindowOpenCloseScreenshots:
    """Screenshots for opening a window through the main menu."""

    before: Path
    menu_open: Path
    opened: Path
    closed: Path


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
    return wait_for_visible_window(title, timeout=timeout, process=process)


def wait_for_visible_window(
    title: str,
    *,
    timeout: float = 10.0,
    process: subprocess.Popen[bytes] | None = None,
) -> str:
    deadline = time.monotonic() + timeout
    last_error = ""
    while time.monotonic() < deadline:
        if process is not None and process.poll() is not None:
            raise AssertionError(f"application exited before window appeared: {process.returncode}")
        result = run_xdotool("search", "--onlyvisible", "--name", title, check=False)
        if result.returncode == 0:
            windows = [line.strip() for line in result.stdout.splitlines() if line.strip()]
            if windows:
                return windows[0]
        last_error = (result.stderr or result.stdout).strip()
        time.sleep(0.1)
    raise TimeoutError(f"window named {title!r} did not appear; last xdotool output: {last_error}")


def wait_for_no_visible_window(title: str, *, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = run_xdotool("search", "--onlyvisible", "--name", title, check=False)
        windows = [line.strip() for line in result.stdout.splitlines() if line.strip()]
        if result.returncode != 0 or not windows:
            return
        time.sleep(0.1)
    raise TimeoutError(f"window named {title!r} remained visible")


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


def scaled_skin_point(window_id: str, rect: SkinRect, base_width: int = BASE_MAIN_WIDTH) -> tuple[int, int]:
    center_x, center_y = rect.center()
    geometry = window_geometry(window_id)
    scale = geometry.width / base_width
    return (round(center_x * scale), round(center_y * scale))


def click_skin_rect(window_id: str, rect: SkinRect, base_width: int = BASE_MAIN_WIDTH) -> None:
    x, y = scaled_skin_point(window_id, rect, base_width)
    click_window_coordinate(window_id, x, y)


def drag_skin_rect(
    window_id: str,
    rect: SkinRect,
    *,
    end_fraction: float,
    horizontal: bool,
    base_width: int = BASE_MAIN_WIDTH,
) -> None:
    geometry = window_geometry(window_id)
    scale = geometry.width / base_width
    if horizontal:
        start_x = round((rect.x + 1) * scale)
        end_x = round((rect.x + rect.width * end_fraction) * scale)
        y = round((rect.y + rect.height / 2) * scale)
        start = (start_x, y)
        end = (end_x, y)
    else:
        x = round((rect.x + rect.width / 2) * scale)
        start_y = round((rect.y + rect.height - 2) * scale)
        end_y = round((rect.y + rect.height * end_fraction) * scale)
        start = (x, start_y)
        end = (x, end_y)
    run_xdotool("windowactivate", "--sync", window_id, check=False)
    run_xdotool("mousemove", "--window", window_id, str(start[0]), str(start[1]))
    run_xdotool("mousedown", "1")
    try:
        time.sleep(0.05)
        run_xdotool("mousemove", "--window", window_id, str(end[0]), str(end[1]))
        time.sleep(0.05)
    finally:
        run_xdotool("mouseup", "1", check=False)


def drag_playlist_scrollbar_to_bottom(window_id: str, base_y: int = 0) -> None:
    geometry = window_geometry(window_id)
    scale = geometry.width / PLAYLIST_DEFAULT_WIDTH
    x = round((PLAYLIST_DEFAULT_WIDTH - 12) * scale)
    start_y = round((base_y + 20) * scale)
    end_y = round((base_y + PLAYLIST_DEFAULT_HEIGHT - 39) * scale)
    run_xdotool("windowactivate", "--sync", window_id, check=False)
    run_xdotool("mousemove", "--window", window_id, str(x), str(start_y))
    run_xdotool("mousedown", "1")
    try:
        time.sleep(0.05)
        run_xdotool("mousemove", "--window", window_id, str(x), str(end_y))
        time.sleep(0.05)
    finally:
        run_xdotool("mouseup", "1", check=False)


def drag_playlist_resize_handle(
    window_id: str,
    *,
    base_y: int = 0,
    delta_x: int = 0,
    delta_y: int = 58,
    base_width: int = PLAYLIST_DEFAULT_WIDTH,
) -> None:
    geometry = window_geometry(window_id)
    scale = geometry.width / base_width
    start_x = round((PLAYLIST_DEFAULT_WIDTH - 5) * scale)
    start_y = round((base_y + PLAYLIST_DEFAULT_HEIGHT - 5) * scale)
    run_xdotool("windowactivate", "--sync", window_id, check=False)
    run_xdotool("mousemove", "--window", window_id, str(start_x), str(start_y))
    run_xdotool("mousedown", "1")
    try:
        time.sleep(0.1)
        step_count = 8
        total_x = round(delta_x * scale)
        total_y = round(delta_y * scale)
        previous_x = 0
        previous_y = 0
        for step in range(1, step_count + 1):
            next_x = round(total_x * step / step_count)
            next_y = round(total_y * step / step_count)
            run_xdotool(
                "mousemove_relative",
                "--",
                str(next_x - previous_x),
                str(next_y - previous_y),
            )
            previous_x = next_x
            previous_y = next_y
            time.sleep(0.05)
        time.sleep(0.2)
    finally:
        run_xdotool("mouseup", "1", check=False)


def click_screen_coordinate(x: int, y: int) -> None:
    run_xdotool("mousemove", str(x), str(y), "click", "1")


def screenshot_window(window_id: str, requested_path: Path) -> Path:
    return _screenshot_x11_target(window_id, requested_path)


def screenshot_screen(requested_path: Path) -> Path:
    return _screenshot_x11_target("root", requested_path, root=True)


def _screenshot_x11_target(window_id: str, requested_path: Path, root: bool = False) -> Path:
    requested_path.parent.mkdir(parents=True, exist_ok=True)
    if command_exists("import"):
        import_args = ["import", "-window", window_id]
        if root:
            import_args.append("-screen")
        subprocess.run([*import_args, str(requested_path)], check=True)
        if requested_path.is_file() and requested_path.stat().st_size > 0:
            return requested_path
        raise AssertionError(f"screenshot command did not create {requested_path}")

    if command_exists("xwd"):
        actual_path = requested_path if requested_path.suffix == ".xwd" else requested_path.with_suffix(".xwd")
        xwd_target = ["-root"] if root else ["-id", window_id]
        subprocess.run(["xwd", "-silent", *xwd_target, "-out", str(actual_path)], check=True)
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
        return self.scale_skin_point(rects[button])

    def main_toggle_point(self, toggle: MainToggleButton) -> tuple[int, int]:
        return self.scale_skin_point(MAIN_TOGGLE_RECTS[toggle])

    def main_slider_point(self, slider: MainSlider) -> tuple[int, int]:
        return self.scale_skin_point(MAIN_SLIDER_RECTS[slider])

    def scale_skin_point(self, rect: SkinRect) -> tuple[int, int]:
        center_x, center_y = rect.center()
        geometry = self.geometry()
        scale = geometry.width / BASE_MAIN_WIDTH
        return (round(center_x * scale), round(center_y * scale))

    def click_main_button(self, button: MainButton, shaded: bool = False) -> None:
        x, y = self.main_button_point(button, shaded)
        click_window_coordinate(self.window_id, x, y)

    def click_main_toggle(self, toggle: MainToggleButton) -> None:
        x, y = self.main_toggle_point(toggle)
        click_window_coordinate(self.window_id, x, y)

    def drag_main_slider(self, slider: MainSlider, end_fraction: float) -> None:
        drag_skin_rect(
            self.window_id,
            MAIN_SLIDER_RECTS[slider],
            end_fraction=end_fraction,
            horizontal=True,
        )

    def screenshot(self, path: Path) -> Path:
        return screenshot_window(self.window_id, path)

    def screenshot_screen(self, path: Path) -> Path:
        return screenshot_screen(path)

    def focus_main_window(self, settle_delay: float = 0.2) -> None:
        run_xdotool("windowmap", self.window_id, check=False)
        run_xdotool("windowraise", self.window_id, check=False)
        run_xdotool("windowactivate", "--sync", self.window_id, check=False)
        time.sleep(settle_delay)

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

    def toggle_panel_with_screenshots(
        self,
        toggle: MainToggleButton,
        next_screenshot_path: Callable[[], Path],
        *,
        settle_delay: float = 0.1,
        transition_delay: float = 0.3,
    ) -> ToggleOpenCloseScreenshots:
        """Open and close a panel through its main toggle with root screenshots."""
        before = self.screenshot_screen(next_screenshot_path())
        opening_pressed = self.press_main_toggle_for_activation_screenshot(
            toggle,
            next_screenshot_path(),
            settle_delay=settle_delay,
        )
        time.sleep(transition_delay)
        opened = self.screenshot_screen(next_screenshot_path())
        closing_pressed = self.press_main_toggle_for_activation_screenshot(
            toggle,
            next_screenshot_path(),
            settle_delay=settle_delay,
        )
        time.sleep(transition_delay)
        closed = self.screenshot_screen(next_screenshot_path())
        return ToggleOpenCloseScreenshots(
            before=before,
            opening_pressed=opening_pressed,
            opened=opened,
            closing_pressed=closing_pressed,
            closed=closed,
        )

    def press_main_toggle_for_activation_screenshot(
        self,
        toggle: MainToggleButton,
        path: Path,
        *,
        settle_delay: float = 0.1,
    ) -> Path:
        """Hold a toggle, screenshot the root, then release on-toggle to activate."""
        x, y = self.main_toggle_point(toggle)
        run_xdotool("windowactivate", "--sync", self.window_id, check=False)
        run_xdotool("mousemove", "--window", self.window_id, str(x), str(y))
        run_xdotool("mousedown", "1")
        try:
            time.sleep(settle_delay)
            return self.screenshot_screen(path)
        finally:
            run_xdotool("mouseup", "1", check=False)

    def preferences_with_screenshots(
        self,
        next_screenshot_path: Callable[[], Path],
        *,
        transition_delay: float = 0.3,
    ) -> WindowOpenCloseScreenshots:
        """Open preferences via Ctrl+P, close it, and screenshot every state."""
        self.focus_main_window(transition_delay)
        before = self.screenshot_screen(next_screenshot_path())
        run_xdotool("key", "ctrl+p")
        preferences_window = wait_for_visible_window("Preferences", timeout=3.0)
        time.sleep(transition_delay)
        opened = self.screenshot_screen(next_screenshot_path())
        self.close_window(preferences_window)
        self.focus_main_window(transition_delay)
        closed = self.screenshot_screen(next_screenshot_path())
        return WindowOpenCloseScreenshots(before=before, opened=opened, closed=closed)

    def preferences_via_menu_with_screenshots(
        self,
        next_screenshot_path: Callable[[], Path],
        *,
        transition_delay: float = 0.3,
    ) -> MenuWindowOpenCloseScreenshots:
        """Open preferences by clicking the player menu, then close it."""
        self.focus_main_window(transition_delay)
        before = self.screenshot_screen(next_screenshot_path())
        self.click_main_button(MainButton.MENU)
        time.sleep(transition_delay)
        menu_open = self.screenshot_screen(next_screenshot_path())
        x, y = self.main_menu_preferences_point()
        click_screen_coordinate(x, y)
        preferences_window = wait_for_visible_window("Preferences", timeout=3.0)
        time.sleep(transition_delay)
        opened = self.screenshot_screen(next_screenshot_path())
        self.close_window(preferences_window)
        self.focus_main_window(transition_delay)
        closed = self.screenshot_screen(next_screenshot_path())
        return MenuWindowOpenCloseScreenshots(
            before=before,
            menu_open=menu_open,
            opened=opened,
            closed=closed,
        )

    def main_menu_preferences_point(self) -> tuple[int, int]:
        geometry = self.geometry()
        scale = geometry.width / BASE_MAIN_WIDTH
        # GTK positions the popover below the skinned menu button. Preferences
        # is the third menu item; these are base-skin-relative root coordinates.
        return (geometry.x + round(36 * scale), geometry.y + round(39 * scale))

    def close_window(self, window_id: str) -> None:
        run_xdotool("windowclose", window_id)
        wait_for_no_visible_window("Preferences", timeout=3.0)


def wait_for_process_exit(process: subprocess.Popen[bytes], timeout: float = 5.0) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        return_code = process.poll()
        if return_code is not None:
            return return_code
        time.sleep(0.05)
    raise TimeoutError("application did not exit after coordinate click")
