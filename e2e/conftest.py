"""Shared helpers for Python GUI E2E tests."""

from __future__ import annotations

import contextlib
import os
import re
import shutil
import subprocess
import time
from collections.abc import Iterable, Iterator
from dataclasses import dataclass, field
from importlib import import_module
from pathlib import Path
from typing import Any

pytest: Any = import_module("pytest")

control_socket: Any = import_module("control_socket")
from android import AndroidDevice
from gui import MainWindow, parse_xdotool_int


REPO_ROOT = Path(__file__).resolve().parents[1]
TARGET_DIR = Path(os.environ.get("CARGO_TARGET_DIR", str(REPO_ROOT / "target")))
if not TARGET_DIR.is_absolute():
    TARGET_DIR = REPO_ROOT / TARGET_DIR
APP_BINARY = TARGET_DIR / "debug" / "xmms-rs"
MAIN_WINDOW_TITLE = "XMMS Renascene Rust Preview"
EGUI_WINDOW_TITLE = "XMMS Renascene egui"
BASE_MAIN_WIDTH = 275


@dataclass(frozen=True)
class GuiFrontend:
    name: str
    window_title: str


GTK_FRONTEND = GuiFrontend("gtk", MAIN_WINDOW_TITLE)
EGUI_FRONTEND = GuiFrontend("egui", EGUI_WINDOW_TITLE)
GUI_FRONTENDS = (GTK_FRONTEND, EGUI_FRONTEND)


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



def command_path(name: str) -> str | None:
    found = shutil.which(name)
    if found is not None:
        return found
    cargo_bin_candidate = Path("/usr/local/cargo/bin") / name
    if cargo_bin_candidate.is_file():
        return str(cargo_bin_candidate)
    return None


def command_exists(name: str) -> bool:
    return command_path(name) is not None


@pytest.fixture(scope="session", autouse=True)
def build_gui_frontends(request: Any) -> None:
    if request.session.items and all(
        item.get_closest_marker("android") is not None for item in request.session.items
    ):
        return
    if os.environ.get("XMMS_E2E_SKIP_BUILD") == "1":
        if not APP_BINARY.exists():
            pytest.skip(f"{APP_BINARY} does not exist and XMMS_E2E_SKIP_BUILD=1")
        return
    cargo = command_path("cargo")
    if cargo is None:
        pytest.skip("cargo is required to build the GTK/egui frontends")
    assert cargo is not None
    build_env = os.environ.copy()
    build_env["PATH"] = f"/usr/local/cargo/bin:{build_env.get('PATH', '')}"
    subprocess.run(
        [cargo, "build", "--manifest-path", "Cargo.toml", "--features", "egui-ui", "--quiet"],
        cwd=REPO_ROOT,
        env=build_env,
        check=True,
    )


@pytest.fixture(autouse=True)
def require_x11_tools(request: Any) -> None:
    if request.node.get_closest_marker("android") is not None:
        return
    if not os.environ.get("DISPLAY"):
        pytest.skip("DISPLAY is not set; run with xvfb-run, e.g. xvfb-run -a python -m pytest e2e")
    if request.node.get_closest_marker("no_xdotool") is not None:
        return
    if not command_exists("xdotool"):
        pytest.skip("xdotool is required for coordinate-based GUI E2E tests")


def gui_environment(tmp_path: Path) -> dict[str, str]:
    env = os.environ.copy()
    env.pop("WAYLAND_DISPLAY", None)
    env.update(
        {
            "GDK_BACKEND": "x11",
            "GSK_RENDERER": env.get("GSK_RENDERER", "cairo"),
            "GDK_DISABLE": env.get("GDK_DISABLE", "gl"),
            "NO_AT_BRIDGE": "1",
            "WINIT_UNIX_BACKEND": "x11",
            "WGPU_BACKEND": env.get("WGPU_BACKEND", "gl"),
            "LIBGL_ALWAYS_SOFTWARE": env.get("LIBGL_ALWAYS_SOFTWARE", "1"),
            "XMMS_GSTREAMER_AUDIO_SINK": env.get(
                "XMMS_GSTREAMER_AUDIO_SINK", "fakesink"
            ),
            "XMMS_NON_UNIQUE": "1",
            "XMMS_RS_CONFIG_DIR": str(tmp_path / "config"),
            "XMMS_RS_LOG": env.get("XMMS_RS_LOG", "trace"),
            "RUST_BACKTRACE": env.get("RUST_BACKTRACE", "1"),
        }
    )
    return env


