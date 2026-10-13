"""Validate saved separate-UID release-client receipts without weakening checks."""

import argparse
import json
import re
from pathlib import Path


def report(path: Path) -> dict:
    contents = path.read_text()
    match = re.search(r"result (\{[^\n]*\})", contents)
    if match is None:
        raise ValueError(f"Missing external-client result in {path}")
    try:
        value = json.loads(match.group(1))
    except json.JSONDecodeError as error:
        raise ValueError(f"Invalid external-client JSON in {path}") from error
    return value


def verify(before: dict, after: dict, player_uid: int) -> None:
    if before.get("clientUid") == player_uid or after.get("clientUid") == player_uid:
        raise ValueError("Probe does not have a separate UID")
    for value in [before, after]:
        if value.get("state") != 3 or value.get("debugEvents") != 0:
            raise ValueError("Playback not active or debug actions produced events")
        if value.get("width") != 320 or value.get("height") != 320:
            raise ValueError("Release client did not receive artwork")
    for field in ["title", "mediaId", "checksum", "queueSize"]:
        if before.get(field) is None or before[field] != after.get(field):
            raise ValueError(f"Debug actions changed release state: {field}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--player-uid", required=True, type=int)
    args = parser.parse_args()
    verify(report(args.before), report(args.after), args.player_uid)
    print("Separate-UID release artwork read and debug-action denial verified")


if __name__ == "__main__":
    main()
