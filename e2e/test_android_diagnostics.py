"""Unit coverage for failure-only swipe artifact collection."""

import subprocess
from importlib import import_module
from pathlib import Path
from typing import Any
from unittest.mock import Mock

pytest: Any = import_module("pytest")
save_swipe_diagnostics = import_module("android_diagnostics").save_swipe_diagnostics

pytestmark = pytest.mark.no_xdotool


def test_swipe_artifacts_preserve_full_logs_and_separate_attempts(
    tmp_path: Path, monkeypatch: Any,
) -> None:
    monkeypatch.setenv("XMMS_E2E_SCREENSHOT_DIR", str(tmp_path))
    device = Mock()
    full_log = "lifecycle\n" + "audio driver noise\n" * 2_000 + "gesture release\n"
    device.command.return_value = subprocess.CompletedProcess([], 0, full_log, "stderr")
    directory = save_swipe_diagnostics(device, "attempt-1", "start=(1, 2)", previous_log="init")
    assert directory is not None
    save_swipe_diagnostics(device, "attempt-2", "start=(3, 4)", directory=directory)
    assert (directory / "attempt-1-logcat.txt").read_text() == f"exit_code=0\n{full_log}\nstderr"
    assert (directory / "attempt-1-before-logcat.txt").read_text() == "init"
    assert (directory / "attempt-1-input.txt").read_text() == "start=(1, 2)"
    assert (directory / "attempt-2-input.txt").read_text() == "start=(3, 4)"
    assert (directory / "attempt-2-media-session.txt").exists()
    # A subsequent swipe in the same test must not overwrite earlier artifacts.
    assert save_swipe_diagnostics(device, "attempt-1", "another swipe") != directory


def test_swipe_diagnostics_records_command_errors_without_raising(
    tmp_path: Path, monkeypatch: Any,
) -> None:
    monkeypatch.setenv("XMMS_E2E_SCREENSHOT_DIR", str(tmp_path))
    device = Mock()
    device.command.side_effect = OSError("ADB unavailable")
    directory = save_swipe_diagnostics(device, "attempt-1", "failure")
    assert directory is not None
    assert "ADB unavailable" in (directory / "attempt-1-logcat.txt").read_text()
    assert device.command.call_count == 5


def test_failed_swipe_helper_saves_every_attempt_before_recovery(
    tmp_path: Path, monkeypatch: Any,
) -> None:
    monkeypatch.setenv("XMMS_E2E_SCREENSHOT_DIR", str(tmp_path))
    run_swipe = import_module("test_android_ui")._run_playlist_swipe_until_log
    device = Mock(spec=import_module("android").AndroidDevice)
    device.command.return_value = subprocess.CompletedProcess([], 0, "full log", "")
    device.assert_log_contains.side_effect = AssertionError("missing gesture")
    with pytest.raises(AssertionError, match="All swipe-attempt diagnostics"):
        run_swipe(
            device, points=lambda: (10, 20, 10, 100),
            duration_ms=300, expected_log="gesture",
        )
    assert device.recover_input_dispatch.call_count == 2
    directories = list(tmp_path.glob("android-swipe-diagnostics/*/swipe-*"))
    assert len(directories) == 1
    for attempt in range(1, 4):
        prefix = directories[0] / f"attempt-{attempt}"
        assert prefix.with_name(prefix.name + "-logcat.txt").exists()
        details = prefix.with_name(prefix.name + "-input.txt").read_text()
        assert "duration_ms=300" in details
        assert "start=(10, 20)" in details


def test_swipe_diagnostics_output_error_does_not_mask_original_failure(
    tmp_path: Path, monkeypatch: Any,
) -> None:
    output_file = tmp_path / "not-a-directory"
    output_file.write_text("occupied")
    monkeypatch.setenv("XMMS_E2E_SCREENSHOT_DIR", str(output_file))
    assert save_swipe_diagnostics(Mock(), "attempt-1", "failure") is None
