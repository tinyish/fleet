# Runs and workflows

A run is one execution of a workflow a pack ships, started with `fleet run` and
stopped with `fleet cancel`. This area covers a run's directory and pinned
inputs, the record item it files, the steps a workflow marks on the stream, and
the controller re-running a run that waits on something. The arguments of `fleet
run` and `fleet cancel` are taken by `cli/src/item.rs`, in the items area.

## Modules

- `cli/src/step.rs` — Runs `fleet event step`, which a workflow's process calls
  to mark a step started or closed.
  Depends on: `cli/src/exit.rs`, `cli/src/item.rs`, `controller/src/events.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/stream.rs`,
  `core/src/item/mod.rs`, `core/src/seat/actor.rs`.
- `controller/src/runs.rs` — Carries the controller's half of a run: it re-runs
  a waiting run when its wake arrives, holds one that keeps failing, and lets go
  of a finished run's seats.
  Depends on: `controller/src/clock.rs`, `controller/src/config.rs`,
  `controller/src/events.rs`, `controller/src/lifecycle.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/mod.rs`,
  `controller/src/project/stream.rs`, `controller/src/project/wiring.rs`,
  `controller/src/transient.rs`, `core/src/entry.rs`, `core/src/item/brief.rs`,
  `core/src/item/hold.rs`, `core/src/item/mod.rs`, `core/src/item/run.rs`,
  `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.
- `core/src/item/pins.rs` — Writes a run's pinned files into its directory, and
  reads the directory's hash back off the disk.
  Depends on: `core/src/digest.rs`, `core/src/item/mod.rs`.
- `core/src/item/run.rs` — Runs `fleet run` and `fleet cancel`: it resolves the
  workflow, pins its inputs, files the run's record and starts the pack's run
  command.
  Depends on: `core/src/entry.rs`, `core/src/item/brief.rs`,
  `core/src/item/doctor.rs`, `core/src/item/mod.rs`, `core/src/item/pins.rs`,
  `core/src/pack.rs`, `core/src/policy.rs`, `core/src/resolve.rs`,
  `core/src/runtime.rs`, `core/src/seat/actor.rs`, `core/src/settings.rs`,
  `core/src/store/mod.rs`.

## Flow

1. `cli/src/main.rs` hands `fleet run <workflow>` to `cli/src/item.rs`, which
   resolves the project and calls `core/src/item/run.rs`.
2. `core/src/item/run.rs` resolves the workflow through the pack layers, reads
   the owning pack's runtime and runs its doctor, all before it writes anything.
3. `core/src/item/run.rs` files the run's record item in the store and makes the
   run's directory, where `core/src/item/pins.rs` writes the pinned inputs and
   hashes them.
4. `core/src/item/run.rs` starts the pack's run command and reads the run's
   outcome from its exit.
5. While it runs, the workflow marks its steps with `fleet event step`, which
   `cli/src/step.rs` appends to the stream.
6. A run left waiting is taken up by `controller/src/runs.rs` on the
   controller's tick, which re-runs it when its wake arrives and lets go of its
   seats when it ends.

## Depends on other areas

- items — the stop, the brief, the hold and the entries a run writes, and
  `cli/src/item.rs`, which takes its arguments.
- packs — resolving the workflow, its runtime and the pack's settings.
- onboarding — the doctor a run's pack must pass, and the project register.
- seats — the actor a run acts as, and the transient seats it spawns and
  retires.
- status — the event stream a run's steps and wakes are written to.
- store — the run's record item and its holds.
- shared — the clock, digests, the platform layer, the policy and the exit
  table.
