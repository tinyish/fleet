# fleet

fleet runs AI coding agents on your projects. It hands items of work from a
store to seats, which are agent sessions in tmux or people, and moves each item
from dispatch through delivery and review to a landing on the trunk. Guards
refuse dangerous commands before they run, and every step is written to the
item's timeline.

## Who it is for

A developer who runs several agents, such as Claude Code, on their own git
repositories on macOS or Linux, and wants them to work while nobody watches
without losing track of what each one did.

## Features

Each area's map is `brain/specs/<area>/mod.md`; the map's overview is
`brain/specs/mod.md`.

| Area | Features (spec files under `brain/specs/<area>/`) |
| --- | --- |
| onboarding | `doctor.md`, `create.md`, `start.md`, `stop.md`, `prime.md` |
| packs | `add.md`, `remove.md`, `list.md`, `check.md`, `layering.md`, `defaults.md`, `runtime.md`, `settings.md` |
| items | `dispatch.md`, `brief.md`, `deliver.md`, `hold.md`, `clear.md`, `review.md`, `land.md`, `show.md`, `list.md` |
| runs | `run.md`, `cancel.md`, `steps.md`, `waiting.md` |
| seats | `identity.md`, `add.md`, `observe.md`, `events.md`, `rest.md`, `clear-halt.md`, `nudge.md`, `attach.md`, `spawn.md`, `feed.md`, `retire.md` |
| guards | `hook.md`, `check.md`, `shell-trap.md`, `record.md`, `release-ref.md`, `production-write.md` |
| status | `status.md`, `event-tail.md`, `event-show.md` |
| store | `contract.md`, `schema.md`, `check.md` |
| agent | `contract.md`, `schema.md`, `check.md` |
| routines | `files.md`, `firing.md`, `list.md`, `check.md`, `run.md`, `history.md` |
| shared | `exit-codes.md`, `output.md`, `item-ids.md`, `locations.md` |

## Limits fleet never crosses

- The core crate never depends on the controller, and never reads the process
  table.
- fleet reaches a store or an agent only through the adapter a pack ships. The
  binary carries no store and no agent of its own.
- The binary carries only the defaults every fleet gets. Roles, values and
  workflows come from packs.
- fleet never reads or writes an item's notes. They are a person's.
- A landing lands only a commit its reviewer accepted, pushes nothing after a
  suite fails twice, and says `NOT TESTED` when it was handed no suite.
- `fleet create` never initialises or rewrites the project's store.
- `fleet stop` never touches a seat's session, and `fleet seat retire` never
  deletes a branch.
- `fleet status` reads what the controller published, never the process
  table, and writes nothing.
- A run reads only the inputs pinned for it when it was opened.
- Exit 3, could not tell, is never a success.
