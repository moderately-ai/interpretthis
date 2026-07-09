# Agent instructions

## Testing

- **Use `cargo nextest run` instead of `cargo test`.**
- Filter examples:
  - `cargo nextest run -E 'test(method_kwargs)'`
  - `cargo nextest run --lib`
  - `cargo nextest run --all-targets`
- Do not default to `cargo test` unless nextest is unavailable and you note why.
