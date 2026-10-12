# Seats and the controller

A seat is fleet's worker: an agent's session in a terminal, or a person. This
area covers seat ids, names and this machine's identity, `fleet seat add`, the
controller's poll loop (`fleet observe`) that watches every seat and acts on it,
named and transient seats, rest and halt, and `fleet seat nudge` and `fleet seat
attach`.

## Modules

- `cli/src/attach.rs` — Runs `fleet seat attach`, which opens a seat's session
  in the person's own terminal.
  Depends on: `cli/src/exit.rs`, `cli/src/transient.rs`,
  `controller/src/clock.rs`, `controller/src/config.rs`,
  `controller/src/events.rs`, `controller/src/host/mod.rs`,
  `controller/src/host/tmux.rs`, `controller/src/platform/mod.rs`,
  `controller/src/project/wiring.rs`.
- `cli/src/nudge.rs` — Runs `fleet seat nudge`, which types one message into a
  live seat's session from outside it.
  Depends on: `cli/src/exit.rs`, `cli/src/item.rs`, `cli/src/seams.rs`,
  `cli/src/transient.rs`, `cli/src/ui.rs`, `controller/src/effect.rs`,
  `controller/src/events.rs`, `controller/src/observe.rs`,
  `controller/src/project/stream.rs`, `controller/src/project/wiring.rs`,
  `controller/src/projection.rs`, `controller/src/seat.rs`.
- `cli/src/seat_add.rs` — Runs `fleet seat add`, which lists a new seat in the
  fleet's `fleet.toml`.
  Depends on: `cli/src/envelope.rs`, `cli/src/exit.rs`, `cli/src/transient.rs`,
  `controller/src/platform/mod.rs`, `core/src/fs.rs`, `core/src/item/mod.rs`,
  `core/src/seat/identity.rs`.
- `cli/src/transient.rs` — Takes the arguments of `fleet seat spawn`, `feed` and
  `retire`, and hands them to the controller's transient-seat primitives.
  Depends on: `cli/src/envelope.rs`, `cli/src/exit.rs`, `cli/src/item.rs`,
  `controller/src/clock.rs`, `controller/src/host/mod.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/mod.rs`,
  `controller/src/project/wiring.rs`, `controller/src/sessions.rs`,
  `controller/src/transient.rs`, `core/src/entry.rs`,
  `core/src/item/land/mod.rs`, `core/src/item/mod.rs`,
  `core/src/seat/identity.rs`, `core/src/seat/mod.rs`, `core/src/store/mod.rs`.
- `controller/src/config.rs` — Reads and writes `config.json`, the machine's
  list of managed seats.
  Depends on: `controller/src/platform/mod.rs`, `core/src/fs.rs`,
  `core/src/seat/identity.rs`.
- `controller/src/decide.rs` — Turns what one poll saw of a seat into one
  verdict for the effects layer.
  Depends on: `controller/src/observe.rs`.
- `controller/src/effect.rs` — Carries out each seat's verdict, and writes the
  event for what it did.
  Depends on: `controller/src/events.rs`, `controller/src/host/mod.rs`,
  `controller/src/platform/mod.rs`, `controller/src/policy.rs`,
  `controller/src/sessions.rs`, `core/src/agent/mod.rs`, `core/src/item/run.rs`,
  `core/src/seat/actor.rs`, `core/src/seat/identity.rs`.
- `controller/src/host/mod.rs` — Defines the host, where a seat's session runs,
  and what the controller asks of it.
  Depends on: `core/src/seat/identity.rs`.
- `controller/src/host/tmux.rs` — Answers the host with tmux, on fleet's own
  server.
  Depends on: `controller/src/host/mod.rs`, `controller/src/platform/mod.rs`,
  `core/src/process.rs`.
- `controller/src/observe.rs` — Reads one poll's view of each seat: its
  session's presence on the host and its activity from the agent.
  Depends on: `controller/src/config.rs`, `controller/src/host/mod.rs`,
  `controller/src/projection.rs`, `core/src/agent/mod.rs`,
  `core/src/seat/identity.rs`.
