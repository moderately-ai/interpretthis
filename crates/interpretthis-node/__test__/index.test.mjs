// Copyright 2026 Thomas Santerre and Moderately AI Inc.
//
// SPDX-License-Identifier: MIT OR Apache-2.0

import assert from 'node:assert/strict'
import test from 'node:test'
import { setTimeout as sleep } from 'node:timers/promises'

import { Interpreter, STATE_FORMAT_VERSION } from '../index.js'

// --- basics ---------------------------------------------------------------

test('executes and captures stdout', async () => {
  const result = await new Interpreter().execute("print('hello')")
  assert.equal(result.ok, true)
  assert.equal(result.stdout, 'hello\n')
  assert.equal(result.error, undefined)
})

test('injects variables and reads them back', async () => {
  const interp = new Interpreter()
  await interp.execute('total = x + y', { x: 2, y: 3 })
  assert.equal(interp.getVariable('total'), 5)
  assert.deepEqual([...interp.stateKeys()].sort(), ['total', 'x', 'y'])
})

test('exposes the state format version', () => {
  assert.equal(typeof STATE_FORMAT_VERSION, 'number')
})

// --- failure is data, not a thrown error ----------------------------------

test('a failing run resolves, keeping partial stdout', async () => {
  // The whole reason execute() does not throw: a script that prints and *then*
  // fails has told you something useful in both halves, and that pair is what
  // gets fed back to a model.
  const result = await new Interpreter().execute("print('before')\nboom")

  assert.equal(result.ok, false)
  assert.equal(result.stdout, 'before\n')
  assert.equal(result.error.kind, 'name')
  assert.match(result.error.message, /boom/)
})

test('an uncaught script exception reports its type name', async () => {
  const result = await new Interpreter().execute("raise ValueError('bad input')")
  assert.equal(result.error.kind, 'exception')
  assert.equal(result.error.typeName, 'ValueError')
})

// --- sandbox boundary -----------------------------------------------------

test('dangerous builtins do not exist', async () => {
  for (const source of ["eval('1+1')", "open('/etc/passwd')"]) {
    const result = await new Interpreter().execute(source)
    assert.equal(result.error.kind, 'name', source)
  }
})

test('introspection escapes are a security error', async () => {
  const result = await new Interpreter().execute('x = ().__class__')
  assert.equal(result.error.kind, 'security')
})

test('the operation limit is enforced', async () => {
  const interp = new Interpreter(null, { maxOperations: 1000 })
  const result = await interp.execute('for i in range(1000000):\n    pass')
  assert.equal(result.error.kind, 'limitExceeded')
})

// --- tools ----------------------------------------------------------------

test('a synchronous tool', async () => {
  const interp = new Interpreter({ double: ({ n }) => n * 2 })
  const result = await interp.execute('print(double(n=21))')
  assert.equal(result.stdout, '42\n')
})

test('an async tool', async () => {
  const interp = new Interpreter({
    fetch: async ({ key }) => {
      await sleep(1)
      return `value-for-${key}`
    },
  })
  const result = await interp.execute("print(fetch(key='a'))")
  assert.equal(result.stdout, 'value-for-a\n')
})

test('sync and async tools together, through one code path', async () => {
  const interp = new Interpreter({
    add: ({ a, b }) => a + b,
    triple: async ({ n }) => {
      await sleep(1)
      return n * 3
    },
  })
  const result = await interp.execute('print(add(a=triple(n=2), b=1))')
  assert.equal(result.stdout, '7\n')
})

test('the event loop is NOT blocked while a script runs', async () => {
  // The load-bearing property. If execute() blocked the loop, this timer could
  // not fire while the script is running — and no async tool could ever resolve,
  // because resolving a promise requires the loop.
  let ticks = 0
  const ticker = setInterval(() => {
    ticks += 1
  }, 5)

  const interp = new Interpreter({
    slow: async () => {
      await sleep(60)
      return 'done'
    },
  })
  const result = await interp.execute('print(slow())')
  clearInterval(ticker)

  assert.equal(result.stdout, 'done\n')
  assert.ok(ticks > 0, 'the event loop was blocked during execute()')
})

