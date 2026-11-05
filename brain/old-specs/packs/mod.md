# Packs

A pack is a folder of agents, skills, routines, workflows, health checks and
adapters that a fleet installs. This area covers checking, adding, removing and
listing packs, the defaults every fleet gets, which travel inside the binary,
how installed packs layer over the defaults and shadow each other, the runtime a
pack pins, and pack settings in `fleet.toml`.

## Feature specs

- `brain/specs/packs/add.md` — `fleet pack add`: fetching a pack and its imports
  at a version, pinned in `packs.lock`. Not written yet.
- `brain/specs/packs/remove.md` — `fleet pack remove`: taking an installed pack
  out and dropping its pin. Not written yet.
- `brain/specs/packs/list.md` — `fleet pack list`: printing the installed packs
  from `packs.lock`. Not written yet.
- `brain/specs/packs/check.md` — `fleet pack check`: the pack format, and each
  defect a pack has, named. Not written yet.
- `brain/specs/packs/layering.md` — how installed packs order over the defaults,
  import one level deep, and shadow each other. Not written yet.
- `brain/specs/packs/defaults.md` — the defaults every fleet gets from the
  binary, written into the machine directory. Not written yet.
- `brain/specs/packs/runtime.md` — the runtime a pack pins or imports for its
  workflows. Not written yet.
- `brain/specs/packs/settings.md` — a pack's declared settings, and the values a
  fleet sets for them in `fleet.toml`. Not written yet.

## Modules

- `cli/src/pack.rs` — Runs `fleet pack check`, `add`, `remove` and `list`.
  Depends on: `cli/src/exit.rs`, `cli/src/ui.rs`, `controller/src/clock.rs`,
  `controller/src/platform/mod.rs`, `core/src/add.rs`, `core/src/defaults.rs`,
  `core/src/lock.rs`, `core/src/pack.rs`, `core/src/remove.rs`,
  `core/src/resolve.rs`.
- `core/src/add.rs` — Fetches one pack by git source and version, checks it
  against what is installed, and pins it in the lock.
  Depends on: `core/src/lock.rs`, `core/src/pack.rs`, `core/src/process.rs`,
  `core/src/resolve.rs`.
- `core/src/defaults.rs` — Writes the binary's embedded defaults into the
  machine directory as the bottom pack layer, and pins them in the lock.
  Depends on: `core/src/digest.rs`, `core/src/embedded.rs`, `core/src/lock.rs`,
  `core/src/pack.rs`.
- `core/src/embedded.rs` — Holds the files of the crate's `defaults/` tree that
  travel inside the binary.
  Depends on: `core/build.rs`.
- `core/src/lock.rs` — Reads and writes `packs.lock`, the record of each
  installed pack's source, version and commit.
  Depends on: `core/src/fs.rs`.
- `core/src/pack.rs` — Defines the pack format: its top-level names, its
  manifest, its adapters and the rules each slot follows.
  Depends on: `core/src/guard/hook.rs`, `core/src/guard/mod.rs`,
  `core/src/item/brief.rs`, `core/src/os_litter.rs`, `core/src/process.rs`,
  `core/src/registry.rs`, `core/src/resolve.rs`.
- `core/src/registry.rs` — Reads the shadow registry, the list of files a pack
  on top may replace.
  Depends on: nothing in fleet.
- `core/src/remove.rs` — Removes an installed pack by the source it was added
  with.
  Depends on: `core/src/defaults.rs`, `core/src/lock.rs`, `core/src/pack.rs`,
  `core/src/resolve.rs`.
- `core/src/resolve.rs` — Orders the installed packs over the defaults into
  layers, and resolves each relative path to one file.
  Depends on: `core/src/defaults.rs`, `core/src/pack.rs`,
  `core/src/registry.rs`.
- `core/src/runtime.rs` — Reads the `[runtime]` table a pack pins or imports,
  and gives the `PATH` that pack's children run on.
  Depends on: `core/src/add.rs`, `core/src/item/mod.rs`, `core/src/lock.rs`,
  `core/src/pack.rs`, `core/src/process.rs`, `core/src/resolve.rs`.
- `core/src/settings.rs` — Checks the values a fleet sets for a pack in
  `fleet.toml` against the keys that pack's manifest declares.
  Depends on: `core/src/pack.rs`, `core/src/resolve.rs`.

## Flow

1. `cli/src/main.rs` hands `fleet pack add <source> <version>` to
   `cli/src/pack.rs`, which finds the machine's packs folder and lock.
2. `core/src/add.rs` fetches the pack with git at that version, and reads its
   manifest through `core/src/pack.rs`.
3. `core/src/add.rs` checks the pack against the installed layers, which
   `core/src/resolve.rs` orders over the defaults and holds to
   `core/src/registry.rs`'s shadow list.
4. `core/src/lock.rs` adds the pack's source, version and commit to
   `packs.lock`.

## Depends on other areas

- items — the pack-layer type the brief defines, and the stop a verb refuses
  with.
- guards — the hook table and guard classes a pack's manifest declares.
- shared — the process runner, atomic writes, the OS litter list, the clock, the
  platform layer and the exit table.
