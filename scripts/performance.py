"""Performance scenario orchestration and result reporting."""

from __future__ import annotations

import json
import math
import os
import platform as host_platform
import shutil
import socket
import statistics
import subprocess
import time
import wave
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SUPPORTED_PLATFORMS = ("gtk", "egui", "android")
DESKTOP_PLATFORMS = frozenset(("gtk", "egui"))
ALL_PLATFORMS = frozenset(SUPPORTED_PLATFORMS)


@dataclass(frozen=True)
class Scenario:
    name: str
    platforms: frozenset[str]
    duration_seconds: int
    playlist_entries: int = 0
    audio: bool = False
    expected_hotspots: tuple[str, ...] = ()


SCENARIOS = {
    scenario.name: scenario
    for scenario in (
        Scenario(
            "coldstart",
            ALL_PLATFORMS,
            2,
            expected_hotspots=("saved-state loading", "skin loading", "backend initialization", "first render"),
        ),
        Scenario("load100playlist", ALL_PLATFORMS, 2, 100, expected_hotspots=("playlist parsing", "first render")),
        Scenario("load1kplaylist", ALL_PLATFORMS, 3, 1_000, expected_hotspots=("playlist parsing", "view-model construction")),
        Scenario("load10kplaylist", ALL_PLATFORMS, 5, 10_000, expected_hotspots=("title formatting", "initial rendering")),
        Scenario("idle", ALL_PLATFORMS, 60, expected_hotspots=("repaint scheduling", "persistence writes")),
        Scenario(
            "playbackvisualizer",
            ALL_PLATFORMS,
            60,
            audio=True,
            expected_hotspots=("spectrum analysis", "rendering", "texture uploads"),
        ),
        Scenario(
            "playbacknovisualizer",
            ALL_PLATFORMS,
            60,
            audio=True,
            expected_hotspots=("playback processing", "persistence"),
        ),
        Scenario(
            "scroll10kplaylist",
            ALL_PLATFORMS,
            30,
            10_000,
            expected_hotspots=("visible-row work", "playlist rendering", "presentation"),
        ),
        Scenario(
            "playbackcontrols",
            ALL_PLATFORMS,
            12,
            20,
            audio=True,
            expected_hotspots=("input dispatch", "backend completion", "queued work"),
        ),
        Scenario(
            "layoutchange",
            ALL_PLATFORMS,
            15,
            1_000,
            expected_hotspots=("layout work", "texture regeneration", "stabilization"),
        ),
        Scenario(
            "backgroundplayback",
            frozenset(("android",)),
            600,
            20,
            audio=True,
            expected_hotspots=("JNI polling", "service work", "persistence"),
        ),
        Scenario(
            "widgets",
            frozenset(("android",)),
            30,
            20,
            audio=True,
            expected_hotspots=("bitmap allocation", "RemoteViews updates", "marquee"),
        ),
    )
}

METRIC_NAMES = (
    "startup_ms",
    "input_ready_ms",
    "frame_time_median_ms",
    "frame_time_p95_ms",
    "frame_time_p99_ms",
    "missed_frames",
    "cpu_user_seconds",
    "cpu_system_seconds",
    "peak_rss_bytes",
    "allocations",
    "mutex_contention",
    "bytes_written",
    "files_written",
    "ipc_calls",
    "jni_calls",
    "wakeups",
    "battery_mah",
)


def validate_scenario(scenario: str, platform: str) -> Scenario:
    if platform not in SUPPORTED_PLATFORMS:
        raise ValueError(
            f"unsupported platform '{platform}'; expected one of {', '.join(SUPPORTED_PLATFORMS)}"
        )
    try:
        spec = SCENARIOS[scenario]
    except KeyError as exc:
        raise ValueError(
            f"unsupported scenario '{scenario}'; expected one of {', '.join(sorted(SCENARIOS))}"
        ) from exc
    if platform not in spec.platforms:
        supported = ", ".join(sorted(spec.platforms))
        raise ValueError(
            f"scenario '{scenario}' is unsupported on platform '{platform}'; supported: {supported}"
        )
    return spec


