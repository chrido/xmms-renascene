#!/usr/bin/env python3
# pyright: reportUnusedExpression=false
"""Build and package XMMS Renascene as a Flatpak."""

import hashlib
import json
import logging
import os
import subprocess
import sys
import tomllib
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent
if __package__ is None:
    sys.path.insert(0, str(REPO_DIR))
    __package__ = "scripts"

from .commandline import cli, cli_follow, configure_logging, raise_on_error, required_command, stdout  # type: ignore[import-not-found]
from .fire_lite import FireLite, alias  # type: ignore[import-not-found]

APP_ID = "org.xmms.Renascene"
MANIFEST = "org.xmms.Renascene.yml"
DEFAULT_REMOTE = "flathub"
DEFAULT_REMOTE_URL = "https://flathub.org/repo/flathub.flatpakrepo"
RUNTIME_REFS = (
    "org.gnome.Platform//49",
    "org.gnome.Sdk//49",
    # GNOME 49 uses the freedesktop 25.08 SDK extension branch for Rust.
    "org.freedesktop.Sdk.Extension.rust-stable//25.08",
)
INSTALLATION_CHOICES = {"--user", "--system"}


def env_default(name: str, fallback: str) -> str:
    return os.environ.get(name, fallback)


def git_commit_sha() -> str:
    github_sha = os.environ.get("GITHUB_SHA")
    if github_sha:
        return github_sha
    try:
        result = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO_DIR, check=True, text=True, stdout=subprocess.PIPE)
        return result.stdout.strip()
    except Exception:
        return "unknown"


def default_bundle_name() -> str:
    return f"xmms-renascene_{git_commit_sha()}.flatpack"


