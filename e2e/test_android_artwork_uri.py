"""Separate-UID URI delivery, lazy paged browse and known-current recovery."""

import json
import time
from importlib import import_module
from pathlib import Path
from typing import Any

from test_android_artwork import (
    ANDROID_AUTO_PROBE_ACTIVITY,
    AndroidDevice,
    _prepare_audio,
)

pytest: Any = import_module("pytest")
pytestmark = pytest.mark.android
CLIENT = "org.xmms.artworkclient/.ArtworkClientActivity"


@pytest.fixture(scope="session")
def android_artwork_client(android_device: AndroidDevice) -> AndroidDevice:
    """Install the separate-UID client even when a URI test runs on its own."""
    apk = Path(__file__).resolve().parents[1] / "target/android/artwork-test-client/artwork-client.apk"
    if not apk.is_file():
        raise AssertionError("Build external client with python3 scripts/build_artwork_test_client.py")
    android_device.command("install", "-r", str(apk))
    return android_device


def _client(device: AndroidDevice, mode: str, uri: str = "", *, stats: bool = False, audit_provider: bool = False) -> dict[str, Any]:
    device.shell("am", "force-stop", "org.xmms.artworkclient")
    device.clear_logcat()
    args = ["am", "start", "-n", CLIENT, "--es", "mode", mode]
    if audit_provider:
        args.extend(["--ez", "audit_provider", "true"])
    if stats:
        args.extend(["--ez", "stats", "true"])
    if uri:
        args.extend(["--es", "uri", uri])
    device.shell(*args)
    deadline = time.monotonic() + 12
    while time.monotonic() < deadline:
        log = device.command("logcat", "-d", "-s", "XMMS_EXTERNAL_ARTWORK:I", "*:S").stdout
        for line in reversed(log.splitlines()):
            if "result {" in line:
                return json.loads(line.split("result ", 1)[1])
        time.sleep(0.1)
    raise AssertionError("Separate-UID artwork client timed out")


def test_android_artwork_uri_external_security_browse_and_missing_recovery(android_artwork_client: AndroidDevice) -> None:
    device = android_artwork_client
    _prepare_audio(device)
    first = _client(device, "high-resolution", stats=True, audit_provider=True)
    assert first["uriWidth"] == first["uriHeight"] == 512
    assert first["allUriKeysMatch"] and not first["duplicateBitmapFields"]
    assert first["unknownRequestsQuiet"]
    assert first["width"] == 320 and first["writeRejected"]
    assert first["invalidReadsRejected"] == 6 and first["writeModesRejected"] == 5
    assert first["typedWidth"] == 512 and first["oversizedRejected"]
    assert first["mutationsRejected"] == 4 and first["unsupportedMimeRejected"]
    assert first["clientUriOpenNs"] > 0 and first["clientUriDecodeNs"] > 0
    assert first["clientUriBitmapBytes"] >= 512 * 512 * 4
    assert first["browseCount"] == 2 and first["browseBitmaps"] == 0 and first["browseUris"] > 0
    browse = first["workerStats"]["browseArtwork"]
    assert browse["peakActive"] <= 1 and browse["pending"] <= 1 and browse["renders"] <= 2
    assert first["workerStats"]["peakActive"] <= 1
    assert first["workerStats"]["uriCache"]["diskCacheEntries"] <= 160
    assert first["clientUid"] != int(device.shell("run-as", "org.xmms.renascene", "id", "-u").stdout.strip())
    identifier = first["uri"].rsplit("/", 1)[1]
    device.shell("run-as", "org.xmms.renascene", "rm", "cache/media-artwork/" + identifier + ".png")
    missing = _client(device, "read-uri", first["uri"])
    assert missing["readRejected"]
    recovered = _client(device, "high-resolution")
    assert recovered["uriWidth"] == 512 and recovered["uri"] != first["uri"]
    identifier = recovered["uri"].rsplit("/", 1)[1]
    device.write_private_bytes("cache/media-artwork/" + identifier + ".png", b"corrupt")
    corrupt = _client(device, "read-uri", recovered["uri"])
    assert corrupt["readRejected"]
    final = _client(device, "high-resolution")
    assert final["uriWidth"] == 512 and final["uri"] != recovered["uri"]
    output = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/uri/client-results.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    # An actual uninstall/reinstall must not revive old random URI mappings.
    device.force_stop()
    device.command("uninstall", "org.xmms.renascene")
    player_apk = Path(__file__).resolve().parents[1] / "target/debug/apk/xmms-renascene.apk"
    device.command("install", str(player_apk))
    _prepare_audio(device)
    stale = _client(device, "read-uri", final["uri"])
    assert stale["readRejected"]
    fresh = _client(device, "high-resolution")
    assert fresh["uriWidth"] == 512 and fresh["uri"] != final["uri"]
    output.write_text(json.dumps({"first": first, "missing": missing, "recovered": recovered, "corrupt": corrupt,
                                 "final": final, "staleAfterReset": stale, "freshAfterReset": fresh}, indent=2))


def test_android_artwork_unpaged_large_playlist_never_prefetches(android_artwork_client: AndroidDevice) -> None:
    device = android_artwork_client
    _prepare_audio(device)
    playlist = "#EXTM3U\n" + "".join(f"#EXTINF:180,Track {index}\nfile:///data/user/0/org.xmms.renascene/files/imports/first.wav\n" for index in range(200))
    device.write_private_file("files/config/xmms-renascene/playlist.m3u", playlist)
    result = _client(device, "browse-unpaged", stats=True)
    assert result["browseCount"] == 200 and result["browseBitmaps"] == 0 and result["browseUris"] == 0
    browse = result["workerStats"]["browseArtwork"]
    assert browse["attempts"] == browse["renders"] == browse["pending"] == 0
    # Root opt-in may warm the one selected track; it must not scale with the
    # 200 browse entries. Lazy browse itself performed no work above.
    assert result["workerStats"]["renders"] <= 1
    assert result["browseLatencyMs"] < 2500
    output = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/uri/unpaged-results.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2))


def test_android_artwork_cache_pressure_grace_and_delayed_readers(android_device: AndroidDevice) -> None:
    device = android_device
    device.force_stop()
    device.clear_logcat()
    device.shell("am", "start", "-n", ANDROID_AUTO_PROBE_ACTIVITY, "-a", "org.xmms.renascene.e2e.ARTWORK_CACHE_TEST")
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        log = device.command("logcat", "-d", "-s", "XMMS_AUTO_PROBE:I", "*:S").stdout
        assert "cache-test-failed" not in log, log
        for line in log.splitlines():
            if "cache-test {" in line:
                result = json.loads(line.split("cache-test ", 1)[1])
                assert result["normalCount"] <= 128 and result["normalBytes"] <= 32 * 1024 * 1024
                assert result["hardCount"] <= 160 and result["hardBytes"] <= 40 * 1024 * 1024
                assert result["delayedReader"] and result["gracePressure"]
                output = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/uri/cache-pressure.json"
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_text(json.dumps(result, indent=2))
                return
        time.sleep(0.2)
    raise AssertionError("Cache pressure test timed out")
