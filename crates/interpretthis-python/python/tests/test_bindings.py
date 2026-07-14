# Copyright 2026 Thomas Santerre and Moderately AI Inc.
#
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Behaviour of the Python bindings, exercised against the built extension."""

from __future__ import annotations

import asyncio
import datetime as dt
import math
from decimal import Decimal
from fractions import Fraction
from typing import Any

import pytest

import interpretthis
from interpretthis import (
    Config,
    ExecutionResult,
    Interpreter,
    LimitExceededError,
    PythonException,
    SandboxNameError,
    SecurityError,
    StateFormatError,
    Tool,
    ToolError,
)

# --- basics ---------------------------------------------------------------


def test_executes_and_captures_stdout() -> None:
    result = Interpreter().execute("print('hello')")
    assert result.ok
    assert result.stdout == "hello\n"
    assert result.error is None


def test_injects_variables_and_reads_them_back() -> None:
    interp = Interpreter()
    result = interp.execute("total = x + y", {"x": 2, "y": 3})
    assert result.ok
    assert interp.get_variable("total") == 5
    assert "total" in interp.state_keys()


def test_version_and_state_format_version_are_exposed() -> None:
    assert isinstance(interpretthis.__version__, str)
    assert isinstance(interpretthis.STATE_FORMAT_VERSION, int)


# --- failure is data, not an exception ------------------------------------


def test_failure_returns_a_result_and_keeps_partial_stdout() -> None:
    # The whole reason execute() does not raise: a script that prints and *then*
    # fails has told you something useful in both halves, and that pair is what
    # gets fed back to a model.
    result = Interpreter().execute("print('before')\nboom")

    assert not result.ok
    assert result.stdout == "before\n"
    assert isinstance(result.error, SandboxNameError)
    assert "boom" in str(result.error)


def test_check_raises_the_typed_error() -> None:
    result = Interpreter().execute("undefined_name")
    with pytest.raises(SandboxNameError):
        result.check()


def test_check_returns_self_on_success_so_it_chains() -> None:
    assert Interpreter().execute("print('ok')").check().stdout == "ok\n"


def test_sandbox_errors_subclass_their_builtin_twin() -> None:
    # SandboxNameError really is a NameError, so `except NameError` works as the
    # name promises — that is why the classes live in Python.
    result = Interpreter().execute("nope")
    assert isinstance(result.error, NameError)
    assert isinstance(result.error, interpretthis.InterpretThisError)


def test_uncaught_script_exception_reports_its_type_name() -> None:
    result = Interpreter().execute("raise ValueError('bad input')")
    assert isinstance(result.error, PythonException)
    assert result.error.type_name == "ValueError"


# --- sandbox boundary -----------------------------------------------------


@pytest.mark.parametrize(
    "source", ["eval('1+1')", "exec('x=1')", "open('/etc/passwd')"]
)
def test_dangerous_builtins_do_not_exist(source: str) -> None:
    # A blocked *name* is not "forbidden", it is simply absent — so the sandbox
    # reports NameError, exactly as CPython would for any undefined name. There
    # is nothing to leak in the error message.
    result = Interpreter().execute(source)
    assert isinstance(result.error, SandboxNameError)


@pytest.mark.parametrize("source", ["x = ().__class__", "x = [].__class__.__bases__"])
def test_introspection_escape_attempts_are_a_security_error(source: str) -> None:
    # A blocked *attribute* is different: the object exists and the attribute is
    # refused. These are the class-walk chains an escape starts with, so they get
    # their own error class rather than being dressed up as an AttributeError.
    result = Interpreter().execute(source)
    assert isinstance(result.error, SecurityError)


def test_imports_outside_the_allowlist_are_rejected() -> None:
    result = Interpreter().execute("import socket")
    assert isinstance(result.error, PythonException)
    assert result.error.type_name == "ModuleNotFoundError"


def test_operation_limit_is_enforced() -> None:
    interp = Interpreter(config=Config(max_operations=1_000))
    result = interp.execute("for i in range(1_000_000):\n    pass")
    assert isinstance(result.error, LimitExceededError)


def test_recursion_limit_is_enforced() -> None:
    interp = Interpreter(config=Config(max_recursion_depth=10))
    result = interp.execute("def f():\n    return f()\nf()")
    assert not result.ok
    assert isinstance(result.error, RecursionError)


# --- tools ----------------------------------------------------------------


def test_sync_tool() -> None:
    def double(n: int) -> int:
        return n * 2

    interp = Interpreter(tools={"double": double})
    result = interp.execute("print(double(n=21))")
    assert result.check().stdout == "42\n"


def test_async_tool_on_the_sync_path() -> None:
    # A coroutine tool with no caller event loop anywhere: the interpreter has to
    # start its own background loop to drive it. If that were broken this hangs
    # rather than fails, hence the timeout on the suite.
    async def fetch(key: str) -> str:
        await asyncio.sleep(0)
        return f"value-for-{key}"

    interp = Interpreter(tools={"fetch": fetch})
    result = interp.execute("print(fetch(key='a'))")
    assert result.check().stdout == "value-for-a\n"


def test_sync_and_async_tools_together() -> None:
    def add(a: int, b: int) -> int:
        return a + b

    async def triple(n: int) -> int:
        await asyncio.sleep(0)
        return n * 3

    interp = Interpreter(tools={"add": add, "triple": triple})
    result = interp.execute("print(add(a=triple(n=2), b=1))")
    assert result.check().stdout == "7\n"


def test_per_call_tools_override_registered_ones() -> None:
    interp = Interpreter(tools={"who": lambda: "registered"})
    result = interp.execute("print(who())", tools={"who": lambda: "per-call"})
    assert result.check().stdout == "per-call\n"


def test_parallelizable_tools_run_concurrently() -> None:
    async def slow(n: int) -> int:
        await asyncio.sleep(0.05)
        return n

    interp = Interpreter(tools={"slow": Tool(slow, parallelizable=True)})
    result = interp.execute(
        "a = slow(n=1)\nb = slow(n=2)\nc = slow(n=3)\nprint(a + b + c)"
    )
    assert result.check().stdout == "6\n"


def test_tool_positional_args_arrive_as_arg0_arg1() -> None:
    def joiner(**kwargs: Any) -> str:
        return f"{kwargs['arg0']}-{kwargs['arg1']}"

    interp = Interpreter(tools={"joiner": joiner})
    assert Interpreter is not None
    result = interp.execute("print(joiner('a', 'b'))", tools={"joiner": joiner})
    assert result.check().stdout == "a-b\n"


def test_raising_tool_becomes_a_tool_error() -> None:
    def boom() -> None:
        raise RuntimeError("tool exploded")

    result = Interpreter(tools={"boom": boom}).execute("boom()")
    assert isinstance(result.error, ToolError)
    assert result.error.tool_name == "boom"
    assert "tool exploded" in str(result.error)


def test_a_script_can_catch_a_failing_tool() -> None:
    def boom() -> None:
        raise RuntimeError("tool exploded")

    interp = Interpreter(tools={"boom": boom})
    result = interp.execute("try:\n    boom()\nexcept Exception:\n    print('caught')")
    assert result.check().stdout == "caught\n"


def test_blocked_tool_name_is_rejected_at_construction() -> None:
    # Not at first call: a blocked name should fail loudly when you build the
    # interpreter, not silently do nothing until some script happens to call it.
    with pytest.raises(ValueError, match="dangerous builtin"):
        Interpreter(tools={"eval": lambda: None})


def test_non_callable_tool_is_rejected() -> None:
    with pytest.raises(ValueError, match="must be a callable"):
        Interpreter(tools={"nope": 42})  # type: ignore[dict-item]


# --- the deadlock guard ---------------------------------------------------


def test_reentrant_execute_raises_instead_of_deadlocking() -> None:
    # The interpreter holds its state lock across the entire run, including
    # across the await for a tool. That lock is not reentrant, so a nested
    # execute() would block forever — holding up the very tool whose completion
    # it is waiting on. No timeout fires and nothing is logged: it just hangs.
    # The guard converts that silent hang into a loud error.
    #
    # If this regresses, this test HANGS rather than failing, which is why the
    # suite carries a timeout.
    interp = Interpreter()

    def reenter() -> str:
        interp.execute("x = 1")  # must raise, not hang
        return "unreachable"

    result = interp.execute("print(reenter())", tools={"reenter": reenter})

    assert isinstance(result.error, ToolError)
    assert "already running" in str(result.error)


# --- value conversion -----------------------------------------------------


@pytest.mark.parametrize(
    "value",
    [
        None,
        True,
        False,
        0,
        -7,
        2**70,  # promotes past i64 and must stay exact
        -(2**70),
        3.5,
        "text",
        b"bytes",
        [1, "two", None],
        (1, 2),
        {"a": 1, "b": [2, 3]},
        {1: "int-key", "s": "str-key", (1, 2): "tuple-key"},
        Decimal("0.1"),
        Fraction(3, 4),
        dt.date(2026, 7, 13),
        dt.datetime(2026, 7, 13, 12, 30, 45),
        dt.time(12, 30, 45),
        dt.timedelta(days=1, seconds=30),
    ],
)
def test_values_round_trip_unchanged(value: object) -> None:
    interp = Interpreter()
    interp.execute("out = value", {"value": value}).check()
    assert interp.get_variable("out") == value


def test_sets_round_trip() -> None:
    interp = Interpreter()
    interp.execute("out = value", {"value": {1, 2, 3}}).check()
    assert interp.get_variable("out") == {1, 2, 3}


def test_big_ints_stay_exact() -> None:
    # to_json() would have stringified this. The native converter must not.
    big = 2**200 + 1
    interp = Interpreter()
    interp.execute("out = n * 1", {"n": big}).check()
    assert interp.get_variable("out") == big


def test_bool_is_not_silently_widened_to_int() -> None:
    # bool subclasses int in Python; an isinstance check in the wrong order turns
    # True into 1.
    interp = Interpreter()
    interp.execute("out = flag", {"flag": True}).check()
    assert interp.get_variable("out") is True


def test_aware_datetime_keeps_its_offset() -> None:
    aware = dt.datetime(2026, 7, 13, 12, 0, tzinfo=dt.timezone(dt.timedelta(hours=-5)))
    interp = Interpreter()
    interp.execute("out = when", {"when": aware}).check()
    assert interp.get_variable("out") == aware


def test_numeric_lookalike_is_not_silently_a_decimal() -> None:
    # A non-Decimal object with a numeric __str__ must not be reinterpreted as a
    # Decimal by the str-parse extractor — it is an unsupported inbound type.
    class Money:
        def __str__(self) -> str:
            return "1.50"

    interp = Interpreter()
    with pytest.raises(TypeError):
        interp.execute("out = m", {"m": Money()}).check()


def test_fraction_lookalike_is_not_silently_a_fraction() -> None:
    class Ratio:
        numerator = 3
        denominator = 4

    interp = Interpreter()
    with pytest.raises(TypeError):
        interp.execute("out = r", {"r": Ratio()}).check()


def test_real_decimal_and_fraction_still_round_trip() -> None:
    interp = Interpreter()
    interp.execute("out = d", {"d": Decimal("2.5")}).check()
    assert interp.get_variable("out") == Decimal("2.5")
    interp.execute("out = f", {"f": Fraction(5, 8)}).check()
    assert interp.get_variable("out") == Fraction(5, 8)


def test_aware_time_is_rejected_not_silently_naive() -> None:
    # The interpreter has no tz-aware time; dropping the tzinfo would silently
    # change the value, so an aware time is rejected.
    aware_time = dt.time(12, 30, tzinfo=dt.timezone(dt.timedelta(hours=2)))
    interp = Interpreter()
    with pytest.raises(TypeError):
        interp.execute("out = t", {"t": aware_time}).check()
    # A naive time still round-trips.
    interp.execute("out = t", {"t": dt.time(12, 30)}).check()
    assert interp.get_variable("out") == dt.time(12, 30)


def test_lists_are_copied_not_aliased() -> None:
    # The documented contract. Aliasing would hand sandboxed code a live handle
    # on host memory, which is the thing this interpreter exists to prevent.
    host_list = [1, 2, 3]
    interp = Interpreter()
    interp.execute("data.append(4)", {"data": host_list}).check()

    assert host_list == [1, 2, 3], "the sandbox must not mutate the caller's list"
    assert interp.get_variable("data") == [1, 2, 3, 4]


def test_unsupported_python_object_is_rejected_by_name() -> None:
    class Custom:
        pass

    with pytest.raises(TypeError, match="Custom"):
        Interpreter().execute("pass", {"obj": Custom()})


def test_unsupported_sandbox_value_is_rejected_by_name() -> None:
    # A sandbox function has no Python analogue. It must raise naming the type,
    # never quietly hand back None — that would turn a boundary error into a
    # wrong answer somewhere further downstream.
    interp = Interpreter()
    interp.execute("def f():\n    pass").check()

    with pytest.raises(TypeError, match="function"):
        interp.get_variable("f")


def test_nan_round_trips() -> None:
    interp = Interpreter()
    interp.execute("out = value", {"value": float("nan")}).check()
    out = interp.get_variable("out")
    assert isinstance(out, float) and math.isnan(out)


# --- state persistence ----------------------------------------------------


def test_state_exports_and_resumes_in_a_fresh_interpreter() -> None:
    first = Interpreter()
    first.execute("counter = 41").check()
    blob = first.export_state()
    assert isinstance(blob, bytes)

    second = Interpreter()
    second.import_state(blob)
    second.execute("counter = counter + 1").check()
    assert second.get_variable("counter") == 42


def test_importing_a_foreign_state_blob_is_refused() -> None:
    # The interpreter never silently migrates a state format.
    with pytest.raises(StateFormatError):
        Interpreter().import_state(b"\xff\xff\xff\xff not a state blob")


# --- config ---------------------------------------------------------------


def test_config_defaults_are_the_interpreter_defaults() -> None:
    assert Config().max_recursion_depth == 1000  # matches CPython
    assert Config().max_execution_time is None


def test_config_overrides_only_what_is_given() -> None:
    config = Config(max_operations=5)
    assert config.max_operations == 5
    assert config.max_recursion_depth == Config().max_recursion_depth


def test_config_rejects_a_nonsense_timeout() -> None:
    with pytest.raises(ValueError, match="non-negative"):
        Config(max_execution_time=-1.0)


# --- async entry point ----------------------------------------------------


async def test_execute_async_returns_a_result() -> None:
    result = await Interpreter().execute_async("print('async')")
    assert isinstance(result, ExecutionResult)
    assert result.check().stdout == "async\n"


async def test_async_tool_runs_on_the_callers_event_loop() -> None:
    # The load-bearing property of execute_async: a tool coroutine is scheduled
    # on the *caller's* loop, so it may await objects bound to it. Driving it on
    # a foreign loop would misbehave, which is why the sync and async paths bind
    # different loops.
    caller_loop = asyncio.get_running_loop()
    seen: list[asyncio.AbstractEventLoop] = []

    async def probe() -> str:
        seen.append(asyncio.get_running_loop())
        await asyncio.sleep(0)
        return "ok"

    interp = Interpreter(tools={"probe": probe})
    result = await interp.execute_async("print(probe())")

    assert result.check().stdout == "ok\n"
    assert seen == [caller_loop]


async def test_the_event_loop_is_not_blocked_during_execute_async() -> None:
    # If execute_async blocked the loop, this concurrent task could not make
    # progress while the script runs.
    ticks = 0

    async def ticker() -> None:
        nonlocal ticks
        for _ in range(5):
            await asyncio.sleep(0.01)
            ticks += 1

    async def slow() -> str:
        await asyncio.sleep(0.05)
        return "done"

    interp = Interpreter(tools={"slow": slow})
    task = asyncio.create_task(ticker())
    result = await interp.execute_async("print(slow())")
    await task

    assert result.check().stdout == "done\n"
    assert ticks == 5
