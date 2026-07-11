"""ADB helpers for Android emulator E2E tests."""

from __future__ import annotations

import os
import re
import shlex
import subprocess
import tempfile
import time
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

    def start_activity(self) -> None:
        self.clear_logcat()
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
            focus = self.shell("dumpsys", "window", check=False).stdout
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

    def framebuffer_png(self) -> bytes:
        command = [str(self.adb)]
        if self.serial:
            command.extend(["-s", self.serial])
        command.extend(["exec-out", "screencap", "-p"])
        result = subprocess.run(command, capture_output=True, check=False)
        if result.returncode != 0 or not result.stdout:
            raise AssertionError("Could not capture the Android framebuffer")
        return result.stdout

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
