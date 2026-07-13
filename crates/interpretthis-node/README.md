# interpretthis

Run untrusted or LLM-generated **Python** inside a sandbox — from Node.

`interpretthis` evaluates a Python AST under resource limits and an allowlisted
language surface. It is not CPython in a subprocess and not a container: the
interpreter is written in Rust and simply has no filesystem, network, or process
access to give away. Capabilities reach the script only through *tools* you
inject.

```bash
npm install interpretthis
```

Prebuilt binaries ship for macOS, Linux (glibc and musl), and Windows, on x64 and
arm64. Node 22+.

## Why

You want a model to write Python that transforms data, scores records, or
orchestrates your tools — without giving it a real Python process. This is the
evaluator: you own the tools, the limits, and what happens to the result.

## Quick start

```js
import { Interpreter } from 'interpretthis'

const interp = new Interpreter({
  double: ({ n }) => n * 2,
})

const result = await interp.execute('answer = double(n=x)\nprint(answer)', { x: 21 })

console.log(result.stdout)              // '42\n'
console.log(interp.getVariable('answer')) // 42
```

Failure is **data, not a thrown error**. `execute` resolves to a result carrying
`stdout` *and* `error`, because a script that prints three lines and then throws
has told you something useful in both halves — and that pair is exactly what you
feed back to a model:

```js
const result = await interp.execute("print('working...')\nresult = 1 / 0")

result.ok            // false
result.stdout        // 'working...\n'
result.error.kind    // 'exception'
result.error.message // 'ZeroDivisionError: division by zero (at line 2)'
```

`error.kind` is a stable string you can switch on: `syntax`, `security`,
`runtime`, `limitExceeded`, `recursionLimit`, `tool`, `name`, `type`, `value`,
`attribute`, `assertion`, `exception`, `stateFormat`.

## Tools

A tool is any function — sync or `async`. The script calls it by name; arguments
arrive as an object.

```js
const interp = new Interpreter({
  fetch: async ({ url }) => {
    const response = await fetch(url)
    return response.text()
  },
})

await interp.execute("page = fetch(url='https://example.com')")
```

`execute` is asynchronous by necessity, not by style: a JS tool callback can only
resolve while the event loop is free, so the run happens off-thread and the loop
stays available to service your tools.

Tools that are safe to overlap can say so, and the interpreter will run them
concurrently:

```js
const interp = new Interpreter({
  fetch: { func: fetchPage, parallelizable: true },
})

// These three overlap rather than running end to end.
await interp.execute(`
a = fetch(url='https://example.com/1')
b = fetch(url='https://example.com/2')
c = fetch(url='https://example.com/3')
print(a + b + c)
`)
```

A tool that throws becomes a catchable `Exception` inside the script, and an
`error.kind === 'tool'` result for you if the script does not catch it.

## Limits

```js
const interp = new Interpreter(null, {
  maxOperations: 1_000_000,
  maxMemoryBytes: 64 * 1024 * 1024,
  maxExecutionTime: 5, // seconds
  maxRecursionDepth: 100,
})
```

Exceeding one stops the script with `error.kind === 'limitExceeded'` — with
whatever it printed before then still in `stdout`.

## Types across the boundary

Most things map to the obvious counterpart. Two asymmetries are worth knowing,
because they are deliberate rather than accidental:

- **A JS array always becomes a Python `list`, never a `tuple`** — JavaScript has
  no tuple to distinguish. A Python tuple arrives here as a *frozen* array.
- **A sandbox integer arrives as a `number` when it fits, and a `bigint` when it
  exceeds `Number.MAX_SAFE_INTEGER`.** Silently losing digits would be worse.

Python `bytes` ↔ `Buffer`, `set` ↔ `Set`, `datetime` ↔ `Date`. A `dict` becomes a
plain object when every key is a string, and a `Map` otherwise — an object would
turn `{1: 'a'}` into `{'1': 'a'}` and lose the key's type.

Values crossing the boundary are **copied, not aliased**: mutations the script
makes are not visible on your objects. Read results back with `getVariable`.

Anything with no counterpart — a function, class, or instance defined inside the
sandbox — throws a `TypeError` naming the type rather than quietly becoming
`null`.

## Resumable state

```js
const blob = interp.exportState() // Buffer; sign or encrypt it yourself

const later = new Interpreter()
later.importState(blob)
await later.execute('counter = counter + 1')
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

## Licence

MIT OR Apache-2.0, at your option.

The prebuilt native binaries statically link `malachite` (LGPL-3.0-only), reached
transitively via the Python parser. See the bundled `NOTICE`.
