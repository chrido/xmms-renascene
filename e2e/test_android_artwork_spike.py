"""Compatibility and instrumentation checks against actual native controls."""

import json
import time
from importlib import import_module
from pathlib import Path
from typing import Any

from PIL import Image

from test_android_artwork import (
    ANDROID_AUTO_PROBE_ACTIVITY,
    AndroidDevice,
    _client,
    _events,
    _metadata,
    _prepare_audio,
)

pytest: Any = import_module("pytest")
pytestmark = pytest.mark.android


def test_android_artwork_timing_export_and_lock_screen(android_device: AndroidDevice) -> None:
    device = android_device
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/chunk-7/instrumentation"
    _prepare_audio(device)
    stats, metadata = _client(device, root / "source", command="publish", export=True)
    assert stats["requestNs"] > 0 and stats["completedNs"] >= stats["requestNs"]
    assert stats["nativeRenderNs"] > 0 and stats["arrayTransferNs"] > 0
    assert stats["bitmapNs"] > 0 and stats["metadataCallNs"] > 0
    assert stats["notificationCallNs"] > 0
    assert metadata["requestToClientReadyNs"] >= stats["completedNs"] - stats["requestNs"]
    with Image.open(root / "source/source.png") as source:
        assert source.size == (320, 320)
        pixels = source.convert("RGB").tobytes()
        assert len({pixels[i:i + 3] for i in range(0, len(pixels), 3)}) > 20
    device.shell("cmd", "statusbar", "expand-notifications")
    time.sleep(0.5)
    device.screenshot(root / "notification.png")
    device.shell("cmd", "statusbar", "collapse")
    device.shell("locksettings", "set-disabled", "false", check=False)
    device.shell("input", "keyevent", "SLEEP")
    device.shell("input", "keyevent", "WAKEUP")
    time.sleep(0.5)
    device.screenshot(root / "lockscreen.png")
    window = device.shell("dumpsys", "window").stdout
    (root / "lockscreen-window.txt").write_text(window)
    device.shell("wm", "dismiss-keyguard", check=False)
    # Source palette is checked separately: System UI crops/darkens/tints it.
    with Image.open(root / "notification.png") as screenshot:
        pixels = screenshot.convert("RGB").tobytes()
        assert sum(pixels[i + 2] > pixels[i] + 20 and pixels[i + 2] > pixels[i + 1] + 15
                   for i in range(0, len(pixels), 3)) > 10000


def test_android_artwork_cold_delay_is_measured_and_debug_output_is_opt_in(android_device: AndroidDevice) -> None:
    device = android_device
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/chunk-7/cold-delay"
    _prepare_audio(device)
    device.clear_logcat()
    device.shell("am", "start", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--ei", "cold_delay_ms", "500")
    deadline = time.monotonic() + 8
    log = ""
    events: list[dict[str, Any]] = []
    while time.monotonic() < deadline:
        log = device.command("logcat", "-d").stdout
        events = _events(log)
        if any(e["status"] == "ready" for e in events):
            break
        time.sleep(0.1)
    ready = next(e for e in events if e["status"] == "ready")
    assert ready["resolveNs"] >= 450_000_000
    assert ready["completedNs"] - ready["requestNs"] >= ready["resolveNs"]
    assert "I XmmsArtwork:" not in log  # session events still report stats on request
    root.mkdir(parents=True, exist_ok=True)
    (root / "events.json").write_text(json.dumps(events, indent=2) + "\n")
    (root / "logcat.txt").write_text(log)


def test_android_artwork_observer_has_client_times_without_polling(android_device: AndroidDevice) -> None:
    device = android_device
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/chunk-7/observe"
    _prepare_audio(device)
    _client(device, root / "start", command="publish")
    device.clear_logcat()
    device.shell("am", "start", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "command", "observe")
    log = ""
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        log = device.command("logcat", "-d").stdout
        if _events(log) and _metadata(log):
            break
        time.sleep(0.1)
    else:
        raise AssertionError("Observer did not receive session/metadata callbacks: " + log[-6000:])
    device.shell("input", "keyevent", "HOME")
    # The client subscribes to session/metadata events; it has no recurring timer.
    source = (Path(__file__).resolve().parents[1] / "android/java/org/xmms/renascene/XmmsAutoProbeActivity.java").read_text()
    assert '"observe"' in source and "registerCallback(artworkCallback, handler)" in source
