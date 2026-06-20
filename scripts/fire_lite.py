import asyncio
import contextlib
import inspect
from collections.abc import Callable, Coroutine, Mapping
from typing import Any
import io
import os
import re
import shlex
import shutil
import sys
import traceback
import unittest

try:
    from .commandline import ConsoleColor as c
except ImportError:
    from commandline import ConsoleColor as c


def alias(names: list[str]):
    def decorator(func):
        existing = getattr(func, "__fire_lite_aliases__", ())
        setattr(func, "__fire_lite_aliases__", tuple(existing) + tuple(names))
        return func

    return decorator


class FireLite:
    __slots__ = ["classes"]
    _ROOT_KEY = "__fire_lite_root__"

    def __init__(self, classes: list[tuple[list[str], object]]):
        self.classes = classes

    def _method_map(self) -> dict[str, tuple[list[str], object, Mapping[str, inspect.Parameter], Callable[..., Coroutine[Any, Any, Any]]]]:
        async_methods = [
            (path, method, cls)
            for path, cls in self.classes
            for method in dir(cls)
            if inspect.iscoroutinefunction(getattr(cls, method)) and not method.startswith("_")
        ]

        methods = {}
        for path, method, cls in async_methods:
            func = getattr(cls, method)
            signature = inspect.signature(func)
            func_attr_source = getattr(func, "__func__", func)
            command_names = [
                method.replace("_", "-"),
                *getattr(func_attr_source, "__fire_lite_aliases__", ()),
            ]
            for command_name in command_names:
                command_path = path + command_name.replace("_", "-").split()
                method_key = " ".join(command_path)
                if method_key in methods:
                    raise ValueError(f"Duplicate FireLite command: {method_key}")
                methods[method_key] = (command_path, cls, signature.parameters, func)
        return methods

    def _bash_completion_script(self, executable_name: str | None = None) -> str:
        method_map = self._method_map()
        command_children: dict[str, set[str]] = {self._ROOT_KEY: {"completions"}}
        command_options: dict[str, list[str]] = {}

        for command_name, (path, _, params, _) in method_map.items():
            for i, token in enumerate(path):
                parent_key = " ".join(path[:i]) if i > 0 else self._ROOT_KEY
                command_children.setdefault(parent_key, set()).add(token)
            option_names = []
            for param_name, param in params.items():
                if param.kind == inspect.Parameter.VAR_POSITIONAL:
                    continue
                option_name = param_name.replace("_", "-")
                if param.annotation == bool:
                    option_names.append(f"--{option_name}")
                else:
                    option_names.append(f"--{option_name}=")
            command_options[command_name] = sorted(option_names)

        executable_name = executable_name or os.environ.get("FIRE_LITE_EXECUTABLE") or sys.argv[0] or "repo"
        executable_name = executable_name or "repo"
        executable_path = (
            os.path.realpath(executable_name)
            if "/" in executable_name
            else shutil.which(executable_name) or executable_name
        )
        executable_path = os.path.realpath(executable_path)
        registration_targets = []
        for target in [executable_name, executable_path]:
            if target and target not in registration_targets:
                registration_targets.append(target)
        completion_name = executable_name.split("/")[-1] or "repo"
        function_name = "_" + re.sub(r"\W+", "_", executable_name).strip("_")
        if not function_name or function_name == "_":
            function_name = "_fire_lite"
        function_name += "_completion"

        lines = [
            "# bash completion for FireLite commands",
            f"_fire_lite_target={shlex.quote(executable_path)}",
            "declare -A _fire_lite_children=()",
            "declare -A _fire_lite_options=()",
        ]

        for command_key in sorted(command_children.keys()):
            children = " ".join(sorted(command_children[command_key]))
            lines.append(f"_fire_lite_children[{shlex.quote(command_key)}]={shlex.quote(children)}")
        for command_key in sorted(command_options.keys()):
            options = " ".join(command_options[command_key])
            lines.append(f"_fire_lite_options[{shlex.quote(command_key)}]={shlex.quote(options)}")

        lines.extend(
            [
                f"{function_name}() {{",
                "    local cur command_key path_key word suggestions",
                "    COMPREPLY=()",
                '    cur="${COMP_WORDS[COMP_CWORD]}"',
                "    local resolved_cmd=",
                f"    local root_key={shlex.quote(self._ROOT_KEY)}",
                '    if [[ "${COMP_WORDS[0]}" == */* ]]; then',
                '        resolved_cmd="$(cd -- "$(dirname -- "${COMP_WORDS[0]}")" 2>/dev/null && printf \'%s/%s\' "$(pwd -P)" "$(basename -- "${COMP_WORDS[0]}")")"',
                "    else",
                '        resolved_cmd="$(command -v -- "${COMP_WORDS[0]}" 2>/dev/null)"',
                "    fi",
                '    if [[ -z "$resolved_cmd" || "$(readlink -f -- "$resolved_cmd" 2>/dev/null || printf \'%s\' "$resolved_cmd")" != "$_fire_lite_target" ]]; then',
                "        return 0",
                "    fi",
                '    if [[ ${#COMP_WORDS[@]} -ge 2 && "${COMP_WORDS[1]}" == "completions" ]]; then',
                "        if [[ $COMP_CWORD -eq 2 ]]; then",
                '            COMPREPLY=( $(compgen -W "bash" -- "$cur") )',
                "        fi",
                "        return 0",
                "    fi",
                "    local -a path_tokens=()",
                "    local i",
                "    for ((i=1; i<COMP_CWORD; i++)); do",
                '        word="${COMP_WORDS[i]}"',
                '        [[ "$word" == --* ]] && continue',
                '        path_key="${path_tokens[*]:-$root_key}"',
                '        if [[ -n "${_fire_lite_children[$path_key]:-}" && " ${_fire_lite_children[$path_key]} " == *" $word "* ]]; then',
                '            path_tokens+=("$word")',
                "            continue",
                "        fi",
                "        break",
                "    done",
                '    command_key="${path_tokens[*]:-$root_key}"',
                '    suggestions="${_fire_lite_children[$command_key]:-}"',
                '    if [[ -n "${_fire_lite_options[$command_key]:-}" ]]; then',
                '        suggestions="$suggestions ${_fire_lite_options[$command_key]}"',
                "    fi",
                "    if [[ $COMP_CWORD -eq 1 ]]; then",
                '        suggestions="$suggestions -h --help help --skillsmd completions"',
                "    fi",
                '    COMPREPLY=( $(compgen -W "$suggestions" -- "$cur") )',
                "    return 0",
                "}",
                f"# completion targets for {completion_name}",
            ]
        )
        for target in registration_targets:
            lines.append(f"complete -F {function_name} {shlex.quote(target)}")
        return "\n".join(lines)

    def dispatchArgs(self, cmdargs: list[str]) -> int:
        m = self._method_map()

        def print_help():
            print(f"{c.GREEN}Usage: repo <method> [args...] [--<arg1>=<value1> --<arg2>=<value2> ...]{c.RESET}")
            for short, (path, cls, params, func) in m.items():
                doc = func.__doc__.strip() if func.__doc__ else "No help message"
                print(f"{c.YELLOW}{' '.join(path)}{c.RESET}: {doc}")
                for param, value in params.items():
                    if value.kind == inspect.Parameter.VAR_POSITIONAL:
                        print(f"  {c.BLUE}{param}...{c.RESET}={value}")
                        continue
                    print(f"  {c.BLUE}--{param.replace('_', '-')}{c.RESET}={value}")
            print(f"{c.GREEN}Bash completions{c.RESET}:")
            print(f"  {c.YELLOW}repo completions bash{c.RESET}: print the bash completion script")
            print("  To enable completions in the current shell:")
            print(f"    {c.BLUE}source ./repo completions bash{c.RESET}")
            print(f"    {c.BLUE}source <(./repo completions bash){c.RESET}")

        def generate_skills_md_on_stdout():
            print("---")
            print(f"name: drepo")
            print(
                f"description: The ./drepo tool is a command line interface for running various tasks related to the development and maintenance of the ms-sora project. It provides a convenient way to run tests, manage docker images, and perform other common tasks without having to remember complex commands or navigate through multiple directories. The drepo tool is designed to be extensible, allowing developers to easily add new commands and functionality as needed.\n"
            )
            print("---")

            print("# Core functionality\n")

            for short, (path, cls, params, func) in m.items():
                doc = func.__doc__.strip() if func.__doc__ else "No help message"
                print(f"## ./drepo {' '.join(path)}\n")
                print(f"{doc}\n")
                if len(params) > 0:
                    print("### Arguments\n")
                    for param, value in params.items():
                        prefix = "" if value.kind == inspect.Parameter.VAR_POSITIONAL else "--"
                        suffix = "..." if value.kind == inspect.Parameter.VAR_POSITIONAL else ""
                        print(f"- `{prefix}{param}{suffix}`: {value}\n")

        if len(cmdargs) == 0 or cmdargs[0] in set(["-h", "help", "--help"]):
            print_help()
            return 0
        if cmdargs[0] in set(["--skillsmd"]):
            generate_skills_md_on_stdout()
            return 0
        if cmdargs[0] == "completions":
            if len(cmdargs) != 2 or cmdargs[1] != "bash":
                print("Usage: repo completions bash")
                return 1
            print(self._bash_completion_script())
            return 0
        elif len(cmdargs) > 0:
            method = None
            method_arg_count = 0
            for i in range(len(cmdargs), 0, -1):
                candidate = " ".join(arg.replace("_", "-") for arg in cmdargs[:i])
                if candidate in m:
                    method = candidate
                    method_arg_count = i
                    break
            if method not in m:
                print(f"method {' '.join(cmdargs)} not available, showing help:")
                print_help()
                return 1
            else:
                _, cls, params, func = m[method]

            # go through all args, split them into a list of args and kwargs
            m_args = {}
            positional_args = []
            remaining_args = cmdargs[method_arg_count:]
            if remaining_args and remaining_args[0] in {"-h", "--help", "help"}:
                print_help()
                return 0
            positional_params = [
                name
                for name, param in params.items()
                if param.kind
                in (
                    inspect.Parameter.POSITIONAL_ONLY,
                    inspect.Parameter.POSITIONAL_OR_KEYWORD,
                )
                and param.default is inspect.Parameter.empty
            ]
            var_positional_param = next(
                (param for param in params.values() if param.kind == inspect.Parameter.VAR_POSITIONAL),
                None,
            )
            option_params = {
                k: v
                for k, v in params.items()
                if v.kind != inspect.Parameter.VAR_POSITIONAL
            }
            i = 0
            while i < len(remaining_args):
                arg = remaining_args[i]
                if arg.startswith("--"):
                    key_value = arg.split("=", 1)
                    key = key_value[0].lstrip("--").replace("-", "_")
                    if var_positional_param is not None and key not in option_params:
                        positional_args.append(arg)
                        i += 1
                        continue
                    if len(key_value) == 2:
                        m_args[key] = key_value[1]
                    elif i + 1 < len(remaining_args) and not remaining_args[i + 1].startswith("--"):
                        m_args[key] = remaining_args[i + 1]
                        i += 1
                    else:
                        m_args[key] = True
                else:
                    positional_args.append(arg)
                i += 1

            for param_name, arg in zip(positional_params, positional_args):
                m_args[param_name] = arg
            positional_args = positional_args[len(positional_params) :]
            if positional_args and var_positional_param is None:
                print(f"Unexpected positional arguments for method '{method}': {' '.join(positional_args)}")
                print_help()
                return 1

            # check if each arg actually exists on the method
            for k in m_args.keys():
                if k not in option_params:
                    print(f"Argument '{k}' not found in method '{method}'")
                    print_help()
                    return 1

            # convert the args to the correct type
            for k, v in m_args.items():
                if params[k].annotation == bool:
                    m_args[k] = str(v).lower() in set(["true", "1", "yes"])
                elif params[k].annotation == int:
                    m_args[k] = int(v)
                elif params[k].annotation == float:
                    m_args[k] = float(v)
                elif params[k].annotation == str:
                    m_args[k] = v.strip()
                elif params[k].annotation in (set[str], set[str] | None):
                    m_args[k] = {w.strip() for w in v.split(",")}
                elif params[k].annotation == list[str]:
                    m_args[k] = [w.strip() for w in v.split(",")]

            try:
                ret_code = asyncio.run(func(*positional_args, **m_args))
                if isinstance(ret_code, int):
                    return ret_code
                return 0
            except Exception as e:
                print(f"Error executing method '{method}': {str(e)}")
                traceback.print_exc()
                return 1
        return 0


