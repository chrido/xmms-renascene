import asyncio
import codecs
import contextlib
from collections.abc import Iterable
from functools import cache
from datetime import datetime, timedelta
import io
import os
import json
import logging
import shlex
import shutil
import subprocess
import re
import sys
import tempfile
import threading
import unittest
from unittest import mock
from typing import (
    Any,
    Awaitable,
    Callable,
    TypeVar,
)

T = TypeVar("T")


class ConsoleColor:
    # Some Colors
    RED = "\033[31m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    WHITE = "\033[37m"

    # For messaging
    FATAL = "\033[0;101m\033[37m"

    # Reset colors
    RESET = "\033[0m\033[39m"


def configure_logging():
    # configure logger to print on commandline
    logging.basicConfig(
        level=logging.INFO,
        format="\033[32mlogger>\033[0m\033[39m %(asctime)s - %(levelname)s - %(message)s",
    )


def _to_list(args: list[str] | str | None) -> list[str]:
    if args is None:
        return []
    if isinstance(args, str):
        return [args]
    else:
        return args


def command_exists(command: str) -> bool:
    return shutil.which(command) is not None


def required_command(commands: str | Iterable[str]) -> None:
    command_names = [commands] if isinstance(commands, str) else list(commands)
    missing_commands = [command for command in command_names if not command_exists(command)]
    if missing_commands:
        raise RuntimeError(f"Missing required command(s): {', '.join(missing_commands)}")


class CmdResult:
    command: str
    exit_code: int
    stdout: str
    stderr: str
    mask: list[str]

    def __init__(
        self,
        command: str,
        exit_code: int,
        stdout: str,
        stderr: str,
        mask: list[str] | str = [],
    ) -> None:
        self.command = command
        self.exit_code = exit_code
        self.stdout = stdout
        self.stderr = stderr
        self.mask = _to_list(mask)

    def stdout_or_error(self) -> str:
        if self.exit_code == 0:
            return self.stdout
        else:
            raise Exception(
                f"Command failed with exit code {self.exit_code}:\nstdout: {self.stdout}\n\nstderr: {self.stderr}"
            )

    def raise_on_error(self) -> "CmdResult":
        if self.exit_code != 0:
            raise Exception(
                f"Command failed with exit code {self.exit_code}:\nstdout: {self.stdout}\n\nstderr: {self.stderr}"
            )
        else:
            return self

    def exit_on_error(
        self,
        print_stderr: bool = False,
        print_stdout: bool = False,
        print_out: bool | None = None,
        onerror: Callable | None = None,
        message: str | None = None,
    ) -> "CmdResult":
        if print_out:
            print_stderr = print_out
            print_stdout = print_out

        if self.exit_code != 0:
            print(
                ConsoleColor.FATAL
                + f"failed, exit code: {self.exit_code} ==>"
                + ConsoleColor.RESET
                + " "
                + _apply_mask(self.command, self.mask),
                flush=True,
            )
            if print_stdout:
                print(
                    ConsoleColor.GREEN + "stdout: " + ConsoleColor.RESET + self.stdout,
                    flush=True,
                )
            if print_stderr:
                print(
                    ConsoleColor.RED + "stderr: " + ConsoleColor.RESET + self.stderr,
                    flush=True,
                )

            if message:
                print(f"{ConsoleColor.RED}{message}{ConsoleColor.RESET}")

            if onerror:
                onerror()

            sys.exit(self.exit_code)
        else:
            return self

    def __str__(self) -> str:
        return (
            f"CmdResult(command={self.command}, exit_code={self.exit_code}, stdout={self.stdout}, stderr={self.stderr})"
        )

    def __repr__(self) -> str:
        return self.__str__()

    def json_or_error(self) -> dict[str, str]:
        if self.exit_code == 0:
            return json.loads(self.stdout)
        else:
            raise Exception(
                f"Command failed with exit code {self.exit_code}: stderr: {self.stderr}, stdout: {self.stdout}"
            )

    def success(self) -> bool:
        return self.exit_code == 0

    def parse_bool(self) -> bool:
        return self.stdout.strip().lower() == "true"

    def __or__(self, result_transform: Callable[["CmdResult"], Any]) -> Any:
        return result_transform(self)


def _to_command(args: list[str] | str, cwd: str | None = None) -> str:
    def format_command(args: list[str] | str) -> str:
        if isinstance(args, list):
            if len(args) < 1:
                raise Exception("Needs at least one argument")
            return " ".join(shlex.quote(arg) for arg in args)
        elif isinstance(args, str):
            return args
        else:
            raise Exception("Needs at least one argument")

    if cwd:
        return f"cd {shlex.quote(cwd)} && {format_command(args)}"
    else:
        return format_command(args)


def _apply_mask(command: str, mask: list[str] | str | None = None) -> str:
    mask = _to_list(mask)

    # Always mask common secrets
    for r in [r"AccountKey=([\w|+=/]*)", r"sig=(\w*)"]:
        command = re.sub(r, "***masked***", command)

    for m in mask:
        if m.strip():
            command = re.sub(m, "***masked***", command)
    return command


def _format_return(output):
    if output and isinstance(output, bytes):
        return output.decode("utf-8")
    elif output and isinstance(output, str):
        return output
    elif output:
        return str(output)
    else:
        return ""


async def _resolve_exit_code(proc: asyncio.subprocess.Process) -> int:
    exit_code = proc.returncode
    if exit_code is None:
        exit_code = await proc.wait()
    if exit_code is None:
        raise Exception("Process did not return an exit code")
    return exit_code


def _log_command(
    command: str,
    mask: list[str] | str | None = None,
    env: dict[str, str] | None = None,
) -> None:
    if env and os.environ.get("LOG_ENV", "false").strip().lower() in (
        "1",
        "true",
        "yes",
    ):
        env_str = " ".join(f"{k}='{v}'" for k, v in env.items())
        command = f"{env_str} {command}"
    print(
        ConsoleColor.BLUE + "cmd===> " + ConsoleColor.RESET + _apply_mask(command, mask),
        flush=True,
    )


async def acmd_json(
    args: list[str] | str,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
) -> dict[str, str]:
    return (await acmd(args, env=env, mask=mask, cwd=cwd)).json_or_error()


async def acmd_stdout(
    args: list[str] | str,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
) -> str:
    return (await acmd(args, env=env, mask=mask, cwd=cwd)).stdout_or_error()


async def acmd_raise(
    args: list[str] | str,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
) -> CmdResult:
    return (await acmd(args, env=env, mask=mask, cwd=cwd)).raise_on_error()


async def acmd(
    args: list[str] | str,
    stdin: asyncio.StreamReader | None = None,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
    log_command: bool = True,
) -> CmdResult:
    command = _to_command(args, cwd)
    if log_command:
        _log_command(command, mask, env)

    proc = await asyncio.create_subprocess_shell(
        command,
        stdin=stdin,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=env,
        shell=True,
    )
    stdout, stderr = await proc.communicate()
    exit_code = await _resolve_exit_code(proc)

    return CmdResult(
        command=command,
        exit_code=exit_code,
        stdout=_format_return(stdout),
        stderr=_format_return(stderr),
        mask=_to_list(mask),
    )


async def acmd_input(
    args: list[str] | str,
    input_data: str | bytes,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
    log_command: bool = True,
) -> CmdResult:
    command = _to_command(args, cwd)
    if log_command:
        _log_command(command, mask, env)

    proc = await asyncio.create_subprocess_shell(
        command,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=env,
        shell=True,
    )
    stdin_bytes = input_data.encode("utf-8") if isinstance(input_data, str) else input_data
    stdout_bytes, stderr_bytes = await proc.communicate(stdin_bytes)
    exit_code = await _resolve_exit_code(proc)

    return CmdResult(
        command=command,
        exit_code=exit_code,
        stdout=_format_return(stdout_bytes),
        stderr=_format_return(stderr_bytes),
        mask=_to_list(mask),
    )


def _run_awaitable(awaitable: Awaitable[T]) -> T:
    try:
        asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(awaitable)

    result: list[T] = []
    errors: list[BaseException] = []

    def run_in_thread() -> None:
        try:
            result.append(asyncio.run(awaitable))
        except BaseException as e:
            errors.append(e)

    thread = threading.Thread(target=run_in_thread)
    thread.start()
    thread.join()

    if errors:
        raise errors[0]
    if not result:
        raise RuntimeError("Awaitable completed without returning a result")
    return result[0]


def _result_identity(result: CmdResult) -> CmdResult:
    return result


def _without_trailing_line_ending(value: str) -> str:
    if value.endswith("\r\n"):
        return value[:-2]
    if value.endswith("\n"):
        return value[:-1]
    return value


class CliResultTransform:
    def __init__(self, transform: Callable[[CmdResult], Any]) -> None:
        self.transform = transform

    def __call__(self, result: CmdResult) -> Any:
        return self.transform(result)

    def __or__(self, next_transform: Callable[[Any], Any]) -> "CliResultTransform":
        return CliResultTransform(lambda result: next_transform(self(result)))


def log_output(result: CmdResult) -> CmdResult:
    for output in (result.stdout, result.stderr):
        for line in output.rstrip("\n").splitlines():
            logging.info(line)
    return result


stdout = CliResultTransform(lambda result: _without_trailing_line_ending(result.stdout_or_error()))
stderr = CliResultTransform(lambda result: _without_trailing_line_ending(result.raise_on_error().stderr))
stdout_stderr = CliResultTransform(
    lambda result: (
        _without_trailing_line_ending(result.raise_on_error().stdout),
        _without_trailing_line_ending(result.stderr),
    )
)
asjson = CliResultTransform(lambda result: result.json_or_error())
raise_on_error = CliResultTransform(lambda result: result.raise_on_error())
logged = CliResultTransform(log_output)
exit_code = CliResultTransform(lambda result: result.exit_code)


class CliExec:
    def __init__(
        self,
        stdin: asyncio.StreamReader | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
        result_transform: Callable[[CmdResult], Any] = _result_identity,
        log_command: bool = True,
    ) -> None:
        self.stdin = stdin
        self.env = env
        self.mask = mask
        self.cwd = cwd
        self.result_transform = result_transform
        self.log_command = log_command

    def __call__(
        self,
        stdin: asyncio.StreamReader | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
        log_command: bool | None = None,
    ) -> "CliExec":
        return CliExec(
            stdin=[stdin, self.stdin][stdin is None],
            env=[env, self.env][env is None],
            mask=[mask, self.mask][mask is None],
            cwd=[cwd, self.cwd][cwd is None],
            result_transform=self.result_transform,
            log_command=[log_command, self.log_command][log_command is None],
        )

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "CliExec":
        return CliExec(
            stdin=self.stdin,
            env=self.env,
            mask=self.mask,
            cwd=self.cwd,
            result_transform=result_transform,
            log_command=self.log_command,
        )

    def __rmatmul__(self, args: list[str] | str) -> Any:
        command = CliCommand(args=args, cli_exec=self)
        if self.result_transform is _result_identity:
            return command
        return self.result_transform(command.result())


class CliCommand:
    def __init__(
        self,
        args: list[str] | str,
        cli_exec: CliExec,
        input_data: str | bytes | None = None,
    ) -> None:
        self.args = args
        self.cli_exec = cli_exec
        self.input_data = input_data
        self._result: CmdResult | None = None

    def result(self) -> CmdResult:
        if self._result is None:
            if self.input_data is None:
                self._result = _run_awaitable(
                    acmd(
                        self.args,
                        stdin=self.cli_exec.stdin,
                        env=self.cli_exec.env,
                        mask=self.cli_exec.mask,
                        cwd=self.cli_exec.cwd,
                        log_command=self.cli_exec.log_command,
                    )
                )
            else:
                if self.cli_exec.stdin is not None:
                    raise ValueError("Cannot combine configured stdin with pipe input")
                self._result = _run_awaitable(
                    acmd_input(
                        self.args,
                        input_data=self.input_data,
                        env=self.cli_exec.env,
                        mask=self.cli_exec.mask,
                        cwd=self.cli_exec.cwd,
                        log_command=self.cli_exec.log_command,
                    )
                )
        return self._result

    def __getattr__(self, name: str) -> Any:
        return getattr(self.result(), name)

    def __repr__(self) -> str:
        return repr(self.result())

    def __str__(self) -> str:
        return str(self.result())

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> Any:
        return result_transform(self.result())

    def __ror__(self, input_data: str | bytes) -> "CliCommand":
        if not isinstance(input_data, (str, bytes)):
            return NotImplemented
        return CliCommand(args=self.args, cli_exec=self.cli_exec, input_data=input_data)


class AsyncCliExec:
    def __init__(
        self,
        stdin: asyncio.StreamReader | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
        result_transform: Callable[[CmdResult], Any] = _result_identity,
        log_command: bool = True,
    ) -> None:
        self.stdin = stdin
        self.env = env
        self.mask = mask
        self.cwd = cwd
        self.result_transform = result_transform
        self.log_command = log_command

    def __call__(
        self,
        stdin: asyncio.StreamReader | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
        log_command: bool | None = None,
    ) -> "AsyncCliExec":
        return AsyncCliExec(
            stdin=[stdin, self.stdin][stdin is None],
            env=[env, self.env][env is None],
            mask=[mask, self.mask][mask is None],
            cwd=[cwd, self.cwd][cwd is None],
            result_transform=self.result_transform,
            log_command=[log_command, self.log_command][log_command is None],
        )

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "AsyncCliExec":
        return AsyncCliExec(
            stdin=self.stdin,
            env=self.env,
            mask=self.mask,
            cwd=self.cwd,
            result_transform=result_transform,
            log_command=self.log_command,
        )

    def __rmatmul__(self, args: list[str] | str) -> Any:
        command = AsyncCliCommand(args=args, async_cli_exec=self)
        if self.result_transform is _result_identity:
            return command
        return AsyncCliTransformedCommand(command, self.result_transform)


class AsyncCliCommand:
    def __init__(
        self,
        args: list[str] | str,
        async_cli_exec: AsyncCliExec,
        input_data: str | bytes | None = None,
    ) -> None:
        self.args = args
        self.async_cli_exec = async_cli_exec
        self.input_data = input_data
        self._task: asyncio.Task[CmdResult] | None = None

    async def _run(self) -> CmdResult:
        if self.input_data is None:
            return await acmd(
                self.args,
                stdin=self.async_cli_exec.stdin,
                env=self.async_cli_exec.env,
                mask=self.async_cli_exec.mask,
                cwd=self.async_cli_exec.cwd,
                log_command=self.async_cli_exec.log_command,
            )
        if self.async_cli_exec.stdin is not None:
            raise ValueError("Cannot combine configured stdin with pipe input")
        return await acmd_input(
            self.args,
            input_data=self.input_data,
            env=self.async_cli_exec.env,
            mask=self.async_cli_exec.mask,
            cwd=self.async_cli_exec.cwd,
            log_command=self.async_cli_exec.log_command,
        )

    def _ensure_task(self) -> asyncio.Task[CmdResult]:
        if self._task is None:
            self._task = asyncio.create_task(self._run())
        return self._task

    def __await__(self):
        return self._ensure_task().__await__()

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "AsyncCliTransformedCommand":
        return AsyncCliTransformedCommand(self, result_transform)

    def __ror__(self, input_data: str | bytes) -> "AsyncCliCommand":
        if not isinstance(input_data, (str, bytes)):
            return NotImplemented
        return AsyncCliCommand(
            args=self.args,
            async_cli_exec=self.async_cli_exec,
            input_data=input_data,
        )


class AsyncCliTransformedCommand:
    def __init__(
        self,
        command: AsyncCliCommand,
        result_transform: Callable[[CmdResult], Any],
    ) -> None:
        self.command = command
        self.result_transform = result_transform

    async def _run(self) -> Any:
        return self.result_transform(await self.command)

    def __await__(self):
        return self._run().__await__()


class CliFollowExec:
    def __init__(
        self,
        prefix: str | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        progress: Any | None = None,
        cwd: str | None = None,
        result_transform: Callable[[CmdResult], Any] = _result_identity,
    ) -> None:
        self.prefix = prefix
        self.env = env
        self.mask = mask
        self.progress = progress
        self.cwd = cwd
        self.result_transform = result_transform

    def __call__(
        self,
        prefix: str | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        progress: Any | None = None,
        cwd: str | None = None,
    ) -> "CliFollowExec":
        return CliFollowExec(
            prefix=[prefix, self.prefix][prefix is None],
            env=[env, self.env][env is None],
            mask=[mask, self.mask][mask is None],
            progress=[progress, self.progress][progress is None],
            cwd=[cwd, self.cwd][cwd is None],
            result_transform=self.result_transform,
        )

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "CliFollowExec":
        return CliFollowExec(
            prefix=self.prefix,
            env=self.env,
            mask=self.mask,
            progress=self.progress,
            cwd=self.cwd,
            result_transform=result_transform,
        )

    def __rmatmul__(self, args: list[str] | str) -> Any:
        command = CliFollowCommand(args=args, cli_follow_exec=self)
        if self.result_transform is _result_identity:
            return command
        return self.result_transform(command.result())


class CliFollowCommand:
    def __init__(
        self,
        args: list[str] | str,
        cli_follow_exec: CliFollowExec,
    ) -> None:
        self.args = args
        self.cli_follow_exec = cli_follow_exec
        self._result: CmdResult | None = None

    def result(self) -> CmdResult:
        if self._result is None:
            self._result = _run_awaitable(
                acmd_follow(
                    self.args,
                    prefix=self.cli_follow_exec.prefix,
                    env=self.cli_follow_exec.env,
                    mask=self.cli_follow_exec.mask,
                    progress=self.cli_follow_exec.progress,
                    cwd=self.cli_follow_exec.cwd,
                )
            )
        return self._result

    def __getattr__(self, name: str) -> Any:
        return getattr(self.result(), name)

    def __repr__(self) -> str:
        return repr(self.result())

    def __str__(self) -> str:
        return str(self.result())

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> Any:
        return result_transform(self.result())


class AsyncCliFollowExec:
    def __init__(
        self,
        prefix: str | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        progress: Any | None = None,
        cwd: str | None = None,
        result_transform: Callable[[CmdResult], Any] = _result_identity,
    ) -> None:
        self.prefix = prefix
        self.env = env
        self.mask = mask
        self.progress = progress
        self.cwd = cwd
        self.result_transform = result_transform

    def __call__(
        self,
        prefix: str | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        progress: Any | None = None,
        cwd: str | None = None,
    ) -> "AsyncCliFollowExec":
        return AsyncCliFollowExec(
            prefix=[prefix, self.prefix][prefix is None],
            env=[env, self.env][env is None],
            mask=[mask, self.mask][mask is None],
            progress=[progress, self.progress][progress is None],
            cwd=[cwd, self.cwd][cwd is None],
            result_transform=self.result_transform,
        )

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "AsyncCliFollowExec":
        return AsyncCliFollowExec(
            prefix=self.prefix,
            env=self.env,
            mask=self.mask,
            progress=self.progress,
            cwd=self.cwd,
            result_transform=result_transform,
        )

    def __rmatmul__(self, args: list[str] | str) -> Any:
        command = AsyncCliFollowCommand(args=args, async_cli_follow_exec=self)
        if self.result_transform is _result_identity:
            return command
        return AsyncCliFollowTransformedCommand(command, self.result_transform)


class AsyncCliFollowCommand:
    def __init__(
        self,
        args: list[str] | str,
        async_cli_follow_exec: AsyncCliFollowExec,
    ) -> None:
        self.args = args
        self.async_cli_follow_exec = async_cli_follow_exec
        self._task: asyncio.Task[CmdResult] | None = None

    async def _run(self) -> CmdResult:
        return await acmd_follow(
            self.args,
            prefix=self.async_cli_follow_exec.prefix,
            env=self.async_cli_follow_exec.env,
            mask=self.async_cli_follow_exec.mask,
            progress=self.async_cli_follow_exec.progress,
            cwd=self.async_cli_follow_exec.cwd,
        )

    def _ensure_task(self) -> asyncio.Task[CmdResult]:
        if self._task is None:
            self._task = asyncio.create_task(self._run())
        return self._task

    def __await__(self):
        return self._ensure_task().__await__()

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "AsyncCliFollowTransformedCommand":
        return AsyncCliFollowTransformedCommand(self, result_transform)


class AsyncCliFollowTransformedCommand:
    def __init__(
        self,
        command: AsyncCliFollowCommand,
        result_transform: Callable[[CmdResult], Any],
    ) -> None:
        self.command = command
        self.result_transform = result_transform

    async def _run(self) -> Any:
        return self.result_transform(await self.command)

    def __await__(self):
        return self._run().__await__()


cli = CliExec()
acli = AsyncCliExec()
cli_follow = CliFollowExec()
acli_follow = AsyncCliFollowExec()


class AppExec:
    def __init__(
        self,
        app: str,
        cli_exec: CliExec | AsyncCliExec | CliFollowExec | AsyncCliFollowExec = acli,
    ) -> None:
        self.app = app
        self.cli_exec = cli_exec

    def __call__(
        self,
        stdin: asyncio.StreamReader | None = None,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
        prefix: str | None = None,
        progress: Any | None = None,
    ) -> "AppExec":
        if isinstance(self.cli_exec, CliFollowExec | AsyncCliFollowExec):
            if stdin is not None:
                raise ValueError("follow commands do not support stdin")
            return AppExec(
                self.app,
                self.cli_exec(
                    prefix=prefix,
                    env=env,
                    mask=mask,
                    progress=progress,
                    cwd=cwd,
                ),
            )
        if prefix is not None or progress is not None:
            raise ValueError("non-follow commands do not support prefix or progress")
        return AppExec(
            self.app,
            self.cli_exec(
                stdin=stdin,
                env=env,
                mask=mask,
                cwd=cwd,
            ),
        )

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "AppExec":
        return AppExec(self.app, self.cli_exec | result_transform)

    def __rmatmul__(self, args: list[str] | str) -> Any:
        if isinstance(args, list):
            return [self.app, *args] @ self.cli_exec
        if isinstance(args, str):
            command = shlex.quote(self.app)
            if args:
                command = f"{command} {args}"
            return command @ self.cli_exec
        return NotImplemented


async def acmd_with_retry(
    args: list[str] | str,
    mask: list[str] | str | None = None,
    max_retry_attempts: int = 3,
    backoff_seconds: int = 5,
) -> CmdResult | None:
    result: CmdResult | None = None

    for attempt in range(max_retry_attempts):
        result = await acmd(args=args, mask=mask)
        if result.success():
            return result
        if attempt < max_retry_attempts - 1:
            await asyncio.sleep(backoff_seconds)

    return result


class Progress:
    def handle(self, _: str):
        pass

    def finished(self):
        pass


class DevopsPytestProgress(Progress):
    def __init__(self, application: str):
        self.progress = r"\[\s*(?P<percent>([0-9]+))%\]"
        self.application = application

    def handle(self, line: str):
        m = re.search(self.progress, line)
        if m:
            percent = int(m.group("percent"))
            print(f"##vso[task.setprogress value={percent};]{self.application}")

    def finished(self):
        print(f"##vso[task.setprogress value=100;]{self.application}")


def with_pytest_progress(application_name: str):
    if os.environ.get("TF_BUILD") is not None:
        # When we are in a devops pipeline, we want a nice percentage indicator
        return DevopsPytestProgress(application_name)
    else:
        # For local execution, we don't want the additional printout
        return Progress()


prefix_length_semaphore = asyncio.Semaphore(1)
current_prefixes = []
longest_prefix = 0


async def add_prefix(prefix: str) -> None:
    global longest_prefix
    async with prefix_length_semaphore:
        current_prefixes.append(prefix)
        longest_prefix = max(longest_prefix, len(prefix))


async def remove_prefix(prefix: str) -> None:
    global longest_prefix
    async with prefix_length_semaphore:
        current_prefixes.remove(prefix)
        longest_prefix = max(len(p) for p in current_prefixes) if current_prefixes else 0


async def acmd_follow(
    args: list[str] | str,
    prefix: str | None = None,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    progress: Progress | None = None,
    cwd: str | None = None,
) -> CmdResult:
    if progress is None:
        progress = Progress()

    command = _to_command(args, cwd)
    _log_command(command, mask, env)

    proc = await asyncio.create_subprocess_shell(
        command,
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        shell=True,
        env=env,
    )

    stdout_buffer = io.StringIO()
    stdout_line = ""
    stderr_buffer = io.StringIO()
    stderr_line = ""

    def line_prefix() -> str:
        if prefix:
            extended_prefix = prefix + " " * (longest_prefix - len(prefix))
            return ConsoleColor.BLUE + f"[{extended_prefix}] " + ConsoleColor.RESET
        else:
            return ""

    def print_newlines(lineprefix: str, s: str) -> str:
        cmd_prefix = line_prefix()

        while s.find("\n") != -1:
            idx = s.find("\n")
            line = s[:idx]
            while line.find("\r") != -1:  # We only print the last when we have a \r
                line = line[: line.find("\r")]
            progress.handle(line)
            line = _apply_mask(line, mask)
            print(cmd_prefix + lineprefix + line, end="\n", flush=True)
            s = s[idx + 1 :]
        return s

    if prefix:
        await add_prefix(prefix)

    print(ConsoleColor.RESET, flush=True)
    stdout_dec = codecs.getincrementaldecoder("utf-8")(errors="ignore")
    stderr_dec = codecs.getincrementaldecoder("utf-8")(errors="ignore")
    activity_signal = asyncio.Event()

    async def consume_stream(
        stream: asyncio.StreamReader,
        decoder: codecs.IncrementalDecoder,
        line_prefix_color: str,
        buffer: io.StringIO,
        current_line: str,
    ) -> str:
        while True:
            chunk = await stream.read(1000)
            decoded = decoder.decode(chunk or b"", final=not chunk)
            if decoded:
                buffer.write(decoded)
                current_line += decoded
                current_line = print_newlines(line_prefix_color, current_line)
                activity_signal.set()
            if not chunk:
                return current_line

    async def report_inactivity(proc_wait_task: asyncio.Task[int]) -> None:
        while True:
            activity_task = asyncio.create_task(activity_signal.wait())
            sleep_task = asyncio.create_task(asyncio.sleep(timedelta(minutes=5).total_seconds()))
            done, pending = await asyncio.wait(
                {proc_wait_task, activity_task, sleep_task},
                return_when=asyncio.FIRST_COMPLETED,
            )
            for task in pending:
                task.cancel()
            if pending:
                await asyncio.gather(*pending, return_exceptions=True)

            if proc_wait_task in done:
                return
            if activity_task in done:
                activity_signal.clear()
                continue

            print(
                line_prefix()
                + ConsoleColor.YELLOW
                + "No output from command for 5 minutes, still running: "
                + ConsoleColor.RESET
                + _apply_mask(command, mask),
                flush=True,
            )

    if proc.stdout is None or proc.stderr is None:
        raise Exception("Failed to create subprocess output streams")

    try:
        async with asyncio.TaskGroup() as tg:
            proc_wait_task = tg.create_task(proc.wait())
            stdout_task = tg.create_task(
                consume_stream(
                    proc.stdout,
                    stdout_dec,
                    ConsoleColor.GREEN + "stdout> " + ConsoleColor.RESET,
                    stdout_buffer,
                    stdout_line,
                )
            )
            stderr_task = tg.create_task(
                consume_stream(
                    proc.stderr,
                    stderr_dec,
                    ConsoleColor.RED + "stderr> " + ConsoleColor.RESET,
                    stderr_buffer,
                    stderr_line,
                )
            )
            tg.create_task(report_inactivity(proc_wait_task))
        stdout_line = stdout_task.result()
        stderr_line = stderr_task.result()
    finally:
        print(ConsoleColor.RESET, flush=True)
        if prefix:
            await remove_prefix(prefix)

    progress.finished()
    exit_code = await _resolve_exit_code(proc)

    return CmdResult(
        command=command,
        exit_code=exit_code,
        stdout=stdout_buffer.getvalue(),
        stderr=stderr_buffer.getvalue(),
        mask=_to_list(mask),
    )


async def acmd_follow_raise(
    args: list[str] | str,
    prefix: str | None = None,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    progress: Progress | None = None,
    cwd: str | None = None,
) -> CmdResult:
    return (await acmd_follow(args, prefix, env, mask, progress, cwd)).raise_on_error()


class CommandCtx:
    def __init__(self):
        self.commands = []

    def next(self, command: str) -> None:
        self.commands.append(command)


class CmdSequence:
    def __init__(
        self,
        cwd: str | None = None,
        mask: list[str] | str | None = None,
    ) -> None:
        self.cwd = cwd
        self.mask = mask

    async def __aenter__(self) -> CommandCtx:
        self.sequence = CommandCtx()
        return self.sequence

    async def __aexit__(self, exc_type, exc_value, traceback) -> None:
        for command in self.sequence.commands:
            (await acmd(command, cwd=self.cwd, mask=self.mask)).raise_on_error()


class CommandEnvironmentContext:
    def __init__(
        self,
        env: dict[str, str] | None = None,
        cwd: str | None = None,
        prefix: str | None = None,
        mask: list[str] | str | None = None,
        result_transform: Callable[[CmdResult], Any] = _result_identity,
    ) -> None:
        self.env = env
        self.cwd = cwd
        self.prefix = prefix
        self.result_transform = result_transform
        if mask is None:
            self.mask = []
        elif isinstance(mask, str):
            self.mask = [mask]
        else:
            self.mask = mask

    def join_mask(self, mask: list[str] | str | None) -> list[str]:
        if mask is None:
            return self.mask
        elif isinstance(mask, str):
            return self.mask + [mask]
        else:
            return self.mask + mask

    async def acmd(
        self,
        args: list[str] | str,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
    ) -> CmdResult:
        return await acmd(
            args,
            env=[env, self.env][env is None],
            mask=self.join_mask(mask),
            cwd=[cwd, self.cwd][cwd is None],
        )

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "CommandEnvironmentContext":
        return CommandEnvironmentContext(
            env=self.env,
            cwd=self.cwd,
            prefix=self.prefix,
            mask=self.mask,
            result_transform=result_transform,
        )

    def __rmatmul__(self, args: list[str] | str) -> Any:
        return CliExec(
            env=self.env,
            mask=self.mask,
            cwd=self.cwd,
            result_transform=self.result_transform,
        ).__rmatmul__(args)

    async def acmd_stdout(
        self,
        args: list[str] | str,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
    ) -> str:
        return await acmd_stdout(
            args,
            env=[env, self.env][env is None],
            mask=self.join_mask(mask),
            cwd=[cwd, self.cwd][cwd is None],
        )

    async def acmd_json(
        self,
        args: list[str] | str,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
    ) -> dict[str, str]:
        return await acmd_json(
            args,
            env=[env, self.env][env is None],
            mask=self.join_mask(mask),
            cwd=[cwd, self.cwd][cwd is None],
        )

    async def acmd_raise(
        self,
        args: list[str] | str,
        env: dict[str, str] | None = None,
        mask: list[str] | str | None = None,
        cwd: str | None = None,
    ) -> CmdResult:
        return await acmd_raise(
            args,
            env=[env, self.env][env is None],
            mask=self.join_mask(mask),
            cwd=[cwd, self.cwd][cwd is None],
        )

    async def acmd_follow(
        self,
        args: list[str] | str,
        prefix: str | None = None,
        mask: list[str] | str | None = None,
        progress: Progress | None = None,
        cwd: str | None = None,
    ) -> CmdResult:
        return await acmd_follow(
            args=args,
            env=self.env,
            prefix=[prefix, self.prefix][prefix is None],
            mask=self.join_mask(mask),
            progress=progress,
            cwd=[cwd, self.cwd][cwd is None],
        )

    async def acmd_follow_raise(
        self,
        args: list[str] | str,
        prefix: str | None = None,
        mask: list[str] | str | None = None,
        progress: Progress | None = None,
        cwd: str | None = None,
    ) -> CmdResult:
        return await acmd_follow_raise(
            args=args,
            env=self.env,
            prefix=[prefix, self.prefix][prefix is None],
            mask=self.join_mask(mask),
            progress=progress,
            cwd=[cwd, self.cwd][cwd is None],
        )

    def get_env(self) -> dict[str, str]:
        if self.env is None:
            return {}
        return self.env


class CommandEnvironment:
    def __init__(
        self,
        env: dict[str, str] | None = None,
        cwd: str | None = None,
        prefix: str | None = None,
        mask: list[str] | str | None = None,
        result_transform: Callable[[CmdResult], Any] = _result_identity,
    ) -> None:
        self.env = env
        self.cwd = cwd
        self.prefix = prefix
        self.mask = mask
        self.result_transform = result_transform

    def __enter__(self) -> CommandEnvironmentContext:
        return CommandEnvironmentContext(
            self.env,
            self.cwd,
            self.prefix,
            self.mask,
            self.result_transform,
        )

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        pass

    def __rmatmul__(self, args: list[str] | str) -> Any:
        return self.__enter__().__rmatmul__(args)

    def __or__(self, result_transform: Callable[[CmdResult], Any]) -> "CommandEnvironment":
        return CommandEnvironment(
            env=self.env,
            cwd=self.cwd,
            prefix=self.prefix,
            mask=self.mask,
            result_transform=result_transform,
        )


def cmd(
    arg: list[str] | str,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
) -> CmdResult:
    command = _to_command(arg, cwd)
    _log_command(command, mask, env)

    cp = subprocess.run(command, capture_output=True, shell=True, encoding="utf-8", env=env)
    return CmdResult(
        command=command,
        exit_code=cp.returncode,
        stdout=_format_return(cp.stdout),
        stderr=_format_return(cp.stderr),
    )


def cmd_stdout(
    args: list[str] | str,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
) -> str:
    return cmd(args, env, mask, cwd).stdout_or_error()


def cmd_raise(
    args: list[str] | str,
    env: dict[str, str] | None = None,
    mask: list[str] | str | None = None,
    cwd: str | None = None,
) -> CmdResult:
    return cmd(args, env, mask, cwd).raise_on_error()


def asingleton(func: Callable[[], Awaitable[T]]) -> Callable[[], Awaitable[T]]:
    """
    An async singleton decorator that ensures the decorated function
    is only called once.
    """
    instance: T | None = None
    lock: asyncio.Lock | None = None
    thread_lock = asyncio.Lock()

    async def get_instance() -> T:
        nonlocal instance, lock
        if lock is None:
            async with thread_lock:
                if lock is None:
                    lock = asyncio.Lock()
        if instance is None:
            async with lock:
                if instance is None:
                    instance = await func()
        return instance

    return get_instance


@cache
def is_devops() -> bool:
    """
    Check if the current environment is a devops environment
    """
    return os.environ.get("TF_BUILD") is not None


class RecordingProgress(Progress):
    def __init__(self) -> None:
        self.lines: list[str] = []
        self.finished_called = False

    def handle(self, line: str):
        self.lines.append(line)

    def finished(self):
        self.finished_called = True


class _CommandlineTestMixin:
    @staticmethod
    def python_command(code: str) -> str:
        return f"{shlex.quote(sys.executable)} -c {shlex.quote(code)}"


class TestCommandlineFunctions(_CommandlineTestMixin, unittest.TestCase):
    def test_configure_logging_configures_basic_config(self):
        with mock.patch("logging.basicConfig") as basic_config:
            configure_logging()

        basic_config.assert_called_once_with(
            level=logging.INFO,
            format="\033[32mlogger>\033[0m\033[39m %(asctime)s - %(levelname)s - %(message)s",
        )

    def test_with_pytest_progress_returns_progress_locally(self):
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("TF_BUILD", None)
            progress = with_pytest_progress("unit tests")

        self.assertIsInstance(progress, Progress)
        self.assertNotIsInstance(progress, DevopsPytestProgress)

    def test_with_pytest_progress_returns_devops_progress_in_devops(self):
        with mock.patch.dict(os.environ, {"TF_BUILD": "1"}, clear=False):
            progress = with_pytest_progress("unit tests")

        self.assertIsInstance(progress, DevopsPytestProgress)

    def test_required_command_accepts_existing_single_command(self):
        with mock.patch("shutil.which", return_value="/usr/bin/git") as which:
            required_command("git")

        which.assert_called_once_with("git")

    def test_required_command_accepts_existing_command_iterable(self):
        def which(command: str) -> str | None:
            return f"/usr/bin/{command}"

        with mock.patch("shutil.which", side_effect=which) as mocked_which:
            required_command(("git", "curl"))

        self.assertEqual([call.args[0] for call in mocked_which.call_args_list], ["git", "curl"])

    def test_required_command_raises_for_missing_commands(self):
        def which(command: str) -> str | None:
            return None if command in {"missing-a", "missing-b"} else f"/usr/bin/{command}"

        with mock.patch("shutil.which", side_effect=which):
            with self.assertRaises(RuntimeError) as ctx:
                required_command(["git", "missing-a", "missing-b"])

        message = str(ctx.exception)
        self.assertIn("missing-a", message)
        self.assertIn("missing-b", message)
        self.assertNotIn("git", message)

    def test_cmd_runs_command_with_env_and_cwd(self):
        code = (
            "import json, os\nprint(json.dumps({'value': os.environ['COMMANDLINE_TEST_VALUE'], 'cwd': os.getcwd()}))\n"
        )
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            with contextlib.redirect_stdout(io.StringIO()):
                result = cmd(
                    self.python_command(code),
                    env={"COMMANDLINE_TEST_VALUE": "ok"},
                    cwd=tmpdir,
                )

        self.assertEqual(result.exit_code, 0)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["value"], "ok")
        self.assertEqual(payload["cwd"], tmpdir)

    def test_cmd_stdout_returns_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = cmd_stdout(self.python_command("import sys; sys.stdout.write('hello')"))

        self.assertEqual(result, "hello")

    def test_cmd_quotes_list_arguments(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = cmd([sys.executable, "-c", "import sys; print(sys.argv[1])", "hello world"])

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "hello world\n")

    def test_cmd_raise_raises_on_error(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(Exception) as ctx:
                cmd_raise(self.python_command("import sys; sys.exit(7)"))

        self.assertIn("exit code 7", str(ctx.exception))

    def test_cli_exec_runs_command_with_matmul_operator(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.python_command("import sys; sys.stdout.write('hello')") @ cli

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "hello")

    def test_cli_exec_accepts_list_arguments_with_matmul_operator(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = [sys.executable, "-c", "import sys; sys.stdout.write(sys.argv[1])", "hello world"] @ cli

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "hello world")

    def test_cli_exec_is_configurable(self):
        code = "import json, os\nprint(json.dumps({'value': os.environ['COMMANDLINE_TEST_VALUE'], 'cwd': os.getcwd()}))\n"
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            with contextlib.redirect_stdout(io.StringIO()):
                result = (
                    self.python_command(code) @ cli(
                        env={"COMMANDLINE_TEST_VALUE": "ok"},
                        cwd=tmpdir,
                    )
                ).result()

        self.assertEqual(result.exit_code, 0)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["value"], "ok")
        self.assertEqual(payload["cwd"], tmpdir)

    def test_cli_exec_stdout_transform_returns_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = 'echo "test"' @ cli | stdout

        self.assertEqual(result, "test")

    def test_cli_follow_runs_command_with_matmul_operator(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = 'echo "test"' @ cli_follow

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "test\n")

    def test_cli_follow_stdout_transform_returns_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = 'echo "test"' @ cli_follow | stdout

        self.assertEqual(result, "test")

    def test_cli_follow_json_transform_parses_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = (
                self.python_command("import json; print(json.dumps({'answer': 42}))")
                @ cli_follow
                | asjson
            )

        self.assertEqual(result, {"answer": 42})

    def test_app_exec_works_with_cli_follow(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = (
                "-c 'import json; print(json.dumps({\"answer\": 42}))'"
                @ AppExec(sys.executable, cli_follow)
                | asjson
            )

        self.assertEqual(result, {"answer": 42})

    def test_cli_follow_is_configurable(self):
        progress = RecordingProgress()
        with contextlib.redirect_stdout(io.StringIO()):
            result = (
                self.python_command("import sys; print('out'); print('err', file=sys.stderr)")
                @ cli_follow(progress=progress)
                | stdout_stderr
            )

        self.assertEqual(result, ("out", "err"))
        self.assertCountEqual(progress.lines, ["out", "err"])
        self.assertTrue(progress.finished_called)

    def test_cli_command_accepts_pipe_input_on_stdin(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = "test" | ("cat" @ cli) | stdout

        self.assertEqual(result, "test")

    def test_cli_command_can_pipe_input_to_stdin_consuming_command(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = "test" | ("xargs echo" @ cli) | stdout

        self.assertEqual(result, "test")

    def test_cli_exec_stderr_transform_returns_stderr(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.python_command("import sys; print('error', file=sys.stderr)") @ cli | stderr

        self.assertEqual(result, "error")

    def test_cli_exec_stdout_stderr_transform_returns_tuple(self):
        command = self.python_command("import sys; print('out'); print('err', file=sys.stderr)")
        with contextlib.redirect_stdout(io.StringIO()):
            result = command @ cli | stdout_stderr

        self.assertEqual(result, ("out", "err"))

    def test_cli_exec_json_transform_parses_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.python_command("import json; print(json.dumps({'answer': 42}))") @ cli | asjson

        self.assertEqual(result, {"answer": 42})

    def test_cli_exec_raise_on_error_transform_returns_result_on_success(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.python_command("import sys; sys.stdout.write('ok')") @ cli | raise_on_error

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "ok")

    def test_cli_exec_raise_on_error_transform_raises_on_failure(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(Exception) as ctx:
                "exit 5" @ cli | raise_on_error

        self.assertIn("exit code 5", str(ctx.exception))

    def test_cli_exec_logged_transform_logs_stdout_and_stderr(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with mock.patch("logging.info") as info:
                result = (
                    self.python_command("import sys; print('out'); print('err', file=sys.stderr)")
                    @ cli(log_command=False)
                    | logged
                )

        self.assertEqual(result.exit_code, 0)
        self.assertEqual([call.args[0] for call in info.call_args_list], ["out", "err"])

    def test_cli_exec_logged_transform_logs_failed_command_output_and_preserves_result(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with mock.patch("logging.info") as info:
                result = (
                    self.python_command("import sys; print('out'); print('err', file=sys.stderr); sys.exit(7)")
                    @ cli(log_command=False)
                    | logged
                )

        self.assertEqual(result.exit_code, 7)
        self.assertEqual([call.args[0] for call in info.call_args_list], ["out", "err"])

    def test_cli_exec_exit_code_transform_returns_nonzero_exit_code(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = "exit 7" @ cli(log_command=False) | exit_code

        self.assertEqual(result, 7)

    def test_cli_result_transforms_can_be_composed(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with mock.patch("logging.info") as info:
                result = self.python_command("print('out')") @ cli(log_command=False) | (logged | exit_code)

        self.assertEqual(result, 0)
        info.assert_called_once_with("out")

    def test_command_environment_runs_command_with_matmul_operator(self):
        code = "import json, os\nprint(json.dumps({'value': os.environ['COMMANDLINE_TEST_VALUE'], 'cwd': os.getcwd()}))\n"
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            with contextlib.redirect_stdout(io.StringIO()):
                with CommandEnvironment(env={"COMMANDLINE_TEST_VALUE": "ok"}, cwd=tmpdir) as env:
                    result = (self.python_command(code) @ env).result()

        self.assertEqual(result.exit_code, 0)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["value"], "ok")
        self.assertEqual(payload["cwd"], tmpdir)

    def test_command_environment_object_runs_command_with_matmul_operator(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.python_command("import sys; sys.stdout.write('hello')") @ CommandEnvironment()

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "hello")

    def test_command_environment_stdout_transform_returns_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with CommandEnvironment() as env:
                result = self.python_command("import sys; sys.stdout.write('hello')") @ env | stdout

        self.assertEqual(result, "hello")

    def test_command_environment_accepts_pipe_input_on_stdin(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with CommandEnvironment() as env:
                result = "test" | ("cat" @ env) | stdout

        self.assertEqual(result, "test")

    def test_command_environment_object_json_transform_parses_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = (
                self.python_command("import json; print(json.dumps({'answer': 42}))")
                @ CommandEnvironment()
                | asjson
            )

        self.assertEqual(result, {"answer": 42})

    def test_is_devops_reflects_environment(self):
        is_devops.cache_clear()
        with mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("TF_BUILD", None)
            self.assertFalse(is_devops())

        is_devops.cache_clear()
        with mock.patch.dict(os.environ, {"TF_BUILD": "1"}, clear=False):
            self.assertTrue(is_devops())
        is_devops.cache_clear()


class TestAsyncCommandlineFunctions(_CommandlineTestMixin, unittest.IsolatedAsyncioTestCase):
    async def test_acmd_runs_command_with_env_and_cwd(self):
        code = (
            "import json, os\nprint(json.dumps({'value': os.environ['COMMANDLINE_TEST_VALUE'], 'cwd': os.getcwd()}))\n"
        )
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            with contextlib.redirect_stdout(io.StringIO()):
                result = await acmd(
                    self.python_command(code),
                    env={"COMMANDLINE_TEST_VALUE": "ok"},
                    cwd=tmpdir,
                )

        self.assertEqual(result.exit_code, 0)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["value"], "ok")
        self.assertEqual(payload["cwd"], tmpdir)

    async def test_cli_exec_runs_command_inside_running_event_loop(self):
        code = (
            "import json, os\nprint(json.dumps({'value': os.environ['COMMANDLINE_TEST_VALUE'], 'cwd': os.getcwd()}))\n"
        )
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            with contextlib.redirect_stdout(io.StringIO()):
                result = (
                    self.python_command(code) @ cli(
                        env={"COMMANDLINE_TEST_VALUE": "ok"},
                        cwd=tmpdir,
                    )
                ).result()

        self.assertEqual(result.exit_code, 0)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["value"], "ok")
        self.assertEqual(payload["cwd"], tmpdir)

    async def test_acli_command_is_awaitable_and_transformable_after_await(self):
        with contextlib.redirect_stdout(io.StringIO()):
            command = 'echo "test"' @ acli
            result = (await command) | stdout

        self.assertEqual(result, "test")

    async def test_acli_transformed_command_is_awaitable(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await ('echo "test"' @ acli | stdout)

        self.assertEqual(result, "test")

    async def test_acli_follow_transformed_command_is_awaitable(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await ('echo "test"' @ acli_follow | stdout)

        self.assertEqual(result, "test")

    async def test_acli_follow_json_transform_parses_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await (
                self.python_command("import json; print(json.dumps({'answer': 42}))")
                @ acli_follow
                | asjson
            )

        self.assertEqual(result, {"answer": 42})

    async def test_app_exec_works_with_acli_follow(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await (
                "-c 'import json; print(json.dumps({\"answer\": 42}))'"
                @ AppExec(sys.executable, acli_follow)
                | asjson
            )

        self.assertEqual(result, {"answer": 42})

    async def test_acli_follow_commands_can_run_with_gather(self):
        with contextlib.redirect_stdout(io.StringIO()):
            first, second = await asyncio.gather(
                self.python_command("import sys; sys.stdout.write('one')") @ acli_follow,
                self.python_command("import sys; sys.stdout.write('two')") @ acli_follow,
            )

        self.assertEqual(first | stdout, "one")
        self.assertEqual(second | stdout, "two")

    async def test_acli_commands_can_run_with_gather(self):
        with contextlib.redirect_stdout(io.StringIO()):
            first, second = await asyncio.gather(
                self.python_command("import sys; sys.stdout.write('one')") @ acli,
                self.python_command("import sys; sys.stdout.write('two')") @ acli,
            )

        self.assertEqual(first | stdout, "one")
        self.assertEqual(second | stdout, "two")

    async def test_acli_is_configurable(self):
        code = (
            "import json, os\nprint(json.dumps({'value': os.environ['COMMANDLINE_TEST_VALUE'], 'cwd': os.getcwd()}))\n"
        )
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            with contextlib.redirect_stdout(io.StringIO()):
                result = await (
                    self.python_command(code) @ acli(
                        env={"COMMANDLINE_TEST_VALUE": "ok"},
                        cwd=tmpdir,
                    )
                )

        self.assertEqual(result.exit_code, 0)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["value"], "ok")
        self.assertEqual(payload["cwd"], tmpdir)

    async def test_acli_command_accepts_pipe_input_on_stdin(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await ("test" | ("cat" @ acli) | stdout)

        self.assertEqual(result, "test")

    async def test_acli_logged_transform_logs_stdout_and_stderr(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with mock.patch("logging.info") as info:
                result = await (
                    self.python_command("import sys; print('out'); print('err', file=sys.stderr)")
                    @ acli(log_command=False)
                    | logged
                )

        self.assertEqual(result.exit_code, 0)
        self.assertEqual([call.args[0] for call in info.call_args_list], ["out", "err"])

    async def test_acli_exit_code_transform_returns_nonzero_exit_code(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await ("exit 7" @ acli(log_command=False) | exit_code)

        self.assertEqual(result, 7)

    async def test_app_exec_prepends_app_to_string_command(self):
        code = "import sys; sys.stdout.write('hello')"
        with contextlib.redirect_stdout(io.StringIO()):
            result = await (f"-c {shlex.quote(code)}" @ AppExec(sys.executable) | stdout)

        self.assertEqual(result, "hello")

    async def test_app_exec_prepends_app_to_list_command(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await (
                ["-c", "import sys; sys.stdout.write(sys.argv[1])", "hello world"]
                @ AppExec(sys.executable)
                | stdout
            )

        self.assertEqual(result, "hello world")

    async def test_app_exec_is_configurable(self):
        code = "import json, os\nprint(json.dumps({'value': os.environ['COMMANDLINE_TEST_VALUE'], 'cwd': os.getcwd()}))\n"
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            with contextlib.redirect_stdout(io.StringIO()):
                result = await (
                    ["-c", code]
                    @ AppExec(sys.executable)(
                        env={"COMMANDLINE_TEST_VALUE": "ok"},
                        cwd=tmpdir,
                    )
                )

        self.assertEqual(result.exit_code, 0)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["value"], "ok")
        self.assertEqual(payload["cwd"], tmpdir)

    async def test_command_environment_runs_command_inside_running_event_loop(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.python_command("import sys; sys.stdout.write('hello')") @ CommandEnvironment()

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "hello")

    async def test_acmd_json_returns_json(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await acmd_json(self.python_command("import json; print(json.dumps({'answer': 42}))"))

        self.assertEqual(result, {"answer": 42})

    async def test_acmd_json_raises_when_command_fails(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(Exception) as ctx:
                await acmd_json(self.python_command("import sys; print('{\"answer\": 42}'); sys.exit(3)"))

        self.assertIn("exit code 3", str(ctx.exception))

    async def test_acmd_json_raises_when_stdout_is_not_json(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(json.JSONDecodeError):
                await acmd_json(self.python_command("print('not-json')"))

    async def test_acmd_stdout_returns_stdout(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = await acmd_stdout(self.python_command("import sys; sys.stdout.write('hello')"))

        self.assertEqual(result, "hello")

    async def test_acmd_stdout_raises_on_error(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(Exception) as ctx:
                await acmd_stdout(self.python_command("import sys; sys.stderr.write('boom'); sys.exit(4)"))

        self.assertIn("exit code 4", str(ctx.exception))

    async def test_acmd_raise_raises_on_error(self):
        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(Exception) as ctx:
                await acmd_raise(self.python_command("import sys; sys.exit(5)"))

        self.assertIn("exit code 5", str(ctx.exception))

    async def test_acmd_with_retry_retries_until_success(self):
        with tempfile.TemporaryDirectory(prefix="commandline_test_") as tmpdir:
            marker = os.path.join(tmpdir, "retry-marker")
            code = (
                "import os, sys\n"
                f"marker = {marker!r}\n"
                "if os.path.exists(marker):\n"
                "    sys.stdout.write('success')\n"
                "    sys.exit(0)\n"
                "open(marker, 'w').close()\n"
                "sys.exit(1)\n"
            )
            with contextlib.redirect_stdout(io.StringIO()):
                result = await acmd_with_retry(
                    self.python_command(code),
                    max_retry_attempts=2,
                    backoff_seconds=0,
                )

        self.assertIsNotNone(result)
        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "success")

    async def test_acmd_falls_back_to_wait_when_returncode_is_unset(self):
        proc = mock.Mock()
        proc.communicate = mock.AsyncMock(return_value=(b"hello", b""))
        proc.wait = mock.AsyncMock(return_value=0)
        proc.returncode = None

        with (
            mock.patch("asyncio.create_subprocess_shell", new=mock.AsyncMock(return_value=proc)),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            result = await acmd("echo hello")

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "hello")
        proc.wait.assert_awaited_once()

    async def test_add_prefix_and_remove_prefix_track_longest_prefix(self):
        global longest_prefix
        original_prefixes = list(current_prefixes)
        original_longest_prefix = longest_prefix
        current_prefixes.clear()
        longest_prefix = 0

        try:
            await add_prefix("ab")
            await add_prefix("longer")
            self.assertEqual(current_prefixes, ["ab", "longer"])
            self.assertEqual(longest_prefix, 6)

            await remove_prefix("ab")
            self.assertEqual(current_prefixes, ["longer"])
            self.assertEqual(longest_prefix, 6)

            await remove_prefix("longer")
            self.assertEqual(current_prefixes, [])
            self.assertEqual(longest_prefix, 0)
        finally:
            current_prefixes.clear()
            current_prefixes.extend(original_prefixes)
            longest_prefix = original_longest_prefix

    async def test_remove_prefix_raises_for_unknown_prefix(self):
        global longest_prefix
        original_prefixes = list(current_prefixes)
        original_longest_prefix = longest_prefix
        current_prefixes.clear()
        longest_prefix = 0

        try:
            with self.assertRaises(ValueError):
                await remove_prefix("missing")
        finally:
            current_prefixes.clear()
            current_prefixes.extend(original_prefixes)
            longest_prefix = original_longest_prefix

    async def test_asingleton_only_initializes_once(self):
        call_count = 0
        sentinel = object()

        async def create_value():
            nonlocal call_count
            call_count += 1
            await asyncio.sleep(0.01)
            return sentinel

        singleton = asingleton(create_value)
        result_a, result_b, result_c = await asyncio.gather(singleton(), singleton(), singleton())

        self.assertIs(result_a, sentinel)
        self.assertIs(result_b, sentinel)
        self.assertIs(result_c, sentinel)
        self.assertEqual(call_count, 1)


class TestAcmdFollow(_CommandlineTestMixin, unittest.IsolatedAsyncioTestCase):
    async def test_acmd_follow_streams_stdout_stderr_and_preserves_partial_stdout(self):
        code = (
            "import sys, time\n"
            "print('stdout line', flush=True)\n"
            "time.sleep(0.05)\n"
            "print('stderr line', file=sys.stderr, flush=True)\n"
            "sys.stdout.write('tail')\n"
            "sys.stdout.flush()\n"
        )
        progress = RecordingProgress()
        command = self.python_command(code)

        with contextlib.redirect_stdout(io.StringIO()):
            result = await acmd_follow(command, progress=progress)

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "stdout line\ntail")
        self.assertEqual(result.stderr, "stderr line\n")
        self.assertEqual(progress.lines, ["stdout line", "stderr line"])
        self.assertTrue(progress.finished_called)

    async def test_acmd_follow_raise_raises_on_error(self):
        command = self.python_command("import sys; print('boom', file=sys.stderr); sys.exit(2)")

        with contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaises(Exception) as ctx:
                await acmd_follow_raise(command)

        self.assertIn("exit code 2", str(ctx.exception))

    async def test_acmd_follow_uses_wait_result_when_returncode_is_unset(self):
        proc = mock.Mock()
        proc.stdout = mock.Mock()
        proc.stderr = mock.Mock()
        proc.stdout.read = mock.AsyncMock(side_effect=[b"out\n", b""])
        proc.stderr.read = mock.AsyncMock(side_effect=[b"", b""])
        proc.wait = mock.AsyncMock(return_value=0)
        proc.returncode = None

        with (
            mock.patch("asyncio.create_subprocess_shell", new=mock.AsyncMock(return_value=proc)),
            contextlib.redirect_stdout(io.StringIO()),
        ):
            result = await acmd_follow("echo hello")

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.stdout, "out\n")
        self.assertEqual(result.stderr, "")
        self.assertGreaterEqual(proc.wait.await_count, 1)
