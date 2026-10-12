"""Art-specific lifecycle/revision/failure tests over the real service/backend."""

import json
import re
import time
from importlib import import_module
from pathlib import Path
from typing import Any

from gui import MAIN_BUTTON_RECTS, MainButton
from test_android_artwork import (
    ANDROID_AUTO_PROBE_ACTIVITY,
    ANDROID_PACKAGE,
    AndroidDevice,
    _client,
    _events,
    _metadata,
    _prepare_audio,
)

pytest: Any = import_module("pytest")
pytestmark = pytest.mark.android
ROOT = Path(__file__).resolve().parents[1] / "testoutput/android-artwork/chunk-9"


def test_android_artwork_same_uri_duplicate_titles_eof_and_cold_replacement(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    playlist = "files/config/xmms-renascene/playlist.m3u"
    uri = "file:///data/user/0/org.xmms.renascene/files/imports/"
    device.write_private_file(playlist,
        f"#EXTM3U\n#EXTINF:180,Same Title\n{uri}first.wav\n"
        f"#EXTINF:180,Same Title\n{uri}first.wav\n"
        f"#EXTINF:180,Same Title\n{uri}second.wav\n")
    first, art = _client(device, ROOT / "duplicates/first", command="publish")
    second, duplicate = _client(device, ROOT / "duplicates/duplicate", transport="next")
    assert duplicate["mediaId"] == "track:1" and duplicate["checksum"] == art["checksum"]
    assert second["renders"] == first["renders"] == 1
    # Seek near EOF of duplicate; actual backend EOF advances to distinct file.
    _client(device, ROOT / "duplicates/seek", transport="seek", extras=["--el", "seek_ms", "179000"])
    time.sleep(1.3)
    eof, next_art = _client(device, ROOT / "duplicates/eof")
    assert next_art["mediaId"] == "track:2" and next_art["title"] == "Same Title"
    assert next_art["checksum"] != art["checksum"] and eof["renders"] == 2
    device.force_stop()
    # Same index/title, different content: cold source must not alias old art.
    device.write_private_file(playlist, f"#EXTM3U\n#EXTINF:180,Same Title\n{uri}second.wav\n")
    config = device.read_private_file("files/config/xmms-renascene/config")
    config = re.sub(r"playlist_position=-?\d+", "playlist_position=0", config)
    device.write_private_file("files/config/xmms-renascene/config", config)
    _, replacement = _client(device, ROOT / "duplicates/replacement", command="publish")
    assert replacement["mediaId"] == "track:0" and replacement["checksum"] == next_art["checksum"]


def test_android_artwork_activity_home_finish_and_process_recreation(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    initial, art = _client(device, ROOT / "lifecycle/initial", command="publish")
    device.shell("input", "keyevent", "HOME")
    time.sleep(0.5)
    home, image = _client(device, ROOT / "lifecycle/home")
    assert image["checksum"] == art["checksum"] and home["renders"] == initial["renders"]
    device.restart_app(reset_data=False)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    time.sleep(0.5)
    device.shell("am", "start", "-n", ANDROID_PACKAGE + "/.XmmsActivity",
                 "-a", "org.xmms.renascene.action.FINISH_ACTIVITY")
    time.sleep(0.5)
    finished, image = _client(device, ROOT / "lifecycle/finished")
    assert image["width"] == 320 and image["checksum"] == art["checksum"]
    device.force_stop()
    cold, image = _client(device, ROOT / "lifecycle/cold", command="publish")
    assert cold["renders"] == 1 and image["checksum"] == art["checksum"]
    assert finished["peakActive"] <= 1


def test_android_artwork_palette_read_failure_falls_back_without_retry_storm(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    path = "files/config/xmms/Skins/BrokenPalette/viscolor.txt"
    device.write_private_bytes(path, b"\xff")
    device.write_private_file("files/config/xmms-renascene/config",
        "[xmms]\nartwork_policy=generated\nplaylist_position=0\n"
        "skin=/data/user/0/org.xmms.renascene/files/config/xmms/Skins/BrokenPalette\n")
    stats, art = _client(device, ROOT / "fallback", command="publish")
    assert art["width"] == 320 and art["blueMax"] > 100
    assert stats["attempts"] == 1 and stats["failures"] == 0
    log = (ROOT / "fallback/logcat.txt").read_text()
    assert log.count("Artwork palette fallback;") == 1
    assert stats["paletteFallbacks"] == 1
    assert "FATAL EXCEPTION" not in log


def test_android_artwork_native_completion_cancelled_by_next_and_stop(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    _client(device, ROOT / "cancel/start", command="publish")
    _client(device, ROOT / "cancel/configure", command="configure", extras=["--ei", "delay_ms", "1000"])
    # Next-track rendering is delayed; switch back while it is still running.
    device.clear_logcat()
    device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "transport", "next")
    time.sleep(0.35)
    stats, returned = _client(device, ROOT / "cancel/previous", transport="previous")
    assert returned["title"] == "Artwork First" and returned["width"] == 320
    time.sleep(1.2)
    steady, returned = _client(device, ROOT / "cancel/settled")
    assert returned["mediaId"] == "track:0" and steady["renders"] == 1
    assert steady["peakPending"] <= 1 and steady["peakActive"] <= 1
    device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "transport", "next")
    time.sleep(0.25)
    device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "transport", "stop")
    time.sleep(1.5)
    # The stopped service may remain browser-bound until the probe closes.
    device.shell("input", "keyevent", "BACK")
    time.sleep(0.3)
    # A MediaBrowser binding may keep the component allocated; stop must remove
    # foreground state and bitmap and must not restart audio, even while bound.
    dump = device.shell("dumpsys", "notification", "--noredact").stdout
    sessions = device.shell("dumpsys", "media_session").stdout
    log = device.command("logcat", "-d").stdout
    (ROOT / "cancel/stop-session.txt").write_text(sessions)
    (ROOT / "cancel/stop-logcat.txt").write_text(log)
    assert not ("active=true" in sessions and "org.xmms.renascene/XMMS Renascene" in sessions)
    assert any(e.get("closed") for e in _events(log))
    assert not any("NotificationRecord" in line and ANDROID_PACKAGE in line for line in dump.splitlines())
    assert stats["cacheEntries"] <= 2


def test_android_artwork_cold_resolution_loses_to_foreground_skin_revision(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    palette_path = "files/config/xmms/Skins/RedCold/viscolor.txt"
    device.write_private_file(palette_path, "\n".join(f"{i * 11},0,0" for i in range(24)))
    device.clear_logcat()
    device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--ei", "cold_delay_ms", "2000")
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        if any(e["status"] == "resolving" for e in _events(device.command("logcat", "-d").stdout)):
            break
        time.sleep(0.05)
    else:
        raise AssertionError("Cold worker did not enter delayed resolution")
    device.shell("am", "start", "-n", ANDROID_PACKAGE + "/.XmmsActivity")
    time.sleep(0.8)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    time.sleep(0.3)
    device.tap_usable_fraction(0.5, 0.463)
    time.sleep(0.3)
    device.tap_usable_fraction(0.35, 0.29)
    device.wait_for_private_file_contains("files/config/xmms-renascene/config", "skin=/data/user/0/org.xmms.renascene/files/config/xmms/Skins/RedCold")
    # Finish the earlier delay probe's task before starting a fresh client;
    # it was covered by the player Activity during the race.
    device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "command", "stats")
    time.sleep(0.5)
    device.shell("input", "keyevent", "BACK")
    stats, art = _client(device, ROOT / "cold-race")
    assert art["redMax"] > 100 and art["blueMax"] == 0 and art["greenMax"] == 0
    assert stats["peakActive"] <= 1
    log = device.command("logcat", "-d").stdout
    (ROOT / "cold-race/all-events.json").write_text(json.dumps(_events(log), indent=2) + "\n")
    assert _metadata(log)


@pytest.mark.parametrize("process_fate", ["alive", "killed"])
def test_android_artwork_idle_process_reclamation_restores_selected_palette(
    android_device: AndroidDevice, process_fate: str,
) -> None:
    device = android_device
    _prepare_audio(device)
    selector = "/data/user/0/org.xmms.renascene/files/config/xmms/Skins/ReclaimedRed"
    device.write_private_file("files/config/xmms/Skins/ReclaimedRed/viscolor.txt",
                              "\n".join(f"{i * 11},0,0" for i in range(24)))
    device.write_private_file("files/config/xmms-renascene/config",
                             f"[xmms]\nartwork_policy=generated\nplaylist_position=0\nskin={selector}\n")
    device.start_activity()
    device.main_player_bounds()
    cached_pid = device.cache_process()
    if process_fate == "killed":
        device.kill_background_process()
    device.start_widget_control()
    device.wait_for_media_session_position_at_least(500, timeout=10)
    stats, image = _client(device, ROOT / "reclamation" / process_fate)
    assert image["width"] == 320 and image["redMax"] > 100
    assert image["greenMax"] == 0 and image["blueMax"] == 0
    assert stats["renders"] == 1 and stats["peakActive"] <= 1
    resumed_pid = device.app_pid()
    assert resumed_pid == cached_pid if process_fate == "alive" else resumed_pid != cached_pid
    assert selector in device.read_private_file("files/config/xmms-renascene/config")


def test_android_artwork_same_selector_edit_refreshes_after_skin_reselection(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    path = "files/config/xmms/Skins/EditedPalette/viscolor.txt"
    selector = "/data/user/0/org.xmms.renascene/" + str(Path(path).parent)
    device.write_private_file(path, "\n".join(f"{i * 11},0,0" for i in range(24)))
    device.write_private_file("files/config/xmms-renascene/config",
                             f"[xmms]\nartwork_policy=generated\nplaylist_position=0\nskin={selector}\n")
    first, red = _client(device, ROOT / "same-selector/red", command="publish")
    assert red["redMax"] > 100 and red["blueMax"] == 0
    # Test-only external edit. Android has no palette editor: reselection uses
    # the real loaded-skin hook without changing the persisted selector path.
    device.write_private_file(path, "\n".join(f"0,0,{i * 11}" for i in range(24)))
    device.start_activity()
    device.main_player_bounds()
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.PLAY])
    time.sleep(0.3)
    device.tap_skin_rect(MAIN_BUTTON_RECTS[MainButton.MENU])
    time.sleep(0.3)
    device.tap_usable_fraction(0.5, 0.463)
    time.sleep(0.3)
    device.tap_usable_fraction(0.35, 0.29)
    time.sleep(0.5)
    after, blue = _client(device, ROOT / "same-selector/blue")
    assert blue["blueMax"] > 100 and blue["redMax"] == 0 and blue["greenMax"] == 0
    assert blue["checksum"] != red["checksum"] and blue["title"] == red["title"]
    assert f"skin={selector}" in device.read_private_file("files/config/xmms-renascene/config")
    assert after["peakActive"] <= 1 and first["renders"] == 1


