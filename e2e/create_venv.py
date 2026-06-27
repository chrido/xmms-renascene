#!/usr/bin/env python3
"""Create/update the Python virtualenv used by GUI E2E tests."""

from __future__ import annotations

import subprocess
import sys
import venv
from pathlib import Path


E2E_DIR = Path(__file__).resolve().parent
VENV_DIR = E2E_DIR / ".venv"
REQUIREMENTS = E2E_DIR / "requirements.txt"


def venv_python() -> Path:
    if sys.platform == "win32":
        return VENV_DIR / "Scripts" / "python.exe"
    return VENV_DIR / "bin" / "python"


def main() -> int:
    if not REQUIREMENTS.is_file():
        print(f"missing requirements file: {REQUIREMENTS}", file=sys.stderr)
        return 2

    if not venv_python().exists():
        print(f"creating virtualenv at {VENV_DIR}")
        venv.EnvBuilder(with_pip=True, clear=False, upgrade_deps=False).create(VENV_DIR)

    python = venv_python()
    subprocess.run([str(python), "-m", "pip", "install", "--upgrade", "pip"], check=True)
    subprocess.run([str(python), "-m", "pip", "install", "-r", str(REQUIREMENTS)], check=True)
    print(f"E2E virtualenv ready: {VENV_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
