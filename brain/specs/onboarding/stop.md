# `fleet stop`

Uses: shared.exit-codes, shared.locations, shared.output, onboarding.start

## What it is for

`fleet stop` takes the controller down. It has the service manager unload the
controller's user service, then waits for the controller's own
`controller.stopped` event on the event stream before it says the controller
has stopped. It never touches a seat's session: a stopped controller leaves
every session running where it is, and stop says so.

You meet it when the controller has to go down and come back:

- **After you add or change an agent seat.** The running controller runs only
  the rows of the seat list that the last `fleet start` rendered, so a change
  to `fleet.toml` reaches it through `fleet stop` and then `fleet start`.
- **Before a second `fleet start`.** `fleet start` refuses while a controller
  is running, so a restart is a stop first.
- **When you want the controller off,** for example before you change the
  machine.

A script meets it too: it reads `$?` to learn whether the stop was confirmed,
refused because nothing was running, or could not be told.

## How it behaves

### One call, three steps

```sh
$ fleet stop
stopped dev.fleet.controller — controller.stopped at sequence 412
seats: untouched — their sessions are still running
```

Both lines are on standard error. `fleet stop` prints nothing on standard
output, and takes no argument and no flag but `--help` or `-h`.

1. **It finds out whether a controller is running** for the machine
   directory the stop was called for (shared.locations.R2 to
   shared.locations.R6), by the status query `fleet start` makes
   (onboarding.start.R7 and its Interfaces). Where no controller runs for
   that machine directory, stop refuses, the manager answering that it does
   not know the label being one such case: stopping a controller that is not
   there is a refusal on the record and not a quiet no-op. The one exception
   is a service loaded with no controller running, below. Any other non-zero
   answer means the query could not tell, and stop exits 3 with nothing
   unloaded (onboarding.start.R35 reads the query the same way).
2. **It has the service manager unload the service,** once. The unload also
   keeps the service from coming back at the next login: a stop holds until
   the next `fleet start`.
3. **It waits for a fresh `controller.stopped`** on that machine directory's
   stream, for up to 30 seconds. Fresh means written after the stop began.
   An older `controller.stopped`, from an earlier stop, is exactly what the
   wait must not accept.

The service manager's own answer to the unload is not the confirmation. The
manager answers as soon as it has taken the request, which says nothing about
whether the controller has ended. The controller writes `controller.stopped`
itself when it is told to end (SIGTERM or SIGINT), so the event on the
stream is the controller's own word that it stopped.

The second line is said because it is the one thing a person expects a stop
to have done, and it has not: a stopped controller leaves its seats where
they are.

A service can be loaded with no controller running, such as one that keeps
crashing and that the manager keeps restarting. Stop unloads it too, says on
its line that it unloaded `dev.fleet.controller` with no controller running,
and exits 0. It waits for no `controller.stopped`, since no controller is
there to write one. Without this, nothing would take down a controller that
keeps crashing.

The first word of each line, `stopped` and `seats:`, is coloured as every
verb's lines are (shared.output.R21): only when standard error, the stream
the lines are written on, is a terminal, and neither `NO_COLOR` nor
`TERM=dumb` says otherwise. Otherwise the lines carry no colour codes, so a
script or a test that captures standard error reads them starting with
those words. Today the colour follows standard output, so `fleet stop 2>log`
at a terminal writes colour codes into `log` (fleet-nj2k).

### Outcomes

| What fleet finds | Exit | What it says on standard error |
| --- | --- | --- |
| a controller running for this machine directory, and a fresh `controller.stopped` on its stream within 30 s of the unload | 0 (shared.exit-codes.R1) | a line whose first word is `stopped`, naming `dev.fleet.controller` and the event's sequence; then a line whose first word is `seats:`, saying the sessions are still running |
| the service loaded with no controller running, and the unload accepted | 0 (shared.exit-codes.R1) | a line naming `dev.fleet.controller` and saying it was unloaded with no controller running; then the `seats:` line; no event is waited for |
| no controller running for this machine directory, such as where the manager answers that it does not know the label, other than the service loaded with no controller running | 1 (shared.exit-codes.R2) | `fleet stop:` and a line naming `dev.fleet.controller` and saying there is nothing to stop; nothing is unloaded |
| the status query exits non-zero with any other answer | 3 (shared.exit-codes.R4) | `fleet stop:` and a line naming the query and its answer; nothing is unloaded |
| the service unloaded, and no fresh `controller.stopped` within 30 s | 3 (shared.exit-codes.R4, shared.exit-codes.R9) | `fleet stop:` and a line naming `controller.stopped`, the bound, the event stream's path, and where the service's output is read |
| the service manager is unusable as onboarding.start.R6 says, or the query does not return within onboarding.start.N2's bound | 3 (shared.exit-codes.R4) | `fleet stop:` and the line onboarding.start.R6 or onboarding.start.N2 describes; nothing is unloaded |
| the unload exits non-zero, or does not return within 60 s | 3 (shared.exit-codes.R4) | `fleet stop:` and a line naming the unload command and how it ended; no event is waited for |
| an argument, or a flag other than `--help` or `-h` | 2 (shared.exit-codes.R3) | the `error:` block (shared.exit-codes.R15) |
| a machine directory fleet cannot name: a relative deciding variable, or no `HOME` | 1, before anything is asked or unloaded (shared.locations.R30 to shared.locations.R32) | `fleet stop:` and the line those clauses describe |

