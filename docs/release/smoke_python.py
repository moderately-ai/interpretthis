# Copyright 2026 Thomas Santerre and Moderately AI Inc.
#
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Release acceptance smoke for the `interpretthis` wheel.

Dependency-free on purpose — no pytest. This runs inside a throwaway venv holding
*only* the freshly built wheel, so it is the last thing standing between a built
artifact and PyPI. If it needed a test framework it could not run there.

It refuses to pass if it imported the source tree instead of the installed wheel:
a smoke test that silently exercised `python/interpretthis/` would prove nothing
about the artifact, and would do so quietly.

Usage:  python docs/release/smoke_python.py
"""

from __future__ import annotations

import asyncio
import sys
from decimal import Decimal
from pathlib import Path

import interpretthis
from interpretthis import Config, Interpreter, LimitExceededError, SecurityError, Tool


def fail(message: str) -> None:
    print(f"SMOKE FAILED: {message}", file=sys.stderr)
    raise SystemExit(1)


def check_not_the_source_tree() -> None:
    """Prove we imported the installed wheel, not the repo."""
    module_path = Path(interpretthis.__file__).resolve()
    repo_root = Path(__file__).resolve().parent.parent.parent
    source_package = (
        repo_root / "crates" / "interpretthis-python" / "python" / "interpretthis"
    ).resolve()

    if module_path == source_package or source_package in module_path.parents:
        fail(
            f"imported interpretthis from the source tree ({module_path}); "
            "the smoke must exercise the installed wheel"
        )


def check_metadata() -> None:
    if not isinstance(interpretthis.__version__, str):
        fail("__version__ is not a string")
    if not isinstance(interpretthis.STATE_FORMAT_VERSION, int):
        fail("STATE_FORMAT_VERSION is not an int")


def check_execute_and_variables() -> None:
    interp = Interpreter()
    result = interp.execute("total = sum(range(n))\nprint(total)", {"n": 10})
    result.check()
    if result.stdout != "45\n":
        fail(f"unexpected stdout: {result.stdout!r}")
    if interp.get_variable("total") != 45:
        fail("get_variable did not round-trip")


def check_partial_stdout_survives_failure() -> None:
    result = Interpreter().execute("print('printed')\nraise ValueError('boom')")
    if result.ok:
        fail("expected the script to fail")
    if result.stdout != "printed\n":
        fail(f"partial stdout was lost: {result.stdout!r}")


def check_sync_tool() -> None:
    interp = Interpreter(tools={"double": lambda n: n * 2})
    result = interp.execute("print(double(n=21))")
    if result.check().stdout != "42\n":
        fail("sync tool did not run")


def check_async_tool() -> None:
    """A coroutine tool driven from the blocking entry point.

    The load-bearing one: with no caller event loop anywhere, the interpreter has
    to start its own to drive this. A regression here hangs rather than fails.
    """

    async def fetch(key: str) -> str:
        await asyncio.sleep(0)
        return f"value-for-{key}"

    interp = Interpreter(tools={"fetch": fetch})
    result = interp.execute("print(fetch(key='a'))")
    if result.check().stdout != "value-for-a\n":
        fail("async tool did not run on the sync path")


def check_parallel_tools() -> None:
    async def slow(n: int) -> int:
        await asyncio.sleep(0.02)
        return n

    interp = Interpreter(tools={"slow": Tool(slow, parallelizable=True)})
    result = interp.execute("print(slow(n=1) + slow(n=2))")
    if result.check().stdout != "3\n":
        fail("parallelizable tools did not resolve")


def check_execute_async() -> None:
    async def main() -> str:
        caller_loop = asyncio.get_running_loop()
        seen: list[asyncio.AbstractEventLoop] = []

        async def probe() -> str:
            seen.append(asyncio.get_running_loop())
            return "ok"

        interp = Interpreter(tools={"probe": probe})
        result = await interp.execute_async("print(probe())")
        # Tool coroutines must run on the CALLER's loop, so a tool may await
        # objects bound to it.
        if seen != [caller_loop]:
            fail("tool coroutine did not run on the caller's event loop")
        return result.check().stdout

    if asyncio.run(main()) != "ok\n":
        fail("execute_async did not run")


def check_value_round_trips() -> None:
    cases: list[object] = [
        None,
        True,
        2**200,  # must stay exact; a JSON boundary would stringify it
        3.5,
        "text",
        b"bytes",
        [1, [2, 3]],
        (1, 2),
        {"k": [1, 2]},
        Decimal("0.1"),
    ]
    interp = Interpreter()
    for value in cases:
        interp.execute("out = value", {"value": value}).check()
        if interp.get_variable("out") != value:
            fail(f"value did not round-trip: {value!r}")


def check_lists_are_copied_not_aliased() -> None:
    host_list = [1, 2, 3]
    Interpreter().execute("data.append(4)", {"data": host_list}).check()
    if host_list != [1, 2, 3]:
        fail("the sandbox mutated the caller's list")


def check_sandbox_boundary() -> None:
    if Interpreter().execute("x = ().__class__").error.__class__ is not SecurityError:
        fail("introspection escape was not refused")
    if Interpreter().execute("import socket").ok:
        fail("a non-allowlisted import was permitted")


def check_limits() -> None:
    interp = Interpreter(config=Config(max_operations=1_000))
    result = interp.execute("for i in range(1_000_000):\n    pass")
    if not isinstance(result.error, LimitExceededError):
        fail(f"operation limit not enforced, got {result.error!r}")


def check_state_round_trip() -> None:
    first = Interpreter()
    first.execute("counter = 41").check()
    blob = first.export_state()

    second = Interpreter()
    second.import_state(blob)
    second.execute("counter = counter + 1").check()
    if second.get_variable("counter") != 42:
        fail("state did not survive export/import")


def check_reentrancy_is_refused() -> None:
    """A nested execute must raise, not hang.

    The interpreter holds its state lock across the whole run, and it is not
    reentrant — without the guard this deadlocks silently.
    """
    interp = Interpreter()

    def reenter() -> str:
        interp.execute("x = 1")
        return "unreachable"

    result = interp.execute("print(reenter())", tools={"reenter": reenter})
    if result.ok or "already running" not in str(result.error):
        fail(f"reentrant execute was not refused, got {result.error!r}")


def main() -> None:
    check_not_the_source_tree()
    check_metadata()
    check_execute_and_variables()
    check_partial_stdout_survives_failure()
    check_sync_tool()
    check_async_tool()
    check_parallel_tools()
    check_execute_async()
    check_value_round_trips()
    check_lists_are_copied_not_aliased()
    check_sandbox_boundary()
    check_limits()
    check_state_round_trip()
    check_reentrancy_is_refused()

    print(f"SMOKE OK (interpretthis {interpretthis.__version__})")


if __name__ == "__main__":
    main()
