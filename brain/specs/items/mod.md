# Items and the record

An item is a unit of work kept in the store. This area covers the verbs an item
moves through (`dispatch`, `brief`, `deliver`, `hold`, `clear`, `review` and
`land`), the entry each one appends to the item's timeline, the landing lane,
and `fleet item show` and `fleet item list`.

## Feature specs

- `brain/specs/items/dispatch.md` — `fleet dispatch`: giving a ready item to a
  seat, or asking for a transient one. Not written yet.
- `brain/specs/items/brief.md` — `fleet brief`: the first turn a dispatched seat
  reads. Not written yet.
- `brain/specs/items/deliver.md` — `fleet deliver`: a seat commits its work and
  hands the item to its reviewer. Not written yet.
- `brain/specs/items/hold.md` — `fleet hold`: a seat stops on a question for a
  person. Not written yet.
- `brain/specs/items/clear.md` — `fleet clear`: a person answers a hold, and the
  item is ready again. Not written yet.
- `brain/specs/items/review.md` — `fleet review`: reading a delivery, then
  returning it or accepting it. Not written yet.
- `brain/specs/items/land.md` — `fleet land`: squashing an accepted commit onto
  the trunk through the project's lane. Not written yet.
- `brain/specs/items/show.md` — `fleet item show`: one item with its fields and
  its timeline. Not written yet.
- `brain/specs/items/list.md` — `fleet item list`: the items the store's
  listings answer. Not written yet.

## Modules

- `cli/src/item.rs` — Takes the arguments of the item verbs and of `fleet run`
  and `fleet cancel`, and hands core what only a process knows.
  Depends on: `cli/src/envelope.rs`, `cli/src/exit.rs`, `cli/src/seams.rs`,
  `cli/src/transient.rs`, `cli/src/ui.rs`, `controller/src/clock.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/mod.rs`,
  `core/src/item/brief.rs`, `core/src/item/deliver.rs`,
  `core/src/item/dispatch.rs`, `core/src/item/hold.rs`,
  `core/src/item/land/mod.rs`, `core/src/item/mod.rs`,
  `core/src/item/review.rs`, `core/src/item/run.rs`, `core/src/seat/actor.rs`,
  `core/src/seat/identity.rs`.
- `cli/src/item_show.rs` — Runs `fleet item show` and `fleet item list`, reading
  the store and printing what core renders.
  Depends on: `cli/src/envelope.rs`, `cli/src/exit.rs`,
  `controller/src/project/mod.rs`, `core/src/item/list.rs`,
  `core/src/item/mod.rs`, `core/src/item/show.rs`, `core/src/store/mod.rs`.
- `cli/src/seams.rs` — Answers core's seams from the binary: the ring that types
  into a seat's session, the box load and git.
  Depends on: `cli/src/ui.rs`, `controller/src/config.rs`,
  `controller/src/effect.rs`, `controller/src/host/mod.rs`,
  `controller/src/platform/mod.rs`, `controller/src/policy.rs`,
  `controller/src/project/mod.rs`, `controller/src/project/wiring.rs`,
  `controller/src/sessions.rs`, `controller/src/transient.rs`,
  `core/src/agent/mod.rs`, `core/src/item/land/mod.rs`, `core/src/item/lane.rs`,
  `core/src/item/mod.rs`, `core/src/process.rs`.
- `core/src/entry.rs` — Defines an item's record: the typed entries a store
  keeps on its timeline, and the fold every reader asks.
  Depends on: `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.
- `core/src/input.rs` — Reads what a seat hands in (a delivery, a question, a
  reviewer's findings) against the binary's own types.
  Depends on: `core/src/entry.rs`, `core/src/item/mod.rs`.
- `core/src/item/brief.rs` — Renders `fleet brief`, the first thing a dispatched
  seat reads, whole or not at all.
  Depends on: `core/src/guard/mod.rs`, `core/src/input.rs`,
  `core/src/item/mod.rs`, `core/src/item/show.rs`, `core/src/resolve.rs`,
  `core/src/store/mod.rs`.
- `core/src/item/deliver.rs` — Runs `fleet deliver`, the seat's handoff, which
  commits its work and hands the item to its reviewer.
  Depends on: `core/src/entry.rs`, `core/src/input.rs`,
  `core/src/item/brief.rs`, `core/src/item/dispatch.rs`, `core/src/item/mod.rs`,
  `core/src/policy.rs`, `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.
- `core/src/item/dispatch.rs` — Runs `fleet dispatch`, which writes the order
  that lets a seat begin an item and then rings the seat or asks for a transient
  one.
  Depends on: `core/src/entry.rs`, `core/src/fs.rs`, `core/src/item/brief.rs`,
  `core/src/item/deliver.rs`, `core/src/item/mod.rs`, `core/src/item/show.rs`,
  `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`, `core/src/store/types.rs`.
- `core/src/item/events.rs` — Names the events the item verbs and a run write on
  the stream, and the seam they write through.
  Depends on: `core/src/seat/actor.rs`.
- `core/src/item/hold.rs` — Runs `fleet hold` and `fleet clear`: a seat's
  blocking question raised as a hold, and the answer that clears it.
  Depends on: `core/src/entry.rs`, `core/src/input.rs`,
  `core/src/item/deliver.rs`, `core/src/item/dispatch.rs`,
  `core/src/item/mod.rs`, `core/src/seat/actor.rs`, `core/src/store/mod.rs`.
