"""Build a separate-UID artwork client APK; never included in the player APK."""

from __future__ import annotations

import argparse
import os
import subprocess
import zipfile
from pathlib import Path


def build(output: Path, sdk: Path, build_tools: str = "36.0.0") -> Path:
    root = Path(__file__).resolve().parents[1]
    output.mkdir(parents=True, exist_ok=True)
    tools = sdk / "build-tools" / build_tools
    android_jar = sdk / "platforms/android-36/android.jar"
    classes = output / "classes"
    dex = output / "dex"
    classes.mkdir(exist_ok=True)
    dex.mkdir(exist_ok=True)
    manifest = output / "AndroidManifest.xml"
    manifest.write_text('''<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="org.xmms.artworkclient">
    <uses-sdk android:minSdkVersion="26" android:targetSdkVersion="36"/>
    <queries><package android:name="org.xmms.renascene"/></queries>
    <application android:label="Artwork test client" android:debuggable="true">
      <activity android:name=".ArtworkClientActivity" android:exported="true"/>
    </application></manifest>''')
    subprocess.run(["javac", "-source", "8", "-target", "8", "-classpath", str(android_jar),
                    "-d", str(classes), str(root / "android/test-client/ArtworkClientActivity.java")], check=True)
    subprocess.run([str(tools / "d8"), "--lib", str(android_jar), "--output", str(dex),
                    *map(str, classes.rglob("*.class"))], check=True)
    resources = output / "resources.apk"
    subprocess.run([str(tools / "aapt"), "package", "-f", "-M", str(manifest), "-I", str(android_jar),
                    "-F", str(resources)], check=True)
    unsigned = output / "unsigned.apk"
    with zipfile.ZipFile(resources) as source, zipfile.ZipFile(unsigned, "w") as target:
        target.writestr("AndroidManifest.xml", source.read("AndroidManifest.xml"))
        target.write(dex / "classes.dex", "classes.dex")
    aligned = output / "aligned.apk"
    subprocess.run([str(tools / "zipalign"), "-f", "4", str(unsigned), str(aligned)], check=True)
    apk = output / "artwork-client.apk"
    subprocess.run([str(tools / "apksigner"), "sign", "--ks", str(Path.home() / ".android/debug.keystore"),
                    "--ks-pass", "pass:android", "--out", str(apk), str(aligned)], check=True)
    return apk


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("target/android/artwork-test-client"))
    parser.add_argument("--sdk", type=Path, default=Path(os.environ.get("ANDROID_HOME", Path.home() / ".local/share/android-sdk")))
    args = parser.parse_args()
    print(build(args.output, args.sdk))


if __name__ == "__main__":
    main()
