#!/usr/bin/env python3
# pyright: reportUnusedExpression=false
"""Development helper commands for XMMS Renascene."""

import asyncio
import contextlib
import glob
import hashlib
import logging
import os
import re
import signal
import shlex
import shutil
import subprocess
import sys
import time
import zipfile
from importlib.util import find_spec
from pathlib import Path
from typing import Any, cast

from .commandline import acmd_background, cli_follow, command_exists, configure_logging, raise_on_error, required_command
from .fire_lite import FireLite
from .flatpak import FlatpakInstaller

REPO_DIR = Path(__file__).resolve().parent.parent
RUST_BIN = REPO_DIR / "target" / "debug" / "xmms-rs"
E2E_DIR = REPO_DIR / "e2e"
E2E_VENV = Path(os.environ.get("XMMS_E2E_VENV_DIR", str(E2E_DIR / ".venv")))
E2E_REQUIREMENTS = E2E_DIR / "requirements.txt"
E2E_CREATE_VENV = E2E_DIR / "create_venv.py"
E2E_DOCKERFILE = E2E_DIR / "Dockerfile"
E2E_DOCKER_IMAGE = "xmms-renascene-pye2e"
ANDROID_SDK = Path(
    os.environ.get("ANDROID_HOME", str(Path.home() / ".local" / "share" / "android-sdk"))
)
ANDROID_NDK_VERSION = "27.2.12479018"
ANDROID_TARGET = "aarch64-linux-android"
ANDROID_EMULATOR_TARGET = "x86_64-linux-android"
ANDROID_AVD = "xmms_api35"
ANDROID_PACKAGE = "org.xmms.renascene"
ANDROID_ACTIVITY = "org.xmms.renascene.XmmsActivity"
SCREENSHOT_SCENARIOS: dict[str, tuple[str, ...]] = {
    "main-player-default": ("--reset", "--screenshot-scenario", "main-player-default"),
    "main-player-shaded": ("--reset", "--shade-main", "--screenshot-scenario", "main-player-shaded"),
    "playlist-default": ("--reset", "--playlist", "--screenshot-scenario", "playlist-default"),
    "playlist-with-selection": ("--reset", "--playlist", "--screenshot-scenario", "playlist-with-selection"),
    "playlist-single-song": ("--reset", "--playlist", "--screenshot-scenario", "playlist-single-song"),
    "equalizer-default": ("--reset", "--equalizer", "--screenshot-scenario", "equalizer-default"),
    "equalizer-non-default": ("--reset", "--equalizer", "--screenshot-scenario", "equalizer-non-default"),
    "preferences-default": ("--reset", "--preferences", "--screenshot-scenario", "preferences-default"),
}


def _configure_gtk_environment() -> None:
    os.environ["GDK_DISABLE"] = os.environ.get("XMMS_GDK_DISABLE", "gl")
    os.environ["GSK_RENDERER"] = os.environ.get("XMMS_GSK_RENDERER", "cairo")


def _alsa_pcm_plugin_exists(name: str) -> bool:
    patterns = [
        f"/usr/lib*/alsa-lib/libasound_module_pcm_{name}.so",
        f"/usr/lib/*/alsa-lib/libasound_module_pcm_{name}.so",
    ]
    return any(glob.glob(pattern) for pattern in patterns)


def _select_rodio_alsa_backend() -> str | None:
    requested = os.environ.get("XMMS_RODIO_ALSA_BACKEND", "auto").strip().lower()
    if requested in {"", "auto"}:
        for candidate in ("pipewire", "pulse"):
            if _alsa_pcm_plugin_exists(candidate):
                return candidate
        return None
    if requested in {"system", "default", "alsa", "none"}:
        return None
    if requested in {"pipewire", "pulse"}:
        if not _alsa_pcm_plugin_exists(requested):
            logging.warning(
                "Requested XMMS_RODIO_ALSA_BACKEND=%s, but the ALSA %s plugin was not found",
                requested,
                requested,
            )
        return requested
    logging.warning(
        "Unknown XMMS_RODIO_ALSA_BACKEND=%s; expected auto, pipewire, pulse, or system",
        requested,
    )
    return None


