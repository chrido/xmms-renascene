#!/usr/bin/env python3
# pyright: reportUnusedExpression=false
"""Development helper commands for XMMS Renascene."""

import asyncio
import logging
import os
import sys
from pathlib import Path

from .commandline import acmd_background, cli_follow, command_exists, configure_logging, raise_on_error, required_command
from .fire_lite import FireLite
from .flatpak import FlatpakInstaller

REPO_DIR = Path(__file__).resolve().parent.parent
RUST_BIN = REPO_DIR / "target" / "debug" / "xmms-rs"


def _configure_gtk_environment() -> None:
    os.environ["GDK_DISABLE"] = os.environ.get("XMMS_GDK_DISABLE", "gl")
    os.environ["GSK_RENDERER"] = os.environ.get("XMMS_GSK_RENDERER", "cairo")


def _args_include_gtk_mode(args: tuple[str, ...] | list[str]) -> bool:
    return any(arg in {"--gtk", "--gtk-smoke"} for arg in args)


def _app_args(args: tuple[str, ...]) -> list[str]:
    app_args = list(args)
    if not _args_include_gtk_mode(app_args):
        app_args.insert(0, "--gtk")
    return app_args


class RepoTool:
    def _build_selected_app(self) -> None:
        required_command("cargo")
        logging.info("Building Rust application...")
        ["cargo", "build", "--manifest-path", "Cargo.toml", "--quiet"] @ cli_follow | raise_on_error

    def _ensure_rust_binary(self) -> None:
        if RUST_BIN.exists() and os.access(RUST_BIN, os.X_OK):
            return
        logging.error("Rust binary '%s' is missing. Run without XMMS_EXEC_SKIP_BUILD=1 first.", RUST_BIN)
        sys.exit(127)

    def _build_unless_skipped(self) -> None:
        if os.environ.get("XMMS_EXEC_SKIP_BUILD") != "1":
            self._build_selected_app()

    def _exec_app(self, args: tuple[str, ...]) -> None:
        self._ensure_rust_binary()
        command = [str(RUST_BIN), *_app_args(args)]
        logging.info("Starting %s", " ".join(command))
        os.execvpe(command[0], command, os.environ)

    def _xvfb_environment(self) -> dict[str, str]:
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
            }
        )
        return env

    def _exec_screenshot_under_xvfb(self, args: tuple[str, ...]) -> None:
        required_command("xvfb-run")
        xvfb_server_args = os.environ.get("XMMS_XVFB_SERVER_ARGS", "-screen 0 1024x768x24")
        command = ["xvfb-run", "-a", "-s", xvfb_server_args, str(REPO_DIR / "repo"), "screenshot", *args]
        logging.info("Restarting under Xvfb: %s", " ".join(command))
        os.execvpe("xvfb-run", command, self._xvfb_environment())

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

    async def run(self, *args: str) -> int:
        """Build when needed and start the GTK application."""
        os.chdir(REPO_DIR)
        _configure_gtk_environment()
        self._build_unless_skipped()
        self._exec_app(args)
        return 0

    async def screenshot(self, *args: str) -> int:
        """Capture a root-window screenshot after starting the GTK application."""
        os.chdir(REPO_DIR)
        _configure_gtk_environment()
        self._build_unless_skipped()

        if os.environ.get("XMMS_SCREENSHOT_UNDER_XVFB") != "1":
            self._exec_screenshot_under_xvfb(args)

        screenshot_file = os.environ.get("XMMS_SCREENSHOT_FILE", "screenshot.png")
        screenshot_delay = float(os.environ.get("XMMS_SCREENSHOT_DELAY", "3"))
        proc = await self._start_app_in_background(args)
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
                except asyncio.TimeoutError:
                    proc.kill()
                    await proc.wait()


def dispatch_args(argv: list[str]) -> int:
    return FireLite([([], RepoTool()), ([], FlatpakInstaller())]).dispatchArgs(argv)


def main() -> int:
    configure_logging()
    return dispatch_args(sys.argv[1:])