def gtk_environment(tmp_path: Path) -> dict[str, str]:
    return gui_environment(tmp_path)


def start_gui_process(
    tmp_path: Path,
    frontend: GuiFrontend,
    extra_args: list[str] | None = None,
    log_name: str | None = None,
    extra_env: dict[str, str] | None = None,
) -> Iterator[subprocess.Popen[bytes]]:
    log_path = tmp_path / (log_name or f"xmms-{frontend.name}.log")
    log = log_path.open("wb")
    env = gui_environment(tmp_path)
    if extra_env:
        env.update(extra_env)
    process = subprocess.Popen(
        [str(APP_BINARY), "--frontend", frontend.name, "--reset", *(extra_args or [])],
        cwd=REPO_ROOT,
        env=env,
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    setattr(process, "xmms_log_path", log_path)
    setattr(process, "xmms_frontend", frontend.name)
    setattr(process, "xmms_window_title", frontend.window_title)
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


def start_gtk_process(tmp_path: Path, extra_args: list[str] | None = None, log_name: str = "xmms-gtk.log") -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(tmp_path, GTK_FRONTEND, extra_args, log_name)


@pytest.fixture(params=GUI_FRONTENDS, ids=[frontend.name for frontend in GUI_FRONTENDS])
def gui_frontend(request: Any) -> GuiFrontend:
    return request.param


@pytest.fixture
def gui_app(tmp_path: Path, gui_frontend: GuiFrontend) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(tmp_path, gui_frontend)


@pytest.fixture
def gtk_app(tmp_path: Path) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gtk_process(tmp_path)


def generate_sine_track(path: Path, frequency: int, duration: float) -> Path:
    subprocess.run(
        [
            "ffmpeg",
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            f"sine=frequency={frequency}:duration={duration}",
            "-ac",
            "2",
            "-ar",
            "44100",
            str(path),
        ],
        cwd=REPO_ROOT,
        check=True,
    )
    if not path.is_file() or path.stat().st_size == 0:
        raise AssertionError(f"ffmpeg did not create {path}")
    return path


def generate_sine_tracks(
    tracks_dir: Path,
    specs: Iterable[tuple[str, int, float]],
    *,
    skip_message: str = "ffmpeg is required to create E2E audio tracks",
) -> list[Path]:
    if not command_exists("ffmpeg"):
        pytest.skip(skip_message)
    tracks_dir.mkdir(parents=True, exist_ok=True)
    return [
        generate_sine_track(tracks_dir / filename, frequency, duration)
        for filename, frequency, duration in specs
    ]


@pytest.fixture
def generated_tracks(tmp_path: Path) -> list[Path]:
    return generate_sine_tracks(
        tmp_path / "tracks",
        [
            (f"xmms-e2e-track-{index:02}.wav", 440 + index * 20, 2.0)
            for index in range(18)
        ],
    )


@pytest.fixture
def gui_app_with_tracks(
    tmp_path: Path,
    gui_frontend: GuiFrontend,
    generated_tracks: list[Path],
) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(
        tmp_path,
        gui_frontend,
        [str(track) for track in generated_tracks],
        log_name=f"xmms-{gui_frontend.name}-tracks.log",
    )


@pytest.fixture
def gtk_app_with_tracks(tmp_path: Path, generated_tracks: list[Path]) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gtk_process(
        tmp_path,
        [str(track) for track in generated_tracks],
        log_name="xmms-gtk-tracks.log",
    )


@pytest.fixture
def gtk_socket_port() -> int:
    return control_socket.unused_tcp_port()


@pytest.fixture
def gtk_socket_app(tmp_path: Path, gtk_socket_port: int) -> Iterator[subprocess.Popen[bytes]]:
    log_path = tmp_path / "xmms-gtk-socket.log"
    log = log_path.open("wb")
    process = subprocess.Popen(
        [str(APP_BINARY), "--frontend", "gtk", "--reset", "--socket", str(gtk_socket_port)],
        cwd=REPO_ROOT,
        env=gtk_environment(tmp_path),
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    setattr(process, "xmms_log_path", log_path)
    setattr(process, "xmms_frontend", GTK_FRONTEND.name)
    setattr(process, "xmms_window_title", GTK_FRONTEND.window_title)
    try:
        control_socket.wait_for_socket(gtk_socket_port, timeout=10.0)
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
def gtk_socket_main_window(gtk_socket_app: subprocess.Popen[bytes]) -> MainWindow:
    return wait_for_main_window_with_log(gtk_socket_app)


@pytest.fixture
def control_client(gtk_socket_app: subprocess.Popen[bytes], gtk_socket_port: int) -> Iterator[Any]:
    del gtk_socket_app
    with control_socket.XmmsControlClient(gtk_socket_port) as client:
        yield client


@pytest.fixture
def gui_main_window(gui_app: subprocess.Popen[bytes]) -> MainWindow:
    return wait_for_main_window_with_log(gui_app)


@pytest.fixture
def gtk_main_window(gtk_app: subprocess.Popen[bytes]) -> MainWindow:
    return wait_for_main_window_with_log(gtk_app)


@pytest.fixture
def gui_tracked_main_window(gui_app_with_tracks: subprocess.Popen[bytes]) -> MainWindow:
    return wait_for_main_window_with_log(gui_app_with_tracks)


@pytest.fixture
def gtk_tracked_main_window(gtk_app_with_tracks: subprocess.Popen[bytes]) -> MainWindow:
    return wait_for_main_window_with_log(gtk_app_with_tracks)


def wait_for_main_window_with_log(process: subprocess.Popen[bytes]) -> MainWindow:
    title = getattr(process, "xmms_window_title", MAIN_WINDOW_TITLE)
    try:
        return MainWindow.wait(title, process)
    except Exception as exc:
        raise AssertionError(f"{exc}\n\nApplication log:\n{read_process_log(process)}") from exc


def process_log_path(process: subprocess.Popen[bytes]) -> Path:
    log_path = getattr(process, "xmms_log_path", None)
    if not isinstance(log_path, Path):
        raise AssertionError("GTK process does not expose an xmms_log_path")
    return log_path


def read_process_log(process: subprocess.Popen[bytes]) -> str:
    path = process_log_path(process)
    if not path.exists():
        return ""
    return path.read_text(errors="replace")


def assert_app_log_contains(
    process: subprocess.Popen[bytes],
    *needles: str,
    timeout: float = 5.0,
) -> str:
    deadline = time.monotonic() + timeout
    log = ""
    while time.monotonic() < deadline:
        log = read_process_log(process)
        missing = [needle for needle in needles if needle not in log]
        if not missing:
            return log
        if process.poll() is not None:
            break
        time.sleep(0.1)
    missing = [needle for needle in needles if needle not in log]
    raise AssertionError(
        "application log did not contain expected entries:\n"
        + "\n".join(f"- {needle}" for needle in missing)
        + f"\n\nFull log ({process_log_path(process)}):\n{log}"
    )


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


@pytest.fixture(scope="session")
def android_device(tmp_path_factory: Any) -> Iterator[AndroidDevice]:
    device = AndroidDevice.from_environment()
    startup_output = tmp_path_factory.mktemp("android-startup") / "initial.png"
    if os.environ.get("XMMS_E2E_ANDROID_SKIP_BUILD") == "1":
        device.require_running_emulator()
        device.install_existing_apk()
    else:
        startup_env = os.environ.copy()
        if device.serial:
            startup_env["ANDROID_SERIAL"] = device.serial
        subprocess.run(
            [
                str(REPO_ROOT / "repo"),
                "android-screenshot",
                f"--output={startup_output}",
                "--wait-seconds=1",
            ],
            cwd=REPO_ROOT,
            env=startup_env,
            check=True,
        )
        device = AndroidDevice.from_environment()
    device.grant_runtime_permissions()
    device.set_portrait()
    device.restart_app(reset_data=True)
    try:
        yield device
    finally:
        device.set_portrait()
        device.force_stop()
