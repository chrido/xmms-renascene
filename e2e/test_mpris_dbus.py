"""Black-box MPRIS/D-Bus E2E tests for GTK and egui frontends."""
# pyright: reportMissingImports=false

from __future__ import annotations

import asyncio
import contextlib
import select
import subprocess
import time
from collections.abc import Awaitable, Callable, Iterator
from dataclasses import dataclass
from importlib import import_module
from pathlib import Path
from typing import Any

pytest: Any = import_module("pytest")
pytest.importorskip("dbus_next", reason="dbus-next is required for MPRIS E2E tests")
pytestmark = pytest.mark.no_xdotool

from conftest import (  # noqa: E402 - import after optional dbus-next dependency check.
    GUI_FRONTENDS,
    GuiFrontend,
    command_exists,
    generate_sine_tracks,
    read_process_log,
    start_gui_process,
)
from mpris import (  # noqa: E402 - import after optional dbus-next dependency check.
    PLAYER_IFACE,
    MprisClient,
    variant_value,
)


@dataclass(frozen=True)
class DbusSession:
    address: str

    @property
    def env(self) -> dict[str, str]:
        return {"DBUS_SESSION_BUS_ADDRESS": self.address}


@pytest.fixture
def dbus_session() -> Iterator[DbusSession]:
    """Run each test against an isolated D-Bus session bus."""
    if not command_exists("dbus-daemon"):
        pytest.skip("dbus-daemon is required for MPRIS E2E tests")

    process = subprocess.Popen(
        ["dbus-daemon", "--session", "--nofork", "--print-address=1"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    assert process.stdout is not None
    try:
        ready, _, _ = select.select([process.stdout], [], [], 5.0)
        if not ready:
            stderr = process.stderr.read() if process.stderr is not None else ""
            raise AssertionError(f"dbus-daemon did not print an address; stderr={stderr}")
        address = process.stdout.readline().strip()
        if not address:
            stderr = process.stderr.read() if process.stderr is not None else ""
            raise AssertionError(f"dbus-daemon printed an empty address; stderr={stderr}")
        yield DbusSession(address)
    finally:
        if process.poll() is None:
            process.terminate()
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=5)
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)


@pytest.fixture
def mpris_tracks(tmp_path: Path) -> dict[str, Path]:
    names = ["one", "two", "opened"]
    tracks = generate_sine_tracks(
        tmp_path / "mpris-tracks",
        [(f"{name}.wav", 440 + index * 110, 15.0) for index, name in enumerate(names)],
        skip_message="ffmpeg is required to create MPRIS E2E audio tracks",
    )
    return dict(zip(names, tracks, strict=True))


@contextlib.contextmanager
def launched_mpris_app(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
    extra_args: list[str] | None = None,
) -> Iterator[subprocess.Popen[bytes]]:
    yield from start_gui_process(
        tmp_path,
        frontend,
        extra_args or [],
        log_name=f"xmms-{frontend.name}-mpris.log",
        extra_env=dbus_session.env,
    )


def track_uri(path: Path) -> str:
    return path.resolve().as_uri()


async def connect_client_for_process(process: subprocess.Popen[bytes], dbus_session: DbusSession) -> MprisClient:
    try:
        return await MprisClient.connect(dbus_session.address)
    except Exception as exc:
        raise AssertionError(f"{exc}\n\nApplication log:\n{read_process_log(process)}") from exc


async def with_mpris_client(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
    extra_args: list[str] | None,
    body: Callable[[MprisClient, subprocess.Popen[bytes]], Awaitable[None]],
) -> None:
    with launched_mpris_app(tmp_path, frontend, dbus_session, extra_args) as process:
        client = await connect_client_for_process(process, dbus_session)
        try:
            await body(client, process)
        finally:
            client.disconnect()


def run_mpris_test(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
    body: Callable[[MprisClient, subprocess.Popen[bytes]], Awaitable[None]],
    extra_args: list[str] | None = None,
) -> None:
    asyncio.run(with_mpris_client(tmp_path, frontend, dbus_session, extra_args, body))


async def wait_properties_changed(
    queue: asyncio.Queue[Any],
    property_name: str,
    timeout: float = 5.0,
) -> tuple[str, dict[str, Any], list[str]]:
    deadline = time.monotonic() + timeout
    last: Any = None
    while time.monotonic() < deadline:
        remaining = max(0.01, deadline - time.monotonic())
        last = await asyncio.wait_for(queue.get(), timeout=remaining)
        interface_name, changed, invalidated = last
        if interface_name == PLAYER_IFACE and property_name in changed:
            return interface_name, changed, invalidated
    raise AssertionError(f"PropertiesChanged for {property_name!r} was not observed; last={last!r}")


async def wait_seeked(
    queue: asyncio.Queue[int],
    expected: int | None = None,
    timeout: float = 5.0,
) -> int:
    deadline = time.monotonic() + timeout
    last: int | None = None
    while time.monotonic() < deadline:
        remaining = max(0.01, deadline - time.monotonic())
        last = await asyncio.wait_for(queue.get(), timeout=remaining)
        if expected is None or last == expected:
            return last
    if expected is None:
        raise AssertionError(f"No Seeked signal was observed; last={last!r}")
    raise AssertionError(f"Seeked({expected}) was not observed; last={last!r}")