def _configure_rodio_audio_environment() -> None:
    backend = _select_rodio_alsa_backend()
    if backend is None:
        logging.info("Using system ALSA default for rodio/cpal audio")
        return
    config = REPO_DIR / "target" / f"rodio-{backend}.asoundrc"
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text(
        "</usr/share/alsa/alsa.conf>\n"
        "\n"
        f"pcm.!default {{\n    type {backend}\n}}\n"
        f"ctl.!default {{\n    type {backend}\n}}\n"
    )
    os.environ["ALSA_CONFIG_PATH"] = str(config)
    logging.info(
        "Using ALSA %s plugin for rodio/cpal audio via ALSA_CONFIG_PATH=%s",
        backend,
        config,
    )


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
    if find_spec("PIL") is None:
        if not command_exists("compare"):
            raise RuntimeError("Install Pillow or ImageMagick 'compare' to diff non-PPM screenshots")
        diff.parent.mkdir(parents=True, exist_ok=True)
        command = ["compare", "-metric", "AE", "-fuzz", f"{tolerance}%", str(left), str(right), str(diff)]
        result = subprocess.run(command, text=True, capture_output=True, check=False)
        metric = result.stderr.strip() or result.stdout.strip() or "0"
        try:
            changed = int(float(metric.split()[0]))
        except (ValueError, IndexError):
            changed = 0 if result.returncode == 0 else 1
        return changed, 0

    from PIL import Image, ImageChops

    with Image.open(left).convert("RGB") as left_image, Image.open(right).convert("RGB") as right_image:
        if left_image.size != right_image.size:
            raise ValueError(f"image dimensions differ: {left_image.size} vs {right_image.size}")
        delta_image = ImageChops.difference(left_image, right_image)
        changed = 0
        max_delta = 0
        diff_pixels = []
        for delta, base in zip(list(cast(Any, delta_image.getdata())), list(cast(Any, left_image.getdata()))):
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
    def _android_environment(self) -> dict[str, str]:
        sdk = ANDROID_SDK
        ndk = Path(os.environ.get("ANDROID_NDK_HOME", str(sdk / "ndk" / ANDROID_NDK_VERSION)))
        if not sdk.is_dir():
            raise RuntimeError(
                f"Android SDK not found at {sdk}; set ANDROID_HOME or install it there"
            )
        if not ndk.is_dir():
            raise RuntimeError(
                f"Android NDK {ANDROID_NDK_VERSION} not found at {ndk}; "
                "set ANDROID_NDK_HOME or install the pinned NDK"
            )
        env = os.environ.copy()
        env["ANDROID_HOME"] = str(sdk)
        env["ANDROID_NDK_HOME"] = str(ndk)
        return env

    def _android_apk_command(self, target: str, *, release: bool = False) -> list[str]:
        command = [
            "cargo",
            "apk",
            "build",
            "--target",
            target,
            "--no-default-features",
            "--features",
            "mobile-ui",
            "--lib",
        ]
        if release:
            command.append("--release")
        return command

    def _android_apk_path(self, *, release: bool = False) -> Path:
        profile = "release" if release else "debug"
        return REPO_DIR / "target" / profile / "apk" / "xmms-renascene.apk"

    def _wait_for_android_emulator(self, adb: Path, timeout_seconds: int = 240) -> None:
        subprocess.run([str(adb), "wait-for-device"], check=True, timeout=timeout_seconds)
        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            result = subprocess.run(
                [str(adb), "shell", "getprop", "sys.boot_completed"],
                text=True,
                capture_output=True,
                check=False,
            )
            if result.stdout.strip() == "1":
                subprocess.run([str(adb), "shell", "input", "keyevent", "82"], check=False)
                return
            time.sleep(2)
        raise TimeoutError("Android emulator did not finish booting")

    def _stop_managed_android_emulator(self, adb: Path, pid_path: Path) -> None:
        try:
            pid = int(pid_path.read_text().strip())
        except (FileNotFoundError, ValueError):
            raise RuntimeError(
                "A headless Android emulator is already running but its managed PID is unavailable"
            )
        os.kill(pid, signal.SIGTERM)
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            state = subprocess.run(
                [str(adb), "get-state"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if state.returncode != 0:
                break
            time.sleep(1)
        pid_path.unlink(missing_ok=True)

    def _ensure_android_emulator(self, env: dict[str, str], *, headless: bool) -> Path:
        sdk = Path(env["ANDROID_HOME"])
        adb = sdk / "platform-tools" / "adb"
        emulator = sdk / "emulator" / "emulator"
        if not adb.is_file() or not emulator.is_file():
            raise RuntimeError("Android platform-tools and emulator packages are required")
        state_dir = REPO_DIR / "target" / "android"
        mode_path = state_dir / "emulator.mode"
        pid_path = state_dir / "emulator.pid"
        devices = subprocess.run(
            [str(adb), "devices"],
            text=True,
            capture_output=True,
            check=True,
        ).stdout
        if any(line.startswith("emulator-") and "\tdevice" in line for line in devices.splitlines()):
            mode = mode_path.read_text().strip() if mode_path.is_file() else ""
            if headless or mode != "headless":
                return adb
            self._stop_managed_android_emulator(adb, pid_path)

        avd_path = Path.home() / ".android" / "avd" / f"{ANDROID_AVD}.avd"
        if not avd_path.is_dir():
            raise RuntimeError(
                f"Android AVD {ANDROID_AVD!r} is missing; create it with the API 35 "
                "Google APIs x86_64 system image"
            )
        log_path = state_dir / "emulator.log"
        log_path.parent.mkdir(parents=True, exist_ok=True)
        command = [
            str(emulator),
            "-avd",
            ANDROID_AVD,
            "-no-audio",
            "-no-boot-anim",
            "-gpu",
            "swiftshader_indirect",
            "-no-snapshot",
            "-no-metrics",
        ]
        if headless:
            command.append("-no-window")
        with log_path.open("ab") as log_file:
            process = subprocess.Popen(
                command,
                cwd=REPO_DIR,
                env=env,
                stdout=log_file,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        pid_path.write_text(f"{process.pid}\n")
        mode_path.write_text("headless\n" if headless else "windowed\n")
        self._wait_for_android_emulator(adb)
        return adb

    def _dismiss_android_anr_wait(self, adb: Path) -> None:
        subprocess.run(
            [str(adb), "shell", "uiautomator", "dump", "/sdcard/window.xml"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        result = subprocess.run(
            [str(adb), "shell", "cat", "/sdcard/window.xml"],
            text=True,
            capture_output=True,
            check=False,
        )
        match = re.search(
            r'resource-id="android:id/aerr_wait".*?bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"',
            result.stdout,
        )
        if match is None:
            return
        left, top, right, bottom = map(int, match.groups())
        subprocess.run(
            [
                str(adb),
                "shell",
                "input",
                "tap",
                str((left + right) // 2),
                str((top + bottom) // 2),
            ],
            check=False,
        )
        time.sleep(2)

    def _build_android_apk(
        self, env: dict[str, str], target: str, *, release: bool = False
    ) -> bool:
        command = self._android_apk_command(target, release=release)
        env = env.copy()
        page_size_flag = "-C link-arg=-Wl,-z,max-page-size=16384"
        env["RUSTFLAGS"] = " ".join(
            part for part in (env.get("RUSTFLAGS", ""), page_size_flag) if part
        )
        if release:
            env.setdefault(
                "CARGO_APK_RELEASE_KEYSTORE",
                str(Path.home() / ".android" / "debug.keystore"),
            )
            env.setdefault("CARGO_APK_RELEASE_KEYSTORE_PASSWORD", "android")
        logging.info("Building Android APK: %s", " ".join(command))
        if subprocess.run(command, cwd=REPO_DIR, env=env, check=False).returncode != 0:
            return False
        try:
            self._package_android_activity(env, release=release)
        except Exception as err:
            logging.error("failed to package Android file picker activity: %s", err)
            return False
        return True

    def _android_signing_config(
        self,
        env: dict[str, str],
        *,
        release: bool,
    ) -> tuple[Path, str, str, str]:
        if not release:
            return (
                Path.home() / ".android" / "debug.keystore",
                "androiddebugkey",
                "android",
                "android",
            )
        keystore_password = env.get("CARGO_APK_RELEASE_KEYSTORE_PASSWORD", "android")
        return (
            Path(
                env.get(
                    "CARGO_APK_RELEASE_KEYSTORE",
                    str(Path.home() / ".android" / "debug.keystore"),
                )
            ),
            env.get("CARGO_APK_RELEASE_KEYSTORE_KEY_ALIAS", "androiddebugkey"),
            keystore_password,
            env.get("CARGO_APK_RELEASE_KEY_ALIAS_PASSWORD", keystore_password),
        )

    def _package_android_activity(
        self, env: dict[str, str], *, release: bool = False
    ) -> None:
        sdk = Path(env["ANDROID_HOME"])
        build_tools = sdk / "build-tools" / "35.0.0"
        android_jar = sdk / "platforms" / "android-35" / "android.jar"
        java_source_dir = (
            REPO_DIR
            / "android"
            / "java"
            / "org"
            / "xmms"
            / "renascene"
        )
        java_sources = sorted(str(path) for path in java_source_dir.glob("*.java"))
        apk = self._android_apk_path(release=release)
        work_dir = REPO_DIR / "target" / "android" / "java"
        classes_dir = work_dir / "classes"
        dex_dir = work_dir / "dex"
        shutil.rmtree(work_dir, ignore_errors=True)
        classes_dir.mkdir(parents=True)
        dex_dir.mkdir()

        subprocess.run(
            [
                "javac",
                "-Xlint:-options",
                "-source",
                "8",
                "-target",
                "8",
                "-classpath",
                str(android_jar),
                "-d",
                str(classes_dir),
                *java_sources,
            ],
            cwd=REPO_DIR,
            check=True,
        )
        class_files = [str(path) for path in classes_dir.rglob("*.class")]
        subprocess.run(
            [
                str(build_tools / "d8"),
                "--lib",
                str(android_jar),
                "--output",
                str(dex_dir),
                *class_files,
            ],
            cwd=REPO_DIR,
            env=env,
            check=True,
        )

        manifest_source = work_dir / "AndroidManifest.xml"
        manifest_apk = work_dir / "manifest.apk"
        resource_dir = work_dir / "res"
        drawable_dir = resource_dir / "drawable"
        xml_dir = resource_dir / "xml"
        drawable_dir.mkdir(parents=True)
        xml_dir.mkdir()
        shutil.copy2(REPO_DIR / "data" / "org.xmms.Renascene.png", drawable_dir / "icon.png")
        (xml_dir / "automotive_app_desc.xml").write_text(
            """<?xml version="1.0" encoding="utf-8"?>
<automotiveApp>
    <uses name="media" />
</automotiveApp>
""",
            encoding="utf-8",
        )
        debug_probe_activity = (
            """
        <activity
            android:name=".XmmsAutoProbeActivity"
            android:exported="true"
            android:theme="@android:style/Theme.DeviceDefault.NoActionBar" />
"""
            if not release
            else ""
        )
        manifest_source.write_text(
            f"""<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{ANDROID_PACKAGE}"
    android:versionCode="16777472"
    android:versionName="0.1.0">
    <uses-sdk android:minSdkVersion="26" android:targetSdkVersion="35" />
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK" />
    <uses-permission android:name="android.permission.WAKE_LOCK" />
    <application
        android:appCategory="audio"
        android:debuggable="{"false" if release else "true"}"
        android:hasCode="true"
        android:icon="@drawable/icon"
        android:label="XMMS Renascene"
        android:theme="@android:style/Theme.DeviceDefault.NoActionBar.Fullscreen">
        <meta-data
            android:name="com.google.android.gms.car.application"
            android:resource="@xml/automotive_app_desc" />
        <activity
            android:name="{ANDROID_ACTIVITY}"
            android:configChanges="orientation|keyboardHidden|screenSize"
            android:exported="true"
            android:resizeableActivity="true"
            android:screenOrientation="unspecified">
            <meta-data android:name="android.app.lib_name" android:value="xmms_renascene" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
{debug_probe_activity}
        <service
            android:name=".XmmsPlaybackService"
            android:exported="true"
            android:foregroundServiceType="mediaPlayback"
            android:stopWithTask="false">
            <intent-filter>
                <action android:name="android.media.browse.MediaBrowserService" />
            </intent-filter>
        </service>
    </application>
</manifest>
""",
            encoding="utf-8",
        )
        subprocess.run(
            [
                str(build_tools / "aapt"),
                "package",
                "-f",
                "-M",
                str(manifest_source),
                "-I",
                str(android_jar),
                "-S",
                str(resource_dir),
                "-F",
                str(manifest_apk),
            ],
            cwd=REPO_DIR,
            check=True,
        )
        with zipfile.ZipFile(manifest_apk, "r") as compiled:
            android_manifest = compiled.read("AndroidManifest.xml")
            compiled_resources = [
                (entry, compiled.read(entry.filename))
                for entry in compiled.infolist()
                if entry.filename != "AndroidManifest.xml" and not entry.is_dir()
            ]
            compiled_resource_names = {
                entry.filename for entry, _contents in compiled_resources
            }

        unsigned = work_dir / "xmms-renascene-unsigned.apk"
        aligned = work_dir / "xmms-renascene-aligned.apk"
        with zipfile.ZipFile(apk, "r") as source, zipfile.ZipFile(unsigned, "w") as destination:
            for entry in source.infolist():
                upper_name = entry.filename.upper()
                if entry.filename in {"AndroidManifest.xml", "classes.dex"} or (
                    upper_name.startswith("META-INF/")
                    and upper_name.endswith((".SF", ".RSA", ".DSA", "MANIFEST.MF"))
                ) or entry.filename in compiled_resource_names:
                    continue
                destination.writestr(entry, source.read(entry.filename))
            destination.writestr(
                "AndroidManifest.xml", android_manifest, zipfile.ZIP_DEFLATED
            )
            destination.write(dex_dir / "classes.dex", "classes.dex", zipfile.ZIP_DEFLATED)
            for entry, contents in compiled_resources:
                destination.writestr(entry, contents)

        subprocess.run(
            [
                str(build_tools / "zipalign"),
                "-f",
                "-P",
                "16",
                "4",
                str(unsigned),
                str(aligned),
            ],
            cwd=REPO_DIR,
            check=True,
        )
        keystore, key_alias, keystore_password, key_password = (
            self._android_signing_config(env, release=release)
        )
        subprocess.run(
            [
                str(build_tools / "apksigner"),
                "sign",
                "--ks",
                str(keystore),
                "--ks-key-alias",
                key_alias,
                "--ks-pass",
                f"pass:{keystore_password}",
                "--key-pass",
                f"pass:{key_password}",
                "--out",
                str(apk),
                str(aligned),
            ],
            cwd=REPO_DIR,
            check=True,
        )

    def _install_and_launch_android(self, adb: Path) -> bool:
        apk = REPO_DIR / "target" / "debug" / "apk" / "xmms-renascene.apk"
        commands = [
            [str(adb), "install", "-r", str(apk)],
            [str(adb), "shell", "am", "force-stop", "org.xmms.renascene"],
            [
                str(adb),
                "shell",
                "am",
                "start",
                "-W",
                "-n",
                "org.xmms.renascene/org.xmms.renascene.XmmsActivity",
            ],
        ]
        return all(
            subprocess.run(command, cwd=REPO_DIR, check=False).returncode == 0
            for command in commands
        )

    def _run_android_emulator(self) -> int:
        required_command("cargo-apk")
        try:
            env = self._android_environment()
            adb = self._ensure_android_emulator(env, headless=False)
        except Exception as err:
            logging.error("failed to prepare Android emulator: %s", err)
            return 1
        if not self._build_android_apk(env, ANDROID_EMULATOR_TARGET):
            return 1
        if not self._install_and_launch_android(adb):
            return 1
        logging.info("XMMS Renascene is running in the Android emulator")
        return 0

    def _build_selected_app(self) -> None:
        self._build_app("gtk", "gstreamer")

    def _build_app(self, frontend: str, audio_backend: str) -> None:
        required_command("cargo")
        feature_by_frontend = {"gtk": "gtk-ui", "egui": "egui-ui"}
        feature_by_audio_backend = {"gstreamer": "gstreamer-backend", "rodio": "rodio-backend"}
        features = [feature_by_frontend[frontend], feature_by_audio_backend[audio_backend]]
        logging.info(
            "Building Rust application with %s frontend and %s audio backend...",
            frontend,
            audio_backend,
        )
        [
            "cargo",
            "build",
            "--manifest-path",
            "Cargo.toml",
            "--no-default-features",
            "--features",
            ",".join(features),
            "--quiet",
        ] @ cli_follow | raise_on_error

    def _build_gtk_app(self, audio_backend: str = "gstreamer") -> None:
        self._build_app("gtk", audio_backend)

    def _build_egui_app(self, audio_backend: str = "gstreamer") -> None:
        self._build_app("egui", audio_backend)

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

    def _build_frontend_unless_skipped(self, frontend: str, audio_backend: str = "gstreamer") -> None:
        if os.environ.get("XMMS_EXEC_SKIP_BUILD") == "1":
            return
        self._build_app(frontend, audio_backend)

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
                "GSK_RENDERER": os.environ.get("GSK_RENDERER", "cairo"),
                "GDK_DISABLE": os.environ.get("GDK_DISABLE", "gl"),
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

    def _select_run_frontend(self, args: tuple[str, ...]) -> tuple[str, str, tuple[str, ...]]:
        frontend = "gtk"
        audio_backend = "gstreamer"
        explicit_frontend = False
        explicit_audio_backend = False
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
            if arg == "--gstreamer":
                if explicit_audio_backend and audio_backend != "gstreamer":
                    raise ValueError("audio backend specified more than once")
                audio_backend = "gstreamer"
                explicit_audio_backend = True
                index += 1
                continue
            if arg == "--rodio":
                if explicit_audio_backend and audio_backend != "rodio":
                    raise ValueError("audio backend specified more than once")
                audio_backend = "rodio"
                explicit_audio_backend = True
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
        if audio_backend not in {"gstreamer", "rodio"}:
            raise ValueError(
                f"unknown audio backend '{audio_backend}', expected 'gstreamer' or 'rodio'"
            )
        if not explicit_frontend:
            app_args = ["--frontend", frontend, *app_args]
        return frontend, audio_backend, tuple(app_args)

    async def run(self, *args: str) -> int:
        """Build when needed and start the selected frontend.

        Shorthands:
          ./repo run --gtk              -> build GTK with GStreamer and run --frontend gtk
          ./repo run --egui             -> build egui with GStreamer and run --frontend egui
          ./repo run --gtk --rodio      -> build GTK with rodio and run --frontend gtk
          ./repo run --egui --rodio     -> build egui with rodio and run --frontend egui
          ./repo run --android          -> build, install, and launch in the Android emulator
        """
        os.chdir(REPO_DIR)
        if "--android" in args:
            remaining = tuple(arg for arg in args if arg != "--android")
            if remaining:
                logging.error("--android cannot currently be combined with other run arguments")
                return 2
            return self._run_android_emulator()
        try:
            frontend, audio_backend, app_args = self._select_run_frontend(args)
        except ValueError as err:
            logging.error("%s", err)
            return 2
        if frontend == "gtk":
            _configure_gtk_environment()
        if audio_backend == "rodio":
            _configure_rodio_audio_environment()
        self._build_frontend_unless_skipped(frontend, audio_backend)
        self._exec_app(app_args)
        return 0

    async def android_check(self) -> int:
        """Cross-check the Android eframe and rodio application library."""
        os.chdir(REPO_DIR)
        command = [
            "cargo",
            "check",
            "--target",
            ANDROID_TARGET,
            "--no-default-features",
            "--features",
            "mobile-ui",
            "--lib",
        ]
        logging.info("Checking Android build: %s", " ".join(command))
        return subprocess.run(command, cwd=REPO_DIR, check=False).returncode

    async def android_apk_debug(self) -> int:
        """Build the signed arm64 debug APK with cargo-apk."""
        os.chdir(REPO_DIR)
        required_command("cargo-apk")
        try:
            env = self._android_environment()
        except RuntimeError as err:
            logging.error("%s", err)
            return 1
        if self._build_android_apk(env, ANDROID_TARGET):
            logging.info(
                "Android APK written to %s",
                REPO_DIR / "target" / "debug" / "apk" / "xmms-renascene.apk",
            )
            return 0
        return 1

    async def android_apk_emulator(self) -> int:
        """Build the signed x86_64 debug APK used by the Android emulator."""
        os.chdir(REPO_DIR)
        required_command("cargo-apk")
        try:
            env = self._android_environment()
        except RuntimeError as err:
            logging.error("%s", err)
            return 1
        if self._build_android_apk(env, ANDROID_EMULATOR_TARGET):
            logging.info(
                "Android emulator APK written to %s",
                REPO_DIR / "target" / "debug" / "apk" / "xmms-renascene.apk",
            )
            return 0
        return 1

    async def android_apk_release(self, output_dir: str = "release-assets") -> int:
        """Build an arm64 release APK named with its SHA-256 digest."""
        os.chdir(REPO_DIR)
        required_command("cargo-apk")
        try:
            env = self._android_environment()
        except RuntimeError as err:
            logging.error("%s", err)
            return 1
        if not self._build_android_apk(env, ANDROID_TARGET, release=True):
            return 1

        source = self._android_apk_path(release=True)
        digest = hashlib.sha256()
        with source.open("rb") as apk_file:
            for chunk in iter(lambda: apk_file.read(1024 * 1024), b""):
                digest.update(chunk)
        checksum = digest.hexdigest()
        destination_dir = Path(output_dir)
        if not destination_dir.is_absolute():
            destination_dir = REPO_DIR / destination_dir
        destination_dir.mkdir(parents=True, exist_ok=True)
        destination = destination_dir / f"xmms-renascene-{checksum}.apk"
        shutil.copy2(source, destination)
        destination.with_suffix(".apk.sha256").write_text(
            f"{checksum}  {destination.name}\n"
        )
        logging.info("Android release APK written to %s", destination)
        return 0

    async def deploy_android(self, release: bool = False) -> int:
        """Build and install an arm64 APK on the USB-attached Android device."""
        os.chdir(REPO_DIR)
        required_command("cargo-apk")
        try:
            env = self._android_environment()
        except RuntimeError as err:
            logging.error("%s", err)
            return 1

        adb = Path(env["ANDROID_HOME"]) / "platform-tools" / "adb"
        if not adb.is_file():
            logging.error("Android adb was not found at %s", adb)
            return 1

        device_state = subprocess.run(
            [str(adb), "-d", "get-state"],
            text=True,
            capture_output=True,
            check=False,
        )
        if device_state.returncode != 0 or device_state.stdout.strip() != "device":
            detail = device_state.stderr.strip() or device_state.stdout.strip()
            logging.error(
                "no usable USB-attached Android device found%s",
                f": {detail}" if detail else "",
            )
            return 1

        device_abis = subprocess.run(
            [str(adb), "-d", "shell", "getprop", "ro.product.cpu.abilist"],
            text=True,
            capture_output=True,
            check=False,
        )
        if device_abis.returncode != 0:
            logging.error("failed to determine the USB device CPU architecture")
            return 1
        supported_abis = {abi.strip() for abi in device_abis.stdout.split(",")}
        if "arm64-v8a" not in supported_abis:
            logging.error(
                "USB device does not support the arm64-v8a APK; reported ABIs: %s",
                device_abis.stdout.strip() or "unknown",
            )
            return 1

        if not self._build_android_apk(env, ANDROID_TARGET, release=release):
            return 1

        apk = self._android_apk_path(release=release)
        stop_result = subprocess.run(
            [str(adb), "-d", "shell", "am", "force-stop", ANDROID_PACKAGE],
            cwd=REPO_DIR,
            check=False,
        )
        if stop_result.returncode != 0:
            logging.error("failed to force-stop the existing Android app process")
            return 1

        result = subprocess.run(
            [str(adb), "-d", "install", "-r", str(apk)],
            cwd=REPO_DIR,
            check=False,
        )
        if result.returncode != 0:
            logging.error("failed to install Android APK on the USB device")
            return 1
        logging.info("Installed %s on the USB-attached Android device", apk)
        return 0

    async def android_screenshot(
        self,
        output: str = "testoutput/android/xmms-android-initial.png",
        wait_seconds: int = 8,
    ) -> int:
        """Build, launch in the API 35 emulator, and capture an Android screenshot."""
        os.chdir(REPO_DIR)
        required_command("cargo-apk")
        try:
            env = self._android_environment()
            adb = self._ensure_android_emulator(env, headless=True)
        except Exception as err:
            logging.error("failed to prepare Android emulator: %s", err)
            return 1

        if not self._build_android_apk(env, ANDROID_EMULATOR_TARGET):
            return 1
        if not self._install_and_launch_android(adb):
            return 1
        time.sleep(max(0, wait_seconds))
        pid = subprocess.run(
            [str(adb), "shell", "pidof", "org.xmms.renascene"],
            text=True,
            capture_output=True,
            check=False,
        ).stdout.strip()
        if not pid:
            logging.error("Android app is not running after launch")
            return 1

        self._dismiss_android_anr_wait(adb)
        output_path = REPO_DIR / output
        output_path.parent.mkdir(parents=True, exist_ok=True)
        with output_path.open("wb") as screenshot:
            result = subprocess.run(
                [str(adb), "exec-out", "screencap", "-p"],
                stdout=screenshot,
                check=False,
            )
        if result.returncode != 0 or not output_path.is_file() or output_path.stat().st_size == 0:
            logging.error("failed to capture Android screenshot")
            return 1
        logging.info("Android screenshot written to %s", output_path)
        return 0

    async def screenshot(self, *args: str) -> int:
        """Capture a root-window screenshot after starting the selected frontend."""
        os.chdir(REPO_DIR)
        try:
            frontend, audio_backend, selected_args = self._select_run_frontend(args)
        except ValueError as err:
            logging.error("%s", err)
            return 2
        if frontend == "gtk":
            _configure_gtk_environment()
        if audio_backend == "rodio":
            _configure_rodio_audio_environment()
        self._build_frontend_unless_skipped(frontend, audio_backend)
        background, app_args = _split_screenshot_args(selected_args)

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

    def _write_offscreen_frontend_screenshot(self, frontend: str, scenario: str, output: Path) -> None:
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
        logging.info("Capturing offscreen %s screenshot: %s", frontend, " ".join(args))
        args @ cli_follow | raise_on_error
        if not output.is_file() or output.stat().st_size == 0:
            raise RuntimeError(f"{frontend} screenshot was not created at {output}")

    def _write_live_frontend_screenshot(self, frontend: str, scenario: str, output: Path) -> None:
        output.parent.mkdir(parents=True, exist_ok=True)
        args = [
            str(REPO_DIR / "repo"),
            "screenshot",
            "--frontend",
            frontend,
            *_scenario_args(scenario),
        ]
        env = os.environ.copy()
        env.pop("XMMS_EXEC_SKIP_BUILD", None)
        env["XMMS_SCREENSHOT_FILE"] = str(output)
        env.setdefault("XMMS_SCREENSHOT_DELAY", "3")
        logging.info("Capturing live %s screenshot: %s", frontend, " ".join(args))
        result = subprocess.run(args, cwd=REPO_DIR, env=env, check=False)
        if result.returncode != 0:
            raise RuntimeError(f"live {frontend} screenshot command failed with exit code {result.returncode}")
        if not output.is_file() or output.stat().st_size == 0:
            raise RuntimeError(f"live {frontend} screenshot was not created at {output}")

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
        capture_mode: str = "live",
    ) -> int:
        """Capture GTK and egui screenshots for a scenario and write a diff image.

        capture_mode=live captures actual frontend windows under Xvfb/root screenshot.
        capture_mode=offscreen uses the app's --screenshot render path.
        """
        os.chdir(REPO_DIR)
        if capture_mode not in {"live", "offscreen"}:
            logging.error("capture_mode must be 'live' or 'offscreen'")
            return 2
        if capture_mode == "offscreen" and os.environ.get("XMMS_EXEC_SKIP_BUILD") != "1":
            self._build_frontend_diff_app()
        output_root = Path(output_dir)
        gtk_path = Path(gtk_output) if gtk_output else output_root / f"gtk-{scenario}.png"
        egui_path = Path(egui_output) if egui_output else output_root / f"egui-{scenario}.png"
        diff_path = Path(diff_output) if diff_output else output_root / f"diff-{scenario}.png"
        try:
            if capture_mode == "live":
                self._write_live_frontend_screenshot("gtk", scenario, gtk_path)
                self._write_live_frontend_screenshot("egui", scenario, egui_path)
            else:
                self._write_offscreen_frontend_screenshot("gtk", scenario, gtk_path)
                self._write_offscreen_frontend_screenshot("egui", scenario, egui_path)
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

    def _capture_render_screenshots(self, output_root: Path) -> None:
        if os.environ.get("XMMS_EXEC_SKIP_BUILD") != "1":
            self._build_frontend_diff_app()
        for frontend in ("gtk", "egui"):
            for scenario in SCREENSHOT_SCENARIOS:
                self._write_offscreen_frontend_screenshot(
                    frontend,
                    scenario,
                    output_root / frontend / f"{scenario}.png",
                )

    async def render_baseline_capture(self, output_dir: str = "sunsetcairo-screenshots/current") -> int:
        """Capture current offscreen renderer screenshots for all golden scenarios."""
        os.chdir(REPO_DIR)
        try:
            self._capture_render_screenshots(Path(output_dir))
        except Exception as err:
            logging.error("render baseline capture failed: %s", err)
            return 1
        return 0

    def _e2e_venv_python(self) -> Path:
        if sys.platform == "win32":
            return E2E_VENV / "Scripts" / "python.exe"
        return E2E_VENV / "bin" / "python"

    def _ensure_e2e_venv(self) -> Path:
        if not E2E_REQUIREMENTS.is_file():
            raise RuntimeError(f"missing Python E2E requirements file: {E2E_REQUIREMENTS}")
        if not E2E_CREATE_VENV.is_file():
            raise RuntimeError(f"missing Python E2E virtualenv script: {E2E_CREATE_VENV}")
        python = self._e2e_venv_python()
        if python.is_file():
            logging.info("Updating Python E2E virtualenv requirements...")
        else:
            logging.info("Creating Python E2E virtualenv...")
        [sys.executable, str(E2E_CREATE_VENV)] @ cli_follow | raise_on_error
        return python

    def _pye2e_install_hint(self, missing: list[str]) -> str:
        package_by_command = {
            "cargo": "rustup or cargo",
            "dbus-daemon": "dbus",
            "ffmpeg": "ffmpeg",
            "import": "imagemagick",
            "python3": "python3",
            "xdotool": "xdotool",
            "xdpyinfo": "x11-utils",
            "xvfb-run": "xvfb",
            "xwd": "x11-apps",
        }
        packages = [package_by_command.get(command, command) for command in missing]
        unique_packages = list(dict.fromkeys(packages))
        return "sudo apt install -y " + " ".join(unique_packages)

    def _display_is_reachable(self) -> bool:
        display = os.environ.get("DISPLAY")
        if not display:
            return False
        if not command_exists("xdpyinfo"):
            logging.warning("DISPLAY is set but xdpyinfo is missing; assuming DISPLAY=%s is usable", display)
            return True
        result = subprocess.run(
            ["xdpyinfo", "-display", display],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        return result.returncode == 0

    def _pye2e_xvfb_server_args(self) -> str:
        return os.environ.get(
            "XMMS_E2E_XVFB_SERVER_ARGS",
            os.environ.get("XMMS_XVFB_SERVER_ARGS", "-screen 0 1024x768x24"),
        )

    def _pye2e_command(self, python: Path, args: tuple[str, ...]) -> list[str]:
        pytest_command = [str(python), "-m", "pytest", "e2e", *args]
        android_only = any(
            arg == "e2e/test_android_ui.py"
            or arg.endswith("/test_android_ui.py")
            or arg == "android"
            or arg == "-m=android"
            for arg in args
        )
        if android_only:
            logging.info("Running Android Python E2E tests without Xvfb")
            return pytest_command
        disable_xvfb = os.environ.get("XMMS_E2E_USE_XVFB") == "0"
        force_xvfb = os.environ.get("XMMS_E2E_FORCE_XVFB") == "1"
        if disable_xvfb and not force_xvfb:
            logging.info("XMMS_E2E_USE_XVFB=0 set; running Python E2E tests on the current DISPLAY")
            return pytest_command
        if command_exists("xvfb-run"):
            server_args = self._pye2e_xvfb_server_args()
            logging.info("Running Python E2E tests under xvfb-run by default (%s)", server_args)
            return ["xvfb-run", "-a", "-s", server_args, *pytest_command]
        if force_xvfb or not self._display_is_reachable():
            logging.error(
                "xvfb-run is required for this Python E2E run. Install it with: %s",
                self._pye2e_install_hint(["xvfb-run"]),
            )
            raise FileNotFoundError("xvfb-run")
        logging.warning(
            "xvfb-run is missing; falling back to the current DISPLAY. On GNOME/Wayland this may show screen-sharing prompts. Install it with: %s",
            self._pye2e_install_hint(["xvfb-run"]),
        )
        return pytest_command

    def _warn_missing_pye2e_tools(self) -> None:
        missing = [command for command in ["cargo", "dbus-daemon", "ffmpeg", "xdotool"] if not command_exists(command)]
        if os.environ.get("XMMS_E2E_USE_XVFB") != "0" and not command_exists("xvfb-run"):
            missing.append("xvfb-run")
        if not command_exists("import") and not command_exists("xwd"):
            missing.extend(["import", "xwd"])
        if missing:
            logging.warning(
                "Some Python E2E tools are missing (%s). Tests may fail or skip. Debian/Ubuntu setup: %s",
                ", ".join(dict.fromkeys(missing)),
                self._pye2e_install_hint(list(dict.fromkeys(missing))),
            )

    async def pye2e(self, *args: str) -> int:
        """Run Python GUI E2E tests inside e2e/.venv.

        Creates/updates the virtualenv from e2e/requirements.txt, checks common
        local E2E tools, and runs pytest under xvfb-run by default so local
        GNOME/Wayland sessions are not touched. Set XMMS_E2E_USE_XVFB=0 to use
        the current DISPLAY instead. Extra args are passed to pytest, e.g.
        `./repo pye2e -k gtk`.
        """
        os.chdir(REPO_DIR)
        self._warn_missing_pye2e_tools()
        try:
            python = self._ensure_e2e_venv()
            command = self._pye2e_command(python, args)
        except Exception as err:
            logging.error("failed to prepare Python E2E run: %s", err)
            return 1
        logging.info("Running Python E2E tests: %s", " ".join(shlex.quote(part) for part in command))
        result = subprocess.run(command, cwd=REPO_DIR, check=False)
        return result.returncode

    def _build_pye2e_docker_image(self, image: str = E2E_DOCKER_IMAGE) -> int:
        """Build the Docker image that contains Xvfb and screenshot tools for Python E2E."""
        os.chdir(REPO_DIR)
        required_command("docker")
        command = ["docker", "build", "-f", str(E2E_DOCKERFILE), "-t", image, "."]
        logging.info("Building Python E2E Docker image: %s", " ".join(shlex.quote(part) for part in command))
        return subprocess.run(command, cwd=REPO_DIR, check=False).returncode

    def _docker_image_exists(self, image: str) -> bool:
        result = subprocess.run(
            ["docker", "image", "inspect", image],
            cwd=REPO_DIR,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        return result.returncode == 0

    async def pye2e_docker(self, *args: str) -> int:
        """Run Python E2E tests in Docker with an in-container Xvfb server.

        The image includes GTK/GStreamer build dependencies, Xvfb, xdotool,
        ImageMagick `import`, and xwd. Extra args are passed through to pytest.
        Set XMMS_E2E_DOCKER_IMAGE to override the image tag, or
        XMMS_E2E_DOCKER_SKIP_BUILD=1 to reuse an existing image.
        """
        os.chdir(REPO_DIR)
        required_command("docker")
        image = os.environ.get("XMMS_E2E_DOCKER_IMAGE", E2E_DOCKER_IMAGE)
        if os.environ.get("XMMS_E2E_DOCKER_SKIP_BUILD") != "1" or not self._docker_image_exists(image):
            build_result = self._build_pye2e_docker_image(image)
            if build_result != 0:
                return build_result

        output_dir = REPO_DIR / "testoutput"
        output_dir.mkdir(parents=True, exist_ok=True)
        command = [
            "docker",
            "run",
            "--rm",
            "-v",
            f"{REPO_DIR}:/workspace",
            "-v",
            f"{output_dir}:/testoutput",
            "-e",
            "XMMS_E2E_SCREENSHOT_DIR=/testoutput",
            "-e",
            "XMMS_E2E_VENV_DIR=/tmp/xmms-renascene-pye2e-venv",
            "-e",
            "XMMS_E2E_USE_XVFB=0",
        ]
        if hasattr(os, "getuid") and hasattr(os, "getgid"):
            # Preserve host UID/GID so mounted artifacts are writable by the caller,
            # but also expose passwd/group entries so services such as dbus-daemon
            # can resolve the current UID inside the container.
            if Path("/etc/passwd").is_file():
                command.extend(["-v", "/etc/passwd:/etc/passwd:ro"])
            if Path("/etc/group").is_file():
                command.extend(["-v", "/etc/group:/etc/group:ro"])
            command.extend(
                [
                    "--user",
                    f"{os.getuid()}:{os.getgid()}",
                    "-e",
                    "HOME=/tmp/xmms-e2e-home",
                    "-e",
                    "CARGO_HOME=/tmp/xmms-e2e-cargo",
                    "-e",
                    "CARGO_TARGET_DIR=/tmp/xmms-e2e-target",
                ]
            )
        command.extend([image, "./repo", "pye2e", *args])
        logging.info("Running Python E2E tests in Docker: %s", " ".join(shlex.quote(part) for part in command))
        return subprocess.run(command, cwd=REPO_DIR, check=False).returncode


def dispatch_args(argv: list[str]) -> int:
    return FireLite([([], RepoTool()), ([], FlatpakInstaller())]).dispatchArgs(argv)


def main() -> int:
    configure_logging()
    return dispatch_args(sys.argv[1:])
