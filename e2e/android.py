"""ADB helpers for Android emulator E2E tests."""

from __future__ import annotations

import os
import re
import shlex
import subprocess
import tempfile
import time
from collections import defaultdict
from io import BytesIO
from dataclasses import dataclass
from pathlib import Path

from PIL import Image

from gui import BASE_MAIN_WIDTH, SkinRect


ANDROID_PACKAGE = "org.xmms.renascene"
ANDROID_ACTIVITY = f"{ANDROID_PACKAGE}/org.xmms.renascene.XmmsActivity"
ANDROID_AUTO_PROBE_ACTIVITY = (
    f"{ANDROID_PACKAGE}/org.xmms.renascene.XmmsAutoProbeActivity"
)
ANDROID_HOME_PACKAGE = "com.google.android.apps.nexuslauncher"
MAIN_PLAYER_BASE_HEIGHT = 116


@dataclass(frozen=True)
class DisplayGeometry:
    width: int
    height: int
    left_inset: int
    top_inset: int
    right_inset: int
    bottom_inset: int

    @property
    def usable_width(self) -> int:
        return self.width - self.left_inset - self.right_inset

    @property
    def usable_height(self) -> int:
        return self.height - self.top_inset - self.bottom_inset


@dataclass(frozen=True)
class AndroidDevice:
    adb: Path
    serial: str | None = None

    @classmethod
    def from_environment(cls) -> AndroidDevice:
        sdk = Path(
            os.environ.get(
                "ANDROID_HOME",
                str(Path.home() / ".local" / "share" / "android-sdk"),
            )
        )
        adb = sdk / "platform-tools" / "adb"
        if not adb.is_file():
            raise RuntimeError(f"Android adb was not found at {adb}")
        serial = os.environ.get("ANDROID_SERIAL")
        if serial is None:
            devices = subprocess.run(
                [str(adb), "devices"],
                text=True,
                capture_output=True,
                check=True,
            ).stdout
            serial = next(
                (
                    line.split("\t", 1)[0]
                    for line in devices.splitlines()
                    if line.startswith("emulator-") and "\tdevice" in line
                ),
                None,
            )
        return cls(adb, serial)

    def command(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        command = [str(self.adb)]
        if self.serial:
            command.extend(["-s", self.serial])
        command.extend(args)
        return subprocess.run(
            command,
            text=True,
            capture_output=True,
            check=check,
        )

    def shell(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        return self.command("shell", *args, check=check)

    def clear_logcat(self) -> None:
        self.command("logcat", "-c", check=False)

    def require_running_emulator(self) -> None:
        devices = self.command("devices").stdout
        if not any(
            line.startswith("emulator-") and "\tdevice" in line
            for line in devices.splitlines()
        ):
            raise RuntimeError(
                "No Android emulator is running; omit XMMS_E2E_ANDROID_SKIP_BUILD "
                "to let ./repo start the managed emulator"
            )

    def install_existing_apk(self) -> None:
        apk = Path(__file__).resolve().parents[1] / "target/debug/apk/xmms-renascene.apk"
        if not apk.is_file():
            raise RuntimeError(f"Android APK does not exist at {apk}")
        result = self.command("install", "-r", str(apk), check=False)
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).strip()
            raise RuntimeError(f"Could not install existing Android APK: {detail}")

    def apk_xmltree(self, resource: str) -> str:
        build_tools = os.environ.get("ANDROID_BUILD_TOOLS", "35.0.0")
        aapt = self.adb.parents[1] / "build-tools" / build_tools / "aapt"
        apk = Path(__file__).resolve().parents[1] / "target/debug/apk/xmms-renascene.apk"
        if not aapt.is_file():
            raise RuntimeError(f"Android aapt was not found at {aapt}")
        return subprocess.run(
            [str(aapt), "dump", "xmltree", str(apk), resource],
            text=True,
            capture_output=True,
            check=True,
        ).stdout

    def grant_runtime_permissions(self) -> None:
        self.shell(
            "pm",
            "grant",
            ANDROID_PACKAGE,
            "android.permission.POST_NOTIFICATIONS",
            check=False,
        )

    def force_stop(self) -> None:
        self.shell("am", "force-stop", ANDROID_PACKAGE, check=False)

    def app_pid(self) -> str:
        return self.shell("pidof", ANDROID_PACKAGE, check=False).stdout.strip()

    def close_activity(self) -> str:
        pid = self.app_pid()
        if not pid:
            raise AssertionError("Android app process is not running")
        self.shell("input", "keyevent", "4")
        time.sleep(1.0)
        return pid

    def go_home(self) -> None:
        self.shell("input", "keyevent", "3")
        self.wait_for_focus(ANDROID_HOME_PACKAGE)

    def start_activity(self) -> None:
        self.clear_logcat()
        self.shell(
            "am",
            "force-stop",
            "com.google.android.documentsui",
            check=False,
        )
        self.shell("am", "start", "-W", "-n", ANDROID_ACTIVITY)
        self.wait_for_app()

    def restart_app(self, *, reset_data: bool = False) -> None:
        self.force_stop()
        if reset_data:
            self.shell("pm", "clear", ANDROID_PACKAGE)
            self.grant_runtime_permissions()
        self.start_activity()

    def wait_for_app(self, timeout: float = 15.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            pid = self.shell("pidof", ANDROID_PACKAGE, check=False).stdout.strip()
            window_dump = self.shell("dumpsys", "window", check=False).stdout
            focus = next(
                (
                    line
                    for line in window_dump.splitlines()
                    if "mCurrentFocus=" in line
                ),
                "",
            )
            if pid and ANDROID_PACKAGE in focus:
                time.sleep(0.5)
                return
            time.sleep(0.2)
        raise TimeoutError("Android app did not become active")

    def wait_for_service(self, service_name: str, timeout: float = 5.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            services = self.shell(
                "dumpsys",
                "activity",
                "services",
                ANDROID_PACKAGE,
                check=False,
            ).stdout
            if service_name in services:
                return
            time.sleep(0.2)
        raise TimeoutError(f"Android service did not start: {service_name}")

    def wait_for_service_absent(self, service_name: str, timeout: float = 5.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            services = self.shell(
                "dumpsys",
                "activity",
                "services",
                ANDROID_PACKAGE,
                check=False,
            ).stdout
            if service_name not in services:
                return
            time.sleep(0.2)
        raise TimeoutError(f"Android service did not stop: {service_name}")

    def wait_for_focus(self, package: str, timeout: float = 5.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            window_dump = self.shell("dumpsys", "window", check=False).stdout
            focus = next(
                (
                    line
                    for line in window_dump.splitlines()
                    if "mCurrentFocus=" in line
                ),
                "",
            )
            if package in focus:
                time.sleep(0.7)
                return
            time.sleep(0.2)
        raise TimeoutError(f"Android package did not receive focus: {package}")

    def tap_ui_text(self, text: str, timeout: float = 5.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            remote_path = "/data/local/tmp/xmms-ui.xml"
            self.shell("uiautomator", "dump", remote_path, check=False)
            dump = self.shell("cat", remote_path, check=False).stdout
            for node in re.findall(r"<node\b[^>]*>", dump):
                label = re.search(r'\btext="([^"]*)"', node)
                bounds = re.search(
                    r'\bbounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"',
                    node,
                )
                if (
                    label is not None
                    and label.group(1).casefold() == text.casefold()
                    and bounds is not None
                ):
                    left, top, right, bottom = map(int, bounds.groups())
                    self.shell(
                        "input",
                        "tap",
                        str((left + right) // 2),
                        str((top + bottom) // 2),
                    )
                    return
            time.sleep(0.2)
        raise TimeoutError(f"Android UI text did not appear: {text}")

    def wait_for_external_file(self, path: str, timeout: float = 5.0) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.shell("test", "-f", path, check=False).returncode == 0:
                return
            time.sleep(0.2)
        raise TimeoutError(f"Android external file was not created: {path}")

    def set_portrait(self) -> None:
        self._set_rotation(0)

    def set_landscape(self) -> None:
        self._set_rotation(1)

    def _set_rotation(self, rotation: int) -> None:
        self.shell("settings", "put", "system", "accelerometer_rotation", "0")
        self.shell("settings", "put", "system", "user_rotation", str(rotation))
        expected_landscape = rotation in {1, 3}
        deadline = time.monotonic() + 10.0
        while time.monotonic() < deadline:
            geometry = self.display_geometry()
            if (geometry.width > geometry.height) == expected_landscape:
                time.sleep(0.5)
                return
            time.sleep(0.2)
        raise TimeoutError(f"Android display did not rotate to {rotation}")

    def display_geometry(self) -> DisplayGeometry:
        window_dump = self.shell("dumpsys", "window").stdout
        bounds = re.search(
            r"mGlobalConfiguration=.*?mBounds=Rect\(0, 0 - (\d+), (\d+)\)",
            window_dump,
        )
        if bounds is None:
            raise AssertionError("Could not determine Android display bounds")
        width, height = map(int, bounds.groups())
        left = top = right = bottom = 0
        source_pattern = re.compile(
            r"type=(?:statusBars|navigationBars|displayCutout) "
            r"frame=\[(\d+),(\d+)\]\[(\d+),(\d+)\] visible=true"
        )
        for match in source_pattern.finditer(window_dump):
            source_left, source_top, source_right, source_bottom = map(int, match.groups())
            if source_left == 0 and source_right == width and source_top == 0:
                top = max(top, source_bottom)
            elif source_left == 0 and source_right == width and source_bottom == height:
                bottom = max(bottom, height - source_top)
            elif source_top == 0 and source_bottom == height and source_left == 0:
                left = max(left, source_right)
            elif source_top == 0 and source_bottom == height and source_right == width:
                right = max(right, width - source_left)
        return DisplayGeometry(width, height, left, top, right, bottom)

    def main_player_scale(self) -> float:
        left, _top, right, _bottom = self.main_player_bounds()
        return (right - left) / BASE_MAIN_WIDTH

    def landscape_playlist_bounds(self) -> tuple[int, int, int, int]:
        deadline = time.monotonic() + 8.0
        while time.monotonic() < deadline:
            geometry = self.display_geometry()
            if geometry.width <= geometry.height:
                raise AssertionError("Android display is not in landscape")
            with Image.open(BytesIO(self.framebuffer_png())) as screenshot:
                left = geometry.width // 2
                right_region = screenshot.convert("L").crop(
                    (
                        left,
                        geometry.top_inset,
                        geometry.width - geometry.right_inset,
                        geometry.height - geometry.bottom_inset,
                    )
                )
                visible = right_region.point(lambda value: 255 if value >= 18 else 0)
                bounds = visible.getbbox()
            if bounds is not None:
                panel_left, panel_top, panel_right, panel_bottom = bounds
                if (
                    panel_right - panel_left >= geometry.usable_width * 0.2
                    and panel_bottom - panel_top >= geometry.usable_height * 0.2
                ):
                    return (
                        panel_left + left,
                        panel_top + geometry.top_inset,
                        panel_right + left,
                        panel_bottom + geometry.top_inset,
                    )
            time.sleep(0.2)
        raise AssertionError("Playlist did not move to the right side in landscape")

    def portrait_docked_stack_bounds(self) -> tuple[int, int, int, int]:
        deadline = time.monotonic() + 8.0
        while time.monotonic() < deadline:
            geometry = self.display_geometry()
            if geometry.width >= geometry.height:
                raise AssertionError("Android display is not in portrait")
            bounds = self.main_player_bounds()
            safe_right = geometry.width - geometry.right_inset
            if (
                abs(bounds[0] - geometry.left_inset) <= 4
                and abs(bounds[2] - safe_right) <= 4
                and bounds[3] - bounds[1] >= geometry.usable_height * 0.85
            ):
                return bounds
            time.sleep(0.2)
        raise AssertionError(
            "Portrait docked panels retained a landscape offset or black band"
        )

    def framebuffer_png(self) -> bytes:
        command = [str(self.adb)]
        if self.serial:
            command.extend(["-s", self.serial])
        command.extend(["exec-out", "screencap", "-p"])
        result = subprocess.run(command, capture_output=True, check=False)
        if result.returncode != 0 or not result.stdout:
            raise AssertionError("Could not capture the Android framebuffer")
        return result.stdout

    def wait_for_rendered_screen(
        self,
        *,
        changed_from: bytes | Path | None = None,
        timeout: float = 5.0,
        stable_for: float = 0.3,
        minimum_changed_fraction: float = 0.01,
    ) -> bytes:
        geometry = self.display_geometry()
        reference = (
            _rendered_screen_pixels(changed_from, geometry)
            if changed_from is not None
            else None
        )
        deadline = time.monotonic() + timeout
        candidate: tuple[tuple[int, int], bytes] | None = None
        candidate_since = 0.0
        latest_png = b""
        while time.monotonic() < deadline:
            latest_png = self.framebuffer_png()
            rendered = _rendered_screen_pixels(latest_png, geometry)
            if reference is not None and (
                _changed_pixel_fraction(reference, rendered)
                < minimum_changed_fraction
            ):
                candidate = None
                time.sleep(0.1)
                continue
            now = time.monotonic()
            if rendered != candidate:
                candidate = rendered
                candidate_since = now
            elif now - candidate_since >= stable_for:
                return latest_png
            time.sleep(0.1)
        expectation = (
            "change and stabilize" if reference is not None else "stabilize"
        )
        raise AssertionError(
            f"Android framebuffer did not {expectation} within {timeout:.1f}s"
        )

    def wait_for_rendered_screenshot(
        self,
        path: Path,
        *,
        changed_from: bytes | Path | None = None,
        timeout: float = 5.0,
    ) -> Path:
        png = self.wait_for_rendered_screen(
            changed_from=changed_from,
            timeout=timeout,
        )
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(png)
        return path

    def rendered_screens_match(
        self,
        first: bytes | Path,
        second: bytes | Path,
    ) -> bool:
        geometry = self.display_geometry()
        return _rendered_screen_pixels(
            first,
            geometry,
        ) == _rendered_screen_pixels(second, geometry)

    def main_player_bounds(self) -> tuple[int, int, int, int]:
        deadline = time.monotonic() + 5.0
        while time.monotonic() < deadline:
            geometry = self.display_geometry()
            with Image.open(BytesIO(self.framebuffer_png())) as screenshot:
                crop_right = (
                    geometry.width
                    if geometry.width <= geometry.height
                    else geometry.width // 2
                )
                crop_bottom = geometry.height - geometry.bottom_inset
                player_region = screenshot.convert("L").crop(
                    (0, 0, crop_right, crop_bottom)
                )
                visible = player_region.point(lambda value: 255 if value >= 18 else 0)
                bounds = visible.getbbox()
            if bounds is not None:
                left, _top, right, _bottom = bounds
                if right - left >= geometry.width * 0.2:
                    return bounds
            time.sleep(0.2)
        raise AssertionError("Could not find the rendered player in the Android screenshot")

    def tap_skin_rect(
        self,
        rect: SkinRect,
        bounds: tuple[int, int, int, int] | None = None,
    ) -> None:
        left, top, right, _bottom = bounds or self.main_player_bounds()
        scale = (right - left) / BASE_MAIN_WIDTH
        center_x, center_y = rect.center()
        x = round(left + center_x * scale)
        y = round(top + center_y * scale)
        self.shell("input", "tap", str(x), str(y))
        time.sleep(0.3)

    def tap_usable_fraction(self, x_fraction: float, y_fraction: float) -> None:
        geometry = self.display_geometry()
        x = round(geometry.left_inset + geometry.usable_width * x_fraction)
        y = round(geometry.top_inset + geometry.usable_height * y_fraction)
        self.shell("input", "tap", str(x), str(y))
        time.sleep(0.3)

    def tap_horizontal_button_group(
        self,
        button_index: int,
        *,
        button_count: int,
        timeout: float = 5.0,
    ) -> None:
        if not 0 <= button_index < button_count:
            raise ValueError("button_index must identify a button in the group")
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            geometry = self.display_geometry()
            with Image.open(BytesIO(self.framebuffer_png())) as screenshot:
                centers = _horizontal_button_group_centers(
                    screenshot.convert("RGB"),
                    geometry,
                    button_count,
                )
            if centers is not None:
                x, y = centers[button_index]
                self.shell("input", "tap", str(x), str(y))
                time.sleep(0.3)
                return
            time.sleep(0.2)
        raise AssertionError(
            f"Could not find a horizontal group of {button_count} Android buttons"
        )

    def swipe_usable_fraction(
        self,
        start_x_fraction: float,
        start_y_fraction: float,
        end_x_fraction: float,
        end_y_fraction: float,
        duration_ms: int = 300,
    ) -> None:
        geometry = self.display_geometry()
        start_x = round(
            geometry.left_inset + geometry.usable_width * start_x_fraction
        )
        start_y = round(
            geometry.top_inset + geometry.usable_height * start_y_fraction
        )
        end_x = round(geometry.left_inset + geometry.usable_width * end_x_fraction)
        end_y = round(geometry.top_inset + geometry.usable_height * end_y_fraction)
        self.shell(
            "input",
            "swipe",
            str(start_x),
            str(start_y),
            str(end_x),
            str(end_y),
            str(duration_ms),
        )
        time.sleep(0.4)

    def screenshot(self, path: Path) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(self.framebuffer_png())
        if not path.is_file() or path.stat().st_size == 0:
            raise AssertionError(f"Android screenshot was not created at {path}")
        return path

    def assert_log_contains(self, *needles: str, timeout: float = 5.0) -> str:
        deadline = time.monotonic() + timeout
        log = ""
        while time.monotonic() < deadline:
            log = self.command("logcat", "-d", check=False).stdout
            missing = [needle for needle in needles if needle not in log]
            if not missing:
                return log
            time.sleep(0.2)
        missing = [needle for needle in needles if needle not in log]
        raise AssertionError(
            "Android logcat did not contain expected entries:\n"
            + "\n".join(f"- {needle}" for needle in missing)
            + f"\n\nRecent logcat:\n{log[-12000:]}"
        )

    def wait_for_private_file_contains(
        self,
        path: str,
        needle: str,
        timeout: float = 5.0,
    ) -> str:
        deadline = time.monotonic() + timeout
        contents = ""
        while time.monotonic() < deadline:
            result = self.shell(
                "run-as",
                ANDROID_PACKAGE,
                "cat",
                path,
                check=False,
            )
            contents = result.stdout
            if needle in contents:
                return contents
            time.sleep(0.2)
        raise AssertionError(f"{path} did not contain {needle!r}:\n{contents}")

    def wait_for_private_file_not_contains(
        self,
        path: str,
        needle: str,
        timeout: float = 5.0,
    ) -> str:
        deadline = time.monotonic() + timeout
        contents = ""
        while time.monotonic() < deadline:
            result = self.shell(
                "run-as",
                ANDROID_PACKAGE,
                "cat",
                path,
                check=False,
            )
            contents = result.stdout
            if result.returncode == 0 and needle not in contents:
                return contents
            time.sleep(0.2)
        raise AssertionError(f"{path} still contained {needle!r}:\n{contents}")

    def wait_for_private_file_int_at_least(
        self,
        path: str,
        key: str,
        minimum: int,
        timeout: float = 5.0,
    ) -> int:
        deadline = time.monotonic() + timeout
        contents = ""
        while time.monotonic() < deadline:
            result = self.shell(
                "run-as",
                ANDROID_PACKAGE,
                "cat",
                path,
                check=False,
            )
            contents = result.stdout
            match = re.search(rf"^{re.escape(key)}=(-?\d+)$", contents, re.MULTILINE)
            if match is not None and int(match.group(1)) >= minimum:
                return int(match.group(1))
            time.sleep(0.2)
        raise AssertionError(
            f"{path} did not contain {key}>={minimum}:\n{contents}"
        )

    def read_private_file(self, path: str) -> str:
        result = self.shell(
            "run-as",
            ANDROID_PACKAGE,
            "cat",
            path,
            check=False,
        )
        if result.returncode != 0:
            detail = (result.stderr or result.stdout).strip()
            raise AssertionError(f"Could not read private file {path}: {detail}")
        return result.stdout

    def private_file_exists(self, path: str) -> bool:
        return (
            self.shell(
                "run-as",
                ANDROID_PACKAGE,
                "test",
                "-f",
                path,
                check=False,
            ).returncode
            == 0
        )

    def wait_for_private_file_absent(
        self,
        path: str,
        timeout: float = 5.0,
    ) -> None:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if not self.private_file_exists(path):
                return
            time.sleep(0.2)
        raise AssertionError(f"Private file still exists: {path}")

    def write_private_file(self, path: str, contents: str) -> None:
        self.write_private_bytes(path, contents.encode("utf-8"))

    def write_private_bytes(self, path: str, contents: bytes) -> None:
        parent = str(Path(path).parent)
        remote_path = "/data/local/tmp/xmms-renascene-e2e-upload"
        with tempfile.NamedTemporaryFile() as source:
            source.write(contents)
            source.flush()
            self.command("push", source.name, remote_path)
        try:
            script = (
                f"mkdir -p {shlex.quote(parent)} && "
                f"cp {shlex.quote(remote_path)} {shlex.quote(path)}"
            )
            self.command(
                "shell",
                f"run-as {shlex.quote(ANDROID_PACKAGE)} sh -c {shlex.quote(script)}",
            )
        finally:
            self.shell("rm", "-f", remote_path, check=False)


def _horizontal_button_group_centers(
    screenshot: Image.Image,
    geometry: DisplayGeometry,
    button_count: int,
) -> list[tuple[int, int]] | None:
    pixels = screenshot.load()
    min_width = round(geometry.usable_width * 0.15)
    max_width = round(geometry.usable_width * 0.32)
    runs: dict[tuple[tuple[int, int, int], int, int], list[int]] = defaultdict(list)
    for y in range(
        geometry.top_inset,
        geometry.height - geometry.bottom_inset,
    ):
        x = geometry.left_inset
        usable_right = geometry.width - geometry.right_inset
        while x < usable_right:
            color = pixels[x, y]
            left = x
            x += 1
            while x < usable_right and pixels[x, y] == color:
                x += 1
            if min_width <= x - left <= max_width:
                runs[(color, left, x)].append(y)

    candidates = [
        (color, left, right, min(ys), max(ys), len(ys))
        for (color, left, right), ys in runs.items()
        if len(ys) >= 20
    ]
    groups: list[
        tuple[int, list[tuple[tuple[int, int, int], int, int, int, int, int]]]
    ] = []
    for color in {candidate[0] for candidate in candidates}:
        colored = sorted(
            (candidate for candidate in candidates if candidate[0] == color),
            key=lambda candidate: candidate[1],
        )
        for start in range(len(colored) - button_count + 1):
            group = colored[start : start + button_count]
            if any(
                group[index][2] > group[index + 1][1]
                or group[index + 1][1] - group[index][2]
                > geometry.usable_width * 0.15
                for index in range(button_count - 1)
            ):
                continue
            overlap_top = max(candidate[3] for candidate in group)
            overlap_bottom = min(candidate[4] for candidate in group)
            if overlap_bottom - overlap_top < 20:
                continue
            covered_width = sum(candidate[2] - candidate[1] for candidate in group)
            if covered_width < geometry.usable_width * 0.55:
                continue
            groups.append(
                (
                    (overlap_bottom - overlap_top)
                    * sum(candidate[5] for candidate in group),
                    group,
                )
            )
    if not groups:
        return None
    group = max(groups, key=lambda item: item[0])[1]
    top = min(candidate[3] for candidate in group)
    bottom = max(candidate[4] for candidate in group)
    return [
        ((candidate[1] + candidate[2]) // 2, (top + bottom) // 2)
        for candidate in group
    ]


def _rendered_screen_pixels(
    screenshot: bytes | Path,
    geometry: DisplayGeometry,
) -> tuple[tuple[int, int], bytes]:
    contents = screenshot.read_bytes() if isinstance(screenshot, Path) else screenshot
    with Image.open(BytesIO(contents)) as image:
        rendered = image.convert("RGB").crop(
            (
                geometry.left_inset,
                geometry.top_inset,
                geometry.width - geometry.right_inset,
                geometry.height - geometry.bottom_inset,
            )
        )
        return rendered.size, rendered.tobytes()


def _changed_pixel_fraction(
    first: tuple[tuple[int, int], bytes],
    second: tuple[tuple[int, int], bytes],
) -> float:
    if first[0] != second[0]:
        return 1.0
    first_pixels = first[1]
    second_pixels = second[1]
    if len(first_pixels) != len(second_pixels):
        return 1.0
    sample_stride = 3 * 16
    sampled_offsets = range(0, len(first_pixels), sample_stride)
    changed = sum(
        first_pixels[offset : offset + 3] != second_pixels[offset : offset + 3]
        for offset in sampled_offsets
    )
    sample_count = (len(first_pixels) + sample_stride - 1) // sample_stride
    return changed / sample_count
