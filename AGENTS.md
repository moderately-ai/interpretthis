# Agent instructions

## Testing

- **Use `cargo nextest run` instead of `cargo test`.**
- Filter examples:
  - `cargo nextest run -E 'test(method_kwargs)'`
  - `cargo nextest run --lib`
  - `cargo nextest run --all-targets`
- Do not default to `cargo test` unless nextest is unavailable and you note why.

## Recursion tests and native stack

Each Python call frame currently costs a large amount of native stack
(`Box::pin` futures hold full match-arm state). Prefer:

- Setting `InterpreterConfig::max_recursion_depth` **below** the host stack
  ceiling so tests observe `RecursionError` rather than SIGABRT.
- Using shallow depths (e.g. 2–3) for "under cap succeeds" checks until the
  eval trampoline ticket lands.

Do not raise default-thread recursion smoke depths without profiling stack use.