@pytest.mark.parametrize("frontend", GUI_FRONTENDS, ids=[frontend.name for frontend in GUI_FRONTENDS])
def test_mpris_introspection_and_initial_properties(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
    mpris_tracks: dict[str, Path],
) -> None:
    async def body(client: MprisClient, _process: subprocess.Popen[bytes]) -> None:
        for name, expected in [
            ("Identity", "XMMS Renascene"),
            ("DesktopEntry", "org.xmms.Renascene"),
        ]:
            assert await client.get_root_property(name) == expected
        assert "file" in await client.get_root_property("SupportedUriSchemes")

        for name, expected in [("PlaybackStatus", "Stopped"), ("Rate", 1.0)]:
            assert await client.get_player_property(name) == expected
        for name in ["CanControl", "CanPlay", "CanPause", "CanSeek"]:
            assert await client.get_player_property(name)

        metadata = await client.get_player_property("Metadata")
        assert {
            key: variant_value(metadata[key])
            for key in ["mpris:trackid", "xesam:url", "xesam:title"]
        } == {
            "mpris:trackid": "/org/xmms/Track/0",
            "xesam:url": track_uri(mpris_tracks["one"]),
            "xesam:title": "one",
        }

    run_mpris_test(
        tmp_path,
        frontend,
        dbus_session,
        body,
        [str(mpris_tracks["one"])],
    )


@pytest.mark.parametrize("frontend", GUI_FRONTENDS, ids=[frontend.name for frontend in GUI_FRONTENDS])
def test_mpris_transport_methods_drive_playback(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
    mpris_tracks: dict[str, Path],
) -> None:
    async def body(client: MprisClient, _process: subprocess.Popen[bytes]) -> None:
        await client.player.call_play()
        await client.wait_player_property("PlaybackStatus", "Playing")

        await client.player.call_next()
        await client.wait_metadata_url(track_uri(mpris_tracks["two"]))

        await client.player.call_previous()
        await client.wait_metadata_url(track_uri(mpris_tracks["one"]))

        await client.player.call_pause()
        await client.wait_player_property("PlaybackStatus", "Paused")

        await client.player.call_play_pause()
        await client.wait_player_property("PlaybackStatus", "Playing")

        await client.player.call_stop()
        await client.wait_player_property("PlaybackStatus", "Stopped")

    run_mpris_test(
        tmp_path,
        frontend,
        dbus_session,
        body,
        [str(mpris_tracks["one"]), str(mpris_tracks["two"])],
    )


@pytest.mark.parametrize("frontend", GUI_FRONTENDS, ids=[frontend.name for frontend in GUI_FRONTENDS])
def test_mpris_open_uri_and_volume_property(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
    mpris_tracks: dict[str, Path],
) -> None:
    async def body(client: MprisClient, _process: subprocess.Popen[bytes]) -> None:
        opened_uri = track_uri(mpris_tracks["opened"])
        await client.player.call_open_uri(opened_uri)
        await client.wait_player_property("PlaybackStatus", "Playing")
        metadata = await client.wait_metadata_url(opened_uri)
        assert variant_value(metadata["xesam:title"]) == "opened"

        await client.set_player_property("Volume", "d", 0.25)
        await client.wait_player_property_predicate(
            "Volume",
            lambda value: abs(value - 0.25) < 0.001,
        )

    run_mpris_test(tmp_path, frontend, dbus_session, body)


@pytest.mark.parametrize("frontend", GUI_FRONTENDS, ids=[frontend.name for frontend in GUI_FRONTENDS])
def test_mpris_seek_and_properties_signals(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
    mpris_tracks: dict[str, Path],
) -> None:
    async def body(client: MprisClient, _process: subprocess.Popen[bytes]) -> None:
        seeked: asyncio.Queue[int] = asyncio.Queue()
        props_changed: asyncio.Queue[Any] = asyncio.Queue()
        client.player.on_seeked(lambda position: seeked.put_nowait(position))
        client.props.on_properties_changed(
            lambda interface_name, changed, invalidated: props_changed.put_nowait(
                (interface_name, changed, invalidated)
            )
        )

        await client.player.call_play()
        _interface_name, changed, _invalidated = await wait_properties_changed(
            props_changed,
            "PlaybackStatus",
        )
        assert variant_value(changed["PlaybackStatus"]) == "Playing"

        before_seek_position = await client.get_player_property("Position")
        await client.player.call_seek(1_000_000)
        relative_seek_position = await wait_seeked(seeked)
        assert relative_seek_position >= before_seek_position + 1_000_000
        assert relative_seek_position <= before_seek_position + 1_750_000
        assert await client.get_player_property("Position") >= relative_seek_position

        await client.player.call_set_position("/org/xmms/Track/0", 2_000_000)
        assert await wait_seeked(seeked, 2_000_000) == 2_000_000
        assert await client.get_player_property("Position") >= 2_000_000

    run_mpris_test(
        tmp_path,
        frontend,
        dbus_session,
        body,
        [str(mpris_tracks["one"])],
    )


@pytest.mark.parametrize("frontend", GUI_FRONTENDS, ids=[frontend.name for frontend in GUI_FRONTENDS])
def test_mpris_raise_method_succeeds(
    tmp_path: Path,
    frontend: GuiFrontend,
    dbus_session: DbusSession,
) -> None:
    async def body(client: MprisClient, process: subprocess.Popen[bytes]) -> None:
        await client.root.call_raise()
        assert process.poll() is None

    run_mpris_test(tmp_path, frontend, dbus_session, body)
