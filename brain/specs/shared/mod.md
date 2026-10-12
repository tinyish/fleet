# Shared plumbing

The plumbing every area uses and none owns: the exit table and the `--json`
envelope every command shares (what was the conventions area), the look of the
terminal, the command line's parse, process spawning, atomic writes, digests,
the clock, the platform layer, the `fleet.toml` key census and the controller's
policy, resolving the project a call acts in, the adapter call both contracts
share, the crate roots, and the fakes and stubs the tests drive.

## Feature specs

- `brain/specs/shared/exit-codes.md` — the exit table every command shares, and
  how a refusal reads.
- `brain/specs/shared/output.md` — the `--json` envelope every verb that takes
  the flag prints. Not written yet.
- `brain/specs/shared/item-ids.md` — naming an item by its full id, and what a
  verb does with a suffix. Not written yet.
- `brain/specs/shared/locations.md` — the machine directory, and finding the
  fleet and project a call acts in.

## Modules

- `cli/examples/fleet-agent-stub.rs` — Builds the fake agent adapter beside
  `fleet` for the cli's own tests.
  Depends on: `controller/src/test_support/agent_stub.rs`.
- `cli/examples/fleet-store-stub.rs` — Builds the fake store adapter beside
  `fleet` for the cli's own tests.
  Depends on: `core/src/test_support/stub.rs`.
- `cli/examples/fleet-tmux-stub.rs` — Builds the fake tmux client beside `fleet`
  for the cli's own tests.
  Depends on: `controller/src/bin/tmux_stub.rs`.
- `cli/src/adapter_check.rs` — Holds what `fleet store check` and `fleet agent
  check` share: the `--adapter` form, the scratch directory and the tally.
  Depends on: `cli/src/exit.rs`, `core/src/adapter/check.rs`,
  `core/src/pack.rs`.
- `cli/src/envelope.rs` — Prints the one JSON document every verb the SDK calls
  answers with under `--json`.
  Depends on: `cli/src/exit.rs`.
- `cli/src/exit.rs` — Defines the exit table every command shares, and how a
  refusal is printed.
  Depends on: `cli/src/envelope.rs`, `core/src/item/mod.rs`.
- `cli/src/main.rs` — Parses the `fleet` command line and hands each command
  family to its module.
  Depends on: `cli/src/exit.rs`, `cli/src/ui.rs`, `controller/src/events.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/wiring.rs`,
  `controller/src/run.rs`, `controller/src/runs.rs`, `controller/src/seat.rs`,
  `core/src/guard/mod.rs`.
- `cli/src/ui.rs` — Decides how the binary looks: its status lines, prompts and
  waits.
  Depends on: `cli/src/exit.rs`.
- `controller/src/bin/agent_stub.rs` — Builds `fleet-agent-stub`, the fake agent
  adapter, for tests.
  Depends on: `controller/src/test_support/agent_stub.rs`.
- `controller/src/bin/tmux_stub.rs` — Builds `fleet-tmux-stub`, a fake tmux
  client over a server kept in a file, for tests.
  Depends on: `controller/src/host/mod.rs`, `controller/src/test_support.rs`.
- `controller/src/clock.rs` — Gives UTC stamps, milliseconds, and the clock a
  wait is spent against.
  Depends on: nothing in fleet.
- `controller/src/lib.rs` — Is the controller crate's root, which names its
  modules.
  Depends on: nothing in fleet.
- `controller/src/platform/linux.rs` — Answers the platform layer on Linux.
  Depends on: `controller/src/platform/mod.rs`, `core/src/process.rs`.
- `controller/src/platform/macos.rs` — Answers the platform layer on macOS.
  Depends on: `controller/src/platform/mod.rs`, `core/src/process.rs`.
- `controller/src/platform/mod.rs` — Supplies what the operating system owns:
  the machine directory, process reads, the load average, the child `PATH` and
  the service manager.
  Depends on: `core/src/fs.rs`, `core/src/process.rs`.
- `controller/src/policy.rs` — Reads `fleet.toml` into the controller's policy,
  and keeps the last good one when a change does not parse.
  Depends on: `core/src/agent/mod.rs`, `core/src/item/run.rs`.
- `controller/src/project/mod.rs` — Resolves the project a call acts in: its
  root, machine directory, packs, seats and store.
  Depends on: `controller/src/config.rs`, `controller/src/events.rs`,
  `controller/src/lifecycle.rs`, `controller/src/platform/mod.rs`,
  `controller/src/project/stream.rs`, `core/src/agent/mod.rs`,
  `core/src/defaults.rs`, `core/src/item/mod.rs`, `core/src/policy.rs`,
  `core/src/seat/identity.rs`, `core/src/store/mod.rs`.