- `core/src/item/land/branch.rs` — Classifies the work branch at a landing, and
  tells a retire whether that branch may be released.
  Depends on: `core/src/entry.rs`, `core/src/item/land/mod.rs`,
  `core/src/item/mod.rs`, `core/src/item/show.rs`.
- `core/src/item/land/licence.rs` — Decides what licenses a landing: the accept
  it lands and, for a run, the reviewer's clearance.
  Depends on: `core/src/entry.rs`, `core/src/item/mod.rs`,
  `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.
- `core/src/item/land/mod.rs` — Runs `fleet land`, which squashes an accepted
  commit onto the trunk and records the landing.
  Depends on: `core/src/item/deliver.rs`, `core/src/item/lane.rs`,
  `core/src/item/mod.rs`, `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.
- `core/src/item/land/porcelain.rs` — Reads what git printed during a landing:
  the push's range line and the changed paths.
  Depends on: `core/src/item/land/mod.rs`, `core/src/item/mod.rs`,
  `core/src/store/types.rs`.
- `core/src/item/land/rows.rs` — Prints a landing's check rows as each is read,
  and keeps them for the landed entry.
  Depends on: `core/src/entry.rs`, `core/src/item/land/mod.rs`.
- `core/src/item/land/steps.rs` — Holds a landing's steps, from the checkout to
  the push, in the order a landing takes them.
  Depends on: `core/src/entry.rs`, `core/src/item/deliver.rs`,
  `core/src/item/land/branch.rs`, `core/src/item/land/licence.rs`,
  `core/src/item/land/mod.rs`, `core/src/item/land/porcelain.rs`,
  `core/src/item/land/rows.rs`, `core/src/item/land/suite.rs`,
  `core/src/item/land/tree.rs`, `core/src/item/mod.rs`, `core/src/item/run.rs`,
  `core/src/policy.rs`, `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`, `core/src/store/types.rs`.
- `core/src/item/land/suite.rs` — Runs a landing's suite check with its one
  rerun, and its marker command.
  Depends on: `core/src/item/land/mod.rs`, `core/src/item/land/rows.rs`,
  `core/src/item/lane.rs`, `core/src/item/mod.rs`, `core/src/seat/actor.rs`.
- `core/src/item/land/tree.rs` — Puts the checkout back when a landing refuses
  before the push.
  Depends on: `core/src/item/land/mod.rs`, `core/src/item/mod.rs`.
- `core/src/item/lane.rs` — Keeps each project's landing lane, the worktree
  every landing works in, and the lock landings queue on.
  Depends on: `core/src/item/mod.rs`, `core/src/policy.rs`.
- `core/src/item/list.rs` — Answers `fleet item list` from the store's listings,
  one row per item.
  Depends on: `core/src/item/mod.rs`, `core/src/item/show.rs`,
  `core/src/seat/identity.rs`, `core/src/store/mod.rs`.
- `core/src/item/mod.rs` — Holds what the item verbs share: their seams to the
  store, the ring, git and the spawner, and the stop they refuse with.
  Depends on: `core/src/entry.rs`, `core/src/policy.rs`,
  `core/src/seat/actor.rs`, `core/src/seat/identity.rs`,
  `core/src/store/mod.rs`.
- `core/src/item/review.rs` — Runs `fleet review`, the reviewer's read of a
  delivery and the verdict it writes.
  Depends on: `core/src/entry.rs`, `core/src/input.rs`,
  `core/src/item/brief.rs`, `core/src/item/deliver.rs`, `core/src/item/mod.rs`,
  `core/src/item/run.rs`, `core/src/item/show.rs`, `core/src/seat/actor.rs`,
  `core/src/seat/identity.rs`, `core/src/store/mod.rs`.
- `core/src/item/show.rs` — Renders an item and its timeline for `fleet item
  show`, for a person or as JSON.
  Depends on: `core/src/entry.rs`, `core/src/item/land/mod.rs`,
  `core/src/item/mod.rs`, `core/src/seat/identity.rs`, `core/src/store/mod.rs`.

## Flow

1. `cli/src/main.rs` hands `fleet dispatch <item>` to `cli/src/item.rs`, which
   resolves the project and opens its store.
2. `core/src/item/dispatch.rs` reads the item through the store, and refuses one
   that is not ready.
3. `core/src/item/dispatch.rs` writes the order: the assignee, an entry from
   `core/src/entry.rs` on the item's timeline, and the index, then reads all
   three back.
4. `core/src/item/brief.rs` renders the seat's brief, with the item as
   `core/src/item/show.rs` shows it, into a file.
5. `cli/src/seams.rs` rings a named seat's session; for a transient seat,
   `cli/src/transient.rs` asks the controller to spawn one, and a spawn that
   refuses withdraws the order.

## Depends on other areas

- store — every read and write of an item and its timeline.
- seats — seat ids and actors, the seat list, and the ring and spawn that reach
  a seat's session.
- packs — the layers a brief's templates resolve through.
- guards — the guards in force a brief lists.
- agent — the agent a ring or a spawn opens.
- runs — a run's record, which a review and a landing read.
- shared — the exit table, the envelope, the terminal, the process runner, the
  platform layer and the policy.
