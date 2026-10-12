"""The foreground bitmap title must follow notification/service track changes."""
from __future__ import annotations

import time
import wave
from importlib import import_module
from io import BytesIO
from typing import Any

from PIL import Image

from android import ANDROID_PACKAGE, AndroidDevice
from gui import BASE_MAIN_WIDTH, MAIN_BUTTON_RECTS, MainButton

pytest: Any = import_module("pytest")
pytestmark = pytest.mark.android


def _wait_for_title(device: AndroidDevice, title: str) -> None:
    deadline = time.monotonic() + 10
    session = ""
    while time.monotonic() < deadline:
        session = device.shell("dumpsys", "media_session").stdout
        if f"description={title}" in session:
            return
        time.sleep(0.1)
    raise AssertionError(f"Media session did not publish {title}: {session}")


def _title_pixels(device: AndroidDevice, path: Any) -> bytes:
    left, top, right, _ = device.main_player_bounds()
    scale = (right - left) / BASE_MAIN_WIDTH
    device.screenshot(path)
    with Image.open(path) as image:
        # Only the non-scrolling bitmap title, excluding timer/visualization.
        box = (
            round(left + 111 * scale), round(top + 27 * scale),
            round(left + 264 * scale), round(top + 33 * scale),
        )
        return image.convert("RGB").crop(box).tobytes()


def _wait_for_changed_title_pixels(device: AndroidDevice, test_output: Any, previous: bytes) -> bytes:
    deadline = time.monotonic() + 10
    candidate = previous
    stable_since = time.monotonic()
    while time.monotonic() < deadline:
        pixels = _title_pixels(device, test_output.screenshot_path())
        if pixels != candidate:
            candidate = pixels
            stable_since = time.monotonic()
        if pixels != previous and time.monotonic() - stable_since >= 0.5:
            return pixels
        time.sleep(0.1)
    raise AssertionError("Player title did not change and settle")


@pytest.mark.parametrize("recreate_activity", [False, True])
def test_android_player_title_follows_background_notification_next(
    android_device: AndroidDevice, test_output: Any, recreate_activity: bool,
) -> None:
    device = android_device
    device.force_stop()
    device.shell("pm", "clear", ANDROID_PACKAGE)
    device.grant_runtime_permissions()
    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8_000)
        wav.writeframes(bytes([128]) * 8_000 * 180)
    device.write_private_bytes("files/imports/title-refresh.wav", audio.getvalue())
    uri = "file:///data/user/0/org.xmms.renascene/files/imports/title-refresh.wav"
    device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        f"#EXTM3U\n#EXTINF:180,FIRST\n{uri}\n#EXTINF:180,SECOND\n{uri}\n",
    )
    device.start_activity()
    initial = _title_pixels(device, test_output.screenshot_path())
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    device.wait_for_service("XmmsPlaybackService")
    _wait_for_title(device, "FIRST")
    first = _wait_for_changed_title_pixels(device, test_output, initial)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.NEXT])
    _wait_for_title(device, "SECOND")
    expected = _wait_for_changed_title_pixels(device, test_output, first)
    assert first != expected, "Reference titles must render differently"
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PREVIOUS])
    _wait_for_title(device, "FIRST")
    assert _wait_for_changed_title_pixels(device, test_output, expected) == first
    pid = device.app_pid()
    if recreate_activity:
        device.close_activity()
    else:
        device.go_home()
    # Same MediaSession transport path as the notification's Next action.
    device.shell("cmd", "media_session", "dispatch", "next")
    _wait_for_title(device, "SECOND")
    device.start_activity()
    assert device.app_pid() == pid, "The playback service must remain alive"
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        actual = _title_pixels(device, test_output.screenshot_path())
        if actual == expected:
            break
        time.sleep(0.1)
    else:
        raise AssertionError("Player still renders the old title after notification Next")
    _wait_for_title(device, "SECOND")  # UI must not revert the service's selection.
    device.assert_no_app_crash()
