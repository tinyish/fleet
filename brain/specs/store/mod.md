# The store contract

The store keeps the work graph: items, their timelines and their holds. fleet
reaches it only through an adapter a pack ships. This area covers that contract:
its types and verbs, the call to the adapter, `fleet store schema` and `fleet
store check`.

## Modules

- `cli/src/store.rs` — Runs `fleet store check` against a store adapter, and
  `fleet store schema`.
  Depends on: `cli/src/adapter_check.rs`, `cli/src/exit.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/mod.rs`,
  `core/src/defaults.rs`, `core/src/pack.rs`, `core/src/policy.rs`,
  `core/src/store/conformance.rs`, `core/src/store/mod.rs`,
  `core/src/store/schema.rs`.
- `core/src/store/conformance.rs` — States the store contract as checks that any
  store can be asked.
  Depends on: `core/src/adapter/check.rs`, `core/src/entry.rs`,
  `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.
- `core/src/store/exec.rs` — Answers the store contract by calling a store
  adapter's executable, one process per call.
  Depends on: `core/src/adapter/exec.rs`, `core/src/entry.rs`,
  `core/src/seat/actor.rs`, `core/src/store/mod.rs`, `core/src/store/types.rs`.
- `core/src/store/mod.rs` — Defines the `Store` trait every verb reaches the
  work graph through, and opens a project's store adapter.
  Depends on: `core/src/adapter/exec.rs`, `core/src/adapter/mod.rs`,
  `core/src/adapter/open.rs`, `core/src/entry.rs`, `core/src/pack.rs`,
  `core/src/policy.rs`, `core/src/seat/actor.rs`.
- `core/src/store/schema.rs` — Generates the store contract's JSON Schema
  document, which `fleet store schema` prints.
  Depends on: `core/src/entry.rs`, `core/src/schema.rs`,
  `core/src/seat/actor.rs`, `core/src/store/mod.rs`, `core/src/store/types.rs`.
- `core/src/store/types.rs` — Defines the store contract's requests and answers
  as fleet's own types.
  Depends on: `core/src/adapter/exec.rs`, `core/src/seat/actor.rs`,
  `core/src/seat/identity.rs`.

## Flow

1. A verb such as `fleet dispatch` opens the project's store through
   `core/src/store/mod.rs`, which finds the adapter the fleet names.
2. `core/src/store/exec.rs` builds the verb's request from
   `core/src/store/types.rs`.
3. `core/src/adapter/exec.rs` (shared) runs the adapter once, with the request
   on stdin, under a deadline.
4. `core/src/store/exec.rs` reads the answer off stdout into
   `core/src/store/types.rs`, and its exit into an answer, a refusal or
   could-not-tell.

## Depends on other areas

- items — the entry format a timeline holds.
- seats — the seat ids and actors a write carries.
- packs — finding the adapter in an installed pack or the defaults.
- shared — the adapter call, its checks and opener, the schema builder, the
  policy and the exit table.
