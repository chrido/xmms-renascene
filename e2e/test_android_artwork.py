"""Real service/JNI artwork worker tests; native media artwork display follows later."""

from __future__ import annotations

import json
import re
import subprocess
import time
import wave
from importlib import import_module
from io import BytesIO
from pathlib import Path
from typing import TYPE_CHECKING, Any

from gui import MAIN_BUTTON_RECTS, MainButton

if TYPE_CHECKING:
    AndroidDevice = Any
else:
    AndroidDevice = import_module("android").AndroidDevice
android: Any = import_module("android")
ANDROID_AUTO_PROBE_ACTIVITY: str = android.ANDROID_AUTO_PROBE_ACTIVITY
ANDROID_PACKAGE: str = android.ANDROID_PACKAGE

pytest: Any = import_module("pytest")
pytestmark = pytest.mark.android


def _events(log: str) -> list[dict[str, Any]]:
    return [json.loads(match.group(1)) for match in re.finditer(r"artwork (\{[^\n]*\})", log)]


def _worker_probe(device: AndroidDevice, scenario: str, output: Path) -> tuple[list[dict[str, Any]], str]:
    device.force_stop()
    device.shell("pm", "clear", ANDROID_PACKAGE)
    device.write_private_file(
        "files/config/xmms-renascene/config",
        "[xmms]\nartwork_policy=generated\nplaylist_position=0\n",
    )
    # Worker geometry needs only identity, not an audio decoder or active player.
    device.write_private_file(
        "files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n#EXTINF:60,Artwork Worker Track\n"
        "file:///data/user/0/org.xmms.renascene/files/imports/artwork-worker.wav\n",
    )
    device.clear_logcat()
    args = ["am", "start", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
            "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "case", scenario]
    if scenario in {"close", "replace"}:
        args += ["--ei", "delivery_delay_ms", "2000"]
    if scenario == "burst":
        args += ["--ei", "burst", "100", "--ei", "delay_ms", "100"]
    if scenario == "retry":
        args += ["--ei", "failures", "3"]
    device.shell(*args)
    deadline = time.monotonic() + 12
    log = ""
    final = {"close": "closed", "cache": "cache-hit", "retry": "error", "guard": "main-rejected"}.get(scenario, "ready")
    events: list[dict[str, Any]] = []
    while time.monotonic() < deadline:
        log = device.command("logcat", "-d").stdout
        events = _events(log)
        if any(e.get("status") == final and (scenario != "retry" or e.get("failures") == 3) for e in events):
            break
        time.sleep(0.1)
    else:
        output.mkdir(parents=True, exist_ok=True)
        (output / "logcat.txt").write_text(log)
        raise AssertionError(f"Worker probe {scenario} did not reach {final}: {events}\n{log[-8000:]}")
    # Delayed, closed/replaced outputs must not publish after the probe finishes.
    if scenario in {"close", "replace"}:
        time.sleep(2.1)
        log = device.command("logcat", "-d").stdout
        events = _events(log)
    output.mkdir(parents=True, exist_ok=True)
    (output / "logcat.txt").write_text(log)
    (output / "events.json").write_text(json.dumps(events, indent=2) + "\n")
    (output / "media-session.txt").write_text(device.shell("dumpsys", "media_session").stdout)
    return events, log


@pytest.mark.parametrize("scenario", ["run", "cache", "burst", "close", "replace", "retry", "guard"])
def test_android_artwork_service_worker_without_player_activity(
    android_device: AndroidDevice, scenario: str,
) -> None:
    root = Path(__file__).resolve().parents[1]
    events, log = _worker_probe(android_device, scenario, root / "testoutput/android-artwork/chunk-5" / scenario)
    assert "Android Activity initialized:" not in log, "Player Activity must not be needed for rasterization"
    assert events
    assert all(e["peakActive"] <= 1 and e["peakPending"] <= 1 and e["cacheEntries"] <= 2 for e in events)
    if scenario == "close":
        closed = next(e for e in events if e["status"] == "closed")
        assert closed["accepted"] == 0 and closed["closed"]
        assert any(e["status"] == "rendered" and e["renders"] == 1 for e in events)
        assert not any(e["status"] == "ready" for e in events)
    elif scenario == "guard":
        rejected = next(e for e in events if e["status"] == "main-rejected")
        assert rejected["mainRejected"] is True
        assert rejected["renders"] == 1 and rejected["accepted"] == 1
    elif scenario == "retry":
        final = next(e for e in events if e["status"] == "error" and e["failures"] == 3)
        assert final["attempts"] == 3 and final["renders"] == 0 and final["accepted"] == 0
    else:
        ready = [e for e in events if e["status"] in {"ready", "cache-hit"}][-1]
        assert ready["width"] == 320 and ready["height"] == 320 and ready["checksum"]
        assert ready["accepted"] == 1
        if scenario == "cache":
            assert ready["cacheHits"] == 1 and ready["renders"] == 1 and ready["attempts"] == 1
        if scenario == "burst":
            assert ready["generation"] == 100
        if scenario == "replace":
            old = next(e for e in events if e["status"] == "rendered")
            assert ready["epoch"] > old["epoch"]
            assert old["accepted"] == 0
            assert not any(e["status"] == "ready" and e["epoch"] == old["epoch"] for e in events)


def _metadata(log: str) -> list[dict[str, Any]]:
    return [json.loads(m.group(1)) for m in re.finditer(r"metadata-art (\{[^\n]*\})", log)]


def _client(device: AndroidDevice, output: Path, *, command: str = "stats", transport: str | None = None, export: bool = False, extras: list[str] | None = None, timed: bool = False, timeout_seconds: float = 10) -> tuple[dict[str, Any], dict[str, Any]]:
    device.clear_logcat()
    probe_token = str(time.monotonic_ns())
    args = ["am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
            "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "command", command,
            "--es", "probe_token", probe_token]
    if export:
        args += ["--ez", "export", "true"]
    if transport is not None:
        args += ["--es", "transport", transport]
        if transport in {"next", "previous"}:
            args += ["--es", "expected_media_id", "track:1" if transport == "next" else "track:0"]
    args += extras or []
    device.shell(*args)
    deadline = time.monotonic() + timeout_seconds
    log = ""
    while time.monotonic() < deadline:
        log = device.command("logcat", "-d").stdout
        events = [e for e in _events(log) if e.get("probeToken") == probe_token]
        metadata = [e for e in _metadata(log) if e.get("probeToken") == probe_token]
        if events and metadata and not events[-1].get("mutationPending", False) and (
                events[-1].get("status") in {"disabled", "closed", "error", "main-rejected"} or (
                    events[-1].get("completedNs", 0) >= events[-1].get("requestNs", 1)
                    and events[-1].get("hasMetadataArt", False) and metadata[-1].get("width") == 320
                    and events[-1].get("checksum") == metadata[-1].get("checksum"))) and (not timed or any(
                m.get("requestTag") == events[-1].get("requestTag") and m.get("requestToClientReadyNs", 0) > 0 for m in metadata)) and (not export or (
                "artwork-export ready" in log and any(m.get("requestToClientReadyNs", 0) > 0 for m in metadata))):
            break
        time.sleep(0.1)
    else:
        output.mkdir(parents=True, exist_ok=True)
        (output / "logcat.txt").write_text(log)
        raise AssertionError(f"No controller artwork response: {log[-6000:]}")
    output.mkdir(parents=True, exist_ok=True)
    (output / "logcat.txt").write_text(log)
    (output / "events.json").write_text(json.dumps(events, indent=2) + "\n")
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (output / "media-session.txt").write_text(device.shell("dumpsys", "media_session").stdout)
    (output / "notifications.txt").write_text(device.shell("dumpsys", "notification", "--noredact").stdout)
    if export:
        # AndroidDevice uses text-mode output; read binary PNG through adb directly.
        args = [str(device.adb)] + (["-s", device.serial] if device.serial else [])
        source = subprocess.run([*args, "exec-out", "run-as", ANDROID_PACKAGE, "cat", "files/artwork-probe.png"], capture_output=True, check=True).stdout
        (output / "source.png").write_bytes(source)
    return events[-1], metadata[-1]


def _prepare_audio(device: AndroidDevice) -> None:
    device.force_stop()
    device.shell("pm", "clear", ANDROID_PACKAGE)
    device.grant_runtime_permissions()
    audio = BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8000)
        wav.writeframes(bytes([128]) * 8000 * 180)
    for name in ["first", "second"]:
        device.write_private_bytes(f"files/imports/{name}.wav", audio.getvalue())
    device.write_private_file("files/config/xmms-renascene/config", "[xmms]\nartwork_policy=generated\nplaylist_position=0\n")
    device.write_private_file("files/config/xmms-renascene/playlist.m3u",
        "#EXTM3U\n#EXTINF:180,Artwork First\nfile:///data/user/0/org.xmms.renascene/files/imports/first.wav\n"
        "#EXTINF:180,Artwork Second\nfile:///data/user/0/org.xmms.renascene/files/imports/second.wav\n")


