# Routines

Routines are standing duties: a routine file names a trigger (a cron schedule, a
cooldown or a condition) and an action (ring a seat, file an item or run a
command), and the controller fires it when it is due. This area covers routine
files, their state, and `fleet routine list`, `check`, `run` and `history`. It
is an area of its own because no docs area covers routines; the packs, status
and exit-code docs mention them only in passing.

## Feature specs

- `brain/specs/routines/files.md` — a routine file: its trigger and its action.
  Not written yet.
- `brain/specs/routines/firing.md` — the controller firing a due routine, its
  state, and the events it writes. Not written yet.
- `brain/specs/routines/list.md` — `fleet routine list`: every routine loaded,
  its next due and its last outcome. Not written yet.
- `brain/specs/routines/check.md` — `fleet routine check`: whether a routine is
  due now, without firing it. Not written yet.
- `brain/specs/routines/run.md` — `fleet routine run`: firing one routine now.
  Not written yet.
- `brain/specs/routines/history.md` — `fleet routine history`: the routine
  events on the stream. Not written yet.

## Modules

- `cli/src/routines.rs` — Runs `fleet routine list`, `check`, `run` and
  `history`.
  Depends on: `cli/src/exit.rs`, `controller/src/clock.rs`,
  `controller/src/config.rs`, `controller/src/events.rs`,
  `controller/src/host/mod.rs`, `controller/src/observe.rs`,
  `controller/src/platform/mod.rs`, `controller/src/policy.rs`,
  `controller/src/project/wiring.rs`, `controller/src/projection.rs`,
  `controller/src/routines/action.rs`, `controller/src/routines/file.rs`,
  `controller/src/routines/load.rs`, `controller/src/routines/mod.rs`,
  `controller/src/routines/state.rs`, `controller/src/routines/trigger.rs`,
  `controller/src/sessions.rs`, `core/src/agent/mod.rs`, `core/src/item/mod.rs`,
  `core/src/seat/identity.rs`.
- `controller/src/routines/action.rs` — Carries out a due routine: it rings a
  seat, files an item or runs a command.
  Depends on: `controller/src/effect.rs`, `controller/src/host/mod.rs`,
  `controller/src/observe.rs`, `controller/src/platform/mod.rs`,
  `controller/src/policy.rs`, `controller/src/routines/file.rs`,
  `controller/src/routines/mod.rs`, `controller/src/routines/state.rs`,
  `controller/src/routines/trigger.rs`, `core/src/agent/mod.rs`,
  `core/src/defaults.rs`, `core/src/process.rs`, `core/src/seat/actor.rs`,
  `core/src/store/mod.rs`.
- `controller/src/routines/file.rs` — Reads one routine file,
  `orders/<name>.toml`, into a routine or into the defect that says why it is
  not one.
  Depends on: `controller/src/routines/trigger.rs`, `core/src/seat/identity.rs`,
  `core/src/store/types.rs`.
- `controller/src/routines/load.rs` — Loads the routines of the fleet install,
  every installed pack and every project.
  Depends on: `controller/src/config.rs`, `controller/src/project/mod.rs`,
  `controller/src/routines/file.rs`, `core/src/defaults.rs`,
  `core/src/seat/identity.rs`.
- `controller/src/routines/mod.rs` — Evaluates the routines on the controller's
  tick, and fires the ones that are due.
  Depends on: `controller/src/clock.rs`, `controller/src/events.rs`,
  `controller/src/observe.rs`, `core/src/seat/identity.rs`.
- `controller/src/routines/state.rs` — Keeps `orders/state.json`, when each
  routine was last asked and when it last fired.
  Depends on: `controller/src/platform/mod.rs`, `core/src/fs.rs`.
- `controller/src/routines/trigger.rs` — Decides whether a routine's trigger
  (cron, cooldown or condition) is due at a given moment.
  Depends on: `controller/src/clock.rs`, `controller/src/routines/file.rs`.

## Flow

1. On each tick, `controller/src/run.rs` (seats) has
   `controller/src/routines/load.rs` load the routines of the fleet install,
   every installed pack and every project.
2. `controller/src/routines/file.rs` reads each `orders/<name>.toml`, and names
   a defect in one it cannot read.
3. `controller/src/routines/mod.rs` reads each routine's last run from
   `controller/src/routines/state.rs`, and asks
   `controller/src/routines/trigger.rs` whether it is due.
4. `controller/src/routines/action.rs` carries out a due routine: it rings a
   seat, files an item or runs a command.
5. `controller/src/routines/mod.rs` writes the firing and its outcome on the
   stream, and `controller/src/routines/state.rs` records it.

## Depends on other areas

- seats — the seat list, the host and ring a routine's nudge uses, the session
  table and the actor a routine acts as.
- store — filing a routine's item.
- items — the stop a refusal is carried in.
- agent — opening the agent a ring uses.
- packs — the defaults directory, one of a routine's roots.
- status — the event stream a firing is written to, and the projection `fleet
  routine` reads.
- shared — the clock, the platform layer, the policy, the process runner and the
  exit table.