Exit 3 after the unload is not a success. The service is unloaded, but fleet
did not read back that the controller ended, so a script treats the outcome
as unknown (shared.exit-codes.R9). A controller killed outright, with no
signal it can answer, writes no `controller.stopped`, and stop then exits 3.

The unload is given 60 seconds, the bound onboarding.start.N2 gives the
query and the load: it is a local call to the platform's own manager, and a
minute is a deadline nothing healthy meets. With the query's bound and the
30-second wait, the longest a stop can take is about two and a half minutes.

### The unload

The unload is the one service manager call that stop owns. The label, the
status query and what a pid in its answer means are `fleet start`'s
(onboarding.start Interfaces). The Interfaces section below lists the unload
on each platform. Where a person reads what the service printed differs too:

| Platform | Where the service's output is read |
| --- | --- |
| macOS | `service.err.log` in the machine directory (shared.locations.R15) |
| Linux | `journalctl --user -u dev.fleet.controller.service` (shared.locations.R16) |

Where `FLEET_SERVICE_BIN` is set, the file it names stands in for the
manager as onboarding.start.R5 says, for the query and the unload alike,
with the same arguments; a value that is empty or spaces only counts as
unset, as that clause says too.

### Stop needs no fleet

`fleet stop` acts from any directory, inside a project or a git checkout or
not (shared.locations.R28). It needs no fleet at all: not the policy file,
not a project file, not `config.json`. Nor is it refused where finding a
fleet would refuse, as the verbs that act in a project are
(shared.locations.R41, shared.locations.R42, shared.locations.R52). All it
needs is
the service manager and the machine directory's event stream
(shared.locations.R12). A
stop that looked for a project first would refuse from a bare directory, or
from a project whose `fleet.toml` was moved or deleted, while the service
was still loaded, and leave the controller running.

It does need a machine directory it can name. Where the variable that
decides it names a relative path, or none of them is set, stop is refused
before it asks the manager anything, as every verb but `fleet guard` is
(shared.locations.R30 to shared.locations.R32).

### What stop leaves alone

- **Every seat's session,** on Linux as on macOS. Stop ends no session,
  signals none and types into none. This is one of the limits fleet never
  crosses (seed.md). On Linux the unit `fleet start` writes sets no
  `KillMode`, and systemd's default stops every process in the unit's
  control group, which may hold the tmux server every seat's session runs
  in. If it does, the unit changes so that the promise holds; it has not
  been observed on Linux either way (fleet-pz6x).
- **The service file.** It stays where `fleet start` wrote it
  (shared.locations.R18, shared.locations.R19), so a later `fleet start` finds it already
  written. Stop keeps the file but stops the service coming back by itself:
  the controller stays down across logins until the next `fleet start`. A
  stop that lasted only until the next login would bring back, unasked, a
  controller the person took down. Today the macOS file's `RunAtLoad` and
  the Linux unit, left enabled for `default.target`, bring it back at the
  next login (fleet-yb2a).
- **The stream.** Stop appends no event of its own. The only line it waits
  for is the controller's.

### Known gaps

- **A stop can take down another machine directory's controller**
  (fleet-rvct). The service label is one per user, whatever the machine
  directory, so today a stop counts any controller loaded under
  `dev.fleet.controller` as its own. A stop called with a scratch
  `FLEET_DIR` unloads the controller of the real machine directory, waits on
  its own stream, and exits 3 after 30 seconds though that controller did
  stop.
- **A relative `FLEET_SERVICE_BIN` is used** (fleet-bp1j). The stand-in is
  meant to be named by an absolute path, as onboarding.start.R5 says, and
  anything else is an unusable manager. Today a relative value is read
  against the working directory and run.
- **The status query is misread** (fleet-og989). Today any non-zero exit is
  read as nothing loaded, so stop refuses with exit 1 where the query could
  not tell; and a service loaded with no running process reads as nothing
  running, so stop refuses and leaves it loaded.
- **A stop lasts only until the next login** (fleet-yb2a). Today the service
  comes back at the next login without a `fleet start`.
- **On Linux, sessions may not survive a stop** (fleet-pz6x). Unobserved
  either way; the fix must show on Linux that they do.