def percentile(values: list[float], percentile_value: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    rank = max(0, math.ceil(percentile_value * len(ordered)) - 1)
    return ordered[rank]


def summarize_iterations(samples: list[dict[str, Any]]) -> dict[str, Any]:
    metrics: dict[str, Any] = {}
    for name in METRIC_NAMES:
        numeric = [
            float(sample[name])
            for sample in samples
            if isinstance(sample.get(name), (int, float))
        ]
        metrics[name] = statistics.median(numeric) if numeric else None
    elapsed = [
        float(sample["elapsed_ms"])
        for sample in samples
        if isinstance(sample.get("elapsed_ms"), (int, float))
    ]
    metrics["elapsed_median_ms"] = statistics.median(elapsed) if elapsed else None
    metrics["elapsed_p95_ms"] = percentile(elapsed, 0.95)
    metrics["elapsed_p99_ms"] = percentile(elapsed, 0.99)
    return metrics


def metric_deltas(current: dict[str, Any], baseline: dict[str, Any]) -> dict[str, Any]:
    deltas: dict[str, Any] = {}
    for name in sorted(set(current) | set(baseline)):
        after = current.get(name)
        before = baseline.get(name)
        if not isinstance(after, (int, float)) or not isinstance(before, (int, float)):
            deltas[name] = None
            continue
        change = after - before
        percent = None if before == 0 else (change / before) * 100.0
        deltas[name] = {
            "before": before,
            "after": after,
            "change": change,
            "percent": percent,
        }
    return deltas


def regression_status(deltas: dict[str, Any], threshold_percent: float) -> str:
    regressions = [
        name
        for name, delta in deltas.items()
        if isinstance(delta, dict)
        and isinstance(delta.get("percent"), (int, float))
        and delta["percent"] > threshold_percent
    ]
    return "regressed" if regressions else "passed"


class PerformanceRunner:
    def __init__(self, repo_dir: Path):
        self.repo_dir = repo_dir
        self.perf_root = repo_dir / "target" / "perf"

    def run(
        self,
        *,
        scenario: str,
        platform: str,
        iterations: int = 5,
        warmup: int = 1,
        duration: int = 0,
        baseline: str = "",
        expected_hotspot: str = "",
        regression_threshold: float = 10.0,
        diagnostics: bool = True,
        dry_run: bool = False,
    ) -> Path:
        spec = validate_scenario(scenario, platform)
        if iterations < 1:
            raise ValueError("iterations must be at least 1")
        if warmup < 0:
            raise ValueError("warmup must not be negative")
        run_duration = duration or spec.duration_seconds
        timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        result_dir = self.perf_root / platform / scenario / timestamp
        result_dir.mkdir(parents=True, exist_ok=False)
        inputs = self._prepare_inputs(result_dir, spec)
        metadata = self._metadata(spec, platform, iterations, warmup, run_duration, inputs)
        self._write_json(result_dir / "metadata.json", metadata)

        build = self._build(platform, result_dir, dry_run=dry_run)
        samples: list[dict[str, Any]] = []
        for index in range(warmup + iterations):
            measured = index >= warmup
            sample = self._measure_iteration(
                platform,
                spec,
                run_duration,
                inputs,
                result_dir,
                index,
                build,
                dry_run=dry_run,
            )
            if measured:
                samples.append(sample)

        profiler = self._capture_profile(
            platform,
            spec,
            run_duration,
            inputs,
            result_dir,
            build,
            diagnostics=diagnostics,
            dry_run=dry_run,
        )
        metrics = {
            "schema_version": 1,
            "scenario": scenario,
            "platform": platform,
            "headline_tracing_enabled": False,
            "iterations": samples,
            "summary": summarize_iterations(samples),
            "thresholds": {"regression_percent": regression_threshold},
            "profiler": profiler,
        }
        baseline_metrics = self._load_metrics(Path(baseline)) if baseline else None
        if baseline_metrics is not None:
            deltas = metric_deltas(metrics["summary"], baseline_metrics["summary"])
            metrics["comparison"] = {
                "baseline": str(Path(baseline).resolve()),
                "deltas": deltas,
                "status": regression_status(deltas, regression_threshold),
            }
        self._write_json(result_dir / "metrics.json", metrics)
        self._write_report(
            result_dir,
            spec,
            platform,
            metrics,
            expected_hotspot or ", ".join(spec.expected_hotspots),
        )
        return result_dir

    def compare(
        self,
        before: Path,
        after: Path,
        *,
        expected_hotspot: str = "",
        regression_threshold: float = 10.0,
        output: Path | None = None,
    ) -> Path:
        before_metrics = self._load_metrics(before)
        after_metrics = self._load_metrics(after)
        for key in ("scenario", "platform"):
            if before_metrics.get(key) != after_metrics.get(key):
                raise ValueError(
                    f"cannot compare different {key} values: "
                    f"{before_metrics.get(key)!r} and {after_metrics.get(key)!r}"
                )
        deltas = metric_deltas(after_metrics["summary"], before_metrics["summary"])
        destination = output or after / "comparison.md"
        destination.parent.mkdir(parents=True, exist_ok=True)
        rows = self._delta_rows(deltas)
        destination.write_text(
            "# Performance comparison\n\n"
            f"- Scenario: `{after_metrics['scenario']}`\n"
            f"- Platform: `{after_metrics['platform']}`\n"
            f"- Before: [{before}]({before.resolve().as_uri()})\n"
            f"- After: [{after}]({after.resolve().as_uri()})\n"
            f"- Expected hotspot: {expected_hotspot or 'not specified'}\n"
            f"- Threshold status: **{regression_status(deltas, regression_threshold)}**\n"
            "- Hotspot conclusion: inspect the side-by-side sampled flamegraphs; "
            "record whether it shrank, disappeared, or moved elsewhere before accepting the change.\n\n"
            "| Metric | Before | After | Change |\n"
            "|---|---:|---:|---:|\n"
            f"{rows}\n\n"
            "## Flamegraphs\n\n"
            f"- [Before sampled CPU flamegraph]({(before / 'flamegraph.svg').resolve().as_uri()})\n"
            f"- [After sampled CPU flamegraph]({(after / 'flamegraph.svg').resolve().as_uri()})\n"
        )
        return destination

    def _prepare_inputs(self, result_dir: Path, spec: Scenario) -> dict[str, str]:
        inputs_dir = result_dir / "inputs"
        inputs_dir.mkdir()
        result: dict[str, str] = {}
        if spec.playlist_entries:
            playlist = inputs_dir / f"playlist-{spec.playlist_entries}.m3u"
            entries = [
                f"#EXTINF:{60 + (index % 240)},Perf Artist {index:05d} - Perf Track {index:05d}\n"
                f"file:///deterministic/perf-track-{index:05d}.wav"
                for index in range(spec.playlist_entries)
            ]
            playlist.write_text("#EXTM3U\n" + "\n".join(entries) + "\n")
            result["playlist"] = str(playlist)
        if spec.audio:
            audio = inputs_dir / "perf-tone.wav"
            self._write_test_tone(audio)
            result["audio"] = str(audio)
        self._write_json(
            inputs_dir / "scenario.json",
            {
                "scenario": {
                    **asdict(spec),
                    "platforms": sorted(spec.platforms),
                },
                "inputs": result,
            },
        )
        return result

    def _write_test_tone(self, path: Path) -> None:
        sample_rate = 8_000
        seconds = 2
        with wave.open(str(path), "wb") as output:
            output.setnchannels(1)
            output.setsampwidth(2)
            output.setframerate(sample_rate)
            frames = bytearray()
            for index in range(sample_rate * seconds):
                value = int(3_000 * math.sin(2 * math.pi * 440 * index / sample_rate))
                frames.extend(value.to_bytes(2, "little", signed=True))
            output.writeframes(frames)

    def _metadata(
        self,
        spec: Scenario,
        platform: str,
        iterations: int,
        warmup: int,
        duration: int,
        inputs: dict[str, str],
    ) -> dict[str, Any]:
        commit = self._command_output(["git", "rev-parse", "HEAD"]) or "unknown"
        device = self._android_device_info() if platform == "android" else self._host_info()
        return {
            "schema_version": 1,
            "commit": commit,
            "build_profile": "profiling",
            "platform": platform,
            "scenario": spec.name,
            "scenario_parameters": {
                "iterations": iterations,
                "warmup": warmup,
                "duration_seconds": duration,
                "playlist_entries": spec.playlist_entries,
            },
            "device_or_host": device,
            "inputs": inputs,
            "created_at": datetime.now(timezone.utc).isoformat(),
        }

    def _host_info(self) -> dict[str, Any]:
        return {
            "hostname": socket.gethostname(),
            "system": host_platform.system(),
            "release": host_platform.release(),
            "machine": host_platform.machine(),
            "python": host_platform.python_version(),
            "cpu_count": os.cpu_count(),
        }

    def _android_device_info(self) -> dict[str, Any]:
        serial = os.environ.get("ANDROID_SERIAL", "")
        model = self._command_output(["adb", "shell", "getprop", "ro.product.model"])
        sdk = self._command_output(["adb", "shell", "getprop", "ro.build.version.sdk"])
        return {"serial": serial, "model": model or "unavailable", "sdk": sdk or "unavailable"}

    def _build(self, platform: str, result_dir: Path, *, dry_run: bool) -> dict[str, Any]:
        if platform == "android":
            android_target = os.environ.get(
                "XMMS_ANDROID_PERF_TARGET", "aarch64-linux-android"
            )
            command = [
                "cargo",
                "apk",
                "build",
                "--target",
                android_target,
                "--profile",
                "profiling",
                "--no-default-features",
                "--features",
                "mobile-ui,perf-tracing",
                "--lib",
            ]
            binary = self.repo_dir / "target" / "profiling" / "apk" / "xmms-renascene.apk"
        else:
            frontend = "gtk-ui" if platform == "gtk" else "egui-ui"
            command = [
                "cargo",
                "build",
                "--profile",
                "profiling",
                "--no-default-features",
                "--features",
                f"{frontend},gstreamer-backend,perf-tracing",
                "--bin",
                "xmms-rs",
            ]
            binary = self.repo_dir / "target" / "profiling" / "xmms-rs"
        log = result_dir / "build.log"
        status = self._run_logged(command, log, dry_run=dry_run)
        if status != 0:
            raise RuntimeError(f"profiling build failed; see {log}")
        if platform == "android" and not dry_run:
            if not shutil.which("adb"):
                raise RuntimeError("adb is required to install the Android profiling APK")
            install = subprocess.run(
                ["adb", "install", "-r", str(binary)],
                cwd=self.repo_dir,
                capture_output=True,
                text=True,
                check=False,
            )
            with log.open("a") as output:
                output.write("\n$ adb install -r " + str(binary) + "\n")
                output.write(install.stdout)
                output.write(install.stderr)
            if install.returncode != 0:
                raise RuntimeError(f"profiling APK installation failed; see {log}")
        artifacts = result_dir / "artifacts"
        artifacts.mkdir()
        copied = ""
        if binary.is_file():
            destination = artifacts / binary.name
            shutil.copy2(binary, destination)
            copied = str(destination)
        return {"command": command, "binary": str(binary), "artifact": copied}

    def _measure_iteration(
        self,
        platform: str,
        spec: Scenario,
        duration: int,
        inputs: dict[str, str],
        result_dir: Path,
        index: int,
        build: dict[str, Any],
        *,
        dry_run: bool,
    ) -> dict[str, Any]:
        log = result_dir / f"iteration-{index:02d}.log"
        metrics_path = result_dir / f"iteration-{index:02d}.json"
        command = self._driver_command(
            platform,
            spec,
            duration,
            inputs,
            build,
            metrics_path,
            profiler=(),
        )
        status = self._run_logged(command, log, dry_run=dry_run)
        if status != 0:
            raise RuntimeError(f"scenario iteration failed; see {log}")
        if metrics_path.is_file():
            return json.loads(metrics_path.read_text())
        sample = self._empty_sample()
        sample.update({"elapsed_ms": 0.0, "dry_run": True})
        self._write_json(metrics_path, sample)
        return sample

    def _capture_profile(
        self,
        platform: str,
        spec: Scenario,
        duration: int,
        inputs: dict[str, str],
        result_dir: Path,
        build: dict[str, Any],
        *,
        diagnostics: bool,
        dry_run: bool,
    ) -> dict[str, Any]:
        raw = result_dir / ("simpleperf.data" if platform == "android" else "perf.data")
        flamegraph = result_dir / "flamegraph.svg"
        log = result_dir / "profiler.log"
        profiler: tuple[str, ...] = ()
        capture_status = "skipped"
        if platform == "android":
            if shutil.which("adb"):
                capture_status = "available"
            self._write_json(
                raw,
                {
                    "status": capture_status,
                    "note": "Android capture uses simpleperf/Perfetto when a debuggable device is connected.",
                },
            )
        elif shutil.which("perf"):
            profiler = ("perf", "record", "-F", "199", "-g", "-o", str(raw), "--")
            capture_status = "captured"
        else:
            self._write_json(raw, {"status": "skipped", "reason": "perf is not installed"})

        metrics_path = result_dir / "profiler-iteration.json"
        command = self._driver_command(
            platform,
            spec,
            duration,
            inputs,
            build,
            metrics_path,
            profiler=profiler,
        )
        if profiler:
            status = self._run_logged(command, log, dry_run=dry_run)
            if status != 0:
                capture_status = "skipped"
                if not raw.is_file():
                    self._write_json(
                        raw,
                        {
                            "status": "skipped",
                            "reason": "perf recording failed; inspect profiler.log",
                        },
                    )
        else:
            log.write_text("Sampled CPU profiler unavailable; headline measurements remain valid.\n")
        flamegraph_status = self._generate_flamegraph(raw, flamegraph, capture_status, dry_run=dry_run)
        diagnostics_result = self._diagnostics(
            platform,
            spec,
            duration,
            inputs,
            result_dir,
            build,
            diagnostics,
            dry_run=dry_run,
        )
        return {
            "sampled_cpu": capture_status,
            "flamegraph": flamegraph_status,
            "raw_recording": raw.name,
            "diagnostics": diagnostics_result,
        }

    def _driver_command(
        self,
        platform: str,
        spec: Scenario,
        duration: int,
        inputs: dict[str, str],
        build: dict[str, Any],
        metrics_path: Path,
        *,
        profiler: tuple[str, ...],
        trace_dir: Path | None = None,
    ) -> list[str]:
        command = [
            "python3",
            "-m",
            "scripts.perf_driver",
            f"--platform={platform}",
            f"--scenario={spec.name}",
            f"--duration={duration}",
            f"--binary={build['binary']}",
            f"--metrics={metrics_path}",
        ]
        if inputs.get("playlist"):
            command.append(f"--playlist={inputs['playlist']}")
        if inputs.get("audio"):
            command.append(f"--audio={inputs['audio']}")
        for part in profiler:
            command.append(f"--profiler-arg={part}")
        if trace_dir is not None:
            command.append(f"--trace-dir={trace_dir}")
        return command

    def _generate_flamegraph(
        self,
        raw: Path,
        destination: Path,
        capture_status: str,
        *,
        dry_run: bool,
    ) -> str:
        if (
            not dry_run
            and capture_status == "captured"
            and raw.is_file()
            and shutil.which("perf")
            and shutil.which("stackcollapse-perf.pl")
            and shutil.which("flamegraph.pl")
        ):
            script = (
                f"perf script -i {raw} | stackcollapse-perf.pl | "
                f"flamegraph.pl --title 'XMMS Renascene' > {destination}"
            )
            status = subprocess.run(
                ["sh", "-c", script],
                cwd=self.repo_dir,
                check=False,
            ).returncode
            if status == 0 and destination.is_file():
                return "generated"
        message = (
            "Dry run: no samples captured."
            if dry_run
            else "Sampled flamegraph unavailable. Install/configure perf and Brendan Gregg's FlameGraph tools."
        )
        destination.write_text(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1200\" height=\"120\">"
            "<rect width=\"100%\" height=\"100%\" fill=\"#202020\"/>"
            f"<text x=\"20\" y=\"65\" fill=\"white\" font-family=\"sans-serif\">{message}</text>"
            "</svg>\n"
        )
        return "placeholder"

    def _diagnostics(
        self,
        platform: str,
        spec: Scenario,
        duration: int,
        inputs: dict[str, str],
        result_dir: Path,
        build: dict[str, Any],
        enabled: bool,
        *,
        dry_run: bool,
    ) -> dict[str, Any]:
        statuses: dict[str, Any] = {}
        if not enabled:
            return {"status": "disabled"}
        if platform == "gtk":
            statuses["sysprof"] = "available" if shutil.which("sysprof-cli") else "skipped"
        if platform == "android":
            statuses["simpleperf"] = "device-dependent"
            statuses["perfetto"] = "device-dependent"
            statuses["span_timeline"] = "device-dependent"
        else:
            trace_dir = result_dir / "span-timeline"
            trace_dir.mkdir()
            trace_metrics = result_dir / "trace-iteration.json"
            trace_command = self._driver_command(
                platform,
                spec,
                duration,
                inputs,
                build,
                trace_metrics,
                profiler=(),
                trace_dir=trace_dir,
            )
            trace_status = self._run_logged(
                trace_command,
                result_dir / "tracing.log",
                dry_run=dry_run,
            )
            statuses["span_timeline"] = "captured" if trace_status == 0 else "failed"
            permitted = os.geteuid() == 0 if hasattr(os, "geteuid") else False
            statuses["off_cpu_ebpf"] = (
                "available"
                if shutil.which("bpftrace") and permitted and not dry_run
                else "skipped-no-capability"
            )
        (result_dir / "diagnostics.json").write_text(json.dumps(statuses, indent=2) + "\n")
        return statuses

    def _write_report(
        self,
        result_dir: Path,
        spec: Scenario,
        platform: str,
        metrics: dict[str, Any],
        expected_hotspot: str,
    ) -> None:
        summary = metrics["summary"]
        comparison = metrics.get("comparison")
        comparison_text = "No baseline supplied."
        if comparison:
            comparison_text = (
                f"Baseline: `{comparison['baseline']}`  \n"
                f"Threshold status: **{comparison['status']}**"
            )
        rows = "\n".join(
            f"| {name} | {self._format_metric(value)} |"
            for name, value in summary.items()
        )
        (result_dir / "report.md").write_text(
            f"# Performance report: {spec.name} / {platform}\n\n"
            f"- Expected hotspot: {expected_hotspot or 'not specified'}\n"
            f"- Sampled CPU flamegraph: [flamegraph.svg](flamegraph.svg)\n"
            f"- Raw profiler data: [{metrics['profiler']['raw_recording']}]"
            f"({metrics['profiler']['raw_recording']})\n"
            "- Span-derived timeline: diagnostic only; headline metrics were captured with tracing disabled.\n"
            f"- Comparison: {comparison_text}\n\n"
            "| Metric | Median/value |\n"
            "|---|---:|\n"
            f"{rows}\n\n"
            "## Acceptance\n\n"
            "Confirm that the expected hotspot shrank or disappeared in the sampled flamegraph, "
            "that equivalent cost did not move elsewhere, and that scenario behavior remained correct.\n\n"
            "## Artifacts\n\n"
            "- [metadata.json](metadata.json)\n"
            "- [metrics.json](metrics.json)\n"
            "- [build.log](build.log)\n"
            "- [profiler.log](profiler.log)\n"
            "- [diagnostics.json](diagnostics.json)\n"
        )

    def _delta_rows(self, deltas: dict[str, Any]) -> str:
        rows = []
        for name, delta in deltas.items():
            if not isinstance(delta, dict):
                rows.append(f"| {name} | n/a | n/a | n/a |")
                continue
            percent = delta["percent"]
            rendered = "n/a" if percent is None else f"{percent:+.2f}%"
            rows.append(
                f"| {name} | {self._format_metric(delta['before'])} | "
                f"{self._format_metric(delta['after'])} | {rendered} |"
            )
        return "\n".join(rows)

    def _load_metrics(self, path: Path) -> dict[str, Any]:
        metrics_path = path / "metrics.json" if path.is_dir() else path
        if not metrics_path.is_file():
            raise ValueError(f"metrics file not found: {metrics_path}")
        value = json.loads(metrics_path.read_text())
        if not isinstance(value.get("summary"), dict):
            raise ValueError(f"invalid metrics file: {metrics_path}")
        return value

    def _run_logged(self, command: list[str], log: Path, *, dry_run: bool) -> int:
        log.parent.mkdir(parents=True, exist_ok=True)
        if dry_run:
            log.write_text("DRY RUN: " + " ".join(command) + "\n")
            return 0
        with log.open("w") as output:
            process = subprocess.run(
                command,
                cwd=self.repo_dir,
                stdout=output,
                stderr=subprocess.STDOUT,
                check=False,
            )
        return process.returncode

    def _command_output(self, command: list[str]) -> str:
        try:
            return subprocess.run(
                command,
                cwd=self.repo_dir,
                text=True,
                capture_output=True,
                check=False,
            ).stdout.strip()
        except OSError:
            return ""

    def _write_json(self, path: Path, value: Any) -> None:
        path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")

    def _empty_sample(self) -> dict[str, Any]:
        return {name: None for name in METRIC_NAMES}

    def _format_metric(self, value: Any) -> str:
        if value is None:
            return "n/a"
        if isinstance(value, float):
            return f"{value:.3f}"
        return str(value)
