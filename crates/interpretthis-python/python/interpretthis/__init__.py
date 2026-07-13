# Copyright 2026 Thomas Santerre and Moderately AI Inc.
#
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Run untrusted or LLM-generated Python inside a sandbox.

``interpretthis`` evaluates a Python AST under resource limits and an allowlisted
language surface. It is **not** CPython in a subprocess: there is no filesystem,
no network, and no process access unless *you* inject it as a tool.

    >>> from interpretthis import Interpreter
    >>>
    >>> def double(n: int) -> int:
    ...     return n * 2
    >>>
    >>> interp = Interpreter(tools={"double": double})
    >>> result = interp.execute("answer = double(n=x)\\nprint(answer)", {"x": 21})
    >>> result.stdout
    '42\\n'
    >>> interp.get_variable("answer")
    42

Tools may be ``async def`` as well; from async code use ``execute_async`` so tool
coroutines run on your own event loop.

Failure is data, not an exception. ``execute`` returns an ``ExecutionResult``
carrying both ``stdout`` and ``error``, because a script that prints three lines
and *then* raises has told you something useful in all four — which is exactly
what you feed back to a model. Call ``result.check()`` where you would rather
raise.
"""

from __future__ import annotations

from ._exceptions import (
    InterpretThisError,
    LimitExceededError,
    PythonException,
    RecursionLimitError,
    SandboxAssertionError,
    SandboxAttributeError,
    SandboxNameError,
    SandboxRuntimeError,
    SandboxSyntaxError,
    SandboxTypeError,
    SandboxValueError,
    SecurityError,
    StateFormatError,
    ToolError,
)
from ._native import (
    STATE_FORMAT_VERSION,
    Config,
    ExecutionResult,
    Interpreter,
    Tool,
    __version__,
)

__all__ = [
    "STATE_FORMAT_VERSION",
    "Config",
    "ExecutionResult",
    "InterpretThisError",
    "Interpreter",
    "LimitExceededError",
    "PythonException",
    "RecursionLimitError",
    "SandboxAssertionError",
    "SandboxAttributeError",
    "SandboxNameError",
    "SandboxRuntimeError",
    "SandboxSyntaxError",
    "SandboxTypeError",
    "SandboxValueError",
    "SecurityError",
    "StateFormatError",
    "Tool",
    "ToolError",
    "__version__",
]
