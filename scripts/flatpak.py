#!/usr/bin/env python3
# pyright: reportUnusedExpression=false
"""Build and install XMMS Renascene as a local Flatpak."""

import logging
import os
import sys
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

    @alias(["install"])
    async def create_flatpack(
        self,
        build_dir: str = env_default("FLATPAK_BUILD_DIR", "build-flatpak"),
        remote: str = env_default("FLATPAK_REMOTE", DEFAULT_REMOTE),
        remote_url: str = env_default("FLATPAK_REMOTE_URL", DEFAULT_REMOTE_URL),
        installation: str = env_default("FLATPAK_INSTALLATION", "--user"),
    ) -> int:
        """Build and install XMMS Renascene as a local Flatpak."""
        if installation not in INSTALLATION_CHOICES:
            logging.error("Invalid installation target '%s'. Expected one of: %s", installation, ", ".join(sorted(INSTALLATION_CHOICES)))
            return 2

        os.chdir(REPO_DIR)
        self._require_flatpak_commands()

        if remote not in self._flatpak_remotes(installation):
            logging.info("Adding Flatpak remote '%s'...", remote)
            ["flatpak", "remote-add", installation, "--if-not-exists", remote, remote_url] @ cli_follow | raise_on_error

        logging.info("Installing build runtimes...")
        ["flatpak", "install", installation, "-y", remote, *RUNTIME_REFS] @ cli_follow | raise_on_error

        logging.info("Building and installing %s...", APP_ID)
        ["flatpak-builder", "--force-clean", "--install", f"--install-deps-from={remote}", installation, "-y", build_dir, MANIFEST] @ cli_follow | raise_on_error

        logging.info("%s is installed. Run it with: flatpak run %s", APP_ID, APP_ID)
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
