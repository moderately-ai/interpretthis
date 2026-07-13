# Releasing

Short checklist for a crates.io release. Always cut from `main`.

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
