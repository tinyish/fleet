# 001: a session's life lives in state

Part 2 of [Session life](overview.md), on top of [000](000-writes.md). C1
of the 2026-11-06 code-health review.

## The row keeps its life

- The `sessions` table gains `end_time_millis INTEGER`, null until the
  session is ended. The version-1 migration is edited in place (core.md §
  Boundaries: the schema stays at version 1 until the first release).
- A row is never deleted. `state::Life` is `Live`, `Gone` or `Ended`, read
  from the row: `end_time_millis` set is `Ended`; else a recorded state of
  gone is `Gone`; else `Live`. `SessionRow` gains `life: Life`.

## Reads

| Read | Gives |
| --- | --- |
| `State::session(id)` | the row of session `id`, whatever its life; none for an id never launched |
| `State::sessions()` | every row not ended, except a seat's gone session once its seat has a newer one; so the live ones, a spawned or orphaned session read gone until it is ended, and a seat's newest session |
| `State::named(here, name, takes)` | what `name` points at: a seat of the fleet in `here`, else `Session(row)` by `session`, else, when `takes` allows, a message queued in the record |

`Named::Ended` goes: `Session(row)` carries the row's life. `names.rs`
keeps only `here` (the fleet holding a directory, made canonical), and
`Resolved::unknown`; its `named` moves into `state`. `steps_with_session`
goes; `steps_with_message` stays for messages, which this part does not
touch.

## Writes

Changing 000's:

- `set_state(id, state)` on a row that is not live records nothing and
  changes nothing. On a live row, gone sets its life gone, records
  `SessionStateChanged{gone}` and drops its messages as `SessionGone`.
- `end_session(id, stop)` on a live or gone row sets `end_time_millis`,
  runs `stop`, records `SessionEnded` and drops its messages as
  `SessionEnded`. On an ended row it changes nothing; the caller has
  already answered *already ended* from its life.

`Step::SessionGone` goes from `core::record`, with its words in the CLI and
its specimen sample.

## The tick

Gone is recorded before any seat is revived, so a seat's session dies the
way a spawned one does:

1. Read `sessions()`; observe the live ones (tmux and their adapters). tmux
   unable to list its panes: every wanted seat shows down, as `blind` does
   now, and the tick stops.
2. The holds see the readings (as now).
3. Record the states that changed (`record_states`), gone among them.
4. Deliver, from this reading, so a paste goes in right after the reading
   that saw its agent idle, never after a launch's wait. The clock is no
   longer moved on by tending's time: tending comes after.
5. Tend the seats from `wanted_seats()` and the rows as recorded: a seat
   with a live session keeps it; one with none launches, once its backoff
   allows. `Held.alive` becomes "its life is live"; tending no longer asks
   tmux itself (`observe::alive` goes from `tend_seats`).

Launching a seat stops the tmux session of its gone session (`Act::Launch`'s
`clear`, now only the seat's newest, gone) before running the new one; it
changes no row. A seat's newest session is its last launched, by insertion
order, never by the clock. `kill-session` on a session tmux no longer has is
already taken as done. `sessions()` is served by an index on the seat.

Ending a session says its seat comes back only when the session ended was
live and its seat is wanted: ending a seat's old, gone session records it
ended and says nothing of its seat.

## Readers, each now one question

| Where | Today | After |
| --- | --- | --- |
| `messages::recipient` | session's pane alive (tmux) | `Session(row)`: live takes it; gone or ended refused *session ended* (core.md § Messages) |
| `sessions::endable` | a seat's session whose pane is alive (tmux) | a seat's live session from `sessions()`; `Session(row)` ended answers *already ended* |
| `watch::first_poll` | a row, else the record's ending steps (`ends`, `judge`) | `Session(row)`: live or gone judged from its recorded state by `waiting::already`; ended refused *ended* |
| `watch::live`, `overview::seat_overview` | the first row whose state is not gone | the seat's live row, else its newest |
| `waiting::for_seat`, `for_session` | match `SessionGone` and `SessionStateChanged{gone}` | `SessionStateChanged{gone}` only |

`observe::alive` goes if nothing else calls it; `watch::ends` and
`waiting::judge`'s ended path go with the record scan.

## Tests

`state`'s inline tests hold one table of life cases, each read back through
`session`, `sessions` and `named`:

| Case | Life | In `sessions()` |
| --- | --- | --- |
| spawned, launched | live | yes |
| spawned, read gone | gone | yes |
| spawned, read gone, then ended | ended | no |
| spawned, ended while live | ended | no |
| seat, read gone | gone | yes |
| seat, read gone, then revived | old gone, new live | new only |
| seat, ended, revived | old ended, new live | new only |
| a reading after the session was ended | ended, nothing recorded | no |

Scenarios keep: a seat's agent dying, recorded gone, then revived in the
same tick, with `SessionStateChanged{gone}` before the new
`SessionLaunched`; a message to a spawned session whose agent dies, dropped,
and the wait on it ending; a message sent to a session that died since the
last tick, taken, then dropped at the next tick. The per-verb checks of
gone and ended that the table now covers are deleted.