test('parallelizable tools overlap', async () => {
  const interp = new Interpreter({
    slow: {
      func: async ({ n }) => {
        await sleep(40)
        return n
      },
      parallelizable: true,
    },
  })

  const started = Date.now()
  // Bind each call before using any of them. A parallelizable tool returns a
  // lazy proxy that is only forced when its value is needed, so writing
  // `slow(n=1) + slow(n=2) + slow(n=3)` would force the first two at the `+`
  // before the third had even been dispatched, and cap the overlap at two.
  const result = await interp.execute(
    'a = slow(n=1)\nb = slow(n=2)\nc = slow(n=3)\nprint(a + b + c)',
  )
  const elapsed = Date.now() - started

  assert.equal(result.stdout, '6\n')
  // Three 40ms tools run sequentially would take >= 120ms.
  assert.ok(elapsed < 110, `expected overlap, took ${elapsed}ms`)
})

test('per-call tools override registered ones', async () => {
  const interp = new Interpreter({ who: () => 'registered' })
  const result = await interp.execute('print(who())', null, { who: () => 'per-call' })
  assert.equal(result.stdout, 'per-call\n')
})

test('a throwing tool becomes a tool error', async () => {
  const interp = new Interpreter({
    boom: () => {
      throw new Error('tool exploded')
    },
  })
  const result = await interp.execute('boom()')
  assert.equal(result.error.kind, 'tool')
  assert.equal(result.error.toolName, 'boom')
  assert.match(result.error.message, /tool exploded/)
})

test('a script can catch a failing tool', async () => {
  const interp = new Interpreter({
    boom: () => {
      throw new Error('tool exploded')
    },
  })
  const result = await interp.execute('try:\n    boom()\nexcept Exception:\n    print("caught")')
  assert.equal(result.stdout, 'caught\n')
})

test('a blocked tool name is rejected at construction', () => {
  // Not at first call: it should fail loudly when you build the interpreter,
  // rather than silently do nothing until some script happens to call it.
  assert.throws(() => new Interpreter({ eval: () => null }), /dangerous builtin/)
})

// --- the deadlock guard ---------------------------------------------------

test('a reentrant execute is refused rather than deadlocking', async () => {
  // The interpreter holds its state lock across the whole run, including across
  // the await for a tool, and that lock is not reentrant. A nested execute()
  // would block forever, holding up the very tool it is waiting on — no timeout,
  // nothing logged. The guard converts that silent hang into a loud error.
  const interp = new Interpreter()

  const result = await interp.execute('print(reenter())', null, {
    reenter: async () => {
      await assert.rejects(() => interp.execute('x = 1'), /already running/)
      return 'refused'
    },
  })

  assert.equal(result.stdout, 'refused\n')
})

// --- value conversion -----------------------------------------------------

test('values round-trip', async () => {
  const cases = [
    null,
    true,
    false,
    0,
    -7,
    3.5,
    'text',
    [1, 'two', null],
    { a: 1, b: [2, 3] },
  ]

  const interp = new Interpreter()
  for (const value of cases) {
    await interp.execute('out = value', { value })
    assert.deepEqual(interp.getVariable('out'), value, JSON.stringify(value))
  }
})

test('big integers cross as BigInt rather than losing digits', async () => {
  // A JS `number` cannot hold this. Silently truncating would be the worst
  // possible outcome, so the boundary promotes to BigInt instead.
  const interp = new Interpreter()
  await interp.execute('out = n * 1', { n: 2n ** 200n })
  assert.equal(interp.getVariable('out'), 2n ** 200n)
})

test('an integer that fits stays a number', async () => {
  const interp = new Interpreter()
  await interp.execute('out = 42')
  assert.equal(interp.getVariable('out'), 42)
  assert.equal(typeof interp.getVariable('out'), 'number')
})

test('bytes cross as a Buffer', async () => {
  const interp = new Interpreter()
  await interp.execute('out = data', { data: Buffer.from('hello') })
  assert.deepEqual(interp.getVariable('out'), Buffer.from('hello'))
})

test('a Set round-trips', async () => {
  const interp = new Interpreter()
  await interp.execute('out = value', { value: new Set([1, 2, 3]) })
  const out = interp.getVariable('out')
  assert.ok(out instanceof Set)
  assert.deepEqual([...out].sort(), [1, 2, 3])
})

test('a Python frozenset projects to a JS Set', async () => {
  // JavaScript has no frozenset, so a Python frozenset comes back as a Set
  // (immutability is lost across the boundary, like tuple -> array).
  const interp = new Interpreter()
  await interp.execute('out = frozenset([3, 1, 2, 1])')
  const out = interp.getVariable('out')
  assert.ok(out instanceof Set)
  assert.deepEqual([...out].sort(), [1, 2, 3])
})