def test_android_artwork_publishes_and_deduplicates_background_transport(android_device: AndroidDevice) -> None:
    device = android_device
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/chunk-6/publication"
    _prepare_audio(device)
    first, art = _client(device, root / "first", command="publish")
    assert art["title"] == "Artwork First" and art["mediaId"] == "track:0"
    assert art["width"] == 320 and art["height"] == 320 and art["opaque"]
    assert first["hasMetadataArt"] and first["renders"] == 1
    assert first["metadataPublications"] == 2 and first["notificationPublications"] == 2
    time.sleep(1)
    steady, same = _client(device, root / "steady")
    assert steady["renders"] == first["renders"] and same["checksum"] == art["checksum"]
    assert steady["metadataPublications"] == first["metadataPublications"]
    for action in ["pause", "resume", "seek"]:
        stats, same = _client(device, root / action, transport=action)
        assert stats["renders"] == 1 and stats["metadataPublications"] == 2
        assert same["checksum"] == art["checksum"]
    stale, same = _client(device, root / "stale", command="stale")
    assert stale["rejectedProjections"] >= 1 and same["title"] == "Artwork First"
    assert stale["metadataPublications"] == 2 and stale["renders"] == 1
    second, second_art = _client(device, root / "next", transport="next")
    assert second_art["title"] == "Artwork Second" and second_art["mediaId"] == "track:1"
    assert second_art["checksum"] != art["checksum"] and second["renders"] == 2
    previous, previous_art = _client(device, root / "previous", transport="previous")
    assert previous_art["title"] == "Artwork First" and previous_art["checksum"] == art["checksum"]
    assert previous["renders"] == 2  # ready image cache reused atomically
    device.shell("cmd", "statusbar", "expand-notifications")
    time.sleep(0.5)
    device.screenshot(root / "notification.png")
    device.shell("cmd", "statusbar", "collapse")


