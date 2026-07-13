# interpretthis

Run untrusted or LLM-generated Python inside a sandbox — from Python.

`interpretthis` evaluates a Python AST under resource limits and an allowlisted
language surface. It is **not** CPython in a subprocess and not a container: the
interpreter is written in Rust and simply has no filesystem, network, or process
access to give away. Capabilities reach the script only through *tools* you
inject.

```bash
pip install interpretthis
```

## Why

You want a model to write Python that transforms data, scores records, or
orchestrates your tools — without handing it a real Python process. `exec()` in a
`try` block is not a sandbox. This is the evaluator: you own the tools, the
limits, and what happens to the result.

## Quick start

```python
from interpretthis import Interpreter

def double(n: int) -> int:
    return n * 2

interp = Interpreter(tools={"double": double})

result = interp.execute("answer = double(n=x)\nprint(answer)", {"x": 21})

print(result.stdout)              # '42\n'
print(interp.get_variable("answer"))  # 42
```

Failure is **data, not an exception**. `execute` returns an `ExecutionResult`
carrying `stdout` *and* `error`, because a script that prints three lines and
then raises has told you something useful in both halves — and that pair is
exactly what you feed back to a model:

```python
result = interp.execute("print('working...')\nresult = 1 / 0")

result.ok        # False
result.stdout    # 'working...\n'
result.error     # PythonException('ZeroDivisionError: division by zero ...')
result.check()   # raises, if you would rather
```

## Tools

A tool is any callable — sync or `async`. The script calls it by name; arguments
arrive as keywords.

```python
import aiohttp

async def fetch(url: str) -> str:
    async with aiohttp.ClientSession() as session:
        async with session.get(url) as response:
            return await response.text()

interp = Interpreter(tools={"fetch": fetch})
interp.execute("page = fetch(url='https://example.com')")
```

Coroutine tools work from the blocking `execute()` too — the interpreter runs
them on a background event loop it starts on first use. Inside async code, use
`execute_async` instead, which schedules tool coroutines on *your* loop so they
may await objects bound to it (an `aiohttp` session, an `asyncio.Lock`):

```python
result = await interp.execute_async("page = fetch(url='https://example.com')")
```

Tools that are safe to overlap can say so, and the interpreter will run them
concurrently:

```python
from interpretthis import Tool

interp = Interpreter(tools={"fetch": Tool(fetch, parallelizable=True)})
```

A tool that raises becomes a catchable `Exception` inside the script, and a
`ToolError` for you if the script does not catch it.

## Limits

```python
from interpretthis import Config

interp = Interpreter(config=Config(
    max_operations=1_000_000,
    max_memory_bytes=64 * 1024 * 1024,
    max_execution_time=5.0,        # seconds
    max_recursion_depth=100,
))
```

Exceeding one stops the script with a `LimitExceededError` — with whatever it
printed before then still in `stdout`.

## Resumable state

Variables and classes persist across `execute` calls on one interpreter, and can
be checkpointed:

```python
blob = interp.export_state()   # bytes; sign or encrypt them yourself

later = Interpreter()
later.import_state(blob)
later.execute("counter = counter + 1")
```

## What it is not

- **Not full CPython.** A large subset, deliberately chosen. See
  [`CONFORMANCE.md`](https://github.com/moderately-ai/interpretthis/blob/main/CONFORMANCE.md).
- **Tools are trusted.** A tool with side effects extends the trust boundary by
  exactly as much as it does. That is the point, and it is your call.
- **`async`/`await` inside the script is not supported.** Await around
  `execute`, not within it.

Security boundary:
[`THREAT_MODEL.md`](https://github.com/moderately-ai/interpretthis/blob/main/THREAT_MODEL.md).

## Types

The package ships `py.typed` and is checked under `mypy --strict`.

## Licence

MIT OR Apache-2.0, at your option.

Binary wheels statically link `malachite` (LGPL-3.0-only), reached transitively
via the Python parser. See the bundled `NOTICE`.
