# Releasing

Three registries ship from this repo, from one version:

| Registry | Package | Tag | Workflow |
| --- | --- | --- | --- |
| crates.io | `interpretthis` (source) | `v*` | manual `cargo publish` |
| PyPI | `interpretthis` (wheels + sdist) | `python-v*` | `release-python.yml` |
| npm | `interpretthis` (native addons) | `npm-v*` | `release-npm.yml` |

Keep the version in `[workspace.package]`, `crates/interpretthis-node/package.json`,
and the changelog in step. Always cut from `main`.

**Neither binding workflow publishes on its own.** A pushed tag builds and smokes
the artifacts and stops there. Publishing needs a `workflow_dispatch` with
`publish: true` **and** approval on the protected `pypi` / `npm` environment —
that required reviewer is the human gate. Configure both environments with a
required reviewer before the first release, or the gate does not exist.

## Before the first binary publish

Every dependency is permissively licensed (MIT/Apache-2.0/BSD/ISC/Zlib/Unicode),
enforced by `cargo deny` in CI. `rustpython-parser` is built with its `num-bigint`
feature, so the LGPL-3.0-only `malachite` bignum is not in the tree — the binary
artifacts carry no copyleft obligation. `NOTICE` (shipped in both packages)
records this.

- [ ] `interpretthis` claimed on PyPI and npm
- [ ] PyPI Trusted Publishing configured for this repo + workflow (no API token)
- [ ] npm Trusted Publishing configured (no `NPM_TOKEN`)
- [ ] `pypi` and `npm` GitHub Environments exist, each with a required reviewer

## crates.io

## Preconditions

- [ ] Working tree clean; `main` is green on CI
- [ ] `CHANGELOG.md` has an entry for the version
- [ ] `Cargo.toml` `version` matches the changelog / tag
- [ ] Package list looks right (`tickets/` / `scripts/` excluded)
- [ ] Logged into crates.io (`cargo login` or `CARGO_REGISTRY_TOKEN`)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo nextest run --all-targets
cargo package -p interpretthis --list    # review contents
cargo package -p interpretthis           # build + verify the tarball
```

The root is a virtual workspace, so `-p` is required: only `crates/interpretthis`
is published to crates.io. The binding crates are `publish = false` and ship to
PyPI and npm on their own tags.

## Tag and publish

```bash
# Annotated tag matching Cargo.toml version
git tag -a v0.1.0 -m "v0.1.0"
git push origin main --tags

cargo publish -p interpretthis   # from a clean tree; do not use --allow-dirty
```

After the first publish, docs.rs builds automatically from crates.io.

## PyPI

Rehearse locally first — the same smoke CI runs, against a wheel installed into a
throwaway venv with no index, so it cannot silently exercise the source tree:

```bash
cd crates/interpretthis-python
uv venv .venv && source .venv/bin/activate
uv pip install 'maturin>=1.7,<2.0' pytest pytest-asyncio ruff mypy
maturin develop && pytest && mypy && ruff check python
(cd /tmp && python -m mypy.stubtest interpretthis._native)

maturin build --release --out dist
python -m venv /tmp/smoke && /tmp/smoke/bin/pip install --find-links dist --no-index interpretthis
/tmp/smoke/bin/python ../../docs/release/smoke_python.py    # must print SMOKE OK
```

Then:

1. Dispatch `release-python` with `publish: false`. Every matrix leg must build
   *and* install-smoke green on its native runner.
2. Re-dispatch with `publish: true` and approve the `pypi` environment.
3. Tag: `git tag -a python-v0.4.0 -m "python-v0.4.0" && git push --tags`.

## npm

```bash
cd crates/interpretthis-node
npm install && npm run build:debug && npm test
```

Then the same two-step: dispatch with `publish: false`, confirm all eight platform
addons built and smoked (musl inside Alpine, the rest natively), re-dispatch with
`publish: true`, approve the `npm` environment, tag `npm-v0.4.0`.

## Rollback

**Yank, never delete.** A deleted version breaks every lockfile that pinned it.

- crates.io: `cargo yank --version X.Y.Z interpretthis`
- PyPI: yank the release in the web UI (it stays installable by exact pin, and
  stops being selected by a range)
- npm: `npm deprecate interpretthis@X.Y.Z "…"`; `npm unpublish` only within 72h and
  only if nothing depends on it

Then ship a fixed patch version. Do not re-use a version number: the registries
reject it, and any consumer who already fetched the bad one keeps it.

## GitHub

- Releases: create a GitHub Release from the tag (notes from CHANGELOG)
- Security: [SECURITY.md](./.github/SECURITY.md) or `security@moderately.ai`
- Contribution / OSS inquiries: `opensource@moderately.ai` (see CONTRIBUTING.md)
- Topics: `rust`, `python`, `interpreter`, `sandbox`, `llm`, `ast`

## Dependency updates (PRs disabled)

GitHub pull requests are fully disabled on this repo. Dependency hygiene:

1. Periodically run `cargo update` on `main` (or pin bumps in `Cargo.toml`).
2. Run `cargo deny check` (advisories + licenses) and full CI locally.
3. Commit `Cargo.lock` changes with Conventional Commits, e.g.
   `chore(deps): cargo update`.
4. Push directly to `main` after CI is green.

Ignored advisories for unmaintained `unic-*` / `paste` (via `rustpython-parser`)
live in `deny.toml` with reasons — re-evaluate when the parser upgrades.

## Benchmark baselines

`crates/interpretthis/benches/baseline.json` records criterion medians for regression envelopes.
After material interpreter changes, run `cargo bench --bench interpreter` and
update the file (see `_comment` / `_comment_refresh` inside the JSON).
