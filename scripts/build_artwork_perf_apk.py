"""Build optimized native code with debuggable Java probes for measured runs.

This is NOT a production release APK. It retains explicit debuggability guards
and opt-in timing probes; compare Off/Generated using this same artifact.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
from pathlib import Path

from scripts.repo import RepoTool


def build(target: str, output: Path) -> Path:
    tool = RepoTool()
    environment = tool._android_environment()
    command = tool._android_apk_command(target, release=True)
    environment["RUSTFLAGS"] = "-C link-arg=-Wl,-z,max-page-size=16384"
    environment.setdefault("CARGO_APK_RELEASE_KEYSTORE", str(Path.home() / ".android/debug.keystore"))
    environment.setdefault("CARGO_APK_RELEASE_KEYSTORE_PASSWORD", "android")
    subprocess.run(command, env=environment, check=True)
    source = tool._android_apk_path(release=True)
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, output)

    class ProbePackage(RepoTool):
        def _android_apk_path(self, *, release: bool = False) -> Path:
            return output

    ProbePackage()._package_android_activity(environment, release=False)
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="x86_64-linux-android")
    parser.add_argument("--output", type=Path, default=Path("target/perf/artwork/optimized-probes.apk"))
    args = parser.parse_args()
    print(build(args.target, args.output))


if __name__ == "__main__":
    main()