def test_android_artwork_real_service_teardown_rejects_delayed_completion(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    _, image = _client(device, ROOT / "service-recreate/start", command="publish")
    _client(device, ROOT / "service-recreate/delay", command="configure", extras=["--ei", "delay_ms", "1500"])
    device.shell("am", "start", "-f", "0x18000000", "-n", ANDROID_AUTO_PROBE_ACTIVITY,
                 "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER", "--es", "transport", "next")
    time.sleep(0.25)
    # Close the debug client's task before explicitly stopping the service so
    # destruction (not only worker.close) actually occurs in the same process.
    device.shell("input", "keyevent", "BACK")
    result = device.shell("am", "stop-service", "-n", ANDROID_PACKAGE + "/.XmmsPlaybackService", check=False)
    (ROOT / "service-recreate/stop-command.txt").write_text(result.stdout + result.stderr)
    # ActivityManager's stop-service shell command can return 255 with the
    # successful "Service stopped" message; verify component state below.
    assert "Service stopped" in result.stdout + result.stderr, result.stdout + result.stderr
    device.wait_for_service_absent("XmmsPlaybackService", timeout=10)
    pid = device.app_pid()
    time.sleep(1.6)
    assert not any("NotificationRecord" in line and ANDROID_PACKAGE in line
                   for line in device.shell("dumpsys", "notification", "--noredact").stdout.splitlines())
    # The shared backend was not owned by the destroyed Java worker. Resume via
    # the normal widget started-service path; only the current track may publish.
    device.start_widget_control()
    stats, current = _client(device, ROOT / "service-recreate/resumed")
    assert current["width"] == 320 and current["title"] == "Artwork Second"
    assert current["checksum"] != image["checksum"]
    assert stats["epoch"] >= 2 and stats["accepted"] == 1 and stats["peakActive"] <= 1
    assert device.app_pid() == pid


def test_android_artwork_playlist_reorder_rejects_delayed_old_projection(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    initial, image = _client(device, ROOT / "reorder/start", command="publish")
    reordered, after = _client(device, ROOT / "reorder/replayed", command="reorder-replay",
                              extras=["--ei", "projection_delay_ms", "500"])
    assert after["title"] == "Artwork First" and after["mediaId"] == "track:1"
    assert after["checksum"] == image["checksum"]
    assert reordered["renders"] == initial["renders"] == 1
    assert reordered["rejectedProjections"] >= 1
    restored, after = _client(device, ROOT / "reorder/restored", command="reverse")
    assert after["mediaId"] == "track:0" and after["checksum"] == image["checksum"]
    assert restored["renders"] == 1
    skipped, after = _client(device, ROOT / "reorder/rapid-skip", command="rapid-skip",
                            extras=["--ei", "count", "100"])
    assert after["title"] == "Artwork First" and after["mediaId"] == "track:0"
    assert after["checksum"] == image["checksum"] and skipped["peakActive"] <= 1


def test_android_artwork_same_index_replacement_and_clear_remove_old_art(android_device: AndroidDevice) -> None:
    device = android_device
    _prepare_audio(device)
    _, first = _client(device, ROOT / "clear-replace/start", command="publish")
    stats, replaced = _client(device, ROOT / "clear-replace/replaced", command="replace-current")
    assert replaced["mediaId"] == first["mediaId"] == "track:0"
    assert replaced["title"] == first["title"]
    assert replaced["checksum"] != first["checksum"] and replaced["width"] == 320
    assert stats["renders"] == 2
    stopped, removed = _client(device, ROOT / "clear-replace/cleared", command="clear-playlist")
    assert removed["width"] == 0 and stopped["closed"] and not stopped["hasMetadataArt"]
    dump = device.shell("dumpsys", "notification", "--noredact").stdout
    assert not any("NotificationRecord" in line and ANDROID_PACKAGE in line for line in dump.splitlines())
