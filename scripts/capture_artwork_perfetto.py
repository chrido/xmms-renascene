"""Bounded explicit-device Perfetto capture; does not claim physical evidence."""

import argparse
import subprocess
from pathlib import Path


def capture(serial: str, output: Path, duration: int = 15, adb: str = "adb") -> Path:
    if not serial or not 1 <= duration <= 1800:
        raise ValueError("An explicit serial and 1..1800 second duration are required")
    output.mkdir(parents=True, exist_ok=True)
    remote = "/data/misc/perfetto-traces/xmms-artwork.perfetto-trace"
    command = [adb, "-s", serial, "shell", "perfetto", "-o", remote,
               "-t", f"{duration}s", "-b", "16mb", "sched", "freq", "idle", "am", "wm", "gfx", "view", "binder_driver"]
    receipt = subprocess.run(command, capture_output=True, text=True, timeout=duration + 45, check=False)
    (output / "capture.txt").write_text(receipt.stdout + receipt.stderr)
    if receipt.returncode != 0:
        raise RuntimeError(f"Perfetto failed; see {output}/capture.txt")
    path = output / "artwork.perfetto-trace"
    subprocess.run([adb, "-s", serial, "pull", remote, str(path)], check=True, timeout=30)
    if not path.is_file() or path.stat().st_size == 0:
        raise RuntimeError("Perfetto returned no trace data")
    return path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--duration", type=int, default=15)
    args = parser.parse_args()
    print(capture(args.serial, args.output, args.duration))


if __name__ == "__main__":
    main()
