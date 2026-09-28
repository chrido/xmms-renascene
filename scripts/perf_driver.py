"""Execute one deterministic desktop performance scenario iteration."""

from __future__ import annotations

import argparse
import json
import os
import resource
import shutil
import signal
import socket
import subprocess
import time
from contextlib import suppress
from pathlib import Path
from typing import Any, Protocol


class ProcessHandle(Protocol):
    def poll(self) -> int | None: ...

    def wait(self, timeout: float) -> int: ...


def unused_tcp_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def require_running(process: ProcessHandle, phase: str) -> None:
    code = process.poll()
    if code is not None:
        raise RuntimeError(f"application exited with status {code} {phase}")


def wait_for_socket(
    port: int, timeout: float = 30.0, *, process: ProcessHandle | None = None
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process is not None:
            require_running(process, "before its control socket opened")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise TimeoutError(f"control socket did not open on 127.0.0.1:{port}")


ACK_TIMEOUT = "frontend did not acknowledge command"
FRONTEND_CLOSED = "frontend is no longer accepting commands"


class CommandRejected(RuntimeError):
    def __init__(self, command: str, response: dict[str, Any]):
        self.reason = response.get("error")
        super().__init__(f"command {command!r} was rejected: {response}")


def remaining_time(deadline: float) -> float:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("control command deadline exceeded")
    return remaining


def send_command(
    port: int, command: str, *, timeout: float = 5.0, **fields: Any
) -> float:
    started = time.perf_counter()
    deadline = time.monotonic() + timeout
    with socket.create_connection(
        ("127.0.0.1", port), timeout=remaining_time(deadline)
    ) as connection:
        request = json.dumps({"id": 1, "command": command, **fields}) + "\n"
        connection.settimeout(remaining_time(deadline))
        connection.sendall(request.encode())
        connection.settimeout(remaining_time(deadline))
        with connection.makefile("r", encoding="utf-8") as reader:
            response = reader.readline()
    if not response:
        raise ConnectionError(f"control socket closed before acknowledging {command!r}")
    value = json.loads(response)
    if not value.get("accepted"):
        raise CommandRejected(command, value)
    return (time.perf_counter() - started) * 1000.0


def wait_for_frontend(port: int, process: ProcessHandle, timeout: float = 30.0) -> None:
    """Wait for a no-op processed by the frontend, not just an open listener."""
    deadline = time.monotonic() + timeout
    last_error: Exception | None = None
    while True:
        require_running(process, "before frontend readiness")
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(
                "frontend did not acknowledge readiness before deadline"
            ) from last_error
        try:
            send_command(port, "ping", timeout=min(5.0, remaining))
            return
        except CommandRejected as err:
            if err.reason != ACK_TIMEOUT:
                raise
            last_error = err
        except OSError as err:
            last_error = err
        time.sleep(max(0.0, min(0.1, deadline - time.monotonic())))


def shutdown_desktop(port: int, process: ProcessHandle, timeout: float = 30.0) -> None:
    """Retry idempotent quit requests, but require a confirmed clean exit."""
    deadline = time.monotonic() + timeout
    requested = False
    acknowledged = False
    last_error: Exception | None = None
    while True:
        code = process.poll()
        if code is not None:
            if code != 0 or not requested:
                raise RuntimeError(
                    f"application exited with status {code} before clean shutdown"
                )
            return
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError(
                "application did not exit before shutdown deadline"
            ) from last_error
        if not acknowledged:
            requested = True
            try:
                send_command(port, "quit", timeout=min(5.0, remaining))
                acknowledged = True
            except CommandRejected as err:
                if err.reason not in {ACK_TIMEOUT, FRONTEND_CLOSED}:
                    raise
                last_error = err
            except OSError as err:
                # EOF/reset/refusal may mean quit succeeded before the reply.
                # Only process exit status 0 (above) can make this a success.
                last_error = err
        remaining = deadline - time.monotonic()
        if remaining > 0:
            with suppress(subprocess.TimeoutExpired):
                process.wait(timeout=min(0.1, remaining))


def send_initial_command_when_ready(
    port: int,
    command: str,
    timeout: float = 30.0,
    **fields: Any,
) -> float:
    started = time.perf_counter()
    deadline = time.monotonic() + timeout
    while True:
        try:
            send_command(port, command, **fields)
            return (time.perf_counter() - started) * 1000.0
        except (TimeoutError, OSError) as err:
            if time.monotonic() >= deadline:
                raise TimeoutError(
                    f"frontend did not become ready for initial command {command!r}"
                ) from err
        except RuntimeError as err:
            if (
                "frontend did not acknowledge command" not in str(err)
                or time.monotonic() >= deadline
            ):
                raise
        time.sleep(0.1)


def app_command(args: argparse.Namespace, port: int) -> list[str]:
    command = [
        args.binary,
        "--frontend",
        args.platform,
        "--reset",
        "--socket",
        str(port),
    ]
    if args.playlist:
        command.append(f"--load-playlist={args.playlist}")
    if args.audio and not args.playlist:
        command.append(args.audio)
    if args.scenario in {"scroll10kplaylist", "layoutchange"}:
        command.append("--playlist")
    if args.scenario == "playbacknovisualizer":
        command.append("--visualization=off")
    return command


def scenario_actions(args: argparse.Namespace, port: int) -> list[float]:
    latencies: list[float] = []
    if args.scenario.startswith("playback") or args.scenario in {
        "backgroundplayback",
        "widgets",
        "seek100",
        "pauseresume100",
    }:
        latencies.append(send_initial_command_when_ready(port, "play"))
    if args.scenario == "playbackcontrols":
        for command, fields in (
            ("pause", {}),
            ("play", {}),
            ("seek", {"position_ms": 500}),
            ("next", {}),
            ("previous", {}),
            ("volume", {"value": 75}),
            ("balance", {"value": -15}),
            ("equalizer_show", {}),
            ("equalizer_hide", {}),
        ):
            latencies.append(send_command(port, command, **fields))
        for index in range(20):
            latencies.append(send_command(port, "volume", value=index * 5))
    elif args.scenario == "seek100":
        for index in range(100):
            position_ms = ((index * 37) % 100) * 36_000
            latencies.append(send_command(port, "seek", position_ms=position_ms))
    elif args.scenario == "pauseresume100":
        for _index in range(100):
            latencies.append(send_command(port, "pause"))
            latencies.append(send_command(port, "play"))
    elif args.scenario == "scroll10kplaylist":
        latencies.append(
            send_initial_command_when_ready(port, "playlist_scroll", offset=0)
        )
        for offset in range(250, 10_000, 250):
            latencies.append(send_command(port, "playlist_scroll", offset=offset))
    elif args.scenario == "layoutchange":
        latencies.append(
            send_initial_command_when_ready(
                port,
                "playlist_size",
                width=275,
                height=232,
            )
        )
        for width, height in ((550, 464), (350, 348), (275, 232)):
            latencies.append(
                send_command(port, "playlist_size", width=width, height=height)
            )
    return latencies


def run_desktop(args: argparse.Namespace) -> dict[str, Any]:
    port = unused_tcp_port()
    application = [*args.profiler_arg, *app_command(args, port)]
    command = application
    if shutil.which("xvfb-run"):
        command = ["xvfb-run", "-a", "-s", "-screen 0 1280x800x24", *application]
    environment = os.environ.copy()
    environment.update(
        {
            "XMMS_PERF_RUN": "1",
            "XMMS_PERF_TRACE": "1" if args.trace_dir else "0",
            "XMMS_NON_UNIQUE": "1",
            "GSK_RENDERER": environment.get("GSK_RENDERER", "cairo"),
            "GDK_DISABLE": environment.get("GDK_DISABLE", "gl"),
        }
    )
    if args.trace_dir:
        environment["XMMS_PERF_TRACE_DIR"] = args.trace_dir
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.perf_counter()
    process = subprocess.Popen(
        command,
        env=environment,
        start_new_session=True,
    )
    startup_ms = None
    latencies: list[float] = []
    try:
        wait_for_socket(port, process=process)
        startup_ms = (time.perf_counter() - started) * 1000.0
        wait_for_frontend(port, process)
        input_ready_ms = (time.perf_counter() - started) * 1000.0
        latencies = scenario_actions(args, port)
        remaining = max(0.0, args.duration - (time.perf_counter() - started))
        if remaining:
            time.sleep(remaining)
        shutdown_desktop(port, process)
    finally:
        if process.poll() is None:
            # xvfb-run/perf may be the immediate child. Stop the whole session
            # on failure so the application and X server cannot be orphaned.
            with suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGTERM)
            with suppress(subprocess.TimeoutExpired):
                process.wait(timeout=5)
            with suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
    elapsed_ms = (time.perf_counter() - started) * 1000.0
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    return {
        "elapsed_ms": elapsed_ms,
        "startup_ms": startup_ms,
        "input_ready_ms": input_ready_ms,
        "frame_time_median_ms": None,
        "frame_time_p95_ms": None,
        "frame_time_p99_ms": None,
        "missed_frames": None,
        "cpu_user_seconds": usage_after.ru_utime - usage_before.ru_utime,
        "cpu_system_seconds": usage_after.ru_stime - usage_before.ru_stime,
        "peak_rss_bytes": usage_after.ru_maxrss * 1024,
        "allocations": None,
        "mutex_contention": None,
        "bytes_written": None,
        "files_written": None,
        "ipc_calls": None,
        "jni_calls": None,
        "wakeups": None,
        "battery_mah": None,
        "interaction_latencies_ms": latencies,
    }