test('a Python tuple arrives as a frozen array', async () => {
  const interp = new Interpreter()
  await interp.execute('out = (1, 2)')
  const out = interp.getVariable('out')
  assert.deepEqual(out, [1, 2])
  assert.ok(Object.isFrozen(out), 'a tuple should be frozen to signal it was immutable')
})

test('a dict with non-string keys becomes a Map, preserving key types', async () => {
  // A plain object would stringify `{1: "a"}` into `{"1": "a"}` and lose the
  // key's type.
  const interp = new Interpreter()
  await interp.execute("out = {1: 'a', 2: 'b'}")
  const out = interp.getVariable('out')
  assert.ok(out instanceof Map)
  assert.equal(out.get(1), 'a')
})

test('arrays are copied, not aliased', async () => {
  // Aliasing would hand sandboxed code a live handle on host memory, which is
  // precisely what this interpreter exists to prevent.
  const hostArray = [1, 2, 3]
  const interp = new Interpreter()
  await interp.execute('data.append(4)', { data: hostArray })

  assert.deepEqual(hostArray, [1, 2, 3], 'the sandbox must not mutate the caller array')
  assert.deepEqual(interp.getVariable('data'), [1, 2, 3, 4])
})

test('an unsupported sandbox value throws naming the type', async () => {
  // A sandbox function has no JS analogue. It must throw naming the type, never
  // quietly return null — that turns a boundary error into a wrong answer
  // somewhere downstream.
  const interp = new Interpreter()
  await interp.execute('def f():\n    pass')
  assert.throws(() => interp.getVariable('f'), /function/)
})

// --- state persistence ----------------------------------------------------

test('state exports and resumes in a fresh interpreter', async () => {
  const first = new Interpreter()
  await first.execute('counter = 41')
  const blob = first.exportState()
  assert.ok(Buffer.isBuffer(blob))

  const second = new Interpreter()
  second.importState(blob)
  await second.execute('counter = counter + 1')
  assert.equal(second.getVariable('counter'), 42)
})

test('a foreign state blob is refused', () => {
  // The interpreter never silently migrates a state format.
  assert.throws(() => new Interpreter().importState(Buffer.from('not a state blob')))
})

// --- binding-boundary regressions -----------------------------------------

test('a non-Uint8 typed array is rejected, not read as raw bytes', async () => {
  const interp = new Interpreter()
  assert.throws(
    () => interp.execute('out = a', { a: new Float64Array([1, 2, 3]) }),
    /typed array|Float64Array/,
  )
})

test('a Uint8Array still becomes bytes', async () => {
  const interp = new Interpreter()
  await interp.execute('out = a', { a: new Uint8Array([1, 2, 3]) })
  const out = interp.getVariable('out')
  assert.deepEqual([...out], [1, 2, 3])
})

test('a class instance (Error) is rejected, not collapsed to {}', async () => {
  const interp = new Interpreter()
  assert.throws(
    () => interp.execute('out = e', { e: new Error('boom') }),
    /Error instance|plain objects/,
  )
})

test('a plain object still becomes a dict', async () => {
  const interp = new Interpreter()
  await interp.execute('out = o["a"]', { o: { a: 1, b: 2 } })
  assert.equal(interp.getVariable('out'), 1)
})

test('a Set with an unhashable element is rejected', async () => {
  const interp = new Interpreter()
  assert.throws(
    () => interp.execute('out = s', { s: new Set([[1, 2]]) }),
    /unhashable/,
  )
})

test('i64::MIN comes back as a BigInt without panicking', async () => {
  const interp = new Interpreter()
  await interp.execute('out = -(2**63)')
  assert.equal(interp.getVariable('out'), -9223372036854775808n)
})

test('an oversized range is rejected instead of exhausting memory', async () => {
  const interp = new Interpreter()
  await interp.execute('out = range(10**18)')
  assert.throws(() => interp.getVariable('out'), /too large/)
})

test('a negative maxRecursionDepth is rejected, not wrapped to a huge limit', () => {
  assert.throws(() => new Interpreter(null, { maxRecursionDepth: -1 }), /maxRecursionDepth/)
  assert.throws(() => new Interpreter(null, { maxConcurrentTools: -1 }), /maxConcurrentTools/)
})
