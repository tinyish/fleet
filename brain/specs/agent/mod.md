# The agent contract

The agent is the program a seat runs, such as Claude Code. fleet asks it six
questions through an adapter a pack ships. This area covers that contract: its
types and verbs, the call to the adapter, `fleet agent schema` and `fleet agent
check`.

## Modules

- `cli/src/agent.rs` — Runs `fleet agent check` against an agent adapter, and
  `fleet agent schema`.
  Depends on: `cli/src/adapter_check.rs`, `cli/src/exit.rs`,
  `controller/src/adapter/conformance.rs`, `controller/src/host/mod.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/mod.rs`,
  `core/src/agent/mod.rs`, `core/src/agent/schema.rs`, `core/src/pack.rs`,
  `core/src/policy.rs`.
- `controller/src/adapter/conformance.rs` — States the agent contract as checks
  that any agent adapter can be asked, with live session checks under `--live`.
  Depends on: `controller/src/clock.rs`, `controller/src/effect.rs`,
  `controller/src/host/mod.rs`, `controller/src/platform/mod.rs`,
  `core/src/adapter/check.rs`, `core/src/adapter/exec.rs`,
  `core/src/agent/mod.rs`, `core/src/agent/schema.rs`,
  `core/src/agent/types.rs`, `core/src/process.rs`, `core/src/schema.rs`,
  `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/types.rs`.
- `controller/src/adapter/mod.rs` — Groups the agent contract's conformance
  suite.
  Depends on: nothing in fleet.
- `core/src/agent/exec.rs` — Answers the agent contract by calling an agent
  adapter's executable, one process per call.
  Depends on: `core/src/adapter/exec.rs`, `core/src/agent/mod.rs`,
  `core/src/agent/types.rs`.
- `core/src/agent/mod.rs` — Defines the `Agent` trait, the six questions fleet
  asks the program a seat runs, and opens a fleet's agent adapter.
  Depends on: `core/src/adapter/mod.rs`, `core/src/adapter/open.rs`,
  `core/src/agent/types.rs`, `core/src/defaults.rs`, `core/src/item/mod.rs`,
  `core/src/pack.rs`, `core/src/policy.rs`, `core/src/seat/identity.rs`.
- `core/src/agent/schema.rs` — Generates the agent contract's JSON Schema
  document, which `fleet agent schema` prints.
  Depends on: `core/src/agent/types.rs`, `core/src/schema.rs`.
- `core/src/agent/types.rs` — Defines the agent contract's requests and answers
  as fleet's own types.
  Depends on: `core/src/adapter/exec.rs`, `core/src/seat/identity.rs`,
  `core/src/store/types.rs`.

## Flow

1. The controller's poll asks what each seat's agent is doing, through the
   `Agent` trait in `core/src/agent/mod.rs`.
2. `core/src/agent/exec.rs` builds one `read` request for every seat from
   `core/src/agent/types.rs`.
3. `core/src/adapter/exec.rs` (shared) runs the adapter once, with the request
   on stdin, under a deadline.
4. `core/src/agent/exec.rs` reads each seat's reading off stdout into
   `core/src/agent/types.rs`, and hands them back to the poll.

## Depends on other areas

- store — the types the agent contract shares with the store contract.
- seats — seat ids, and the host and effect layer the live checks start sessions
  on.
- packs — finding the adapter in an installed pack or the defaults.
- items — the stop a refusal is carried in.
- shared — the adapter call, its checks and opener, the process runner, the
  schema builder, the platform layer, the policy and the exit table.
