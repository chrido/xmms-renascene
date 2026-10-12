"""Run the same real APK/controller/System UI artwork spike on an explicit device.

Does not start an AVD, assume emulator-only fixtures, or claim physical timing
from emulator results. Requires a debuggable APK for fixed-path probe export.
"""

from __future__ import annotations

import argparse
import io
import json
import re
import subprocess
import time
import wave
from pathlib import Path


def run(serial: str, apk: Path, output: Path, adb: str = "adb") -> dict:
    output.mkdir(parents=True, exist_ok=True)
    package = "org.xmms.renascene"
    activity = package + "/.XmmsAutoProbeActivity"

    def command(*args: str, check: bool = True) -> subprocess.CompletedProcess:
        return subprocess.run([adb, "-s", serial, *args], capture_output=True, check=check)

    def shell(*args: str, check: bool = True) -> str:
        return command("shell", *args, check=check).stdout.decode(errors="replace")

    deadline = time.monotonic() + 180
    while time.monotonic() < deadline:
        if shell("getprop", "sys.boot_completed", check=False).strip() == "1":
            break
        time.sleep(1)
    else:
        raise RuntimeError(f"Device {serial} did not boot")
    try:
        api = int(shell("getprop", "ro.build.version.sdk").strip())
    except ValueError as error:
        raise RuntimeError(f"Device {serial} returned an invalid Android API level") from error
    device = {"serial": serial, "api": api, "model": shell("getprop", "ro.product.model").strip(),
              "abi": shell("getprop", "ro.product.cpu.abi").strip(),
              "emulator": shell("getprop", "ro.kernel.qemu").strip() == "1"}
    command("install", "-r", str(apk))
    shell("am", "force-stop", package)
    shell("pm", "clear", package)
    shell("pm", "grant", package, "android.permission.POST_NOTIFICATIONS", check=False)
    shell("input", "keyevent", "WAKEUP", check=False)
    shell("wm", "dismiss-keyguard", check=False)
    shell("settings", "put", "global", "window_animation_scale", "0")
    shell("settings", "put", "global", "transition_animation_scale", "0")
    shell("settings", "put", "global", "animator_duration_scale", "0")

    def private_file(path: str, data: bytes) -> None:
        local = output / "upload.tmp"
        local.write_bytes(data)
        command("push", str(local), "/data/local/tmp/artwork-spike-upload")
        # Fixed paths are owned by this script, never interpolated user input.
        shell("run-as", package, "mkdir", "-p", str(Path(path).parent))
        shell("run-as", package, "cp", "/data/local/tmp/artwork-spike-upload", path)
        local.unlink()
        shell("rm", "/data/local/tmp/artwork-spike-upload")

    audio = io.BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(1)
        wav.setframerate(8000)
        wav.writeframes(bytes([128]) * 8000 * 180)
    private_file("files/imports/spike.wav", audio.getvalue())
    private_file("files/config/xmms-renascene/config", b"[xmms]\nartwork_policy=generated\nplaylist_position=0\n")
    private_file("files/config/xmms-renascene/playlist.m3u",
                 b"#EXTM3U\n#EXTINF:180,Artwork Compatibility Spike\nfile:///data/user/0/org.xmms.renascene/files/imports/spike.wav\n")
    # Some API-26 emulator builds deny clearing main log. Isolate by the freshly
    # started app PID instead of depending on log-buffer clearing privileges.
    command("logcat", "-c", check=False)
    shell("am", "start", "-n", activity, "-a", "org.xmms.renascene.e2e.ARTWORK_WORKER",
          "--es", "command", "publish", "--ez", "export", "true")
    deadline = time.monotonic() + 15
    log = ""
    stats = []
    inspected = []
    while time.monotonic() < deadline:
        pid = shell("pidof", package, check=False).strip()
        if not pid:
            time.sleep(0.1)
            continue
        log = command("logcat", "-d", "--pid=" + pid.split()[0]).stdout.decode(errors="replace")
        try:
            stats = [json.loads(m.group(1)) for m in re.finditer(r"artwork (\{[^\n]*\})", log)]
            inspected = [json.loads(m.group(1)) for m in re.finditer(r"metadata-art (\{[^\n]*\})", log)]
        except json.JSONDecodeError as error:
            (output / "logcat.txt").write_text(log)
            raise AssertionError("Invalid artwork probe JSON; see captured logcat") from error
        if stats and any(m.get("requestToClientReadyNs", 0) > 0 for m in inspected) and "artwork-export ready" in log:
            break
        if "FATAL EXCEPTION" in log or "Fatal signal" in log:
            break
        time.sleep(0.1)
    (output / "logcat.txt").write_text(log)
    (output / "media-session.txt").write_text(shell("dumpsys", "media_session"))
    notifications = shell("dumpsys", "notification", "--noredact")
    (output / "notifications.txt").write_text(notifications)
    metadata = next((m for m in reversed(inspected) if m.get("width") == 320 and m.get("requestToClientReadyNs", 0) > 0), None)
    result = {"device": device, "stats": stats[-1] if stats else None, "metadata": metadata}
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    if metadata is None or not metadata.get("opaque") or metadata.get("height") != 320:
        raise AssertionError(f"No valid metadata bitmap on {serial}; see {output}/logcat.txt")
    assert stats[-1]["hasMetadataArt"]
    assert stats[-1]["renders"] == 1
    assert stats[-1]["metadataPublications"] == 2
    assert metadata["checksum"] == stats[-1]["checksum"]
    assert "FATAL EXCEPTION" not in log and "Fatal signal" not in log
    exported = command("exec-out", "run-as", package, "cat", "files/artwork-probe.png").stdout
    (output / "source.png").write_bytes(exported)
    shell("cmd", "statusbar", "expand-notifications", check=False)
    time.sleep(0.5)
    (output / "notification.png").write_bytes(command("exec-out", "screencap", "-p").stdout)
    # Enable the emulator/device's swipe lock (no PIN/password is configured).
    shell("cmd", "statusbar", "collapse", check=False)
    shell("locksettings", "set-disabled", "false", check=False)
    shell("input", "keyevent", "SLEEP", check=False)
    time.sleep(0.3)
    shell("input", "keyevent", "WAKEUP", check=False)
    time.sleep(0.7)
    (output / "lockscreen.png").write_bytes(command("exec-out", "screencap", "-p").stdout)
    result["notification_record"] = package in notifications
    result["timing_note"] = "Emulator timings are functional instrumentation only" if device["emulator"] else "Physical-device sample; repeat controlled workloads before drawing performance conclusions"
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    shell("wm", "dismiss-keyguard", check=False)
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--apk", type=Path, default=Path("target/debug/apk/xmms-renascene.apk"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--adb", default="adb")
    args = parser.parse_args()
    print(json.dumps(run(args.serial, args.apk, args.output, args.adb), indent=2))


if __name__ == "__main__":
    main()
