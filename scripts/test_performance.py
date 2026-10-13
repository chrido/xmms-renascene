import json
import tempfile
import unittest
import wave
from pathlib import Path
from typing import Any
from unittest import mock

from scripts import perf_driver
from scripts.artwork_metrics import (
    COUNTERS,
    TIMINGS,
    distribution,
    records,
    summarize,
    validate_sample,
)
from scripts.performance import (
    PerformanceRunner,
    metric_deltas,
    regression_status,
    summarize_iterations,
    validate_scenario,
)


class PerformanceTest(unittest.TestCase):
    def test_artwork_scenarios_are_android_only_and_include_steady_workload(self):
        for name in ["artworkcold", "artworkwarm", "artworkskip", "artworkskin", "artworksteady", "artworkrecreate"]:
            self.assertEqual(validate_scenario(name, "android").name, name)
            with self.assertRaisesRegex(ValueError, "unsupported on platform"):
                validate_scenario(name, "egui")
        self.assertEqual(validate_scenario("artworksteady", "android").duration_seconds, 600)

    def test_artwork_distribution_keeps_missing_values_null_and_rejects_bad_samples(self):
        self.assertEqual(distribution([]), {"count": 0, "p50": None, "p95": None, "max": None})
        self.assertEqual(distribution([1, 2, 3, 4]), {"count": 4, "p50": 2, "p95": 4, "max": 4})
        for sample in [float("nan"), float("inf"), -1]:
            with self.assertRaises(ValueError):
                distribution([sample])
        report = summarize([], ["missing samples"], emulator=True)
        self.assertFalse(report["performance_verified"])
        self.assertEqual(report["errors"], ["missing samples"])

    def test_artwork_parser_and_schema_require_correlated_valid_images(self):
        self.assertEqual(records('artwork {"renders":1}', "artwork"), [{"renders": 1}])
        with self.assertRaisesRegex(ValueError, "Malformed"):
            records('artwork {bad}', "artwork")
        worker: dict[str, Any] = dict.fromkeys(COUNTERS, 0)
        worker.update(dict.fromkeys(TIMINGS, 1))
        worker.update({"checksum": "abc", "requestTag": "1:1", "requestNs": 10, "completedNs": 20})
        client = {"checksum": "abc", "requestTag": "1:1", "width": 320, "height": 320, "opaque": True}
        for policy in ["generated", "auto", "audio-inspired"]:
            validate_sample(worker, client, policy)
        with self.assertRaisesRegex(ValueError, "Unsupported artwork policy"):
            validate_sample(worker, client, "unrecognized")
        for field, value in [("peakPending", 2), ("cacheEntries", 3), ("renders", -1), ("accepted", True)]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate_sample({**worker, field: value}, client, "generated")
        for altered in [{**client, "checksum": "wrong"}, {**client, "requestTag": "1:0"}, {**client, "opaque": "true"}]:
            with self.assertRaises(ValueError):
                validate_sample(worker, altered, "generated")
        with self.assertRaises(ValueError):
            validate_sample(worker, client, "off")

    def test_artwork_driver_uses_explicit_serial_and_reset_consent(self):
        runner = PerformanceRunner(Path("."))
        with mock.patch.dict("os.environ", {"ANDROID_SERIAL": "test-device", "XMMS_ARTWORK_ALLOW_RESET": "1", "XMMS_ARTWORK_POLICY": "off"}):
            command = runner._driver_command("android", validate_scenario("artworkcold", "android"), 1, {}, {"binary": "app.apk"}, Path("metrics.json"), profiler=())
        self.assertIn("--serial=test-device", command)
        self.assertIn("--artwork-policy=off", command)
        self.assertIn("--allow-data-reset", command)

    def test_validates_platform_specific_scenarios(self):
        self.assertEqual(validate_scenario("coldstart", "gtk").name, "coldstart")
        with self.assertRaisesRegex(ValueError, "unsupported on platform"):
            validate_scenario("widgets", "egui")
        self.assertEqual(validate_scenario("seek100", "egui").audio_seconds, 3_600)
        with self.assertRaisesRegex(ValueError, "unsupported on platform"):
            validate_scenario("seek100", "android")

    def test_summarizes_iterations_with_tail_percentiles(self):
        summary = summarize_iterations(
            [{"elapsed_ms": value, "startup_ms": value} for value in (10, 20, 30, 40, 50)]
        )
        self.assertEqual(summary["elapsed_median_ms"], 30)
        self.assertEqual(summary["elapsed_p95_ms"], 50)
        self.assertEqual(summary["elapsed_p99_ms"], 50)
        self.assertEqual(summary["startup_ms"], 30)

    def test_comparison_flags_regressions(self):
        deltas = metric_deltas({"startup_ms": 120}, {"startup_ms": 100})
        self.assertEqual(regression_status(deltas, 10), "regressed")

    def test_dry_run_writes_self_contained_result(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / "target").mkdir()
            runner = PerformanceRunner(repo)
            result = runner.run(
                scenario="load100playlist",
                platform="gtk",
                iterations=2,
                warmup=1,
                duration=1,
                dry_run=True,
            )
            self.assertTrue((result / "inputs" / "playlist-100.m3u").is_file())
            self.assertTrue((result / "flamegraph.svg").is_file())
            self.assertTrue((result / "metrics.json").is_file())
            self.assertTrue((result / "report.md").is_file())
            metrics = json.loads((result / "metrics.json").read_text())
            self.assertFalse(metrics["headline_tracing_enabled"])
            self.assertEqual(len(metrics["iterations"]), 2)

    def test_compare_requires_matching_scenario_and_platform(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before = root / "before"
            after = root / "after"
            before.mkdir()
            after.mkdir()
            (before / "metrics.json").write_text(
                json.dumps({"scenario": "idle", "platform": "gtk", "summary": {}})
            )
            (after / "metrics.json").write_text(
                json.dumps({"scenario": "idle", "platform": "egui", "summary": {}})
            )
            with self.assertRaisesRegex(ValueError, "different platform"):
                PerformanceRunner(root).compare(before, after)

    def test_android_build_uses_configured_emulator_target(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            result = repo / "result"
            result.mkdir()
            runner = PerformanceRunner(repo)
            with mock.patch.dict(
                "os.environ",
                {"XMMS_ANDROID_PERF_TARGET": "x86_64-linux-android"},
                clear=False,
            ):
                runner._build("android", result, dry_run=True)

            self.assertIn(
                "--target x86_64-linux-android",
                (result / "build.log").read_text(),
            )

    def test_android_build_can_reuse_prebuilt_apk(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            result = repo / "result"
            result.mkdir()
            apk = repo / "target" / "release" / "apk" / "xmms-renascene.apk"
            apk.parent.mkdir(parents=True)
            apk.write_bytes(b"apk")
            runner = PerformanceRunner(repo)
            with mock.patch.dict(
                "os.environ",
                {"XMMS_ANDROID_PERF_APK": str(apk.relative_to(repo))},
                clear=False,
            ):
                build = runner._build("android", result, dry_run=True)

            self.assertEqual(Path(build["binary"]), apk)
            self.assertIn("Using prebuilt", (result / "build.log").read_text())

    def test_seek_scenario_creates_sparse_one_hour_wave(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / "target").mkdir()
            result = PerformanceRunner(repo).run(
                scenario="seek100",
                platform="egui",
                iterations=1,
                warmup=0,
                duration=1,
                dry_run=True,
            )
            audio = result / "inputs" / "perf-tone.wav"
            with wave.open(str(audio), "rb") as source:
                self.assertEqual(source.getframerate(), 8_000)
                self.assertEqual(source.getnframes(), 8_000 * 3_600)

    def test_stress_scenarios_issue_expected_commands(self):
        args = mock.Mock(scenario="seek100")
        with (
            mock.patch.object(
                perf_driver, "send_initial_command_when_ready", return_value=1.0
            ) as initial,
            mock.patch.object(perf_driver, "send_command", return_value=1.0) as send,
        ):
            latencies = perf_driver.scenario_actions(args, 1234)
        self.assertEqual(len(latencies), 101)
        initial.assert_called_once_with(1234, "play")
        seek_calls = send.call_args_list
        self.assertEqual(len(seek_calls), 100)
        self.assertEqual(seek_calls[0], mock.call(1234, "seek", position_ms=0))
        self.assertEqual(
            len({call.kwargs["position_ms"] for call in seek_calls}),
            100,
        )

        args.scenario = "pauseresume100"
        with (
            mock.patch.object(
                perf_driver, "send_initial_command_when_ready", return_value=1.0
            ) as initial,
            mock.patch.object(perf_driver, "send_command", return_value=1.0) as send,
        ):
            latencies = perf_driver.scenario_actions(args, 1234)
        self.assertEqual(len(latencies), 201)
        initial.assert_called_once_with(1234, "play")
        self.assertEqual(send.call_count, 200)
        self.assertEqual(send.call_args_list[:2], [
            mock.call(1234, "pause"),
            mock.call(1234, "play"),
        ])

        args.scenario = "scroll10kplaylist"
        with (
            mock.patch.object(
                perf_driver, "send_initial_command_when_ready", return_value=1.0
            ) as initial,
            mock.patch.object(perf_driver, "send_command", return_value=1.0) as send,
        ):
            latencies = perf_driver.scenario_actions(args, 1234)
        self.assertEqual(len(latencies), 40)
        initial.assert_called_once_with(1234, "playlist_scroll", offset=0)
        self.assertEqual(send.call_count, 39)
        self.assertEqual(
            send.call_args_list[0],
            mock.call(1234, "playlist_scroll", offset=250),
        )

    def test_initial_command_retries_until_frontend_acknowledges(self):
        with (
            mock.patch.object(
                perf_driver,
                "send_command",
                side_effect=[
                    TimeoutError("timed out"),
                    RuntimeError(
                        "command 'play' was rejected: "
                        "{'accepted': False, "
                        "'error': 'frontend did not acknowledge command'}"
                    ),
                    1.0,
                ],
            ) as send,
            mock.patch.object(perf_driver.time, "sleep") as sleep,
        ):
            latency = perf_driver.send_initial_command_when_ready(
                1234,
                "playlist_scroll",
                offset=0,
            )

        self.assertGreaterEqual(latency, 0.0)
        self.assertEqual(send.call_count, 3)
        send.assert_called_with(1234, "playlist_scroll", offset=0)
        self.assertEqual(sleep.call_count, 2)


if __name__ == "__main__":
    unittest.main()
