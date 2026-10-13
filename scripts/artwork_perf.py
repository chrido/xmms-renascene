"""Actual Android artwork scenarios with explicit reset consent and device labels.

Emulator runs are functional/instrumentation evidence, never physical performance
claims. Artifact is installed once and reused for paired Off/Generated runs.
"""

from __future__ import annotations

import hashlib
import importlib
import io
import json
import os
import subprocess
import sys
import time
import wave
from pathlib import Path
from typing import Any

from scripts.artwork_metrics import (
    COUNTERS,
    TIMINGS,
    summarize,
    validate_sample,
    write_outputs,
)

SCENARIOS = {"artworkcold", "artworkwarm", "artworkskip", "artworkskin", "artworksteady", "artworkrecreate"}


def run(args: Any) -> dict[str, Any]:
    if not getattr(args, "allow_data_reset", False):
        raise ValueError("Artwork scenarios clear player test data; pass --allow-data-reset for an approved disposable device")
    serial = getattr(args, "serial", "") or os.environ.get("ANDROID_SERIAL", "")
    if not serial:
        raise ValueError("Artwork scenarios require an explicit --serial or ANDROID_SERIAL")
    policy = getattr(args, "artwork_policy", "generated")
    if policy not in {"off", "generated", "auto", "audio-inspired"}:
        raise ValueError("Unsupported artwork measurement policy")
    root = Path(__file__).resolve().parents[1]
    if str(root / "e2e") not in sys.path:
        sys.path.insert(0, str(root / "e2e"))
    android = importlib.import_module("android")
    helpers = importlib.import_module("test_android_artwork")
    device = android.AndroidDevice(Path(os.environ.get("ANDROID_HOME", str(Path.home() / ".local/share/android-sdk"))) / "platform-tools/adb", serial)
    destination = Path(args.metrics).parent / (Path(args.metrics).stem + "-artwork")
    destination.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    apk = Path(args.binary)
    device.command("install", "-r", str(apk))
    helpers._prepare_audio(device)
    cover_fixture = getattr(args, "artwork_cover_fixture", False)
    if cover_fixture:
        if policy != "auto":
            raise ValueError("Cover fixture requires Auto policy")
        audio = subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=180",
                                "-ac", "1", "-ar", "22050", "-codec:a", "libmp3lame", "-b:a", "32k",
                                "-write_xing", "0", "-id3v2_version", "0", "-f", "mp3", "pipe:1"], check=True, capture_output=True).stdout
        # Small opaque BMP picture, constructed without a test-only Pillow dependency.
        import struct
        picture = b"BM" + struct.pack("<IHHI", 58, 0, 0, 54) + struct.pack("<IiiHHIIiiII", 40, 1, 1, 1, 24, 0, 4, 0, 0, 0, 0) + bytes([10, 40, 200, 0])
        payload = b"\0image/bmp\0\x03\0" + picture
        def syncsafe(number: int) -> bytes:
            return bytes([(number >> 21) & 127, (number >> 14) & 127, (number >> 7) & 127, number & 127])
        frame = b"APIC" + syncsafe(len(payload)) + b"\0\0" + payload
        device.write_private_bytes("files/imports/first.wav", b"ID3\x04\0\0" + syncsafe(len(frame)) + frame + audio)
    count = getattr(args, "artwork_samples", 0) or (100 if args.scenario == "artworkwarm" else 30)
    if not 1 <= count <= 1000:
        raise ValueError("Artwork sample count must be between 1 and 1000")
    if args.scenario == "artworksteady":
        audio = io.BytesIO()
        with wave.open(audio, "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(1)
            wav.setframerate(8000)
            wav.writeframes(bytes([128]) * 8000 * (max(1, args.duration) + 60))
        device.write_private_bytes("files/imports/first.wav", audio.getvalue())
    # Independent aliases of identical WAV bytes make distinct deterministic keys
    # without adding decoder/content variation to the cold renderer workload.
    for index in range(max(30, count if args.scenario == "artworkcold" else 30)):
        device.shell("run-as", android.ANDROID_PACKAGE, "cp", "files/imports/first.wav", f"files/imports/perf-{index}.wav")
    entries = "#EXTM3U\n" + "".join(f"#EXTINF:180,Perf {index}\nfile:///data/user/0/{android.ANDROID_PACKAGE}/files/imports/perf-{index}.wav\n" for index in range(max(30, count)))
    device.write_private_file("files/config/xmms-renascene/playlist.m3u", entries)
    device.write_private_file("files/config/xmms-renascene/config", f"[xmms]\nartwork_policy={policy}\nplaylist_position=0\n")
    emulator = device.shell("getprop", "ro.kernel.qemu").stdout.strip() == "1"
    metadata = {
        "serial": serial, "emulator": emulator, "policy": policy, "scenario": args.scenario,
        "model": device.shell("getprop", "ro.product.model").stdout.strip(),
        "sdk": device.shell("getprop", "ro.build.version.sdk").stdout.strip(),
        "abi": device.shell("getprop", "ro.product.cpu.abi").stdout.strip(),
        "apk_sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
        "build_profile": os.environ.get("XMMS_ARTWORK_APK_PROFILE", "unspecified-not-performance-verified"),
        "samples_requested": count, "cover_fixture": cover_fixture,
        "trace_enabled": getattr(args, "artwork_trace", False),
    }
    (destination / "device.json").write_text(json.dumps(metadata, indent=2) + "\n")
    for label, command in [("thermal-before", ["dumpsys", "thermalservice"]), ("memory-before", ["dumpsys", "meminfo", android.ANDROID_PACKAGE]),
                           ("cpu-before", ["dumpsys", "cpuinfo"]), ("battery-before", ["dumpsys", "batterystats", android.ANDROID_PACKAGE])]:
        receipt = device.shell(*command, check=False)
        (destination / (label + ".txt")).write_text(receipt.stdout + receipt.stderr)
    samples: list[dict[str, Any]] = []
    errors: list[str] = []
    previous_request: str | None = None

    def measure(label: str, *, command: str = "stats", transport: str | None = None, extras: list[str] | None = None) -> dict[str, Any]:
        nonlocal previous_request
        worker, client = helpers._client(device, destination / label, command=command, transport=transport, extras=extras, timed=policy != "off", timeout_seconds=20 if command == "timed-skip" else 10)
        validate_sample(worker, client, policy)
        sample = {key: client.get(key, worker.get(key)) for key in TIMINGS + COUNTERS}
        # An on-demand stats read ten minutes later is not new image latency.
        # Off mode also has no image-delivery latency; never summarize stale
        # request timestamps as new end-to-end performance samples.
        if previous_request == worker.get("requestTag") or policy == "off":
            for key in TIMINGS:
                sample[key] = None
        previous_request = worker.get("requestTag")
        sample.update({"label": label, "policy": policy, "requestTag": worker.get("requestTag"),
                       "requestNs": worker.get("requestNs"), "completedNs": worker.get("completedNs")})
        samples.append(sample)
        return worker

    try:
        initial = measure("initial", command="publish")
        if getattr(args, "artwork_trace", False):
            helpers._client(device, destination / "enable-trace", command="configure", extras=["--ez", "trace", "true"])
        if args.scenario == "artworkcold":
            for index in range(1, count):
                measure(f"cold-{index:03}", transport="play-id", extras=["--es", "media_id", f"track:{index}", "--es", "expected_media_id", f"track:{index}"])
        elif args.scenario == "artworkwarm":
            measure("prime-1", transport="play-id", extras=["--es", "media_id", "track:1", "--es", "expected_media_id", "track:1"])
            samples.clear()
            for index in range(count):
                slot = index % 2
                worker = measure(f"warm-{index:03}", transport="play-id", extras=["--es", "media_id", f"track:{slot}", "--es", "expected_media_id", f"track:{slot}"])
                if worker["renders"] != (0 if policy == "off" else 2):
                    raise ValueError("Warm run rendered a retained key")
        elif args.scenario == "artworkrecreate":
            for index in range(count):
                measure(f"recreate-{index:03}", command="replace")
        elif args.scenario == "artworkskin":
            for index in range(count):
                measure(f"skin-{index:03}", command="perf-palette", extras=["--ei", "palette_serial", str(index + 1)])
        elif args.scenario == "artworkskip":
            measure("skip-100", command="timed-skip", extras=["--ei", "count", "100", "--ei", "interval_ms", "100"])
        elif args.scenario == "artworksteady":
            device.shell("input", "keyevent", "HOME")
            time.sleep(max(1, args.duration))
            final = measure("steady-final")
            for key in ["renders", "attempts", "metadataPublications", "accepted", "workerWakeups"]:
                if final[key] != initial[key]:
                    raise ValueError(f"Steady unchanged key performed extra {key}")
    except (AssertionError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        errors.append(str(error))
        raise
    finally:
        for label, command in [("memory-after", ["dumpsys", "meminfo", android.ANDROID_PACKAGE]), ("cpu-after", ["dumpsys", "cpuinfo"]),
                               ("thermal-after", ["dumpsys", "thermalservice"]), ("battery-after", ["dumpsys", "batterystats", android.ANDROID_PACKAGE]),
                               ("gfx-after", ["dumpsys", "gfxinfo", android.ANDROID_PACKAGE])]:
            receipt = device.shell(*command, check=False)
            (destination / (label + ".txt")).write_text(receipt.stdout + receipt.stderr)
        summary = summarize(samples, errors, emulator=emulator)
        summary["device"] = metadata
        write_outputs(destination, samples, summary)
        device.force_stop()
    return {"elapsed_ms": (time.monotonic() - started) * 1000, "startup_ms": None, "input_ready_ms": None,
            "artwork": summary, "artwork_artifacts": str(destination), "physical_performance_verified": False}