- `controller/src/test_support.rs` — Gives the controller's tests their fakes: a
  clock, a host and an agent.
  Depends on: `controller/src/clock.rs`, `controller/src/host/mod.rs`,
  `core/src/agent/mod.rs`, `core/src/fs.rs`.
- `controller/src/test_support/agent_stub.rs` — Answers the agent contract from
  the fake agent, as an adapter executable for tests.
  Depends on: `controller/src/clock.rs`, `controller/src/test_support.rs`,
  `core/src/adapter/exec.rs`, `core/src/agent/mod.rs`,
  `core/src/agent/types.rs`, `core/src/fs.rs`.
- `controller/src/test_support/reading.rs` — Reads the listings and session logs
  a test scripts the fake agent with.
  Depends on: `controller/src/clock.rs`, `controller/src/projection.rs`,
  `core/src/agent/mod.rs`, `core/src/store/types.rs`.
- `core/build.rs` — Walks the crate's `defaults/` tree at build time into the
  table the binary embeds.
  Depends on: `core/src/os_litter.rs`.
- `core/src/adapter/check.rs` — Runs a contract's conformance checks and records
  what each answered, for either contract.
  Depends on: nothing in fleet.
- `core/src/adapter/exec.rs` — Makes one bounded call of an adapter's executable
  and reads its answer and exit, for either contract.
  Depends on: `core/src/process.rs`.
- `core/src/adapter/mod.rs` — Groups what the store and agent adapters share:
  the call, the checks and the opener.
  Depends on: nothing in fleet.
- `core/src/adapter/open.rs` — Finds the adapter a fleet names, by path or
  through the installed packs, for either contract.
  Depends on: `core/src/item/brief.rs`, `core/src/pack.rs`,
  `core/src/policy.rs`, `core/src/process.rs`, `core/src/runtime.rs`,
  `core/src/supported.rs`.
- `core/src/bin/store_stub.rs` — Builds `fleet-store-stub`, the fake store
  adapter, for tests.
  Depends on: `core/src/test_support/stub.rs`.
- `core/src/digest.rs` — Computes SHA-256 digests that stay the same across
  toolchains.
  Depends on: nothing in fleet.
- `core/src/fs.rs` — Writes a whole file atomically, the same way on every
  platform.
  Depends on: nothing in fleet.
- `core/src/lib.rs` — Is the core crate's root, which names its modules.
  Depends on: nothing in fleet.
- `core/src/os_litter.rs` — Names the files an operating system's file browser
  drops into folders, so walks of a tree skip them.
  Depends on: nothing in fleet.
- `core/src/policy.rs` — Lists every `fleet.toml` key the verbs read, and is the
  only reader allowed to reach one.
  Depends on: nothing in fleet.
- `core/src/process.rs` — Runs a child process under a deadline that kills its
  whole group, and finds programs on `PATH`.
  Depends on: nothing in fleet.
- `core/src/schema.rs` — Builds and checks the JSON Schema documents both
  contracts print.
  Depends on: nothing in fleet.
- `core/src/test_support.rs` — Gives core's tests a fake store, a board over it
  and fixture packs.
  Depends on: `core/src/defaults.rs`, `core/src/embedded.rs`,
  `core/src/entry.rs`, `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`, `core/src/store/types.rs`.
- `core/src/test_support/stub.rs` — Answers the store contract from the fake
  store, as an adapter executable for tests.
  Depends on: `core/src/entry.rs`, `core/src/seat/actor.rs`,
  `core/src/store/mod.rs`, `core/src/store/types.rs`,
  `core/src/test_support.rs`.

## Flow

Used by every area; no flow of its own.

## Depends on other areas

- items — the stop every refusal is carried in, the entry format the fakes keep,
  and the pack-layer type the adapter opener reads.
- seats — the seat ids, the seat list, the host the fakes stand in for, and the
  poll loop `cli/src/main.rs` starts.
- store — the store the fakes answer, and the store the project resolver opens.
- agent — the agent the fakes answer, and the agent the project resolver opens.
- onboarding — the supported releases and the project register.
- packs — the defaults and the embedded files, and the pack format and the
  runtime the adapter opener reads.
- runs — the run pass `cli/src/main.rs` hands the poll loop, and the crash cap
  and environment names the policy takes from a run.
- status — the event stream and the projection the project resolver and
  `cli/src/main.rs` use.
- guards — the guard classes `cli/src/main.rs` parses.
