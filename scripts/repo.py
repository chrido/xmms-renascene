#!/usr/bin/env python3
# pyright: reportUnusedExpression=false
"""Development helper commands for XMMS Renascene."""

import asyncio
import contextlib
import logging
import os
import shlex
import subprocess
import sys
from pathlib import Path

from .commandline import acmd_background, cli_follow, command_exists, configure_logging, raise_on_error, required_command
from .fire_lite import FireLite
from .flatpak import FlatpakInstaller

REPO_DIR = Path(__file__).resolve().parent.parent
RUST_BIN = REPO_DIR / "target" / "debug" / "xmms-rs"
SCREENSHOT_SCENARIOS: dict[str, tuple[str, ...]] = {
    "main-player-default": ("--reset", "--screenshot-scenario", "main-player-default"),
    "main-player-shaded": ("--reset", "--shade-main", "--screenshot-scenario", "main-player-shaded"),
    "playlist-default": ("--reset", "--playlist", "--screenshot-scenario", "playlist-default"),
    "playlist-with-selection": ("--reset", "--playlist", "--screenshot-scenario", "playlist-with-selection"),
    "equalizer-default": ("--reset", "--equalizer", "--screenshot-scenario", "equalizer-default"),
    "equalizer-non-default": ("--reset", "--equalizer", "--screenshot-scenario", "equalizer-non-default"),
    "preferences-default": ("--reset", "--preferences", "--screenshot-scenario", "preferences-default"),
}


def _configure_gtk_environment() -> None:
    os.environ["GDK_DISABLE"] = os.environ.get("XMMS_GDK_DISABLE", "gl")
    os.environ["GSK_RENDERER"] = os.environ.get("XMMS_GSK_RENDERER", "cairo")


def _app_args(args: tuple[str, ...] | list[str]) -> list[str]:
    return list(args)


def _split_screenshot_args(args: tuple[str, ...]) -> tuple[str, tuple[str, ...]]:
    background = os.environ.get("XMMS_SCREENSHOT_BG", "black")
    app_args = []
    for arg in args:
        if arg == "--bgwhite":
            background = "white"
        elif arg == "--bgblack":
            background = "black"
        else:
            app_args.append(arg)
    return background, tuple(app_args)


def _scenario_args(scenario: str) -> tuple[str, ...]:
    try:
        return SCREENSHOT_SCENARIOS[scenario]
    except KeyError as exc:
        known = ", ".join(sorted(SCREENSHOT_SCENARIOS))
        raise ValueError(f"unknown screenshot scenario '{scenario}'. Known scenarios: {known}") from exc


