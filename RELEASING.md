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
cargo package --list    # review contents
cargo package           # build + verify the tarball
```

## Tag and publish

```bash
# Annotated tag matching Cargo.toml version
git tag -a v0.1.0 -m "v0.1.0"
git push origin main --tags

cargo publish           # from a clean tree; do not use --allow-dirty
```

After the first publish, docs.rs builds automatically from crates.io.

## GitHub

- Releases: create a GitHub Release from the tag (notes from CHANGELOG)
- Security: [SECURITY.md](./.github/SECURITY.md) or `security@moderately.ai`
- Contribution / OSS inquiries: `opensource@moderately.ai` (see CONTRIBUTING.md)
- Topics: `rust`, `python`, `interpreter`, `sandbox`, `llm`, `ast`
