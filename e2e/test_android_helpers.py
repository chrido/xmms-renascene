"""Unit coverage for Android test infrastructure helpers."""

from pathlib import Path
from subprocess import CompletedProcess
from unittest.mock import call, patch

import pytest
from PIL import Image

from android import AndroidDevice, DisplayGeometry


@pytest.mark.parametrize(
    ("rotation", "acceleration"),
    [
        (0, "0:9.81:0"),
        (1, "9.81:0:0"),
        (2, "0:-9.81:0"),
        (3, "-9.81:0:0"),
    ],
)
def test_emulator_rotation_sets_settings_and_acceleration(
    rotation: int,
    acceleration: str,
) -> None:
    device = AndroidDevice(Path("adb"), "emulator-5554")
    geometry = DisplayGeometry(
        width=2400 if rotation in {1, 3} else 1080,
        height=1080 if rotation in {1, 3} else 2400,
        left_inset=0,
        top_inset=0,
        right_inset=0,
        bottom_inset=0,
    )
    completed = CompletedProcess([], 0, "", "")

    with (
        patch.object(AndroidDevice, "command", return_value=completed) as command,
        patch.object(AndroidDevice, "display_geometry", return_value=geometry),
        patch("android.time.sleep"),
    ):
        device._set_rotation(rotation)

    assert command.call_args_list == [
        call(
            "shell",
            "settings",
            "put",
            "system",
            "accelerometer_rotation",
            "0",
            check=True,
        ),
        call(
            "shell",
            "settings",
            "put",
            "system",
            "user_rotation",
            str(rotation),
            check=True,
        ),
        call(
            "emu",
            "sensor",
            "set",
            "acceleration",
            acceleration,
            check=False,
        ),
    ]


def test_rotation_rejects_invalid_value_before_running_adb() -> None:
    device = AndroidDevice(Path("adb"), "emulator-5554")

    with patch.object(AndroidDevice, "command") as command:
        with pytest.raises(ValueError, match="between 0 and 3"):
            device._set_rotation(4)

    command.assert_not_called()


def test_media_session_position_uses_playback_position_not_buffered_position() -> None:
    device = AndroidDevice(Path("adb"), "emulator-5554")
    sessions = """
Sessions Stack - have 1 sessions:
  XMMS Renascene org.xmms.renascene/XMMS Renascene/1 (userId=0)
    package=org.xmms.renascene
    state=PlaybackState {state=PLAYING(3), position=9600, buffered position=0}
"""

    with patch.object(
        AndroidDevice,
        "shell",
        return_value=CompletedProcess([], 0, sessions, ""),
    ):
        position = device.wait_for_media_session_position_at_least(500)

    assert position == 9600


def test_player_render_assertion_rejects_black_activity(
    tmp_path: Path,
) -> None:
    screenshot = tmp_path / "black.png"
    Image.new("RGB", (100, 100), "black").save(screenshot)
    device = AndroidDevice(Path("adb"), "emulator-5554")
    geometry = DisplayGeometry(100, 100, 0, 0, 0, 0)

    with patch.object(AndroidDevice, "display_geometry", return_value=geometry):
        with pytest.raises(AssertionError, match="remained black"):
            device.assert_player_rendered(screenshot)


def test_player_render_assertion_accepts_visible_player(
    tmp_path: Path,
) -> None:
    screenshot = tmp_path / "visible.png"
    image = Image.new("RGB", (100, 100), "black")
    for x in range(25, 75):
        for y in range(25, 75):
            image.putpixel((x, y), (80, 80, 80))
    image.save(screenshot)
    device = AndroidDevice(Path("adb"), "emulator-5554")
    geometry = DisplayGeometry(100, 100, 0, 0, 0, 0)

    with patch.object(AndroidDevice, "display_geometry", return_value=geometry):
        device.assert_player_rendered(screenshot)