class FlatpakInstaller:
    def _flatpak_remotes(self, installation: str) -> set[str]:
        remotes = ["flatpak", "remotes", installation, "--columns=name"] @ cli | stdout
        return {line.strip() for line in remotes.splitlines() if line.strip()}

    def _require_flatpak_commands(self) -> None:
        try:
            required_command(("flatpak", "flatpak-builder"))
        except RuntimeError as error:
            logging.error(error)
            logging.error(
                "\n".join(
                    (
                        "Install missing commands with your distribution package manager, for example:",
                        "  Fedora:        sudo dnf install flatpak flatpak-builder",
                        "  Debian/Ubuntu: sudo apt install flatpak flatpak-builder",
                        "  Arch:          sudo pacman -S flatpak flatpak-builder",
                    )
                )
            )
            sys.exit(127)

    def _validate_installation(self, installation: str) -> bool:
        if installation in INSTALLATION_CHOICES:
            return True
        logging.error("Invalid installation target '%s'. Expected one of: %s", installation, ", ".join(sorted(INSTALLATION_CHOICES)))
        return False

    def _add_remote_if_needed(self, remote: str, remote_url: str, installation: str) -> None:
        if remote not in self._flatpak_remotes(installation):
            logging.info("Adding Flatpak remote '%s'...", remote)
            ["flatpak", "remote-add", installation, "--if-not-exists", remote, remote_url] @ cli_follow | raise_on_error

    def _install_build_runtimes(self, remote: str, installation: str) -> None:
        logging.info("Installing build runtimes...")
        ["flatpak", "install", installation, "-y", remote, *RUNTIME_REFS] @ cli_follow | raise_on_error

    def _install_github_build_tools(self) -> None:
        logging.info("Installing Flatpak build tools with apt...")
        required_command(("sudo", "apt-get"))
        ["sudo", "apt-get", "update"] @ cli_follow | raise_on_error
        ["sudo", "apt-get", "install", "--no-install-recommends", "--yes", "appstream", "ca-certificates", "desktop-file-utils", "elfutils", "flatpak", "flatpak-builder", "librsvg2-bin", "librsvg2-common"] @ cli_follow | raise_on_error

    def _generate_cargo_sources(self) -> None:
        logging.info("Generating vendored Cargo source manifest...")
        cargo_lock = tomllib.loads(Path("Cargo.lock").read_text())
        sources = []
        seen = set()
        for package in cargo_lock["package"]:
            if package.get("source") != "registry+https://github.com/rust-lang/crates.io-index":
                continue
            name = package["name"]
            version = package["version"]
            checksum = package["checksum"]
            crate_dir = f"{name}-{version}"
            if crate_dir in seen:
                continue
            seen.add(crate_dir)
            sources.append({"type": "archive", "archive-type": "tar-gzip", "url": f"https://static.crates.io/crates/{name}/{crate_dir}.crate", "sha256": checksum, "dest": f"cargo/vendor/{crate_dir}"})
            sources.append({"type": "inline", "dest": f"cargo/vendor/{crate_dir}", "dest-filename": ".cargo-checksum.json", "contents": json.dumps({"package": checksum, "files": {}})})
        sources.append({"type": "inline", "dest": ".cargo", "dest-filename": "config.toml", "contents": '[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "cargo/vendor"\n'})
        Path("cargo-sources.json").write_text(json.dumps(sources, indent=2) + "\n")
        logging.info("Generated cargo-sources.json with %d crates", len(seen))

    def _validate_cargo_sources(self) -> None:
        logging.info("Validating vendored Cargo sources...")
        [sys.executable, "-m", "json.tool", "cargo-sources.json"] @ cli | raise_on_error

    def _build_and_install(self, build_dir: str, remote: str, installation: str) -> None:
        logging.info("Building and installing %s...", APP_ID)
        ["flatpak-builder", "--force-clean", "--install", f"--install-deps-from={remote}", installation, "-y", build_dir, MANIFEST] @ cli_follow | raise_on_error

    def _build_repo(self, flatpak_repo: str, build_dir: str, remote: str, installation: str) -> None:
        logging.info("Building Flatpak repository %s...", flatpak_repo)
        ["flatpak-builder", "--force-clean", f"--repo={flatpak_repo}", f"--install-deps-from={remote}", installation, build_dir, MANIFEST] @ cli_follow | raise_on_error

    def _build_bundle(self, flatpak_repo: str, bundle: str) -> None:
        logging.info("Building Flatpak bundle %s...", bundle)
        ["flatpak", "build-bundle", flatpak_repo, bundle, APP_ID, "master"] @ cli_follow | raise_on_error

    def _write_checksum(self, bundle: str) -> None:
        bundle_path = Path(bundle)
        checksum = hashlib.sha256(bundle_path.read_bytes()).hexdigest()
        checksum_path = bundle_path.with_name(bundle_path.name + ".sha256")
        checksum_path.write_text(f"{checksum}  {bundle_path.name}\n")
        logging.info("Wrote %s", checksum_path)

    @alias(["install"])
    async def create_flatpack(
        self,
        build_dir: str = env_default("FLATPAK_BUILD_DIR", "build-flatpak"),
        remote: str = env_default("FLATPAK_REMOTE", DEFAULT_REMOTE),
        remote_url: str = env_default("FLATPAK_REMOTE_URL", DEFAULT_REMOTE_URL),
        installation: str = env_default("FLATPAK_INSTALLATION", "--user"),
    ) -> int:
        """Build and install XMMS Renascene as a local Flatpak."""
        if not self._validate_installation(installation):
            return 2

        os.chdir(REPO_DIR)
        self._require_flatpak_commands()
        self._add_remote_if_needed(remote, remote_url, installation)
        self._install_build_runtimes(remote, installation)
        self._generate_cargo_sources()
        self._validate_cargo_sources()
        self._build_and_install(build_dir, remote, installation)

        logging.info("%s is installed. Run it with: flatpak run %s", APP_ID, APP_ID)
        return 0

    @alias(["release"])
    async def build_release_bundle(
        self,
        bundle: str = env_default("FLATPAK_BUNDLE", default_bundle_name()),
        flatpak_repo: str = env_default("FLATPAK_REPO", "flatpak-repo"),
        build_dir: str = env_default("FLATPAK_BUILD_DIR", "build-flatpak"),
        remote: str = env_default("FLATPAK_REMOTE", DEFAULT_REMOTE),
        remote_url: str = env_default("FLATPAK_REMOTE_URL", DEFAULT_REMOTE_URL),
        installation: str = env_default("FLATPAK_INSTALLATION", "--user"),
        install_build_tools: bool = False,
    ) -> int:
        """Build a downloadable single-file Flatpak release bundle."""
        if not self._validate_installation(installation):
            return 2

        os.chdir(REPO_DIR)
        if install_build_tools:
            self._install_github_build_tools()
        self._require_flatpak_commands()
        self._add_remote_if_needed(remote, remote_url, installation)
        self._install_build_runtimes(remote, installation)
        self._generate_cargo_sources()
        self._validate_cargo_sources()
        self._build_repo(flatpak_repo, build_dir, remote, installation)
        self._build_bundle(flatpak_repo, bundle)
        self._write_checksum(bundle)

        logging.info("Release bundle is ready: %s", bundle)
        return 0


def dispatch_args(argv: list[str]) -> int:
    if not argv or argv[0].startswith("--") and argv[0] not in {"--help", "-h"}:
        argv = ["create-flatpack", *argv]
    return FireLite([([], FlatpakInstaller())]).dispatchArgs(argv)


def main() -> int:
    configure_logging()
    return dispatch_args(sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())
