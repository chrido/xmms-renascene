"""Install CI Android SDK packages with bounded retries for corrupt downloads."""

import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

ATTEMPTS = 3
TIMEOUT_SECONDS = 600
# Match the SDK packages requested by the pinned android-emulator-runner action.
# Preinstall them here so its setup does not perform unprotected downloads.
EMULATOR_API_LEVEL = "35"
EMULATOR_BUILD_TOOLS = "37.0.0"


def install(sdk_root: Path, packages: list[str]) -> None:
    sdk_root = sdk_root.resolve(strict=True)
    for arguments in [["--licenses"], *[[package] for package in packages]]:
        for attempt in range(1, ATTEMPTS + 1):
            # A retry must not reuse a corrupt archive from the previous attempt.
            # Licenses and installed packages remain in the SDK root.
            with tempfile.TemporaryDirectory(prefix="xmms-sdk-") as user_home:
                env = {**os.environ, "ANDROID_USER_HOME": user_home}
                try:
                    subprocess.run(
                        ["sdkmanager", f"--sdk_root={sdk_root}", *arguments],
                        input="y\n" * 100 if arguments == ["--licenses"] else None,
                        text=True,
                        env=env,
                        check=True,
                        timeout=TIMEOUT_SECONDS,
                    )
                    break
                except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                    if attempt == ATTEMPTS:
                        raise
                    print(
                        f"SDK installation failed for {arguments}; "
                        f"retrying ({attempt + 1}/{ATTEMPTS})",
                        flush=True,
                    )
                    # sdkmanager also retains partially extracted packages here.
                    # This helper runs serially in a dedicated CI job SDK root.
                    staging = sdk_root / ".temp"
                    if staging.is_symlink():
                        raise RuntimeError(f"Refusing to clean SDK staging symlink: {staging}") from None
                    if staging.exists():
                        shutil.rmtree(staging)
                    time.sleep(10 * attempt)


def main() -> None:
    sdk_root = Path(os.environ["ANDROID_HOME"])
    install(
        sdk_root,
        [
            "platform-tools",
            f"platforms;android-{os.environ['ANDROID_API_LEVEL']}",
            f"build-tools;{os.environ['ANDROID_BUILD_TOOLS']}",
            f"ndk;{os.environ['ANDROID_NDK_VERSION']}",
            f"build-tools;{EMULATOR_BUILD_TOOLS}",
            f"platforms;android-{EMULATOR_API_LEVEL}",
            "emulator",
            f"system-images;android-{EMULATOR_API_LEVEL};google_apis;x86_64",
        ],
    )
    try:
        with open(os.environ["GITHUB_ENV"], "a") as output:
            output.write(f"ANDROID_NDK_HOME={sdk_root}/ndk/{os.environ['ANDROID_NDK_VERSION']}\n")
    except OSError as error:
        raise RuntimeError("Failed to export ANDROID_NDK_HOME to GITHUB_ENV") from error


if __name__ == "__main__":
    main()
