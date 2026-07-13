# Copyright 2026 Thomas Santerre and Moderately AI Inc.
#
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Exception classes raised by the sandbox.

These live in Python, not Rust, and the extension module imports them (see
``src/errors.rs``). That direction is deliberate: it lets each class subclass the
builtin it mirrors, so ``SandboxNameError`` really *is* a ``NameError`` and a
caller's ``except NameError:`` behaves the way the name promises. A Rust-side
``create_exception!`` could only inherit from a single base that Rust can name.

The ``Sandbox`` prefix is load-bearing. These describe a failure *inside the
sandboxed script*, which is a different event from the same-named error occurring
in your own code — shadowing the builtins here would make that distinction
impossible to write down.
"""

from __future__ import annotations

__all__ = [
    "InterpretThisError",
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
    "ToolError",
]


class InterpretThisError(Exception):
    """Base class for every failure originating in the sandbox.

    ``except InterpretThisError`` catches anything the interpreter reports,
    without also catching bugs in your own host code.
    """


class SandboxSyntaxError(InterpretThisError, SyntaxError):
    """The script did not parse."""


class SecurityError(InterpretThisError):
    """The script was rejected by the sandbox boundary.

    A blocked name (``eval``, ``open``, ``os``), a blocked attribute
    (``__globals__``, ``__subclasses__``), or an import outside the allowlist.
    No builtin twin: CPython has no notion of this, and inventing one would
    imply the script merely misbehaved rather than tried to escape.
    """


class SandboxRuntimeError(InterpretThisError, RuntimeError):
    """The script failed at runtime."""


class LimitExceededError(InterpretThisError):
    """The script exhausted a resource budget (operations, memory, stdout, time).

    Not a Python error at all — it means the sandbox stopped the script, so it
    has no builtin twin.
    """


class RecursionLimitError(InterpretThisError, RecursionError):
    """The script exceeded ``max_recursion_depth``."""

    def __init__(self, message: str, limit: int | None = None) -> None:
        super().__init__(message)
        self.limit = limit
        """The configured depth that was breached."""


class ToolError(InterpretThisError):
    """A host tool raised, and the script did not catch it.

    Inside the script this surfaces as a plain ``Exception`` and *is* catchable;
    this class is what reaches you when it is not caught.
    """

    def __init__(self, message: str, tool_name: str | None = None) -> None:
        super().__init__(message)
        self.tool_name = tool_name
        """Name of the tool that failed."""


class SandboxNameError(InterpretThisError, NameError):
    """The script referenced an undefined name."""


class SandboxTypeError(InterpretThisError, TypeError):
    """The script performed an operation on the wrong type."""


class SandboxValueError(InterpretThisError, ValueError):
    """The script passed an invalid value."""


class SandboxAttributeError(InterpretThisError, AttributeError):
    """The script accessed a missing attribute."""


class SandboxAssertionError(InterpretThisError, AssertionError):
    """An ``assert`` in the script failed."""


class PythonException(InterpretThisError):
    """The script raised, and nothing caught it.

    ``type_name`` is the exception class as the *script* saw it — including
    classes the script defined itself, which have no counterpart out here. That
    is why this is one class carrying a name rather than a generated hierarchy.
    """

    def __init__(self, message: str, type_name: str | None = None) -> None:
        super().__init__(message)
        self.type_name = type_name
        """Name of the exception class raised inside the sandbox."""


class StateFormatError(InterpretThisError):
    """An ``import_state`` blob came from an incompatible interpreter.

    The interpreter never silently migrates a state format. Restart the workflow
    from a clean state rather than trying to salvage the blob.
    """

    def __init__(
        self, message: str, found: int | None = None, expected: int | None = None
    ) -> None:
        super().__init__(message)
        self.found = found
        """State format version embedded in the blob."""
        self.expected = expected
        """State format version this build reads and writes."""