- **Colour follows standard output,** not standard error, the stream the
  lines are on. The gap is shared.output.R21's, for every verb (fleet-nj2k).

## Behaviour

- **onboarding.stop.R1**: WHEN `fleet stop` is called, fleet SHALL take the
  controller down from any working directory (shared.locations.R28), whether
  or not a fleet is found from it, and whether or not the fleet's policy
  file, a project file or `config.json` exists or reads.
- **onboarding.stop.R2**: IF no controller runs for the machine directory
  `fleet stop` resolved (shared.locations.R2 to shared.locations.R6), the
  status query answering that the manager does not know
  `dev.fleet.controller` (onboarding.start.R9) among those cases, THEN fleet
  SHALL, except as onboarding.stop.R22 says, refuse with exit 1
  (shared.exit-codes.R2) and unload nothing.
  Today: fleet counts a controller running under `dev.fleet.controller` for
  any machine directory as running, unloads it, and exits 3 after 30 seconds
  when that controller writes to another stream (fleet-rvct).
- **onboarding.stop.R3**: IF fleet refuses as onboarding.stop.R2 says, THEN
  fleet SHALL name `dev.fleet.controller` on its refusal line
  (shared.exit-codes.R12, shared.exit-codes.R13).
- **onboarding.stop.R4**: WHEN a controller is running for the machine
  directory `fleet stop` resolved, fleet SHALL have the service manager
  unload its service, once.
- **onboarding.stop.R5**: IF the service manager is unusable as
  onboarding.start.R6 says of the manager, THEN fleet SHALL exit 3
  (shared.exit-codes.R4) on the line onboarding.start.R6 describes, and
  unload nothing.
  Today: a relative `FLEET_SERVICE_BIN` is read against the working
  directory and used (fleet-bp1j).
- **onboarding.stop.R6**: WHERE `FLEET_SERVICE_BIN` is set, fleet SHALL run
  the file it names for the status query and the unload `fleet stop` makes,
  as onboarding.start.R5 says for `fleet start`, with the arguments the
  Interfaces section gives the unload.
- **onboarding.stop.R7**: WHEN fleet has unloaded the service, fleet SHALL
  accept as confirmation only a `controller.stopped` event written after the
  stop began, on the event stream of the machine directory it resolved
  (shared.locations.R12).
- **onboarding.stop.R8**: WHEN a confirming `controller.stopped` is on the
  stream within the bound onboarding.stop.N1 gives, fleet SHALL exit 0
  (shared.exit-codes.R1).
- **onboarding.stop.R9**: WHEN fleet exits 0 from `fleet stop` on a
  confirming `controller.stopped` (onboarding.stop.R8), fleet SHALL write on
  standard error a line whose first word is `stopped` and which names
  `dev.fleet.controller`, `controller.stopped` and the sequence of the event
  it read.
- **onboarding.stop.R10**: WHEN fleet exits 0 from `fleet stop`, fleet SHALL
  write on standard error, after the line onboarding.stop.R9 or
  onboarding.stop.R22 names, a line whose first word is `seats:` and which
  says the seats' sessions are still running.
- ~~**onboarding.stop.R11**~~ retired 2026-10-13: shared.output.R21 states
  when a line is coloured, `NO_COLOR` and `TERM=dumb` included, for every
  verb's lines, stop's among them (fleet-xgk2).
- **onboarding.stop.R12**: IF no confirming `controller.stopped` is on the
  stream within the bound onboarding.stop.N1 gives, THEN fleet SHALL name on
  its refusal line `controller.stopped`, that bound and the path of the
  event stream it read.
- **onboarding.stop.R13**: WHERE the platform is macOS, IF no confirming
  `controller.stopped` arrives in time, THEN fleet SHALL name on its refusal
  line `service.err.log` in the machine directory (shared.locations.R15).
- **onboarding.stop.R14**: WHERE the platform is Linux, IF no confirming
  `controller.stopped` arrives in time, THEN fleet SHALL name on its refusal
  line `journalctl --user -u dev.fleet.controller.service`
  (shared.locations.R16).
- **onboarding.stop.R15**: IF the unload exits non-zero, THEN fleet SHALL
  exit 3 (shared.exit-codes.R4) on a refusal line that names the command it
  ran and its exit, and wait for no `controller.stopped`.
- **onboarding.stop.R16**: WHEN `fleet stop` is called, on every platform,
  fleet SHALL leave every seat's session as it was: it ends none, signals
  none and sends none any input.
  Today: on Linux this is unobserved; the unit sets no `KillMode`, and
  systemd's default stops every process in the unit's control group, which
  may hold the tmux server the seats' sessions run in (fleet-pz6x).
- **onboarding.stop.R17**: WHEN `fleet stop` is called, fleet SHALL leave the
  service file (shared.locations.R18, shared.locations.R19) in place,
  unchanged.
