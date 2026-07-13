# CPython-vendored test corpus

Adapted subsets of CPython's standard test suite, used by `cpython_imported.rs` as the systematic gap-finder against interpretthis. Each file is run through both the host `python3` (pinned to `python3.12.x` per the parity program) and interpretthis; pass counts on both sides are reported per file. Initial failures are expected — the suite exists to surface gaps, not to gate CI yet.

## Provenance

- **Source:** https://github.com/python/cpython
- **Branch:** `3.12`
- **Commit SHA:** `7c999be49dee7f12703e4b2e07e990544fabd40e` (HEAD of `3.12` at vendoring time)
- **License:** Python Software Foundation License (PSF-2.0). Each file carries an attribution header pointing to its CPython source.

## What lives here

| File | Original | Focus |
|---|---|---|
| `test_dict_adapted.py` | `cpython/Lib/test/test_dict.py` | Dict literal collapse, iteration order, `.keys()/.values()/.items()`, `__missing__`, `update`/`copy`/`get`/`setdefault`, `==` semantics. |
| `test_set_adapted.py` | `cpython/Lib/test/test_set.py` | Set/frozenset semantics, union/intersection/difference/symmetric_difference, subset/superset, mutating ops, equality, frozenset hash. |
| `test_list_adapted.py` | `cpython/Lib/test/test_list.py` | List construction, slicing, mutation (append/extend/insert/pop/remove), comparison, copy, sort, multiplication. |
| `test_int_adapted.py` | `cpython/Lib/test/test_int.py` | Int construction from str/float/bool, base conversion, arithmetic, comparison, bit ops, `bool` ⊂ `int`. |
| `test_float_adapted.py` | `cpython/Lib/test/test_float.py` | Float construction, arithmetic, comparison, repr, inf/nan, conversions. |
| `test_str_adapted.py` | `cpython/Lib/test/test_unicode.py` (3.12 calls it `test_unicode`; same role as `test_str`) | Str literals, indexing, slicing, methods (`split`/`join`/`strip`/`replace`/`find`/`count`), case folding, equality. |
| `test_bytes_adapted.py` | `cpython/Lib/test/test_bytes.py` | Bytes literal forms, indexing, slicing, methods, `bytes`/`bytearray` interactions, encoding/decoding round-trips. |

## Adaptation rules applied

The originals use `unittest`; interpretthis doesn't ship that machinery. Each file was adapted via these mechanical rules:

1. Drop `import unittest`, drop `class TestXxx(unittest.TestCase):` shells. Each `def test_<name>(self):` becomes a top-level `def test_<name>():`, called from a final runner block.
2. Replace assertions:
   - `self.assertEqual(a, b)` → `assert a == b, f"{a!r} != {b!r}"`
   - `self.assertTrue(x)` → `assert x`
   - `self.assertFalse(x)` → `assert not x`
   - `self.assertIs(a, b)` → `assert a is b`
   - `self.assertIsNot(a, b)` → `assert a is not b`
   - `self.assertIn(a, b)` → `assert a in b`
   - `self.assertNotIn(a, b)` → `assert a not in b`
   - `self.assertRaises(E, fn, *args)` → `try: fn(*args); raise AssertionError("expected E") except E: pass`
3. Tests that depend on the following were dropped (a top-of-file comment in each adapted file enumerates what was dropped, per the bucketing below):
   - `sys` internals (`sys.maxsize`, `sys.getsizeof`, `sys.getrefcount`, refcount semantics, C-level allocation behaviour)
   - `gc` module (cycle collection, weakref interaction)
   - File I/O, network, threading, subprocess
   - `pickle` / `copy.deepcopy` round-trips (separate from value equality)
   - `unittest` introspection (`subTest`, `skipUnless`, `skipIf`)
   - `setUp`/`tearDown` fixtures except where trivially inlinable
   - Features documented as out of scope for interpretthis: `async`/`await`, exception groups, metaclasses, `__slots__`
   - Hash-randomization-dependent ordering assertions (we set `PYTHONHASHSEED=0` so iteration is stable; CPython tests sometimes assert "some hash order" which is brittle even at the source)
4. Each file ends with a runner block that prints `X/Y passed` and lists failures. The Rust runner parses this line.
5. No prettification — each adapted test mirrors its CPython source line-for-line except for the assertion mechanical edits and the unittest shell strip. Honest divergence is the point: when interpretthis fails, we want the diff to point at a real semantic gap, not at our edit.

## Refresh procedure

```bash
git clone --depth 1 -b 3.12 https://github.com/python/cpython /tmp/cpython-vendor
cd /tmp/cpython-vendor && git rev-parse HEAD  # record new SHA in this README
# For each adapted file: diff /tmp/cpython-vendor/Lib/test/test_<x>.py against
# the SHA recorded above; manually carry forward upstream test additions/edits.
```

The adaptation work is manual on purpose: CPython's tests evolve, and reading the upstream diff during refresh is the moment we catch new semantics worth modeling.

## Coverage caveat

These are subsets — `~10-20` of the most load-bearing tests per file. The intent of the first cut is *gap discovery*, not full parity. Coverage grows as gaps close; the canonical parity bench remains the byte-diff `parity_corpus/` tree, not this corpus.
