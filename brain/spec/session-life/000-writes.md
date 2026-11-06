# 000: every state write records its own steps

Part 1 of [Session life](overview.md). C2 of the 2026-11-06 code-health
review.

## The rule

`Write::record` becomes private to `state`. Every public write on `Write`
takes what happened, reads what else it needs inside its transaction, and
records its own steps, in the order they are recorded today, and the drops
its change makes. The supervisor never builds a `Step` again; `state::Step`
stops being re-exported for writing (readers of the record still get
`Event`s with steps).

A write that finds nothing to change (no such project, fleet or session,
no project taken, a roster unchanged) records nothing. Turning on or off
what already is still records, as it does today.

## The writes

Each row is one public write after this part; the right column is what it
takes the place of in the supervisor.

| Write | Records | Replaces |
| --- | --- | --- |
| `supervisor_started(version, pid)` | `SupervisorStarted` | `lib.rs:146-150` |
| `create_fleet(fleet, roster, adopted, write_config)` | its roster's steps (as `set_roster`), then `FleetCreated` | `create.rs:69-81`: `create_fleet`, `seats::diff` from empty, `projects::set_roster`, `record(created)` |
| `set_roster(fleet, roster)` | diffs the roster it holds against `roster` once, with `seats::diff`; records `RoleAdded`, `RoleChanged`, `SeatAdded`, `SeatChanged`, `SeatRemoved`, `RoleRemoved` in that order; drops each removed seat's messages | `projects::set_roster` (deleted) and the diff in `reread_all` |
| `apply_reread(changes)` | `ProjectRemoved`, `ProjectAdded` per fleet | `Reread::apply`'s project loop |
| `config_broken(fleet, why)` | `FleetConfigBroken` | `Reread::apply`'s broken loop |
| `forget_fleet(directory)` | `FleetForgotten`; drops its seats' messages | `remove_fleet(dir, FleetForgotten)` |
| `remove_fleet(directory, config)` | `FleetRemoved` with its projects, read before the delete; drops its seats' messages | `remove_fleet(dir, FleetRemoved)` |
| `remove_projects(fleet, projects)` | `ProjectsTaken` | `remove.rs:118-123` |
| `set_active(project, active)` | `ProjectOn` or `ProjectOff`, with the fleet it reads | `projects.rs:325-366` |
| `all_on(on, off)` | each fleet on or off as listed, then `EveryFleetOn` | `set_fleet_active` loop + `record(EveryFleetOn)` |
| `all_off()` | `EveryFleetOff` | `set_all_active(false)` + `record(EveryFleetOff)` |
| `seat_launch_failed(key, why)` | `SeatLaunchFailed` | `tending.rs:295-301` |
| `launch_session(row, launched, run)` | `SessionLaunched`, from the row and the adapter's `launched` (program, model, prompt) | `Prepared.step` and `record(&p.step)` |
| `end_session(id, stop)` | `SessionEnded` from its row; drops its messages as `SessionEnded` | `end_session(id, step, stop)` |
| `set_state(id, state)` | `SessionStateChanged` from its row, when `state` is some | `set_state(id, state, steps)` |
| `queue_message(row, fleet)` | `MessageQueued` | `messages.rs:47-56` |
| `deliver(ids, session, type_in)` | `MessageDelivered` for each; then clears the session's recorded state | `messages.rs:139-155` |

`set_fleet_active` and `set_all_active` go: `all_on` and `all_off` take
their place. `end_session` no longer takes a step, so its drop reason is no
longer guessed from one; 001 adds the gone path.

## A seat is wanted: one rule

`state` answers it once, from the rows its transaction reads: a seat is
wanted when its fleet has a project on, and it is in its fleet's roster
with a role that exists. Its role's digest is part of what is wanted.

- `State::wanted_seats()` gives every wanted seat with its role, for
  tending; it replaces the filter in `tending::tend_seats` (`tending.rs:159-176`).
- `Write::seat_wanted(key, role_digest)` stays, for the launch check in
  `start_seat`, on the same rule.
- `sessions::end`'s `revives` asks `seat_wanted` (any digest) instead of
  only "still in the roster", so a seat whose fleet is off is not said to
  come back.

## Deleted

- `projects::set_roster`, the `Reread.rosters` diffs, `Prepared.step`, and
  every `Step { .. }` literal in `crates/supervisor/src` outside tests.
- The line `fleet: row.fleet.as_ref().map(|f| f.display().to_string())`
  wherever a step was built from a row.

## Tests

In `state`'s inline tests, on a temp SQLite file: for each write above,
the steps it records and the messages it drops, read back from the record.
The roster diff's cases (role added before its seats, removed after them;
a seat moved to another role) move here from the supervisor. Scenarios that
only checked a write's steps or drops are deleted; those crossing modules
stay.