- `controller/src/project/wiring.rs` — Gathers what a transient-seat verb and
  the run pass need from a resolved project: its directories, agent, host and
  policy.
  Depends on: `controller/src/config.rs`, `controller/src/host/mod.rs`,
  `controller/src/platform/mod.rs`, `controller/src/policy.rs`,
  `controller/src/project/mod.rs`, `controller/src/transient.rs`,
  `core/src/agent/mod.rs`, `core/src/item/mod.rs`, `core/src/policy.rs`.
- `controller/src/run.rs` — Runs `fleet observe`, the controller's poll loop,
  which observes, decides, acts and publishes.
  Depends on: `controller/src/clock.rs`, `controller/src/config.rs`,
  `controller/src/decide.rs`, `controller/src/effect.rs`,
  `controller/src/events.rs`, `controller/src/host/mod.rs`,
  `controller/src/observe.rs`, `controller/src/platform/mod.rs`,
  `controller/src/policy.rs`, `controller/src/projection.rs`,
  `controller/src/routines/mod.rs`, `controller/src/runs.rs`,
  `controller/src/sessions.rs`, `core/src/agent/mod.rs`, `core/src/fs.rs`,
  `core/src/item/mod.rs`, `core/src/seat/identity.rs`.
- `controller/src/seat.rs` — Writes the seat events (woke, rest, handed off,
  exited) that tell the controller where a seat is in its life.
  Depends on: `controller/src/config.rs`, `controller/src/events.rs`,
  `controller/src/projection.rs`, `core/src/seat/identity.rs`.
- `controller/src/sessions.rs` — Keeps `sessions.json`, the controller's record
  of the sessions it started.
  Depends on: `controller/src/clock.rs`, `controller/src/decide.rs`,
  `controller/src/events.rs`, `controller/src/platform/mod.rs`,
  `core/src/agent/mod.rs`, `core/src/fs.rs`, `core/src/seat/identity.rs`.
- `controller/src/transient.rs` — Spawns, feeds and retires transient seats,
  each with its own worktree and session.
  Depends on: `controller/src/config.rs`, `controller/src/effect.rs`,
  `controller/src/events.rs`, `controller/src/host/mod.rs`,
  `controller/src/platform/mod.rs`, `controller/src/policy.rs`,
  `controller/src/projection.rs`, `controller/src/runs.rs`,
  `controller/src/sessions.rs`, `core/src/agent/mod.rs`, `core/src/item/mod.rs`,
  `core/src/process.rs`, `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/seat/retire.rs`, `core/src/store/mod.rs`.
- `core/src/seat/actor.rs` — Defines who acts: a seat, a run, a routine or the
  controller, as one typed `<kind>:<id>`.
  Depends on: `core/src/seat/identity.rs`.
- `core/src/seat/identity.rs` — Defines what a seat is (its id, kind and name),
  the `[seats]` table that lists it, and this machine's identity.
  Depends on: `core/src/item/mod.rs`.
- `core/src/seat/mod.rs` — Groups the record half of the seat verbs: identity,
  actor and retire.
  Depends on: nothing in fleet.
- `core/src/seat/retire.rs` — Withdraws the orders a retiring seat still holds,
  so its items can be dispatched again.
  Depends on: `core/src/entry.rs`, `core/src/item/deliver.rs`,
  `core/src/item/mod.rs`, `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.

## Flow

1. `controller/src/run.rs` starts a poll: it reads the seat list from
   `controller/src/config.rs`, and the policy.
2. `controller/src/observe.rs` asks the host (`controller/src/host/mod.rs`,
   answered by `controller/src/host/tmux.rs`) whether each seat's session is
   there, and asks the agent what each one is doing.
3. `controller/src/run.rs` folds in the seat events `controller/src/seat.rs`
   wrote since the last poll.
4. `controller/src/decide.rs` turns each seat's reading into one verdict.
5. `controller/src/effect.rs` carries the verdicts out, such as waking, nudging
   or resting a seat, and records what it started in
   `controller/src/sessions.rs`.
6. `controller/src/run.rs` publishes the projection and writes the events for
   what changed.

## Depends on other areas

- agent — asking each seat's agent what it is doing and how full its context is.
- items — the orders a retire withdraws, the item verbs behind the ring, and the
  landed branch a retire may release.
- runs — the run pass the poll loop drives.
- routines — the routines the poll loop evaluates on its tick.
- status — the event stream it writes and the projection it publishes.
- store — withdrawing a retired seat's orders.
- shared — the clock, the platform layer, the policy, the process runner and the
  exit table.
