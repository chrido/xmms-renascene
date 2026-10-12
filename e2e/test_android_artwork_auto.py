"""Auto artwork: real bounded APIC discovery and generated skin fallback."""

import re
import subprocess
import time
from io import BytesIO
from pathlib import Path

import pytest
from PIL import Image

from test_android_artwork import (
    ANDROID_AUTO_PROBE_ACTIVITY,
    AndroidDevice,
    _client,
    _events,
    _prepare_audio,
)

pytestmark = pytest.mark.android


def _syncsafe(value: int) -> bytes:
    return bytes([(value >> 21) & 127, (value >> 14) & 127, (value >> 7) & 127, value & 127])


def test_android_artwork_auto_uses_cover_and_corrupt_cover_uses_skin(android_device: AndroidDevice) -> None:
    device = android_device
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/extensions/auto"
    _prepare_audio(device)
    image = BytesIO()
    Image.new("RGB", (16, 16), (200, 40, 10)).save(image, format="PNG")
    checksum = 1
    for _ in range(320 * 320):
        checksum = (31 * checksum + 0xffc8280a) & 0xffffffff
    for label, pixels in [("valid", image.getvalue()), ("corrupt", b"not-an-image"),
                          ("unsupported", b"GIF89a"), ("oversized", bytes(1024 * 1024 + 1))]:
        device.force_stop()
        payload = b"\0image/png\0\x03\0" + pixels
        frame = b"APIC" + _syncsafe(len(payload)) + b"\0\0" + payload
        tag = b"ID3\x04\0\0" + _syncsafe(len(frame)) + frame
        device.write_private_bytes("files/imports/cover.mp3", tag)
        # Renderer needs only the file identity; keep normal WAV decoder playback
        # in the playlist, with an ID3 prefix APIC on the private source path.
        device.write_private_bytes("files/imports/first.wav", tag)
        device.write_private_file("files/config/xmms-renascene/config", "[xmms]\nartwork_policy=auto\nplaylist_position=0\n")
        device.clear_logcat()
        device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                     "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER")
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            log = device.command("logcat", "-d").stdout
            events = _events(log)
            if any(e["status"] == "ready" for e in events):
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Auto worker did not return artwork")
        stats = next(e for e in reversed(events) if e["status"] == "ready")
        (root / label).mkdir(parents=True, exist_ok=True)
        (root / label / "logcat.txt").write_text(log)
        assert stats["width"] == 320
        # Worker-only probe carries the actual generated Bitmap checksum; compare
        # the valid cover's known opaque constant-color word hash independently.
        if label == "valid":
            assert stats["checksum"] == format(checksum, "x")
        else:
            assert stats["checksum"] != format(checksum, "x")


@pytest.mark.parametrize("policy_setting", ["artwork_policy=auto\n", ""])
def test_android_artwork_auto_cover_is_published_while_real_mp3_plays(android_device: AndroidDevice, policy_setting: str) -> None:
    device = android_device
    _prepare_audio(device)
    root = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/extensions/auto-playback"
    root.mkdir(parents=True, exist_ok=True)
    # Real playable MP3 frames, not the extraction-only fake ID3 fixture above.
    audio = subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=180",
                            "-ac", "1", "-ar", "22050", "-codec:a", "libmp3lame", "-b:a", "32k",
                            "-write_xing", "0", "-id3v2_version", "0", "-f", "mp3", "pipe:1"],
                           check=True, capture_output=True).stdout
    image = BytesIO()
    Image.new("RGB", (16, 16), (200, 40, 10)).save(image, format="PNG")
    payload = b"\0image/png\0\x03\0" + image.getvalue()
    frame = b"APIC" + _syncsafe(len(payload)) + b"\0\0" + payload
    device.write_private_bytes("files/imports/covered.mp3", b"ID3\x04\0\0" + _syncsafe(len(frame)) + frame + audio)
    device.write_private_file("files/config/xmms-renascene/playlist.m3u",
                             "#EXTM3U\n#EXTINF:180,Playable cover\nfile:///data/user/0/org.xmms.renascene/files/imports/covered.mp3\n")
    device.write_private_file("files/config/xmms-renascene/config", f"[xmms]\n{policy_setting}playlist_position=0\n")
    stats, metadata = _client(device, root / "published", command="publish", export=True)
    checksum = 1
    for _ in range(320 * 320):
        checksum = (31 * checksum + 0xffc8280a) & 0xffffffff
    assert metadata["checksum"] == format(checksum, "x") and metadata["title"] == "Playable cover"
    assert stats["hasMetadataArt"] and stats["renders"] == 1
    source = Image.open(root / "published/source.png").convert("RGB")
    assert source.getpixel((160, 160)) == (200, 40, 10)
    _, later = _client(device, root / "progress", command="stats")
    assert later["checksum"] == metadata["checksum"]
    dump = device.shell("dumpsys", "media_session").stdout
    playing = re.search(r"state=PlaybackState \{state=(?:PLAYING\()?3\)?, position=(\d+)", dump)
    assert playing is not None, dump
    advanced = dump
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        advanced = device.shell("dumpsys", "media_session").stdout
        next_position = re.search(r"state=PlaybackState \{state=(?:PLAYING\()?3\)?, position=(\d+)", advanced)
        if next_position is not None and int(next_position[1]) > int(playing[1]):
            break
        time.sleep(0.2)
    else:
        raise AssertionError("Playable MP3 did not advance: " + advanced)
    (root / "playing.txt").write_text(dump + "\nAFTER\n" + advanced)
