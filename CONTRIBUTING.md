# Contributing

We are **not accepting unsolicited contributions** from outside contributors
at this time (no cold pull requests, patches, or drive-by issues that are
really feature requests in disguise).

## If you want to contribute

Email **[opensource@moderately.ai](mailto:opensource@moderately.ai)** and
briefly say:

1. Who you are
2. **Why** you want to contribute to `interpretthis`
3. What area you care about (if any)

We will follow up if there is a good fit. Please wait for that conversation
before opening a PR.

## Security

Report vulnerabilities privately — **not** via public issues or PRs:

- [security@moderately.ai](mailto:security@moderately.ai), or
- GitHub Security Advisories

Details: [SECURITY.md](./.github/SECURITY.md).

## For maintainers (invited contributors)

Once you have been invited to work on the crate:

### Setup

- Rust **1.88+** (`rust-version` in `Cargo.toml`)
- Host **Python 3.12.x** for the differential parity corpus
- [pre-commit](https://pre-commit.com/) (asserted via `.pre-commit-config.yaml`)
- Optional: [addlicense](https://github.com/google/addlicense) for headers

```bash
pipx install pre-commit   # once
pre-commit install --hook-type pre-commit --hook-type commit-msg
```

### Commit messages (Conventional Commits)

All commits must follow [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

Allowed **types**: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`.

Examples:

```text
feat(eval): support with-statement for user context managers
fix(security): reject dotted imports with CONFORMANCE anchor
docs: clarify tool errors are catchable as Exception
chore: bump rust-version note in CONTRIBUTING
```

The `commit-msg` pre-commit hook rejects non-conforming messages (`--strict`).

### Local gates

```bash
pre-commit run --all-files
cargo check
cargo nextest run --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
scripts/license-headers.sh check   # if addlicense is installed
```

Prefer **`cargo nextest run`** over `cargo test` for day-to-day and agent work.

Guidelines:

- Document **non-obvious** behaviour only (see README / crate docs style).
- Security-surface changes need `THREAT_MODEL.md` / `CONFORMANCE.md` updates
  in the same change.
- New stdlib modules: [`MODULE_TEMPLATE.md`](./MODULE_TEMPLATE.md).
- New `.rs` files: SPDX header from `license-header.txt`.

Any contribution intentionally submitted for inclusion is dual-licensed under
MIT OR Apache-2.0, as stated in the README.

## Dependencies without pull requests

PRs are disabled. Dependency bumps go as direct commits to `main` after
`cargo deny check` and the usual test/clippy gates. See `RELEASING.md`.
