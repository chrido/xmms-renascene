"""Deterministic regressions for frontend readiness and shutdown races."""

import argparse
import io
import json
import signal
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import perf_driver
from scripts.performance import PerformanceRunner


class Clock:
    def __init__(self):
        self.now = 0.0

    def advance(self, seconds):
        self.now += seconds


class Process:
    pid = 12345

    def __init__(self, clock):
        self.clock = clock
        self.returncode: int | None = None
        self.exit_at: float | None = None
        self.exit_code = 0

    def poll(self):
        if self.exit_at is not None and self.clock.now >= self.exit_at:
            self.returncode = self.exit_code
        return self.returncode

    def wait(self, timeout):
        if self.poll() is None:
            self.clock.advance(timeout)
        code = self.poll()
        if code is None:
            raise subprocess.TimeoutExpired("test application", timeout)
        return code


def rejected(command="ping", reason=perf_driver.ACK_TIMEOUT):
    return perf_driver.CommandRejected(command, {"accepted": False, "error": reason})


class DriverLifecycleTest(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        self.process = Process(self.clock)
        self.enterContext(
            mock.patch.object(perf_driver.time, "monotonic", lambda: self.clock.now)
        )
        self.enterContext(
            mock.patch.object(perf_driver.time, "perf_counter", lambda: self.clock.now)
        )
        self.enterContext(
            mock.patch.object(perf_driver.time, "sleep", self.clock.advance)
        )

    def exit_soon(self, *_args, **_kwargs):
        if self.process.exit_at is None:
            self.process.exit_at = self.clock.now + 0.2
        return 1.0

    def test_readiness_retries_missing_frontend_ack_and_transport_failures(self):
        with mock.patch.object(
            perf_driver, "send_command", side_effect=[rejected(), TimeoutError(), 1.0]
        ) as send:
            perf_driver.wait_for_frontend(1234, self.process)
        self.assertEqual(
            send.call_args_list, [mock.call(1234, "ping", timeout=5.0)] * 3
        )

    def test_readiness_deadline_limits_each_attempt(self):
        with (
            mock.patch.object(
                perf_driver, "send_command", side_effect=TimeoutError()
            ) as send,
            self.assertRaisesRegex(TimeoutError, "readiness before deadline"),
        ):
            perf_driver.wait_for_frontend(1234, self.process, timeout=0.25)
        self.assertAlmostEqual(self.clock.now, 0.25)
        budgets = [call.kwargs["timeout"] for call in send.call_args_list]
        self.assertEqual(len(budgets), 3)
        self.assertAlmostEqual(budgets[-1], 0.05)

    def test_readiness_detects_process_exit_including_premature_success(self):
        for code in [0, 1, -9]:
            with self.subTest(code=code):
                self.process.returncode = code
                with mock.patch.object(perf_driver, "send_command") as send:
                    with self.assertRaisesRegex(RuntimeError, f"status {code}"):
                        perf_driver.wait_for_frontend(1234, self.process)
                    send.assert_not_called()

    def test_socket_wait_detects_early_crash(self):
        self.process.returncode = 1
        with self.assertRaisesRegex(RuntimeError, "before its control socket opened"):
            perf_driver.wait_for_socket(1234, process=self.process)

    def test_readiness_does_not_retry_protocol_rejections_or_malformed_json(self):
        for error in [
            rejected(reason="unknown command 'ping'"),
            ValueError("invalid JSON"),
        ]:
            with (
                self.subTest(error=error),
                mock.patch.object(
                    perf_driver, "send_command", side_effect=error
                ) as send,
            ):
                with self.assertRaises(type(error)):
                    perf_driver.wait_for_frontend(1234, self.process)
                send.assert_called_once()

    def test_shutdown_retries_unacknowledged_quit_then_waits_for_clean_exit(self):
        attempts = 0

        def send(*args, **kwargs):
            nonlocal attempts
            attempts += 1
            if attempts == 1:
                raise rejected("quit")
            if attempts == 2:
                raise TimeoutError()
            return self.exit_soon()

        with mock.patch.object(
            perf_driver, "send_command", side_effect=send
        ) as command:
            perf_driver.shutdown_desktop(1234, self.process)
        self.assertEqual(
            command.call_args_list, [mock.call(1234, "quit", timeout=5.0)] * 3
        )
        self.assertEqual(self.process.poll(), 0)

    def test_shutdown_accepts_clean_exit_even_if_ack_is_lost(self):
        for error in [
            ConnectionError("EOF"),
            ConnectionResetError(),
            rejected("quit"),
            rejected("quit", perf_driver.FRONTEND_CLOSED),
        ]:
            with self.subTest(error=error):
                self.process = Process(self.clock)

                def send(*args, error=error, **kwargs):
                    self.exit_soon()
                    raise error

                with mock.patch.object(perf_driver, "send_command", side_effect=send):
                    perf_driver.shutdown_desktop(1234, self.process)
                self.assertEqual(self.process.poll(), 0)

    def test_shutdown_rejects_crash_even_if_connection_disappears(self):
        def send(*args, **kwargs):
            self.process.exit_code = -11
            self.exit_soon()
            raise ConnectionResetError()

        with (
            mock.patch.object(perf_driver, "send_command", side_effect=send),
            self.assertRaisesRegex(RuntimeError, "status -11"),
        ):
            perf_driver.shutdown_desktop(1234, self.process)

    def test_shutdown_does_not_accept_a_premature_zero_exit(self):
        self.process.returncode = 0
        with self.assertRaisesRegex(RuntimeError, "before clean shutdown"):
            perf_driver.shutdown_desktop(1234, self.process)

    def test_acknowledged_quit_still_requires_exit_before_deadline(self):
        with (
            mock.patch.object(perf_driver, "send_command", return_value=1.0) as send,
            self.assertRaisesRegex(TimeoutError, "shutdown deadline"),
        ):
            perf_driver.shutdown_desktop(1234, self.process, timeout=0.25)
        send.assert_called_once_with(1234, "quit", timeout=0.25)
        self.assertAlmostEqual(self.clock.now, 0.25)

    def test_unacknowledged_quit_and_transport_failures_cannot_retry_forever(self):
        for error in [rejected("quit"), TimeoutError(), ConnectionError("EOF")]:
            with self.subTest(error=error):
                start = self.clock.now
                with (
                    mock.patch.object(
                        perf_driver, "send_command", side_effect=error
                    ) as send,
                    self.assertRaisesRegex(TimeoutError, "shutdown deadline"),
                ):
                    perf_driver.shutdown_desktop(1234, self.process, timeout=0.25)
                self.assertAlmostEqual(self.clock.now - start, 0.25)
                self.assertEqual(send.call_count, 3)
                self.assertAlmostEqual(send.call_args.kwargs["timeout"], 0.05)

    def test_shutdown_does_not_swallow_unrelated_rejection_or_bad_json(self):
        for error in [rejected("quit", "unknown command"), ValueError("invalid JSON")]:
            with (
                self.subTest(error=error),
                mock.patch.object(
                    perf_driver, "send_command", side_effect=error
                ) as send,
            ):
                with self.assertRaises(type(error)):
                    perf_driver.shutdown_desktop(1234, self.process)
                send.assert_called_once()

    def args(self, scenario):
        return argparse.Namespace(
            binary="test-app",
            platform="egui",
            scenario=scenario,
            duration=2,
            playlist="playlist.m3u",
            audio="",
            profiler_arg=[],
            trace_dir="",
        )

    def test_playlist_scenarios_wait_for_frontend_before_actions_and_quit(self):
        for scenario in ["load100playlist", "load1kplaylist", "load10kplaylist"]:
            with self.subTest(scenario=scenario):
                self.clock.now = 0.0
                self.process = Process(self.clock)
                order = []

                def socket_ready(*args, order=order, **kwargs):
                    order.append("socket")
                    self.clock.advance(1)

                def frontend_ready(*args, order=order, **kwargs):
                    order.append("frontend")
                    self.clock.advance(2)

                def actions(*args, order=order, **kwargs):
                    order.append("actions")
                    return []

                def shutdown(*args, order=order, **kwargs):
                    order.append("shutdown")
                    self.process.returncode = 0

                with (
                    mock.patch.object(
                        perf_driver, "unused_tcp_port", return_value=1234
                    ),
                    mock.patch.object(perf_driver.shutil, "which", return_value=None),
                    mock.patch.object(
                        perf_driver.subprocess, "Popen", return_value=self.process
                    ),
                    mock.patch.object(
                        perf_driver, "wait_for_socket", side_effect=socket_ready
                    ),
                    mock.patch.object(
                        perf_driver, "wait_for_frontend", side_effect=frontend_ready
                    ),
                    mock.patch.object(
                        perf_driver, "scenario_actions", side_effect=actions
                    ),
                    mock.patch.object(
                        perf_driver, "shutdown_desktop", side_effect=shutdown
                    ),
                ):
                    metrics = perf_driver.run_desktop(self.args(scenario))
                self.assertEqual(order, ["socket", "frontend", "actions", "shutdown"])
                self.assertEqual(metrics["startup_ms"], 1000)
                self.assertEqual(metrics["input_ready_ms"], 3000)
                # Startup time remains included; a slow readiness check is not hidden.
                self.assertEqual(metrics["elapsed_ms"], 3000)

    def test_readiness_failure_cleans_up_entire_process_group(self):
        def killpg(pid, sig):
            if sig == signal.SIGKILL:
                self.process.returncode = -9

        with (
            mock.patch.object(perf_driver, "unused_tcp_port", return_value=1234),
            mock.patch.object(perf_driver.shutil, "which", return_value="xvfb-run"),
            mock.patch.object(
                perf_driver.subprocess, "Popen", return_value=self.process
            ) as popen,
            mock.patch.object(perf_driver, "wait_for_socket"),
            mock.patch.object(
                perf_driver, "wait_for_frontend", side_effect=TimeoutError("not ready")
            ),
            mock.patch.object(perf_driver.os, "killpg", side_effect=killpg) as kill,
            self.assertRaisesRegex(TimeoutError, "not ready"),
        ):
            perf_driver.run_desktop(self.args("load10kplaylist"))
        self.assertTrue(popen.call_args.kwargs["start_new_session"])
        self.assertEqual(
            kill.call_args_list,
            [
                mock.call(self.process.pid, signal.SIGTERM),
                mock.call(self.process.pid, signal.SIGKILL),
            ],
        )


class CommandTransportTest(unittest.TestCase):
    def test_eof_is_transient_but_malformed_json_is_fatal(self):
        for response, error in [
            ("", ConnectionError),
            ("{bad json", json.JSONDecodeError),
        ]:
            with self.subTest(response=response):
                connection = mock.MagicMock()
                connection.makefile.return_value = io.StringIO(response)
                with mock.patch.object(
                    perf_driver.socket, "create_connection"
                ) as connect:
                    connect.return_value.__enter__.return_value = connection
                    with self.assertRaises(error):
                        perf_driver.send_command(1234, "quit")

    def test_transport_uses_remaining_deadline_and_closes_reader(self):
        clock = Clock()
        reader = io.StringIO('{"accepted":true}\n')
        connection = mock.MagicMock()
        connection.makefile.return_value = reader
        connection.sendall.side_effect = lambda _: clock.advance(0.2)

        def connect(*args, **kwargs):
            clock.advance(0.2)
            context = mock.MagicMock()
            context.__enter__.return_value = connection
            return context

        with (
            mock.patch.object(perf_driver.time, "monotonic", lambda: clock.now),
            mock.patch.object(
                perf_driver.socket, "create_connection", side_effect=connect
            ),
        ):
            perf_driver.send_command(1234, "ping", timeout=0.5)
        budgets = [call.args[0] for call in connection.settimeout.call_args_list]
        self.assertAlmostEqual(budgets[0], 0.3)
        self.assertAlmostEqual(budgets[1], 0.1)
        self.assertTrue(reader.closed)
        self.assertEqual(
            json.loads(connection.sendall.call_args.args[0]),
            {"id": 1, "command": "ping"},
        )


class FailureLogTest(unittest.TestCase):
    def test_failed_command_prints_bounded_tail_but_retains_full_log(self):
        with tempfile.TemporaryDirectory() as directory:
            runner = PerformanceRunner(Path(directory))
            log = Path(directory) / "iteration-00.log"
            error_output = io.StringIO()
            command = [
                sys.executable,
                "-c",
                (
                    "import sys; print('EARLY OUTPUT'); "
                    "[print('x' * 400) for _ in range(100)]; "
                    "print('frontend did not acknowledge command'); sys.exit(7)"
                ),
            ]
            with mock.patch("sys.stderr", error_output):
                status = runner._run_logged(command, log, dry_run=False)
            self.assertEqual(status, 7)
            self.assertIn("EARLY OUTPUT", log.read_text())
            self.assertNotIn("EARLY OUTPUT", error_output.getvalue())
            self.assertIn(
                "frontend did not acknowledge command", error_output.getvalue()
            )
            self.assertIn("iteration-00.log: exit 7", error_output.getvalue())
            self.assertLess(len(error_output.getvalue()), 17 * 1024)

    def test_success_and_dry_run_do_not_print_failure_tail(self):
        with tempfile.TemporaryDirectory() as directory:
            runner = PerformanceRunner(Path(directory))
            for dry_run in [False, True]:
                with (
                    self.subTest(dry_run=dry_run),
                    mock.patch("sys.stderr", new_callable=io.StringIO) as out,
                ):
                    status = runner._run_logged(
                        [sys.executable, "-c", "print('ok')"],
                        Path(directory) / "build.log",
                        dry_run=dry_run,
                    )
                    self.assertEqual(status, 0)
                    self.assertEqual(out.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
