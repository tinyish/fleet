# The architecture map

This is the map's overview: fleet's three crates, its areas and the folder for
each, how a request crosses from one area to the next, and which crate and
area may depend on which. It owns no modules. Each area's own `mod.md` lists
the modules it owns, what each one does and depends on, and how work flows
through them.

## Crates

- **core** (`fleet-core`): pack-and-project work. The item verbs and the
  record, packs and their layers, the guard classes, and the store and agent
  contracts. It never reads the process table.
- **controller** (`fleet-controller`): the long-running half. The poll loop
  that watches every seat and acts on it, the hosts sessions run on,
  transient seats, routines, the run pass, the event stream and the projection,
  and everything the operating system supplies.
- **cli** (`fleet-cli`): the `fleet` binary. Its arguments, its exit codes, how
  it looks in a terminal, and what only a process knows.

## Areas

| Folder | What it covers | Crates |
| --- | --- | --- |
| `packs/` | installing, checking and layering packs, the defaults the binary carries, a pack's runtime and settings | core, cli |
| `items/` | the item verbs from `dispatch` to `land`, the item's record, and `fleet item` | core, cli |
| `runs/` | `fleet run` and `fleet cancel`, a run's pins and steps, and the controller's run pass | core, controller, cli |
| `seats/` | seat identity, the controller's poll loop, named and transient seats, `nudge` and `attach` | core, controller, cli |
| `guards/` | `fleet guard` and the four guard classes | core, cli |
| `status/` | `fleet status`, the projection, and the event stream | core, controller, cli |
| `store/` | the store contract, its adapter call, `fleet store check` and `schema` | core, cli |
| `agent/` | the agent contract, its adapter call, `fleet agent check` and `schema` | core, controller, cli |
| `routines/` | routine files, their triggers, actions and state, and `fleet routine` | controller, cli |
| `shared/` | plumbing no single area owns, the exit table and the `--json` envelope, and the test fakes | core, controller, cli |

## How a request crosses areas

An item's life, from `fleet dispatch` to `fleet land`:

1. **shared** parses the command line and resolves the project the call acts
   in.
2. **items** reads the item through **store**, writes the order on its
   timeline, and renders the seat's brief from templates **packs** resolves,
   naming the **guards** in force.
3. **items** rings the named seat's session, or **seats** spawns a transient
   seat with its own worktree, started with the argv the **agent** contract
   gives.
4. While the seat works, **guards** judge each shell command its agent's hook
   hands them, and the controller's poll in **seats** asks the **agent** what
   the seat is doing.
5. The seat runs `fleet deliver`: **items** commits its work, records the
   delivery through **store**, and hands the item to its reviewer.
6. The reviewer runs `fleet review`: **items** writes the verdict on the
   item's timeline through **store**.
7. The reviewer runs `fleet land`: **items** takes the project's lane,
   squashes the accepted commit onto the trunk, runs the suite, pushes, and
   records the landing through **store**.
8. **seats** retires a transient seat and releases its landed branch. Every
   step writes its events to the stream **status** reads, and the
   controller's projection is what `fleet status` prints.

A run (**runs**) drives the same verbs from a workflow, and a routine
(**routines**) can file an item, ring a seat or start a command on a trigger.

## Allowed dependencies

Between crates:

- `core` → nothing in fleet.
- `controller` → `core`.
- `cli` → `core` and `controller`.

Between areas, every area may depend on **shared**. Beyond that, these are the
edges the code has today (2026-10-12):

- **onboarding** → agent, guards, items, packs, runs, seats, status, store.
- **packs** → guards, items.
- **items** → agent, guards, packs, runs, seats, store.
- **runs** → items, onboarding, packs, seats, status, store.
- **seats** → agent, items, routines, runs, status, store.
- **guards** → items, packs, seats, store.
- **status** → agent, items, routines, runs, seats.
- **store** → items, packs, seats.
- **agent** → items, packs, seats, store.
- **routines** → agent, items, packs, seats, status, store.
- **shared** → agent, guards, items, onboarding, packs, runs, seats, status,
  store.

An area's modules also keep to their crate's edges: a module in `core` depends
on no module in `controller` or `cli`, whatever its area.
