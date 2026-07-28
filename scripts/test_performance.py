import json
import tempfile
import unittest
from pathlib import Path

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


if __name__ == "__main__":
    unittest.main()
