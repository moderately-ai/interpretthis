Differential parity corpus. Each `.py` file is a self-contained snippet that the runner (`tests/integration/parity_corpus_runner.rs`) executes through both interpretthis and host `python3`, asserting byte-equal stdout and matching exit codes.

Topic directories mirror `src/eval/`; module directories under `modules/` mirror `src/eval/modules/`. Add new behaviour-checking snippets as `<behaviour>.py` (the filename becomes the test name). Snippets must be deterministic (no time-of-day, no hash-order-sensitive prints — sort sets/dicts before printing) and under 30 lines.

Each topic ships with a `_placeholder.py` stub so reviewers see the shape immediately; replace placeholders with real coverage as features land.