class _RootTestTool:
    def __init__(self):
        self.last_call = None

    @alias(["compile"])
    async def build(self, name: str, force: bool = False, count: int = 1) -> int:
        """Build a named target."""
        self.last_call = {"name": name, "force": force, "count": count}
        return 0

    async def collect(self, *items: str) -> int:
        """Collect positional items."""
        self.last_call = {"items": items}
        return 0

    async def sync_continue(self) -> int:
        """Continue syncing."""
        self.last_call = {"command": "sync_continue"}
        return 0


class _NestedTestTool:
    def __init__(self):
        self.last_call = None

    async def cleanup(self, dry_run: bool = False) -> int:
        self.last_call = {"dry_run": dry_run}
        return 0


class TestFireLite(unittest.TestCase):
    def setUp(self):
        self.root = _RootTestTool()
        self.nested = _NestedTestTool()
        self.fire = FireLite([([], self.root), (["resources"], self.nested)])

    def test_dispatch_args_runs_command_and_converts_arguments(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.fire.dispatchArgs(["build", "--name=api", "--force=true", "--count=2"])

        self.assertEqual(result, 0)
        self.assertEqual(self.root.last_call, {"name": "api", "force": True, "count": 2})

    def test_dispatch_args_maps_dashed_options_to_underscored_parameters(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.fire.dispatchArgs(["resources", "cleanup", "--dry-run"])

        self.assertEqual(result, 0)
        self.assertEqual(self.nested.last_call, {"dry_run": True})

    def test_dispatch_args_runs_alias(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.fire.dispatchArgs(["compile", "--name=api"])

        self.assertEqual(result, 0)
        self.assertEqual(self.root.last_call, {"name": "api", "force": False, "count": 1})

    def test_dispatch_args_supports_positional_varargs(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.fire.dispatchArgs(["collect", "one", "two"])

        self.assertEqual(result, 0)
        self.assertEqual(self.root.last_call, {"items": ("one", "two")})

    def test_dispatch_args_preserves_option_like_varargs(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.fire.dispatchArgs(["collect", "--force", "main"])

        self.assertEqual(result, 0)
        self.assertEqual(self.root.last_call, {"items": ("--force", "main")})

    def test_dispatch_args_maps_underscores_to_hyphens(self):
        with contextlib.redirect_stdout(io.StringIO()):
            result = self.fire.dispatchArgs(["sync-continue"])

        self.assertEqual(result, 0)
        self.assertEqual(self.root.last_call, {"command": "sync_continue"})

    def test_dispatch_args_outputs_bash_completions(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            original_argv = sys.argv
            try:
                sys.argv = ["repo"]
                result = self.fire.dispatchArgs(["completions", "bash"])
            finally:
                sys.argv = original_argv

        rendered = output.getvalue()
        self.assertEqual(result, 0)
        self.assertIn("_fire_lite_target=", rendered)
        self.assertIn('if [[ -z "$resolved_cmd"', rendered)
        self.assertIn("complete -F _repo_completion repo", rendered)
        self.assertIn("_fire_lite_children[__fire_lite_root__]='build collect compile completions resources sync-continue'", rendered)
        self.assertIn("_fire_lite_options[build]='--count= --force --name='", rendered)
        self.assertIn("_fire_lite_options['resources cleanup']=--dry-run", rendered)
        self.assertIn("_fire_lite_children[resources]=cleanup", rendered)

    def test_dispatch_args_rejects_unsupported_completion_shell(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = self.fire.dispatchArgs(["completions", "zsh"])

        self.assertEqual(result, 1)
        self.assertIn("Usage: repo completions bash", output.getvalue())
