"""Client helpers for the xmms-rs JSON-lines control socket."""

from __future__ import annotations

import json
import socket
import time
from dataclasses import dataclass, field
from typing import Any


@dataclass
class XmmsControlClient:
    """Small newline-delimited JSON client for --socket E2E tests."""

    port: int
    timeout: float = 5.0
    _socket: socket.socket | None = field(default=None, init=False, repr=False)
    _reader: Any = field(default=None, init=False, repr=False)
    _next_id: int = field(default=0, init=False)

    def connect(self) -> XmmsControlClient:
        self._socket = socket.create_connection(("127.0.0.1", self.port), timeout=self.timeout)
        self._reader = self._socket.makefile("r", encoding="utf-8")
        return self

    def close(self) -> None:
        if self._reader is not None:
            self._reader.close()
            self._reader = None
        if self._socket is not None:
            self._socket.close()
            self._socket = None

    def __enter__(self) -> XmmsControlClient:
        return self.connect()

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()

    def command(self, command: str, **fields: object) -> dict[str, Any]:
        if self._socket is None or self._reader is None:
            raise RuntimeError("control client is not connected")
        self._next_id += 1
        request = {"id": self._next_id, "command": command, **fields}
        self._socket.sendall((json.dumps(request) + "\n").encode("utf-8"))
        line = self._reader.readline()
        if not line:
            raise AssertionError("control socket closed before acknowledging command")
        try:
            response = json.loads(line)
        except json.JSONDecodeError as exc:
            raise AssertionError(f"invalid JSON ack: {line!r}") from exc
        if response.get("id") != request["id"]:
            raise AssertionError(f"unexpected ack id: request={request!r} response={response!r}")
        if not response.get("accepted", False):
            raise AssertionError(f"command was rejected: request={request!r} response={response!r}")
        return response


def wait_for_socket(port: int, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    last_error: OSError | None = None
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError as exc:
            last_error = exc
            time.sleep(0.1)
    raise TimeoutError(f"control socket did not open on port {port}: {last_error}")


def unused_tcp_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
        if not isinstance(port, int):
            raise AssertionError(f"unexpected socket port value: {port!r}")
        return port
