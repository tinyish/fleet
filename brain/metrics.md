# Metrics

fleet's code-health numbers, one row per release (CONTRIBUTING.md § Code
health). The release health pass runs `python3 tools/metrics` and appends the
row it prints. Only the first row, the baseline, was copied in by hand.

**Read them as a trend.** No number here files an item on its own yet. At each
release, compare the new row with the ones above it and file a health item for
anything that moved the wrong way for no reason. Thresholds that file items
automatically are set once three rows exist.

**Columns:**

- **build**: a clean `cargo build --workspace --all-targets`, wall time.
- **default lane** and **ring lane**: `cargo nextest run --workspace
  --no-fail-fast`, without and with `--profile ring`: tests run, wall time,
  and any failed or timed-out tests.
- **tests**: tests in the default lane per crate, core / controller / cli.
- **lines**: lines of Rust under each crate's `src/`, core / controller / cli.
- **largest source file**: the longest file under the three `src/` trees.
- **clippy**: occurrences of `allow(clippy` in the three crates / warnings
  from `cargo clippy --workspace --all-targets` (`failed` if clippy did not
  finish).
- **sha**: the commit measured, with `-dirty` when the tree had uncommitted
  changes, which makes the row unreproducible.
- **clauses without a scenario**: reads `n/a` until fleet-dtu reports which
  clauses its scenarios cover; the count in brackets is the clauses written.

Timings depend on the machine, so each row says where it was measured when
that changes.

| date | sha | build | default lane | ring lane | tests | lines | largest source file | clippy | clauses without a scenario |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-10-12 | 1a28043 | 23.9s | 1496 · 88.0s · 40 failed | 11 · 26.7s | 707 / 317 / 472 | 32736 / 23716 / 9498 | controller/src/run.rs (1904) | 11 / 0 | n/a (0 clauses) |

The first row is the baseline Phase 0 took before any spec existed (fleet-5qos),
on an Apple M4 with 10 cores, inside an agent sandbox. Its 40 failures are all
in fleet-core's `add` and `remove` tests and may come from the sandbox rather
than the code.
