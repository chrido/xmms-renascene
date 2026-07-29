import json
import tempfile
import unittest
import wave
from pathlib import Path
from unittest import mock

from scripts import perf_driver
from scripts.performance import (
    PerformanceRunner,
    metric_deltas,
    regression_status,
    summarize_iterations,
    validate_scenario,
)


class PerformanceTest(unittest.TestCase):
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
        with mock.patch.object(perf_driver, "send_command", return_value=1.0) as send:
            latencies = perf_driver.scenario_actions(args, 1234)
        self.assertEqual(len(latencies), 101)
        self.assertEqual(send.call_args_list[0], mock.call(1234, "play"))
        seek_calls = send.call_args_list[1:]
        self.assertEqual(len(seek_calls), 100)
        self.assertEqual(seek_calls[0], mock.call(1234, "seek", position_ms=0))
        self.assertEqual(
            len({call.kwargs["position_ms"] for call in seek_calls}),
            100,
        )

        args.scenario = "pauseresume100"
        with mock.patch.object(perf_driver, "send_command", return_value=1.0) as send:
            latencies = perf_driver.scenario_actions(args, 1234)
        self.assertEqual(len(latencies), 201)
        self.assertEqual(send.call_count, 201)
        self.assertEqual(send.call_args_list[1:3], [
            mock.call(1234, "pause"),
            mock.call(1234, "play"),
        ])


if __name__ == "__main__":
    unittest.main()
