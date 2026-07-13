# Copyright 2026 Thomas Santerre and Moderately AI Inc.
#
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Type aliases for the sandbox boundary.

A real module, not stub-only definitions, so ``mypy.stubtest`` can check the
extension's stub against the built module without tripping over names that exist
only in a ``.pyi``.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

__all__ = ["SandboxValue", "ToolFunction"]

SandboxValue = Any
"""What can cross the sandbox boundary, in either direction.

Concretely: ``None``, ``bool``, ``int`` (any size), ``float``, ``str``,
``bytes``, ``list``, ``tuple``, ``set``, ``frozenset``, ``dict``, ``range``,
``Decimal``, ``Fraction``, ``date``, ``datetime``, ``time``, ``timedelta``, and
``timezone``.

Anything else — a function, class, or instance defined *inside* the sandbox —
raises ``TypeError`` naming the type rather than degrading to ``None``. Spelled
``Any`` because the set is a union of builtins that a type checker cannot narrow
usefully at a call site.
"""

ToolFunction = Callable[..., Any]
"""A tool: a sync ``def`` or an ``async def``.

Called with keyword arguments only. Positional arguments from the script arrive
as ``arg0``, ``arg1``, ...; keyword arguments keep their names.
"""
