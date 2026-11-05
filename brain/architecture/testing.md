# Testing

How tinyfleet is tested, and what proves it works. Specs say which tests
each part adds; this says what kind of test goes where, and how they run.

The evals are the layer that remembers what any implementation must do: the
code can be thrown away, the evals are kept.

## Levels

- **Unit tests:** part of the code. They test one implementation and may be
  regenerated with it. A unit test earns its place only for pure logic no
  eval reaches cheaply, such as a parser. They live inline, beside the code.
- **Fitness tests:** check `core.md`'s invariants and dependency rules by
  reading the workspace's dependency graph; a dependency or placement
  `core.md` does not allow fails them.
- **Evals:** transcript cases at the CLI boundary, under
  `tests/cmd/<verb>/`, checked by trycmd on every `cargo test`. A case is
  one command, or a short run of them, in one temp directory, with the
  outside tools faked so nothing reaches the person's real ones. A script
  with no terminal never waiting, or a second setup never nesting a fleet,
  is one case. `tests/cmd/README.md` says how a case is written and run.

## What earns an eval

- **A milestone's or a feature's evals are written once it is built**,
  from three sources: its PRD's use cases and requirements, the
  invariants found while building it, and the old spec clauses Alberto
  keeps at the harvest step.
- **After that, edge cases enter from real use**, never from reading a
  document. A `fix` first adds the eval case that fails on it.
- **Exact text where a change would matter, wildcards where it would not.**
  A case states only what a person or a script would notice if a rewrite
  changed it.

## Running

- `make check` runs every gate a piece must pass: formatting, clippy with
  warnings denied, and every test, each without and with `--all-features`
  so the CLI's specimen builds and stays complete. `make` alone lists the
  targets.
- `make try` (`tools/try`) opens a shell with a sandbox of its own to try a
  piece by hand: three git repositories, a state directory, and a supervisor
  run by hand on that state. Leaving the shell stops that supervisor and
  deletes the sandbox; `tools/try -c '…'` runs commands in one instead. It
  never reaches the real state directory or supervisor, so while that
  supervisor is installed, plain `fleet stop` and starting a stopped
  supervisor are refused inside it.
- Before calling the suite green, run it under a terminal as well as
  piped: fleet's words differ when stdout is a terminal. A test of words,
  colour or symbols uses piped or explicit terminal ink, never the real
  stream.

## Not tested

- What a case can't reach, such as a question asked at a terminal, is
  checked by hand, and the item says so.