- **onboarding.stop.R18**: WHEN `fleet stop` is called, fleet SHALL append no
  event of its own to the event stream.
- **onboarding.stop.R19**: WHEN `fleet stop` is called with no argument and
  no flag, fleet SHALL print nothing on standard output.
- **onboarding.stop.R20**: IF `fleet stop` is given an argument, or a flag
  other than `--help` or `-h`, THEN fleet SHALL exit 2 as
  shared.exit-codes.R3 and shared.exit-codes.R15 say. `fleet stop` takes no `--json`.
- **onboarding.stop.R21**: IF the status query exits non-zero with any
  answer but that the manager does not know `dev.fleet.controller`, THEN
  fleet SHALL exit 3 (shared.exit-codes.R4) on a line naming the query and
  its answer, and unload nothing.
  Today: any non-zero exit is read as nothing loaded, and stop refuses with
  exit 1 (fleet-og989).
- **onboarding.stop.R22**: WHEN the status query reports
  `dev.fleet.controller` loaded with no running process (onboarding.start
  Interfaces), fleet SHALL have the service manager unload it, once, wait
  for no `controller.stopped`, and exit 0 (shared.exit-codes.R1) on a line
  on standard error naming `dev.fleet.controller` and saying it was unloaded
  with no controller running.
  Today: stop reads the state as nothing running, refuses with exit 1, and
  leaves the service loaded (fleet-og989).
- **onboarding.stop.R23**: WHEN `fleet stop` has unloaded the service, fleet
  SHALL keep it from loading again, at a later login or otherwise, until a
  `fleet start` loads it.
  Today: the macOS file's `RunAtLoad` and the Linux unit, left enabled for
  `default.target`, bring the controller back at the next login
  (fleet-yb2a).

## Non-functional

- **onboarding.stop.N1**: IF no confirming `controller.stopped`
  (onboarding.stop.R7) is on the stream within 30 seconds of the unload,
  THEN fleet SHALL stop waiting and exit 3 (shared.exit-codes.R4,
  shared.exit-codes.R9), the
  service already unloaded.
- **onboarding.stop.N2**: IF the status query has not returned within the
  bound onboarding.start.N2 gives it, THEN fleet SHALL exit 3
  (shared.exit-codes.R4) as that clause says, and unload nothing.
- **onboarding.stop.N3**: IF the unload has not returned within 60 seconds,
  THEN fleet SHALL stop waiting on it and exit 3 (shared.exit-codes.R4) on a
  line naming the call, and wait for no `controller.stopped`.

## Interfaces

- **The service manager.** onboarding.start owns the label
  `dev.fleet.controller`, the `FLEET_SERVICE_BIN` seam and the status query
  with what its answer means (onboarding.start.R5, onboarding.start.R6,
  onboarding.start.R7, onboarding.start.N2 and onboarding.start
  Interfaces). Stop adds one call, the unload (onboarding.stop.R4).
  `<uid>` is the calling user's uid. Stop relies on the manager ending the
  controller with a signal the controller answers.

  | Act | macOS: `launchctl` with | Linux: `systemctl` with | What fleet reads back |
  | --- | --- | --- | --- |
  | unload | `bootout gui/<uid>/dev.fleet.controller` | `--user stop dev.fleet.controller.service` | Exit 0 accepts; anything else is onboarding.stop.R15. |

  These calls are today's. They leave the service to load again at the
  next login, which onboarding.stop.R23 rules out, so the unload changes
  with fleet-yb2a. `fleet start` makes the same unload for a service loaded
  and not running, and for one whose start was not confirmed
  (onboarding.start.R36, onboarding.start.N1).

- **The controller** (`fleet observe`, which the service runs): stop relies on
  it appending `controller.stopped` to `events.jsonl` when SIGTERM or SIGINT
  ends it. A controller ended any other way writes none, and stop exits 3
  (onboarding.stop.N1).
- **`fleet start`**: writes the service file and loads the service that stop
  asks about and unloads. Stop leaves that file for the next start
  (onboarding.stop.R17).
- **A stand-in manager** named by `FLEET_SERVICE_BIN` (onboarding.stop.R6)
  must answer the status query as onboarding.start Interfaces says and the
  unload as onboarding.stop.R4 and onboarding.stop.N3 say, and must have
  something write `controller.stopped` on the machine directory's stream
  after the unload, or the stop exits 3 (onboarding.stop.N1).
- **shared.locations**: the machine directory (shared.locations.R2 to
  shared.locations.R6), the refusal where none can be named
  (shared.locations.R30 to shared.locations.R32), its event stream
  (shared.locations.R12) and where the service's output is read
  (shared.locations.R15, shared.locations.R16).

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu. onboarding.stop.R16
and R10 hold on Linux as on macOS, and a scenario shows it on Linux.

