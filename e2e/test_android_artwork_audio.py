"""Audio-inspired mode derives bounded static geometry from private WAV data."""

import time
import wave
from importlib import import_module
from io import BytesIO
from pathlib import Path
from typing import Any

from test_android_artwork import (
    ANDROID_AUTO_PROBE_ACTIVITY,
    AndroidDevice,
    _events,
    _prepare_audio,
)

pytest: Any = import_module("pytest")
pytestmark = pytest.mark.android


def test_android_artwork_audio_summary_changes_geometry_at_same_uri(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    checksums = []
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/extensions/audio"
    for label, value in [("silence", 0), ("tone", 16000), ("repeat-tone", 16000)]:
        device.force_stop()
        audio = BytesIO()
        with wave.open(audio, "wb") as writer:
            writer.setnchannels(1)
            writer.setsampwidth(2)
            writer.setframerate(8000)
            frames = b"".join((value if i % 2 else -value).to_bytes(2, "little", signed=True) for i in range(16000))
            writer.writeframes(frames)
        device.write_private_bytes("files/imports/first.wav", audio.getvalue())
        device.write_private_file("files/config/xmms-renascene/config", "[xmms]\nartwork_policy=audio-inspired\nplaylist_position=0\n")
        device.clear_logcat()
        device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                     "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER")
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            log = device.command("logcat", "-d").stdout
            events = _events(log)
            if any(event["status"] == "ready" for event in events):
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Audio-summary worker did not return artwork")
        result = next(event for event in reversed(events) if event["status"] == "ready")
        assert result["width"] == 320 and result["renders"] == 1 and result["peakActive"] <= 1
        checksums.append(result["checksum"])
        (root / label).mkdir(parents=True, exist_ok=True)
        (root / label / "logcat.txt").write_text(log)
    assert checksums[0] != checksums[1]
    assert checksums[1] == checksums[2]


def test_android_artwork_audio_unsupported_source_matches_generated_fallback(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    device.write_private_bytes("files/imports/first.wav", b"unsupported-format")
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/extensions/audio-fallback"
    checksums = []
    for policy in ["generated", "audio-inspired"]:
        device.force_stop()
        device.write_private_file("files/config/xmms-renascene/config", f"[xmms]\nartwork_policy={policy}\nplaylist_position=0\n")
        device.clear_logcat()
        device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                     "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER")
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            log = device.command("logcat", "-d").stdout
            ready = [event for event in _events(log) if event["status"] == "ready"]
            if ready:
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Unsupported-audio fallback worker did not return")
        stats = ready[-1]
        assert stats["peakActive"] <= 1 and stats["width"] == 320 and stats["renders"] == 1
        checksums.append(stats["checksum"])
        (root / policy).mkdir(parents=True, exist_ok=True)
        (root / policy / "logcat.txt").write_text(log)
        assert "Android Activity initialized:" not in log
    assert checksums[0] == checksums[1]
