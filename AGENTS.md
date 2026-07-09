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
- After per-arm boxed futures, default-thread recursion is ~8–12 levels; keep
  "under cap" tests at ≤6 and RecursionError tests with max_recursion_depth
  below ~12 so the interpreter guard fires before SIGABRT.

Do not raise default-thread recursion smoke depths without profiling stack use.
