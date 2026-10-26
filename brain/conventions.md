# Conventions

How tinyfleet's code is written, beyond what the lints and the size budgets
check: corrections Alberto made once and wants never to make again. Planners
follow these when they write program design, builders when they write code,
and reviewers check them.

Each entry says what to do, why, and where it came from. Add one whenever a
review corrects something that could come up again; change or drop one only
with Alberto.

## Changing code

- **Change a function; don't wrap it.** When a function needs a new
  parameter, add it and update every caller. Don't add a twin that takes the
  parameter and turn the original into a wrapper that passes the old value
  just to leave call sites alone. Two functions for one job is the thing to
  avoid, however small. The same goes for a function that grows a second
  result: return both, and let a caller that needs one ignore the other.
  *(fleet-sibp.3: `client::ask_within` beside `client::ask_for`, merged into
  one `ask_for` that takes its wait; `fleets::read_projects` kept as a
  wrapper of `read_fleet`, removed.)*