def run_android(args: argparse.Namespace) -> dict[str, Any]:
    if not shutil.which("adb"):
        raise RuntimeError("adb is required for Android performance scenarios")
    started = time.perf_counter()
    package = "org.xmms.renascene"
    activity = f"{package}/org.xmms.renascene.XmmsActivity"
    subprocess.run(["adb", "shell", "am", "force-stop", package], check=True)
    subprocess.run(["adb", "shell", "am", "start", "-W", "-n", activity], check=True)
    startup_ms = (time.perf_counter() - started) * 1000.0
    time.sleep(max(0, args.duration))
    subprocess.run(["adb", "shell", "am", "force-stop", package], check=True)
    return {
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "startup_ms": startup_ms,
        "input_ready_ms": startup_ms,
        "frame_time_median_ms": None,
        "frame_time_p95_ms": None,
        "frame_time_p99_ms": None,
        "missed_frames": None,
        "cpu_user_seconds": None,
        "cpu_system_seconds": None,
        "peak_rss_bytes": None,
        "allocations": None,
        "mutex_contention": None,
        "bytes_written": None,
        "files_written": None,
        "ipc_calls": None,
        "jni_calls": None,
        "wakeups": None,
        "battery_mah": None,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--platform", required=True)
    parser.add_argument("--scenario", required=True)
    parser.add_argument("--duration", type=int, required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--metrics", required=True)
    parser.add_argument("--playlist", default="")
    parser.add_argument("--audio", default="")
    parser.add_argument("--profiler-arg", action="append", default=[])
    parser.add_argument("--trace-dir", default="")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    metrics = run_android(args) if args.platform == "android" else run_desktop(args)
    Path(args.metrics).write_text(json.dumps(metrics, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
