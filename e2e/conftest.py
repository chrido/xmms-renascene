"""Shared helpers for Python GUI E2E tests."""

from __future__ import annotations

import contextlib
import os
import re
import shutil
import subprocess
import time
from collections.abc import Iterator
from dataclasses import dataclass, field
from importlib import import_module
from pathlib import Path
from typing import Any

pytest: Any = import_module("pytest")

from gui import MainWindow


REPO_ROOT = Path(__file__).resolve().parents[1]
APP_BINARY = REPO_ROOT / "target" / "debug" / "xmms-rs"
MAIN_WINDOW_TITLE = "XMMS Renascene Rust Preview"
BASE_MAIN_WIDTH = 275


def sanitize_output_name(name: str) -> str:
    sanitized = re.sub(r"[^A-Za-z0-9_.-]+", "_", name).strip("_")
    return sanitized or "unnamed"


@dataclass
class TestOutput:
    """Per-test output folder with numbered screenshot paths."""

    directory: Path
    screenshot_count: int = field(default=0, init=False)

    def screenshot_path(self, suffix: str = ".png") -> Path:
        self.screenshot_count += 1
        normalized_suffix = suffix if suffix.startswith(".") else f".{suffix}"
        return self.directory / f"{self.screenshot_count}{normalized_suffix}"

    def create_video(self) -> Path | None:
        pngs = sorted(
            self.directory.glob("*.png"),
            key=lambda path: parse_xdotool_int(path.stem, path.name) if path.stem.isdigit() else 0,
        )
        if not pngs:
            return None
        if not command_exists("ffmpeg"):
            return None
        video = self.directory / "screenshots.mp4"
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-framerate",
                "1",
                "-start_number",
                "1",
                "-i",
                str(self.directory / "%d.png"),
                "-vf",
                "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                str(video),
            ],
            check=True,
        )
        if not video.is_file() or video.stat().st_size == 0:
            raise AssertionError(f"ffmpeg did not create {video}")
        return video



def command_exists(name: str) -> bool:
    return shutil.which(name) is not None


@pytest.fixture(scope="session", autouse=True)
def build_gtk_frontend() -> None:
    if os.environ.get("XMMS_E2E_SKIP_BUILD") == "1":
        if not APP_BINARY.exists():
            pytest.skip(f"{APP_BINARY} does not exist and XMMS_E2E_SKIP_BUILD=1")
        return
    if not command_exists("cargo"):
        pytest.skip("cargo is required to build the GTK frontend")
    subprocess.run(
        ["cargo", "build", "--manifest-path", "Cargo.toml", "--quiet"],
        cwd=REPO_ROOT,
        check=True,
    )


@pytest.fixture(scope="session", autouse=True)
def require_x11_tools() -> None:
    if not os.environ.get("DISPLAY"):
        pytest.skip("DISPLAY is not set; run with xvfb-run, e.g. xvfb-run -a python -m pytest e2e")
    if not command_exists("xdotool"):
        pytest.skip("xdotool is required for coordinate-based GUI E2E tests")


@pytest.fixture
def gtk_app(tmp_path: Path) -> Iterator[subprocess.Popen[bytes]]:
    log_path = tmp_path / "xmms-gtk.log"
    log = log_path.open("wb")
    env = os.environ.copy()
    env.pop("WAYLAND_DISPLAY", None)
    env.update(
        {
            "GDK_BACKEND": "x11",
            "GSK_RENDERER": env.get("GSK_RENDERER", "cairo"),
            "GDK_DISABLE": env.get("GDK_DISABLE", "gl"),
            "NO_AT_BRIDGE": "1",
            "XMMS_NON_UNIQUE": "1",
            "XMMS_RS_CONFIG_DIR": str(tmp_path / "config"),
        }
    )
    process = subprocess.Popen(
        [str(APP_BINARY), "--frontend", "gtk", "--reset"],
        cwd=REPO_ROOT,
        env=env,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    try:
        yield process
    finally:
        if process.poll() is None:
            process.terminate()
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=5)
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
        log.close()


@pytest.fixture
def gtk_main_window(gtk_app: subprocess.Popen[bytes]) -> MainWindow:
    return MainWindow.wait(MAIN_WINDOW_TITLE, gtk_app)


@pytest.fixture
def test_output(request: Any) -> Iterator[TestOutput]:
    output_root = Path(os.environ.get("XMMS_E2E_SCREENSHOT_DIR", str(REPO_ROOT / "testoutput")))
    output_dir = output_root / sanitize_output_name(request.node.name)
    output_dir.mkdir(parents=True, exist_ok=True)
    output = TestOutput(output_dir)
    yield output
    output.create_video()


@pytest.fixture
def e2e_screenshot_dir(test_output: TestOutput) -> Path:
    return test_output.directory


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


def window_geometry(window_id: str) -> dict[str, int]:
    result = run_xdotool("getwindowgeometry", "--shell", window_id)
    geometry: dict[str, int] = {}
    for line in result.stdout.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        if key in {"X", "Y", "WIDTH", "HEIGHT", "SCREEN"}:
            geometry[key] = parse_xdotool_int(value, line)
    missing = {"X", "Y", "WIDTH", "HEIGHT"} - set(geometry)
    if missing:
        raise AssertionError(f"missing geometry keys {missing} from: {result.stdout!r}")
    return geometry


def click_window_coordinate(window_id: str, x: int, y: int) -> None:
    # Some Xvfb setups have no window manager, so activation may fail; the click
    # itself uses coordinates relative to the target window and is the important part.
    run_xdotool("windowactivate", "--sync", window_id, check=False)
    run_xdotool("mousemove", "--window", window_id, str(x), str(y), "click", "1")


def wait_for_process_exit(process: subprocess.Popen[bytes], timeout: float = 5.0) -> int:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        return_code = process.poll()
        if return_code is not None:
            return return_code
        time.sleep(0.05)
    raise TimeoutError("application did not exit after coordinate click")
