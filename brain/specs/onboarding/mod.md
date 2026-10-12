# Onboarding

Getting fleet onto a machine and into a project: checking what it runs on with
`fleet doctor`, `fleet create` and the agent's and store's packs it installs,
and starting and stopping the controller with `fleet start` and `fleet stop`. It
also covers `fleet prime`, what a seat's session is handed when it starts.

## Feature specs

- `brain/specs/onboarding/doctor.md` — `fleet doctor`: running the doctor checks
  the pack layers declare, and its verdict. Not written yet.
- `brain/specs/onboarding/create.md` — `fleet create`: writing a fleet, embedded
  or standalone, and installing its store's and agent's packs. Not written yet.
- `brain/specs/onboarding/start.md` — `fleet start`: bringing the controller up
  as a service, confirmed from the stream. Not written yet.
- `brain/specs/onboarding/stop.md` — `fleet stop`: taking the controller down,
  leaving every seat's session running. Not written yet.
- `brain/specs/onboarding/prime.md` — `fleet prime`: what a seat's session is
  handed when it starts. Not written yet.

## Modules

- `cli/src/doctor.rs` — Runs `fleet doctor`: every doctor check the pack layers
  carry, on demand, one row each.
  Depends on: `cli/src/envelope.rs`, `cli/src/exit.rs`, `cli/src/item.rs`,
  `cli/src/ui.rs`, `controller/src/platform/mod.rs`,
  `controller/src/project/mod.rs`, `core/src/agent/mod.rs`,
  `core/src/item/brief.rs`, `core/src/item/doctor.rs`, `core/src/item/mod.rs`,
  `core/src/item/run.rs`, `core/src/runtime.rs`, `core/src/store/mod.rs`,
  `core/src/store/types.rs`.
- `cli/src/lifecycle.rs` — Takes the arguments and questions of `fleet create`,
  `fleet start` and `fleet stop`, and installs the agent's and the store's packs
  at create.
  Depends on: `cli/src/exit.rs`, `cli/src/seat_add.rs`, `cli/src/ui.rs`,
  `controller/src/clock.rs`, `controller/src/config.rs`,
  `controller/src/events.rs`, `controller/src/host/mod.rs`,
  `controller/src/lifecycle.rs`, `controller/src/platform/mod.rs`,
  `controller/src/policy.rs`, `controller/src/project/mod.rs`,
  `core/src/add.rs`, `core/src/agent/mod.rs`, `core/src/defaults.rs`,
  `core/src/item/mod.rs`, `core/src/lock.rs`, `core/src/pack.rs`,
  `core/src/policy.rs`, `core/src/seat/identity.rs`, `core/src/store/mod.rs`,
  `core/src/supported.rs`.
- `cli/src/prime.rs` — Prints what a seat's session is handed at its start, for
  `fleet prime`.
  Depends on: `cli/src/exit.rs`, `cli/src/guard.rs`, `controller/src/config.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/mod.rs`,
  `core/src/add.rs`, `core/src/defaults.rs`, `core/src/guard/mod.rs`,
  `core/src/lock.rs`, `core/src/resolve.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`, `core/src/store/types.rs`.
- `controller/src/lifecycle.rs` — Does the work of `fleet create`, `fleet start`
  and `fleet stop`: the machine directory, the seat list, the service file and
  the project register.
  Depends on: `controller/src/config.rs`, `controller/src/events.rs`,
  `controller/src/platform/mod.rs`, `controller/src/policy.rs`,
  `controller/src/projection.rs`, `core/src/agent/mod.rs`, `core/src/fs.rs`,
  `core/src/seat/identity.rs`.
- `core/src/item/doctor.rs` — Finds the doctor checks the pack layers declare,
  runs one, and reads its verdict from its exit.
  Depends on: `core/src/item/brief.rs`, `core/src/item/mod.rs`,
  `core/src/pack.rs`, `core/src/process.rs`, `core/src/resolve.rs`,
  `core/src/runtime.rs`.
- `core/src/supported.rs` — Names the releases of the tools fleet runs on that
  this binary supports: the fleet-packs tag and the tmux floor.
  Depends on: nothing in fleet.

## Flow

1. `cli/src/main.rs` hands `fleet create` to `cli/src/lifecycle.rs`, which asks
   its three questions: the fleet's mode, its agent and its store.
2. `controller/src/lifecycle.rs` writes `fleet.toml`, the project's declaration
   and the seat list, and registers the project on this machine.
3. `cli/src/lifecycle.rs` has `core/src/defaults.rs` (packs) write the defaults
   `core/src/embedded.rs` carries into the machine directory.
4. `cli/src/lifecycle.rs` installs the agent's and the store's packs through
   `core/src/add.rs`, at the fleet-packs tag `core/src/supported.rs` names.

## Depends on other areas

- packs — writing the defaults, installing the agent's and store's packs, the
  lock and the layers.
- items — the pack layers the brief reads through, and the stop a verb refuses
  with.
- runs — the runtime-version check `fleet doctor` shares with `fleet run`.
- seats — the seat list, this machine's identity and the host.
- store — opening the store a fleet names.
- agent — opening the agent a fleet names.
- guards — the guard classes `fleet prime` lists.
- status — the event stream and projection `fleet start` and `fleet stop`
  confirm by.
- shared — the exit table, the terminal, the platform layer, the policy and the
  process runner.
