"""Strict artwork timing/counter schema; never substitutes zero for missing data."""

from __future__ import annotations

import json
import math
import re
from pathlib import Path
from typing import Any

TIMINGS = ("queueNs", "resolveNs", "nativeRenderNs", "arrayTransferNs", "renderCallNs",
           "bitmapNs", "workerNs", "metadataCallNs", "notificationCallNs",
           "inspectionNs", "callbackToInspectNs", "requestToClientReadyNs")
COUNTERS = ("attempts", "renders", "accepted", "cancelled", "stale", "cacheHits",
            "metadataPublications", "notificationPublications", "cacheEntries", "peakPending", "peakActive",
            "ownedBitmapBytes", "postedBitmapBytes", "coalesced", "workerWakeups")


def records(log: str, label: str) -> list[dict[str, Any]]:
    result = []
    for match in re.finditer(re.escape(label) + r" (\{[^\n]*\})", log):
        try:
            item = json.loads(match.group(1))
        except json.JSONDecodeError as error:
            raise ValueError(f"Malformed {label} JSON") from error
        if not isinstance(item, dict):
            raise ValueError(f"Invalid {label} record")
        result.append(item)
    return result


def distribution(values: list[float]) -> dict[str, float | int | None]:
    if not values:
        return {"count": 0, "p50": None, "p95": None, "max": None}
    if any(not math.isfinite(v) or v < 0 for v in values):
        raise ValueError("Invalid timing sample")
    ordered = sorted(values)
    def nearest(fraction: float) -> float:
        return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]
    return {"count": len(values), "p50": nearest(0.5), "p95": nearest(0.95), "max": ordered[-1]}


def validate_sample(worker: dict[str, Any], client: dict[str, Any], policy: str) -> None:
    if policy not in {"off", "generated", "auto", "audio-inspired"}:
        raise ValueError("Unsupported artwork policy")
    for key in COUNTERS:
        value = worker.get(key)
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            raise ValueError(f"Missing/invalid counter {key}")
    if worker["peakPending"] > 1 or worker["peakActive"] > 1 or worker["cacheEntries"] > 2:
        raise ValueError("Artwork queue/cache bound exceeded")
    if policy != "off":
        if client.get("width") != 320 or client.get("height") != 320 or not isinstance(client.get("opaque"), bool) or not client["opaque"]:
            raise ValueError("Missing/invalid client artwork")
        if not client.get("checksum") or client["checksum"] != worker.get("checksum"):
            raise ValueError("Client/worker image mismatch")
        if client.get("requestTag") != worker.get("requestTag"):
            raise ValueError("Uncorrelated request timing")
        for key in TIMINGS:
            value = client.get(key, worker.get(key))
            if not isinstance(value, int) or isinstance(value, bool) or value < 0:
                raise ValueError(f"Missing/invalid timing {key}")
        if worker.get("requestNs", 0) <= 0 or worker.get("completedNs", 0) < worker["requestNs"]:
            raise ValueError("Invalid request/completion clock")
    elif client.get("width") != 0 or worker["renders"] != 0:
        raise ValueError("Off mode generated an image")


def summarize(samples: list[dict[str, Any]], errors: list[str], *, emulator: bool) -> dict[str, Any]:
    values = {}
    for key in TIMINGS:
        timings = [float(s[key]) / 1_000_000 for s in samples if isinstance(s.get(key), (int, float))]
        values[key.removesuffix("Ns") + "Ms"] = distribution(timings)
    return {"schema_version": 1, "samples": len(samples), "errors": errors,
            "timings": values, "emulator": emulator,
            "performance_verified": False if emulator else None,
            "note": "Emulator values validate instrumentation only" if emulator else "Physical samples require controlled workloads and uncertainty audit"}


def write_outputs(output: Path, samples: list[dict[str, Any]], summary: dict[str, Any]) -> None:
    output.mkdir(parents=True, exist_ok=True)
    (output / "samples.json").write_text(json.dumps(samples, indent=2) + "\n")
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    import csv
    with (output / "samples.csv").open("w", newline="") as stream:
        fields = sorted({key for sample in samples for key in sample})
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows(samples)
