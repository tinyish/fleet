# Status and the event stream

What a person reads to see what the fleet is doing: `fleet status`, which prints
the projection the controller published, and the event stream, read with `fleet
event tail` and `fleet event show`.

## Feature specs

- `brain/specs/status/status.md` — `fleet status`: printing the projection the
  controller published. Not written yet.
- `brain/specs/status/event-tail.md` — `fleet event tail`: reading and following
  the event stream. Not written yet.
- `brain/specs/status/event-show.md` — `fleet event show`: printing one event by
  its id. Not written yet.

## Modules

- `cli/src/status.rs` — Runs `fleet status`, which prints the controller's
  published projection with the policy, runs, routines and open holds.
  Depends on: `cli/src/envelope.rs`, `cli/src/exit.rs`,
  `controller/src/clock.rs`, `controller/src/config.rs`,
  `controller/src/events.rs`, `controller/src/platform/mod.rs`,
  `controller/src/policy.rs`,
  `controller/src/project/mod.rs`, `controller/src/projection.rs`,
  `controller/src/routines/mod.rs`, `controller/src/runs.rs`,
  `core/src/item/mod.rs`, `core/src/item/rules.rs`, `core/src/policy.rs`.
- `cli/src/stream.rs` — Runs `fleet event tail` and `fleet event show`, the
  readers of the event stream.
  Depends on: `cli/src/envelope.rs`, `cli/src/exit.rs`,
  `controller/src/config.rs`, `controller/src/events.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/stream.rs`,
  `controller/src/routines/load.rs`, `core/src/item/mod.rs`,
  `core/src/seat/actor.rs`.
- `controller/src/events.rs` — Appends to and reads the event stream,
  `events.jsonl`, one sequenced JSON object per line.
  Depends on: `controller/src/clock.rs`, `controller/src/projection.rs`,
  `core/src/seat/identity.rs`.
- `controller/src/project/stream.rs` — Appends the events core's verbs state to
  the machine directory's stream.
  Depends on: `controller/src/events.rs`, `core/src/item/mod.rs`,
  `core/src/item/run.rs`, `core/src/seat/actor.rs`.
- `controller/src/projection.rs` — Writes and reads `projection.json`, what the
  controller observed at its last poll.
  Depends on: `controller/src/clock.rs`, `controller/src/config.rs`,
  `controller/src/effect.rs`, `controller/src/observe.rs`,
  `controller/src/policy.rs`, `controller/src/routines/mod.rs`,
  `controller/src/seat.rs`, `core/src/agent/mod.rs`,
  `core/src/seat/identity.rs`.
- `core/src/item/rules.rs` — Defines the per-item rules
  (`[[core.flight.rules]]`) that `fleet status` prints.
  Depends on: nothing in fleet.

## Flow

1. `cli/src/main.rs` hands `fleet status` to `cli/src/status.rs`.
2. `cli/src/status.rs` reads the projection the controller last published
   through `controller/src/projection.rs`, and refuses without one.
3. It reads the policy in force, and the item rules `core/src/item/rules.rs`
   defines.
4. It reads the runs off the event stream through `controller/src/events.rs`,
   and the open holds off each registered project's store.
5. It prints the roster, the rules, the routines, the runs and the holds, and
   writes nothing.

## Depends on other areas

- seats — the seat list, the seat events and the verdicts the projection
  reports.
- runs — the run events the runs section is read from.
- routines — the routines section and the routine events.
- items — the stop a refusal is carried in.
- agent — the agent's reading of each seat in the projection.
- shared — the exit table, the envelope, the clock, the platform layer and the
  policy.
