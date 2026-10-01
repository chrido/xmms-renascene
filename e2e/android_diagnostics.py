"""Failure-only Android swipe artifacts; collection must not mask test failures."""

import os
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Protocol


class DiagnosticDevice(Protocol):
    def command(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]: ...


def save_swipe_diagnostics(
    device: DiagnosticDevice,
    label: str,
    details: str,
    directory: Path | None = None,
    previous_log: str = "",
) -> Path | None:
    try:
        if directory is None:
            test_name = os.environ.get("PYTEST_CURRENT_TEST", "android-swipe").split(" (", 1)[0]
            output_root = Path(os.environ.get(
                "XMMS_E2E_SCREENSHOT_DIR",
                str(Path(__file__).resolve().parents[1] / "testoutput"),
            ))
            parent = output_root / "android-swipe-diagnostics" / re.sub(
                r"[^A-Za-z0-9_.-]+", "_", test_name
            )
            parent.mkdir(parents=True, exist_ok=True)
            directory = Path(tempfile.mkdtemp(prefix="swipe-", dir=parent))
        (directory / f"{label}-input.txt").write_text(details)
        (directory / f"{label}-before-logcat.txt").write_text(previous_log)
        for name, arguments in [
            ("logcat", ("logcat", "-d")),
            ("app-logcat", ("logcat", "-d", "RustStdoutStderr:V", "*:S")),
            ("window", ("shell", "dumpsys", "window")),
            ("activity", ("shell", "dumpsys", "activity", "activities")),
            ("media-session", ("shell", "dumpsys", "media_session")),
        ]:
            try:
                result = device.command(*arguments, check=False)
                contents = f"exit_code={result.returncode}\n{result.stdout}\n{result.stderr}"
            except (OSError, subprocess.SubprocessError) as error:
                contents = f"Diagnostic collection failed: {error}"
            (directory / f"{label}-{name}.txt").write_text(contents)
        return directory
    except OSError as error:
        print(f"Could not save Android swipe diagnostics: {error}", flush=True)
        return directory
