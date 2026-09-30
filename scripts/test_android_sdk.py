import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from .android_sdk import ATTEMPTS, install, main


class AndroidSdkTest(unittest.TestCase):
    def test_installs_packages_separately_with_isolated_caches(self):
        with tempfile.TemporaryDirectory() as root:
            with mock.patch("scripts.android_sdk.subprocess.run") as run:
                install(Path(root), ["platform-tools", "ndk;test"])
            self.assertEqual(len(run.call_args_list), 3)
            self.assertEqual(
                [call.args[0][-1] for call in run.call_args_list],
                ["--licenses", "platform-tools", "ndk;test"],
            )
            homes = [call.kwargs["env"]["ANDROID_USER_HOME"] for call in run.call_args_list]
            self.assertEqual(len(set(homes)), 3)
            self.assertTrue(all(not Path(home).exists() for home in homes))
            self.assertTrue(all(call.kwargs["check"] for call in run.call_args_list))

    def test_corrupt_download_is_retried_and_staging_removed(self):
        with tempfile.TemporaryDirectory() as root:
            staging = Path(root) / ".temp"
            staging.mkdir()
            (staging / "corrupt.zip").write_bytes(b"invalid zip")
            installed = Path(root) / "ndk" / "existing"
            installed.mkdir(parents=True)
            with (
                mock.patch(
                    "scripts.android_sdk.subprocess.run",
                    side_effect=[None, subprocess.CalledProcessError(1, "sdkmanager"), None],
                ) as run,
                mock.patch("scripts.android_sdk.time.sleep") as sleep,
            ):
                install(Path(root), ["ndk;test"])
            self.assertFalse(staging.exists())
            self.assertTrue(installed.exists())
            self.assertEqual(run.call_args_list[1].args, run.call_args_list[2].args)
            sleep.assert_called_once_with(10)

    def test_timeout_retries_are_bounded_and_failure_propagates(self):
        with (
            tempfile.TemporaryDirectory() as root,
            mock.patch(
                "scripts.android_sdk.subprocess.run",
                side_effect=subprocess.TimeoutExpired("sdkmanager", 600),
            ) as run,
            mock.patch("scripts.android_sdk.time.sleep") as sleep,
        ):
            with self.assertRaises(subprocess.TimeoutExpired):
                install(Path(root), ["ndk;test"])
            self.assertEqual(run.call_count, ATTEMPTS)
            self.assertEqual(sleep.call_args_list, [mock.call(10), mock.call(20)])

    def test_staging_symlink_is_not_followed(self):
        with (
            tempfile.TemporaryDirectory() as root,
            tempfile.TemporaryDirectory() as target,
            mock.patch(
                "scripts.android_sdk.subprocess.run",
                side_effect=subprocess.CalledProcessError(1, "sdkmanager"),
            ),
        ):
            (Path(root) / ".temp").symlink_to(target, target_is_directory=True)
            with self.assertRaisesRegex(RuntimeError, "staging symlink"):
                install(Path(root), ["ndk;test"])
            self.assertTrue(Path(target).exists())

    def test_main_exports_ndk_home(self):
        with tempfile.TemporaryDirectory() as root:
            output = Path(root) / "github-env"
            with (
                mock.patch.dict(os.environ, {
                    "ANDROID_HOME": root,
                    "ANDROID_API_LEVEL": "36",
                    "ANDROID_BUILD_TOOLS": "36.0.0",
                    "ANDROID_NDK_VERSION": "27.2.12479018",
                    "GITHUB_ENV": str(output),
                }),
                mock.patch("scripts.android_sdk.install") as installer,
            ):
                main()
            installer.assert_called_once_with(Path(root), [
                "platform-tools", "platforms;android-36", "build-tools;36.0.0",
                "ndk;27.2.12479018",
            ])
            self.assertEqual(output.read_text(), f"ANDROID_NDK_HOME={root}/ndk/27.2.12479018\n")