def test_android_artwork_setting_off_removes_art_and_survives_cold_start(android_device: AndroidDevice) -> None:
    device = android_device
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/chunk-6/policy"
    _prepare_audio(device)
    _, before = _client(device, root / "generated", command="publish")
    assert before["width"] == 320
    device.shell("cmd", "statusbar", "collapse")
    device.restart_app(reset_data=False)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    time.sleep(0.5)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    time.sleep(0.3)
    device.tap_usable_fraction(0.5, 0.15)  # Visualization category
    time.sleep(0.3)
    device.tap_usable_fraction(0.5, 0.24)  # Native artwork selector
    time.sleep(0.3)
    device.tap_usable_fraction(0.5, 0.315)  # Off
    device.wait_for_private_file_contains("files/config/xmms-renascene/config", "artwork_policy=off")
    device.screenshot(root / "off-setting.png")
    stats, art = _client(device, root / "off")
    assert art["width"] == 0 and not stats["hasMetadataArt"]
    device.force_stop()
    stats, art = _client(device, root / "cold-off", command="publish")
    assert art["width"] == 0 and not stats["hasMetadataArt"] and stats["renders"] == 0


def test_android_artwork_skin_switch_refreshes_without_track_change(android_device: AndroidDevice) -> None:
    device = android_device
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/chunk-6/skin"
    _prepare_audio(device)
    palette_path = "files/config/xmms/Skins/ArtworkRed/viscolor.txt"
    device.write_private_file(palette_path, "\n".join(f"{i * 11},0,0" for i in range(24)))
    _, original = _client(device, root / "original", command="publish")
    device.restart_app(reset_data=False)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    time.sleep(0.5)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    time.sleep(0.3)
    device.tap_usable_fraction(0.5, 0.463)
    time.sleep(0.3)
    device.tap_usable_fraction(0.35, 0.29)  # Only fixture skin in catalog
    selected = "/data/user/0/org.xmms.renascene/" + str(Path(palette_path).parent)
    device.wait_for_private_file_contains("files/config/xmms-renascene/config", "skin=" + selected)
    device.screenshot(root / "red-skin.png")
    stats, red = _client(device, root / "red")
    assert red["width"] == 320 and red["redMax"] > 100
    assert red["greenMax"] == 0 and red["blueMax"] == 0
    assert red["checksum"] != original["checksum"] and red["title"] == original["title"]
    device.restart_app(reset_data=False)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    # Make the CI race deterministic: stats must describe the pending default
    # palette render, not finish on the preceding red generation's ready status.
    _client(device, root / "restore-delay", command="configure", extras=["--ei", "delay_ms", "1500"])
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    time.sleep(0.3)
    device.tap_usable_fraction(0.5, 0.463)
    time.sleep(0.3)
    device.tap_usable_fraction(0.25, 0.196)  # Default skin
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        config = device.read_private_file("files/config/xmms-renascene/config")
        if not any(line.startswith("skin=") for line in config.splitlines()):
            break
        time.sleep(0.1)
    else:
        raise AssertionError("Default skin selection was not persisted")
    stats, restored = _client(device, root / "restored")
    assert restored["checksum"] == original["checksum"]
    assert restored["blueMax"] > 100
