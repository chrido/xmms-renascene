"""Black-box MPRIS/D-Bus helpers for Python GUI E2E tests."""
# pyright: reportMissingImports=false

from __future__ import annotations

import asyncio
import time
from dataclasses import dataclass
from typing import Any

from dbus_next import Variant
from dbus_next.aio import MessageBus

BUS_NAME = "org.mpris.MediaPlayer2.xmms_renascene"
OBJECT_PATH = "/org/mpris/MediaPlayer2"
ROOT_IFACE = "org.mpris.MediaPlayer2"
PLAYER_IFACE = "org.mpris.MediaPlayer2.Player"
PROPS_IFACE = "org.freedesktop.DBus.Properties"


@dataclass
class MprisClient:
    """Small async client for the app's MPRIS object."""

    bus: MessageBus
    root: Any
    player: Any
    props: Any

    @classmethod
    async def connect(cls, bus_address: str, timeout: float = 5.0) -> "MprisClient":
        bus = await MessageBus(bus_address=bus_address).connect()
        deadline = time.monotonic() + timeout
        last_error: BaseException | None = None
        while time.monotonic() < deadline:
            try:
                introspection = await bus.introspect(BUS_NAME, OBJECT_PATH)
                obj = bus.get_proxy_object(BUS_NAME, OBJECT_PATH, introspection)
                return cls(
                    bus=bus,
                    root=obj.get_interface(ROOT_IFACE),
                    player=obj.get_interface(PLAYER_IFACE),
                    props=obj.get_interface(PROPS_IFACE),
                )
            except Exception as exc:  # noqa: BLE001 - preserve final D-Bus error for assertion text.
                last_error = exc
                await asyncio.sleep(0.05)
        bus.disconnect()
        raise AssertionError(f"MPRIS service did not appear on the session bus: {last_error}")

    def disconnect(self) -> None:
        self.bus.disconnect()

    async def get_root_property(self, name: str) -> Any:
        value = await self.props.call_get(ROOT_IFACE, name)
        return value.value

    async def get_player_property(self, name: str) -> Any:
        value = await self.props.call_get(PLAYER_IFACE, name)
        return value.value

    async def set_player_property(self, name: str, signature: str, value: Any) -> None:
        await self.props.call_set(PLAYER_IFACE, name, Variant(signature, value))

    async def wait_player_property(self, name: str, expected: Any, timeout: float = 5.0) -> Any:
        deadline = time.monotonic() + timeout
        last_value: Any = None
        while time.monotonic() < deadline:
            last_value = await self.get_player_property(name)
            if last_value == expected:
                return last_value
            await asyncio.sleep(0.05)
        raise AssertionError(f"MPRIS property {name} never became {expected!r}; last={last_value!r}")

    async def wait_player_property_predicate(
        self,
        name: str,
        predicate: Any,
        timeout: float = 5.0,
    ) -> Any:
        deadline = time.monotonic() + timeout
        last_value: Any = None
        while time.monotonic() < deadline:
            last_value = await self.get_player_property(name)
            if predicate(last_value):
                return last_value
            await asyncio.sleep(0.05)
        raise AssertionError(f"MPRIS property {name} did not satisfy predicate; last={last_value!r}")

    async def wait_metadata_url(self, expected_url: str, timeout: float = 5.0) -> dict[str, Variant]:
        deadline = time.monotonic() + timeout
        last_metadata: Any = None
        while time.monotonic() < deadline:
            metadata = await self.get_player_property("Metadata")
            last_metadata = metadata
            url = metadata.get("xesam:url")
            if variant_value(url) == expected_url:
                return metadata
            await asyncio.sleep(0.05)
        raise AssertionError(f"MPRIS metadata url never became {expected_url!r}; last={last_metadata!r}")


async def wait_for_queue(queue: asyncio.Queue[Any], timeout: float = 5.0) -> Any:
    return await asyncio.wait_for(queue.get(), timeout=timeout)


def variant_value(value: Any) -> Any:
    """Return the wrapped value for dbus-next Variant values."""
    return value.value if isinstance(value, Variant) else value
