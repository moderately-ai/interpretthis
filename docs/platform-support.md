# Platform support

Three registries ship from this repo, each with a different platform contract:
crates.io (source), PyPI (binary wheels), and npm (binary Node-API addons).

**The rule this document exists to enforce: a registry or a README must never
advertise a platform that CI does not build and smoke.** Everything below maps to
a concrete lane in `.github/workflows/`. An OS is not added to a classifier list,
a `targets` array, or a README until its lane is green.

## Tiers

- **Tier 1 — built and smoked.** CI builds the artifact on this target *and runs a
  real workload against the installed artifact*, on a native runner. A failure
  blocks the release.
- **Tier 2 — compile-verified.** CI compiles for this target on every landing but
  does not exercise it at runtime. Supported; bugs are fixed, but platform-specific
  *runtime* behaviour is not asserted pre-merge.
- **Unsupported.** Out of scope. Listed so the boundary is deliberate.

## The matrix

| Surface | Target | Runner | Tier | Check | Workflow |
|---|---|---|---|---|---|
| Rust crate | `x86_64-unknown-linux-gnu` | `ubuntu-latest` | 1 | fmt, clippy, 717 tests, `cargo package` | `ci.yml`, every push/PR |
| Rust crate | `aarch64-apple-darwin` | `macos-latest` | 1 | same | `ci.yml`, every push/PR |
| Rust crate | other 64-bit hosts | — | best-effort | none | — |
| Python wheel | `manylinux2014` x86_64 | `ubuntu-latest` | 1 | build + clean-venv install smoke | `release-python.yml` |
| Python wheel | macOS arm64 | `macos-14` | 1 | build + native install smoke | `release-python.yml` |
| Python wheel | macOS x86_64 | `macos-15-intel` | 1 | build + native install smoke | `release-python.yml` |
| Python wheel | Windows x86_64 | `windows-latest` | 1 | build + native install smoke | `release-python.yml` |
| Python sdist | — | `ubuntu-latest` | 1 | source build + smoke | `release-python.yml` |
| Python binding | Linux + macOS | both `ci.yml` runners | 2 | `cargo clippy -p interpretthis-python` | `ci.yml`, every push/PR |
| npm addon | darwin arm64/x64 | `macos-14`, `macos-15-intel` | 1 | build + `node --test` on the addon | `release-npm.yml` |
| npm addon | linux x64/arm64 gnu | `ubuntu-latest`, `ubuntu-24.04-arm` | 1 | build + `node --test` | `release-npm.yml` |
| npm addon | linux x64/arm64 musl | same, Alpine container | 1 | build + `node --test` inside `node:24-alpine` | `release-npm.yml` |
| npm addon | win32 x64/arm64 | `windows-latest`, `windows-11-arm` | 1 | build + `node --test` | `release-npm.yml` |
| Node binding | Linux | `ubuntu-latest` | 1 | build + `node --test` | `ci.yml`, every push/PR |

**Deliberately absent from v1: Linux `aarch64` and `musllinux` *wheels*.** They
would need a QEMU-emulated install smoke, which is weaker evidence than the native
lanes above — a wheel that merely *builds* under emulation has not been shown to
*work*. They are the documented growth path: add the lane, get it green, then add
the classifier. (The npm addons *do* cover linux arm64 and musl, because those
lanes run on native arm runners and in a real Alpine container.)

## Python floor: CPython 3.11, one wheel per platform

The extension is built `abi3-py311` (PyO3's stable-ABI mode), so a single
`cp311-abi3` wheel per platform serves CPython 3.11, 3.12, 3.13, and 3.14. That is
why the matrix has four wheels rather than sixteen.

It also means the `Operating System ::` classifiers in
`crates/interpretthis-python/pyproject.toml` are exactly the four platforms above.
Do not add one ahead of its lane.

## Why the pyo3 boundary is compile-checked on every OS

`cargo clippy -p interpretthis-python` runs in the `ci.yml` matrix (Linux and
macOS) *without* the `extension-module` feature, so PyO3 links libpython normally
and the link succeeds anywhere. This is a cheap, fast gate on the exact code where
a platform-specific link break appears — caught on every PR, rather than in a wheel
job that only runs at release time.

## Node floor: Node 22 (Node-API 8)

The addon targets `napi8`, matching `"engines": { "node": ">= 22" }`. Prebuilt
binaries ship for all eight platform triples above; npm installs only the one
matching the consumer's machine via `optionalDependencies`.

## No wasm / browser build (and what it would take)

Unsupported, deliberately. The interpreter's tool system is async and rests on
tokio, and three things break on `wasm32-unknown-unknown`:

1. **`std::time::Instant::now()` panics there**, and it is called unconditionally
   on every run (`src/state.rs`, `src/interpreter.rs`) — so *every* `execute`
   would abort. Fixable with the `web-time` crate, which is a drop-in that
   re-exports std on native.
2. **tokio features must split per target.** `rt-multi-thread` cannot build for
   wasm; `tokio::spawn` (used for parallelizable tools) does work on a
   current-thread runtime, but `tokio::time::timeout` — which enforces
   `max_execution_time` — needs the timer driver, which on wasm requires
   `--cfg tokio_unstable`.
3. **`ToolHandler` requires `Send + Sync`, and `js_sys::Function` is `!Send`.**
   This is what looks fatal and is not: wrapping the JS callback in
   `send_wrapper::SendWrapper` *inside a wasm binding crate* is sound in
   single-threaded wasm and leaves the core's `Send` bounds — and its
   `unsafe_code = "deny"` — untouched. That is the difference between a
   bindings-only change and refactoring every `Pin<Box<dyn Future + Send>>` in
   `src/eval/**`.

So it is tractable, but it is a real piece of work rather than a feature flag, and
it should be gated behind a spike whose only deliverable is a green
`cargo check --target wasm32-unknown-unknown -p interpretthis`. Native N-API is the
Node story until then.