def _load_ppm(path: Path) -> tuple[int, int, list[tuple[int, int, int]]]:
    tokens = []
    for line in path.read_text().splitlines():
        line = line.split("#", 1)[0]
        tokens.extend(line.split())
    if len(tokens) < 4 or tokens[0] != "P3":
        raise ValueError(f"{path} is not a plain PPM (P3) image")
    width, height, max_value = map(int, tokens[1:4])
    if max_value <= 0:
        raise ValueError(f"{path} has invalid max color value {max_value}")
    raw = list(map(int, tokens[4:]))
    if len(raw) != width * height * 3:
        raise ValueError(f"{path} contains {len(raw)} channel values, expected {width * height * 3}")
    pixels = []
    for i in range(0, len(raw), 3):
        pixels.append(tuple((channel * 255) // max_value for channel in raw[i : i + 3]))
    return width, height, pixels


def _write_ppm(path: Path, width: int, height: int, pixels: list[tuple[int, int, int]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    values = ["P3", f"{width} {height}", "255"]
    values.extend(f"{r} {g} {b}" for r, g, b in pixels)
    path.write_text("\n".join(values) + "\n")


def _diff_plain_ppm(left: Path, right: Path, diff: Path, tolerance: int) -> tuple[int, int]:
    left_width, left_height, left_pixels = _load_ppm(left)
    right_width, right_height, right_pixels = _load_ppm(right)
    if (left_width, left_height) != (right_width, right_height):
        raise ValueError(f"image dimensions differ: {left_width}x{left_height} vs {right_width}x{right_height}")
    changed = 0
    max_delta = 0
    diff_pixels = []
    for left_pixel, right_pixel in zip(left_pixels, right_pixels):
        delta = max(abs(a - b) for a, b in zip(left_pixel, right_pixel))
        max_delta = max(max_delta, delta)
        if delta > tolerance:
            changed += 1
            diff_pixels.append((255, 0, 255))
        else:
            gray = sum(left_pixel) // 3
            diff_pixels.append((gray, gray, gray))
    _write_ppm(diff, left_width, left_height, diff_pixels)
    return changed, max_delta


def _diff_images(left: Path, right: Path, diff: Path, tolerance: int) -> tuple[int, int]:
    if left.suffix.lower() == right.suffix.lower() == diff.suffix.lower() == ".ppm":
        return _diff_plain_ppm(left, right, diff, tolerance)
    try:
        from PIL import Image, ImageChops
    except ImportError as err:
        if not command_exists("compare"):
            raise RuntimeError("Install Pillow or ImageMagick 'compare' to diff non-PPM screenshots") from err
        diff.parent.mkdir(parents=True, exist_ok=True)
        command = ["compare", "-metric", "AE", "-fuzz", f"{tolerance}%", str(left), str(right), str(diff)]
        result = subprocess.run(command, text=True, capture_output=True, check=False)
        metric = result.stderr.strip() or result.stdout.strip() or "0"
        try:
            changed = int(float(metric.split()[0]))
        except (ValueError, IndexError):
            changed = 0 if result.returncode == 0 else 1
        return changed, 0

    with Image.open(left).convert("RGB") as left_image, Image.open(right).convert("RGB") as right_image:
        if left_image.size != right_image.size:
            raise ValueError(f"image dimensions differ: {left_image.size} vs {right_image.size}")
        delta_image = ImageChops.difference(left_image, right_image)
        changed = 0
        max_delta = 0
        diff_pixels = []
        for delta, base in zip(list(delta_image.getdata()), list(left_image.getdata())):
            pixel_delta = max(delta)
            max_delta = max(max_delta, pixel_delta)
            if pixel_delta > tolerance:
                changed += 1
                diff_pixels.append((255, 0, 255))
            else:
                gray = sum(base) // 3
                diff_pixels.append((gray, gray, gray))
        diff_image = Image.new("RGB", left_image.size)
        diff_image.putdata(diff_pixels)
        diff.parent.mkdir(parents=True, exist_ok=True)
        diff_image.save(diff)
        return changed, max_delta


class RepoTool:
    def _build_selected_app(self) -> None:
        self._build_gtk_app()

    def _build_gtk_app(self) -> None:
        required_command("cargo")
        logging.info("Building Rust application with default GTK frontend...")
        ["cargo", "build", "--manifest-path", "Cargo.toml", "--quiet"] @ cli_follow | raise_on_error

    def _build_egui_app(self) -> None:
        required_command("cargo")
        logging.info("Building Rust application with egui frontend...")
        [
            "cargo",
            "build",
            "--manifest-path",
            "Cargo.toml",
            "--no-default-features",
            "--features",
            "egui-ui,gstreamer-backend",
            "--quiet",
        ] @ cli_follow | raise_on_error

    def _build_frontend_diff_app(self) -> None:
        required_command("cargo")
        logging.info("Building Rust application with GTK and egui support for screenshot diffing...")
        ["cargo", "build", "--manifest-path", "Cargo.toml", "--features", "egui-ui", "--quiet"] @ cli_follow | raise_on_error

    def _ensure_rust_binary(self) -> None:
        try:
            rust_binary_ready = RUST_BIN.exists() and os.access(RUST_BIN, os.X_OK)
        except OSError:
            rust_binary_ready = False
        if rust_binary_ready:
            return
        logging.error("Rust binary '%s' is missing. Run without XMMS_EXEC_SKIP_BUILD=1 first.", RUST_BIN)
        sys.exit(127)

    def _build_unless_skipped(self) -> None:
        if os.environ.get("XMMS_EXEC_SKIP_BUILD") != "1":
            self._build_selected_app()

    def _build_frontend_unless_skipped(self, frontend: str) -> None:
        if os.environ.get("XMMS_EXEC_SKIP_BUILD") == "1":
            return
        if frontend == "egui":
            self._build_egui_app()
        else:
            self._build_gtk_app()

    def _exec_app(self, args: tuple[str, ...]) -> None:
        self._ensure_rust_binary()
        command = [str(RUST_BIN), *_app_args(args)]
        logging.info("Starting %s", " ".join(command))
        os.execvpe(command[0], command, os.environ)

    def _xvfb_environment(self, background: str) -> dict[str, str]:
        env = os.environ.copy()
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("DBUS_SESSION_BUS_ADDRESS", None)
        env.update(
            {
                "GDK_BACKEND": "x11",
                "GSK_RENDERER": "cairo",
                "GDK_DISABLE": os.environ["GDK_DISABLE"],
                "NO_AT_BRIDGE": "1",
                "XMMS_NON_UNIQUE": "1",
                "XMMS_EXEC_SKIP_BUILD": "1",
                "XMMS_SCREENSHOT_UNDER_XVFB": "1",
                "XMMS_SCREENSHOT_BG": background,
            }
        )
        return env

    def _xvfb_server_args(self, background: str) -> str:
        raw_server_args = os.environ.get("XMMS_XVFB_SERVER_ARGS", "-screen 0 1024x768x24")
        server_args = [arg for arg in shlex.split(raw_server_args) if arg not in {"-wr", "-br"}]
        background_arg = "-wr" if background == "white" else "-br"
        server_args.append(background_arg)
        return shlex.join(server_args)

    def _exec_screenshot_under_xvfb(self, args: tuple[str, ...], background: str) -> None:
        required_command("xvfb-run")
        xvfb_server_args = self._xvfb_server_args(background)
        command = ["xvfb-run", "-a", "-s", xvfb_server_args, str(REPO_DIR / "repo"), "screenshot", *args]
        logging.info("Restarting under Xvfb: %s", " ".join(command))
        os.execvpe("xvfb-run", command, self._xvfb_environment(background))

    async def _start_app_in_background(self, args: tuple[str, ...]) -> asyncio.subprocess.Process:
        self._ensure_rust_binary()
        command = [str(RUST_BIN), *_app_args(args)]
        logging.info("Starting app for screenshot: %s", " ".join(command))
        return await acmd_background(command, cwd=str(REPO_DIR), env=os.environ.copy(), log_command=False)

    def _screenshot_command(self, screenshot_file: str) -> list[str]:
        candidates = [
            ("import", ["import", "-window", "root", "-screen", screenshot_file]),
            ("scrot", ["scrot", screenshot_file]),
            ("gnome-screenshot", ["gnome-screenshot", "-f", screenshot_file]),
            ("grim", ["grim", screenshot_file]),
            ("spectacle", ["spectacle", "-b", "-n", "-o", screenshot_file]),
        ]
        for command, args in candidates:
            if command_exists(command):
                return args
        raise RuntimeError("No screenshot tool found. Install ImageMagick import, scrot, gnome-screenshot, grim, or spectacle.")

    def _take_screenshot(self, screenshot_file: str) -> None:
        path = Path(screenshot_file)
        path.parent.mkdir(parents=True, exist_ok=True)
        command = self._screenshot_command(screenshot_file)
        logging.info("Taking screenshot: %s", " ".join(command))
        command @ cli_follow | raise_on_error
        if not path.is_file() or path.stat().st_size == 0:
            raise RuntimeError(f"Screenshot command did not create {screenshot_file}.")
        logging.info("Screenshot saved to %s", screenshot_file)

    def _select_run_frontend(self, args: tuple[str, ...]) -> tuple[str, tuple[str, ...]]:
        frontend = "gtk"
        explicit_frontend = False
        app_args: list[str] = []
        index = 0
        while index < len(args):
            arg = args[index]
            if arg == "--":
                index += 1
                continue
            if arg == "--gtk":
                frontend = "gtk"
                index += 1
                continue
            if arg == "--egui":
                frontend = "egui"
                index += 1
                continue
            if arg.startswith("--frontend="):
                explicit_frontend = True
                frontend = arg.split("=", 1)[1]
                app_args.append(arg)
                index += 1
                continue
            if arg == "--frontend" and index + 1 < len(args):
                explicit_frontend = True
                frontend = args[index + 1]
                app_args.extend([arg, args[index + 1]])
                index += 2
                continue
            app_args.append(arg)
            index += 1

        if frontend not in {"gtk", "egui"}:
            raise ValueError(f"unknown frontend '{frontend}', expected 'gtk' or 'egui'")
        if not explicit_frontend:
            app_args = ["--frontend", frontend, *app_args]
        return frontend, tuple(app_args)

    async def run(self, *args: str) -> int:
        """Build when needed and start the selected frontend.

        Shorthands:
          ./repo run --gtk  -> build default GTK binary and run --frontend gtk
          ./repo run --egui -> build egui binary and run --frontend egui
        """
        os.chdir(REPO_DIR)
        try:
            frontend, app_args = self._select_run_frontend(args)
        except ValueError as err:
            logging.error("%s", err)
            return 2
        if frontend == "gtk":
            _configure_gtk_environment()
        self._build_frontend_unless_skipped(frontend)
        self._exec_app(app_args)
        return 0

    async def screenshot(self, *args: str) -> int:
        """Capture a root-window screenshot after starting the GTK application."""
        os.chdir(REPO_DIR)
        _configure_gtk_environment()
        self._build_unless_skipped()
        background, app_args = _split_screenshot_args(args)

        if os.environ.get("XMMS_SCREENSHOT_UNDER_XVFB") != "1":
            self._exec_screenshot_under_xvfb(app_args, background)

        screenshot_file = os.environ.get("XMMS_SCREENSHOT_FILE", "screenshot.png")
        try:
            screenshot_delay = float(os.environ.get("XMMS_SCREENSHOT_DELAY", "3"))
        except ValueError:
            logging.error("XMMS_SCREENSHOT_DELAY must be a floating point number")
            return 2
        proc = await self._start_app_in_background(app_args)
        try:
            await asyncio.sleep(screenshot_delay)
            if proc.returncode is not None:
                logging.error("xmms exited before the screenshot could be taken.")
                return proc.returncode or 1
            self._take_screenshot(screenshot_file)
            return 0
        finally:
            if proc.returncode is None:
                proc.terminate()
                try:
                    await asyncio.wait_for(proc.wait(), timeout=5)
                except asyncio.TimeoutError as err:
                    logging.debug("Timed out waiting for screenshot process shutdown: %s", err)
                    proc.kill()
                    await proc.wait()

    def _write_frontend_screenshot(self, frontend: str, scenario: str, output: Path) -> None:
        self._ensure_rust_binary()
        output.parent.mkdir(parents=True, exist_ok=True)
        args = [
            str(RUST_BIN),
            "--frontend",
            frontend,
            "--screenshot",
            str(output),
            *_scenario_args(scenario),
        ]
        logging.info("Capturing %s screenshot: %s", frontend, " ".join(args))
        args @ cli_follow | raise_on_error
        if not output.is_file() or output.stat().st_size == 0:
            raise RuntimeError(f"{frontend} screenshot was not created at {output}")

    async def frontend_screenshot_diff(
        self,
        scenario: str = "main-player-default",
        output_dir: str = "target/screenshots",
        gtk_output: str = "",
        egui_output: str = "",
        diff_output: str = "",
        tolerance: int = 0,
        fail_on_diff_threshold: int = -1,
        keep_intermediate: bool = True,
        update_references: bool = False,
    ) -> int:
        """Capture GTK and egui screenshots for a scenario and write a diff image."""
        os.chdir(REPO_DIR)
        if os.environ.get("XMMS_EXEC_SKIP_BUILD") != "1":
            self._build_frontend_diff_app()
        output_root = Path(output_dir)
        gtk_path = Path(gtk_output) if gtk_output else output_root / f"gtk-{scenario}.png"
        egui_path = Path(egui_output) if egui_output else output_root / f"egui-{scenario}.png"
        diff_path = Path(diff_output) if diff_output else output_root / f"diff-{scenario}.png"
        try:
            self._write_frontend_screenshot("gtk", scenario, gtk_path)
            self._write_frontend_screenshot("egui", scenario, egui_path)
            if update_references:
                logging.info("Reference update requested; keeping freshly captured frontend screenshots")
            changed, max_delta = _diff_images(gtk_path, egui_path, diff_path, tolerance)
        except Exception as err:
            logging.error("frontend screenshot diff failed: %s", err)
            return 1
        finally:
            if not keep_intermediate:
                for path in [gtk_path, egui_path]:
                    with contextlib.suppress(FileNotFoundError):
                        path.unlink()
        logging.info(
            "Screenshot diff written to %s (changed_pixels=%s, max_delta=%s, tolerance=%s)",
            diff_path,
            changed,
            max_delta,
            tolerance,
        )
        if fail_on_diff_threshold >= 0 and changed > fail_on_diff_threshold:
            logging.error("changed pixel count %s exceeds threshold %s", changed, fail_on_diff_threshold)
            return 2
        return 0

    async def frontend_screenshot_diff_self_test(self) -> int:
        """Run a synthetic self-test for the screenshot diff helper."""
        os.chdir(REPO_DIR)
        tmp = REPO_DIR / "target" / "screenshots" / "self-test"
        left = tmp / "left.ppm"
        right = tmp / "right.ppm"
        diff = tmp / "diff.ppm"
        _write_ppm(left, 2, 1, [(0, 0, 0), (10, 10, 10)])
        _write_ppm(right, 2, 1, [(0, 0, 0), (250, 10, 10)])
        changed, max_delta = _diff_images(left, right, diff, tolerance=0)
        if changed != 1 or max_delta != 240 or not diff.is_file():
            logging.error("unexpected diff result: changed=%s max_delta=%s diff_exists=%s", changed, max_delta, diff.is_file())
            return 1
        logging.info("frontend screenshot diff self-test passed")
        return 0


def dispatch_args(argv: list[str]) -> int:
    return FireLite([([], RepoTool()), ([], FlatpakInstaller())]).dispatchArgs(argv)


def main() -> int:
    configure_logging()
    return dispatch_args(sys.argv[1:])
