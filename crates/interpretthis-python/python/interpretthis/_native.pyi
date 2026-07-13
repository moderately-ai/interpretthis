# Copyright 2026 Thomas Santerre and Moderately AI Inc.
#
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Type stubs for the compiled extension module.

Hand-written, and kept honest by ``mypy.stubtest`` against the *installed* wheel
in CI — not against this source tree, which cannot see whether the built module
actually matches.

Two shapes here are dictated by pyo3rather than chosen: constructors are
``__new__`` (a ``#[pymethods] #[new]`` becomes ``tp_new``, not ``__init__``), and
the first argument of ``execute`` is positional-only.
"""

from __future__ import annotations

from collections.abc import Mapping
from typing import final

from ._types import SandboxValue, ToolFunction

# pyo3 populates `__all__` on the built module; it must be spelled out here or
# stubtest cannot match the stub's exports against the runtime's.
__all__ = [
    "STATE_FORMAT_VERSION",
    "Config",
    "ExecutionResult",
    "Interpreter",
    "Tool",
    "__version__",
]

__version__: str

STATE_FORMAT_VERSION: int
"""Wire format of ``export_state`` blobs.

Independent of the package version: a blob is portable between builds that agree
on this number, and no others.
"""

@final
class Config:
    """Resource limits. Every argument defaults to the interpreter's own default."""

    def __new__(
        cls,
        max_operations: int | None = None,
        max_while_iterations: int | None = None,
        max_memory_bytes: int | None = None,
        max_stdout_bytes: int | None = None,
        max_concurrent_tools: int | None = None,
        max_execution_time: float | None = None,
        max_recursion_depth: int | None = None,
        max_int_bits: int | None = None,
    ) -> Config: ...
    @property
    def max_operations(self) -> int: ...
    @property
    def max_while_iterations(self) -> int: ...
    @property
    def max_memory_bytes(self) -> int: ...
    @property
    def max_stdout_bytes(self) -> int: ...
    @property
    def max_concurrent_tools(self) -> int: ...
    @property
    def max_execution_time(self) -> float | None:
        """Wall-clock budget in seconds, or ``None`` for no limit."""

    @property
    def max_recursion_depth(self) -> int: ...
    @property
    def max_int_bits(self) -> int: ...

@final
class Tool:
    """A tool with non-default settings.

    A bare callable is sequential. ``Tool(fn, parallelizable=True)`` lets the
    interpreter run it concurrently with other parallelizable tools — safe only
    if it has no order-dependent side effects.
    """

    def __new__(cls, func: ToolFunction, *, parallelizable: bool = False) -> Tool: ...
    @property
    def func(self) -> ToolFunction: ...
    @property
    def parallelizable(self) -> bool: ...

@final
class ExecutionResult:
    """The outcome of one run. ``stdout`` is populated even when ``error`` is set."""

    @property
    def stdout(self) -> str: ...
    @property
    def ok(self) -> bool: ...
    @property
    def error(self) -> BaseException | None:
        """The failure as an exception *instance*, not raised.

        Inspect ``.tool_name`` / ``.type_name`` without a ``try`` block.
        """

    def check(self) -> ExecutionResult:
        """Raise if the run failed; return ``self`` otherwise, so it chains."""

@final
class Interpreter:
    """A sandboxed Python interpreter with host tool injection.

    State (variables, classes) persists across ``execute`` calls on one
    interpreter. Concurrent or nested runs on a *single* interpreter raise
    ``RuntimeError``: they would interleave that shared state, and a run started
    from inside a tool callback would deadlock. Use one interpreter per
    concurrent run — they are cheap and isolated by design.
    """

    def __new__(
        cls,
        *,
        tools: Mapping[str, ToolFunction | Tool] | None = None,
        config: Config | None = None,
    ) -> Interpreter: ...
    def execute(
        self,
        code: str,
        /,
        variables: Mapping[str, SandboxValue] | None = None,
        *,
        tools: Mapping[str, ToolFunction | Tool] | None = None,
    ) -> ExecutionResult:
        """Run ``code``, blocking until it finishes.

        Tool coroutines are driven on a background event loop this interpreter
        starts on first use. From async code, prefer ``execute_async``.
        """

    async def execute_async(
        self,
        code: str,
        /,
        variables: Mapping[str, SandboxValue] | None = None,
        *,
        tools: Mapping[str, ToolFunction | Tool] | None = None,
    ) -> ExecutionResult:
        """Run ``code`` on the caller's event loop.

        Tool coroutines are scheduled on *your* loop, so a tool may await objects
        bound to it (an ``aiohttp`` session, an ``asyncio.Lock``).
        """

    def get_variable(self, name: str) -> SandboxValue | None:
        """Read a variable out of the interpreter's state.

        ``None`` both when the name is unset and when it holds ``None``; use
        ``state_keys()`` to tell those apart.
        """

    def state_keys(self) -> list[str]: ...
    def accounted_bytes(self) -> int:
        """Bytes the interpreter has accounted for — the counter gating
        ``max_memory_bytes``. Not RSS."""

    def resident_bytes(self) -> int: ...
    def export_state(self) -> bytes:
        """Serialise variables and classes for later resume.

        Signing and encryption are yours to do. Pending tool results are omitted.
        """

    def import_state(self, data: bytes) -> None:
        """Restore state from ``export_state`` bytes.

        Raises ``StateFormatError`` if the blob came from an interpreter with a
        different state format.
        """
