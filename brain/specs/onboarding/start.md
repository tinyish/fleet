# `fleet start`

Uses: shared.exit-codes, shared.locations, onboarding.stop

## What it is for

`fleet start` brings the controller up. The controller is the loop that
watches a fleet's seats and writes the event stream. `fleet start` installs
it as a user service on this machine, labelled `dev.fleet.controller`, and has
the platform's service manager load it. It exits 0 only once the controller
has written its own first line on the stream.

You meet it once a fleet exists: after `fleet create`, and again after every
`fleet stop`. You also meet it after you change the `[seats]` tables in
`fleet.toml`. The rows the controller runs those seats from are written by
`fleet start` and by nothing else, and the controller re-reads `config.json`,
never the `[seats]` tables. So a changed table takes a stop and a start.

There is no `fleet install`. The work an install would do is the first part
of every `fleet start`. That is the machine directory, the seat list, the seat
rows, the service file and the telemetry disclosure. Each step is done only
where it is not done already, so a second start repeats none of it and says
so. That work starts nothing. Loading the service is a second, separate act.

## How it behaves

### The call

```sh
$ fleet start
$ fleet start --foreground
```

It takes no argument and two flags, `--foreground` and `--project <name>`
(shared.locations.R46 to shared.locations.R48), besides `--help` or `-h`.
Anything else is a usage error, exit 2 (shared.exit-codes.R3). It takes no `--json`. Every line it
writes goes to standard error.

### Which fleet it starts

`fleet start` works from any directory (shared.locations.R28). It finds the
policy file of the fleet it starts in this order:

1. It looks for the fleet as every verb that acts in a project does
   (shared.locations.R21 to shared.locations.R24, shared.locations.R39 to
   shared.locations.R53), refusing where those clauses refuse.
   Where that finds a wingman project, the policy is the wingman directory's
   `fleet.toml`. That covers a start from inside one of its projects'
   checkouts or a seat's worktree, and a start from the wingman directory
   itself, which picks the project as shared.locations.R43 to
   shared.locations.R46 say: on a terminal it asks where there are several,
   and a script passes `--project`.
   Where it finds a default fleet's `fleet.toml`, that file is the policy
   (shared.locations.R23). Where the checkout fallback admits the working
   directory, the policy is the file `config.json` names, and the nearest
   checkout is the project (shared.locations.R24).
   Where `config.json` already names another policy file, the start
   refuses, exit 1: the machine runs one fleet, and a start that rendered
   this fleet's seats into the other fleet's seat list would load a
   controller running the other fleet's policy. The line names both policy
   files and how to move the machine to the fleet the start was called in.
   Nothing is written or loaded.
2. Otherwise, the policy file `config.json` names (shared.locations.R11),
   where that file exists.
3. Otherwise it refuses, exit 1 (shared.exit-codes.R2). The line names
   `fleet.toml`, `config.json` and `fleet create`.

A `config.json` that is there but does not read or parse is no answer either
way. It does not say that no fleet is named, so the start cannot tell which
fleet it would start, and answers as shared.locations.R38 says: exit 3, on a
line naming the file, with nothing written or loaded. Today it gets the
no-fleet refusal above, exit 1 (shared.locations.R38's `Today:` line,
fleet-9sr5).

So a wingman fleet can be started from anywhere once `config.json` names
it, inside one of its projects or not.

### What it checks before it writes anything

The checks below run in this order (onboarding.start.R38). The first that
fails ends the call. Up to that point nothing has been written and nothing
loaded, except where the table says otherwise. Each refusal is a
`fleet start:` line (shared.exit-codes.R12, shared.exit-codes.R13).

| Check | Fails when | Exit | The line names |
| --- | --- | --- | --- |
| The machine directory | the variable that decides it names a relative path, or no variable names it (shared.locations.R30 to shared.locations.R32) | 1 | the variable, and that it must be absolute |
| A fleet | no policy file is found, as above | 1 | `fleet.toml`, `config.json`, `fleet create` |
| The seat list | `config.json` is needed to find the policy file and is there but does not read or parse (shared.locations.R38) | 3 | `config.json` and why |
| The project | finding the fleet refuses (shared.locations.R41, shared.locations.R42, shared.locations.R45, shared.locations.R47, shared.locations.R48, shared.locations.R52, shared.locations.R53) | 1, 2 or 3, as that clause says | as that clause says |
| One fleet per machine | the start took a default fleet's or a wingman directory's `fleet.toml` and `config.json` names another policy file | 1 | both policy files, and how to move the machine to the one the start took |
| The service manager | `FLEET_SERVICE_BIN` is not an absolute path to an executable file; or, with it unset, the platform's manager is not on the search path; or the status query cannot be run; or it exits non-zero with any answer but that the manager does not know `dev.fleet.controller` | 3 | `FLEET_SERVICE_BIN`, or the manager and the search path, or the query's command and its answer |
| Already running | the manager's status query reports a pid for `dev.fleet.controller` | 1 | the pid, and when the projection was last published or that none has been |
| The agent | the policy file does not parse; the agent adapter it names does not open, such as when no installed pack carries it; the adapter does not answer `capabilities` or `version`; or its `version` is null, meaning its agent is not installed | 3 | why, and that nothing was loaded; for a pack no one installed, the `fleet pack add` line that installs it |
| tmux | `FLEET_TMUX_BIN` is not an absolute path to an executable file; or, with it unset, no `tmux` is on the search path | 3 | the path or `FLEET_TMUX_BIN`, and the search path |
| The policy | the policy file does not read as the controller's policy | 3 | why |
| The seats | the seats area refuses a `[seats]` table, such as one keyed by a name rather than a seat id; this is found during the first run, after the seat list is written (onboarding.start.R39) | 3 | the table |

The already-running check reads the pid from the manager. It reads the time
of the last poll from the published projection, never from the process
table, because a pid is not a tick. The agent and tmux checks come before
anything is loaded. An agent that can start no session is a controller that
would start no seat, and every seat's session runs on tmux.

`FLEET_SERVICE_BIN` names, by its absolute path, a file to run in place of
the platform's service manager: a stand-in that records the calls, never a
name fleet would have to search for. Set to an empty value or to spaces only,
it counts as unset, and the manager is then found by name on the search path.

A service that is not loaded is not a fault, so the start goes on. Only the
manager's answer that it does not know `dev.fleet.controller` means not
loaded. A query that exits non-zero with any other answer, such as a
`launchctl print` with no GUI session or one that was denied, could not ask
the manager at all. That is could-not-tell, not "not loaded": the start
exits 3 (shared.exit-codes.R4) with nothing written or loaded, where reading
it as not loaded would skip the already-running refusal.

A query that exits 0 and reports no pid means a service is loaded and not
running, such as a controller that keeps crashing. The start goes on, and
when it comes to load the service it first unloads the idle one, so the
load starts it fresh (below). The Interfaces section lists the query on each
platform.

`--foreground` runs these checks too, so a foreground start refuses while the
service is running. A foreground start that finds the service loaded and
not running unloads it first and says so, then runs its loop, so a
service the manager keeps restarting never runs beside it.

### The search path

fleet runs the service manager, the agent's adapter, `plutil` and `loginctl`
from a search path it builds, and looks for `tmux` there too. It never uses
your shell's `PATH`, because a process a service manager launches carries a
small `PATH` of its own, and every session started under it would inherit
that. The Interfaces section lists the path on each platform.

### The first run

Once the checks pass, fleet does the first-run work. It writes one line per
step on standard error, each starting `first run:`.

| Step | What it does | Its line says |
| --- | --- | --- |
| Machine directory | makes it where it is absent | its path |
| Seat list | writes `config.json` naming the policy file, with no rows, where no `config.json` exists; leaves the policy file an existing one names as it is | written or already, and the path |
| Seat rows | renders the policy's agent seats into `config.json`, keying each new row on one project (below) | the project; how many rows were rendered, added, updated and dropped; or that no row was rendered and why |
| Service file | writes it where it is absent or says something other than this start would write; otherwise leaves it untouched. A foreground start never writes it or changes it | written or already, and its path; for a foreground start, that it was left as it is |
| The platform's check | asks the platform about the service file (below) | what it found |
| Nothing started | loads nothing | that `fleet start` loads `dev.fleet.controller` as its second act |
| Telemetry | writes `[telemetry] enabled = false` where the policy does not declare it (below) | what the key reads |

Where no step changed anything, a last `first run:` line says every step was
already done.

A seat list that already exists keeps the policy file it names. A start never
re-points it.

### The seat rows

The rows in `config.json` are what the controller runs. The seats area
defines a seat row: which seats get one, what it carries, how a seat's
worktree is found under a project's worktrees directory, and which rows a
render leaves alone, transient ones among them. The seats area also defines
the worktrees directory itself. What `fleet start` decides is the project
new rows are keyed on, and when it renders at all.

A row's worktree belongs to a seat and a project, never to the directory you
typed the start in. So a row that already names a project keeps it, whatever
directory the start is called from. A wingman fleet started from a second
project, or from a seat's own worktree, would otherwise move every row onto
that directory and lose the worktrees the rows named.

fleet keys each new row on the project found as in "Which fleet it starts":
a wingman project, with its `path` as the project's root, or a default
fleet's project.

Where no project is found, as for a fleet `config.json` names started from
outside every project, fleet renders no row and leaves the rows
`config.json` holds as they are: a guessed project would put a seat in the
wrong checkout, and a start that dropped the rows would load a controller
that runs no named seat. The `first run:` line says no row was rendered, and
that a start from inside one of the fleet's projects renders them.

Where a render does drop rows, as when the policy names no agent seat, the
line counts every row it dropped.

fleet never creates a worktree. Making it is the person's step.

### The service file

shared.locations.R18 and shared.locations.R19 say where the file goes. The
service runs the `fleet` executable that ran `fleet start`, by its full path,
with the one argument `observe`. A service file naming any other binary
would load and then fail. So start the controller from the copy of `fleet`
you mean to keep. A start from another copy rewrites the file to name that
copy, unless it is a foreground start, which leaves the file as it is.

The file sets `FLEET_DIR` as shared.locations.R20 says. It never sets `PATH`.
The controller builds its children's search path itself, and a `PATH` frozen
at install time is what every later session would inherit. The Interfaces
section lists the rest of what the file says on each platform.

After writing the file, fleet asks the platform about it:

- **macOS.** It runs `plutil -lint` on the file. A file that does not parse
  loads as nothing and says so nowhere a person reads. The line says whether
  the file parses, or that `plutil` is not on the search path.
- **Linux.** It reads with `loginctl` whether lingering is on for the user.
  Without lingering, a user service stops when the user's last session ends.
  The line says lingering is on; or that it is off, naming the
  `enable-linger` command for you to run; or that it could not be read.
  fleet never turns lingering on itself, because that changes the machine's
  own login policy.

The check is bounded at 20 seconds. Whatever it answers, or if it does not
answer in time, the start goes on, and a file that does not parse is still
loaded.

### The telemetry disclosure

fleet collects no metric, sends none and has no endpoint. So nothing is
asked. The key is written down instead, so you can find it if a later release
ever has to change it.

| The policy file says | fleet writes | Its line says |
| --- | --- | --- |
| no `[telemetry]` table | appends `[telemetry]` with `enabled = false` | off, and that the key was written false |
| a `[telemetry]` table with no `enabled` | adds `enabled = false` as the table's first line | off, and that the key was written false |
| telemetry declared other than by a `[telemetry]` header, with no `enabled` | nothing | off, and that `enabled = false` is yours to write |
| `enabled = false` | nothing | off |
| `enabled = true` | nothing | the file says true, and nothing in fleet sends a metric |
| `enabled` as something other than true or false | nothing | its type, and that nothing leaves this machine either way |

The line reports the key's value, not just whether the key is there. fleet
never writes `enabled = true`.

This is the one place `fleet start` edits your own file. A file it left
unparsable would be a fleet that cannot start again, so fleet checks that the
edited text parses as TOML before writing it. If it would not parse, the file
is left as it was and the start exits 3.

Two `fleet` commands editing the same file never lose an edit
(shared.locations.R17), and this edit is held to that. Today it falls short:
a start racing `fleet seat add` can lose one of the two edits (fleet-adol).

### The defaults

After the first run, fleet installs the defaults built into this binary
(shared.locations.R8) on every start, not only the first. `fleet create`
refuses a directory that already holds a `fleet.toml`, so on a machine whose
binary was rebuilt, no other call would bring the new set. A line starting
`defaults:` says whether the set was installed, refreshed over one that
differed, or already there.

If the defaults cannot be installed, the line says why, and the start goes
on. A machine whose defaults this binary will not write over still gets its
controller.

### Loading and confirming

Without `--foreground`, fleet first notes the highest sequence on the stream
(`events.jsonl`, shared.locations.R12). Where the status query found the
service loaded and not running, fleet has the manager unload it first, once,
with the unload `fleet stop` makes (onboarding.stop Interfaces), so the load
that follows starts it fresh. An unload that exits non-zero or cannot run
stops the start as a failed load does. Then it asks the service manager to
load the service, once. The Interfaces section lists the load calls on each
platform. On Linux there are two calls, in order. The manager re-reads its
unit directory first, because the second call would not find a unit written
after the manager's last read.

A load call that exits non-zero, or cannot run, stops the start with exit 3.
The line names the command, its exit, and what it printed on standard error.

The manager's own exit says only that it accepted the job, not that the
controller came up. So fleet confirms from the stream. It waits up to 30
seconds for a `controller.started` event whose sequence is above the one it
noted. It noted that sequence before the load because a `controller.started`
left by an earlier run must not count as the confirmation. Thirty seconds
covers one poll interval plus a cold start, with room to spare.

- **Confirmed.** fleet writes a line starting `started` that names
  `dev.fleet.controller`, the event's sequence and the machine directory. It
  exits 0 (shared.exit-codes.R1).
- **Not confirmed within 30 seconds.** fleet has the manager unload the
  service it loaded, keeps every file the start wrote, and exits 3
  (shared.exit-codes.R4, shared.exit-codes.R9). The line names
  `controller.started`, the stream's path, and where the service's output is
  read (shared.locations.R15, shared.locations.R16). A loaded controller that
  never confirmed would be restarted by the manager each time it exits, and a
  `fleet stop` after it would wait for a `controller.stopped` that the same
  broken controller may never write. Today the service stays loaded and fleet
  makes no unload call (fleet-nkgz).

```sh
$ fleet start
first run: machine directory: <machine-dir>
first run: seat list: written at <machine-dir>/config.json
…
defaults: installed 0.1.0 — <machine-dir>/defaults
started dev.fleet.controller — controller.started at sequence 7 — <machine-dir>
$ echo $?
0
```

### Running in the foreground

`fleet start --foreground` makes the same checks, does the same first-run
work and installs the defaults, but writes no service file and changes none.
It loads no service, and a service file it wrote would be run by a later
login or service load that nobody asked for. Today it writes the service
file, or rewrites it to name this copy of `fleet` (fleet-zi1j). Then it
loads nothing. It writes a line starting `running` that names the machine
directory. Then it runs the controller's loop in the calling process until
that process is stopped. The loop is the one the service runs,
`fleet observe`, and the seats area defines what it writes and how it ends.
There is no confirmation: the call exits with the loop's exit.

### The bounds

Every wait `fleet start` makes is bounded.

| Wait | Bound | When it runs out |
| --- | --- | --- |
| The `controller.started` confirmation | 30 s from the load's return | the service unloaded, exit 3, as above |
| Each service manager call (status query, unload, load) | 60 s | exit 3, naming the call |
| Each call of the agent's adapter | the agent area's bound: 20 s, or `FLEET_AGENT_TIMEOUT_MS` | exit 3, as an agent that does not answer |
| The platform's check (`plutil`, `loginctl`) | 20 s | the line says it could not be read; the start goes on |
| Another `fleet` command's edit of `config.json` | 30 s | exit 3, naming `config.json`; nothing more is written and nothing loaded |

### When a start is cut short or runs beside another

A reader never sees `config.json`, the service file or the policy file
part-written. A start killed at any point leaves each of them either as it
was or as written, and the next start finishes whatever steps the killed one
did not.

Two `fleet` commands editing the same file never lose an edit
(shared.locations.R17). A start that writes `config.json` while another
command is editing it waits for it for up to 30 seconds, as long as it gives
the controller to confirm. A spawn edits that file while it cuts a seat's
worktree, and a start must not wait on a slow spawn without end. Past the
bound, the start exits 3 (shared.exit-codes.R4) on a line naming
`config.json`, writes nothing more and loads nothing. Today the wait has no
bound (fleet-euij).

### Known gaps

Each of these has its own `fix` or `behaviour` item. Until the item lands,
the clause carries a `Today:` line.

- **A wingman fleet** cannot be started: fleet knows no wingman directory,
  takes no `--project`, and keys rows on a standalone project registered in
  `projects.toml` (fleet-1s1x.4).
- **A start inside a default fleet, on a machine whose `config.json` names
  another fleet,** renders this fleet's seats into that seat list, writes the
  telemetry key into this fleet's file, and loads a controller that runs the
  other fleet's policy, where it should refuse (fleet-rlojy).
- **The status query** is read as no service loaded on any non-zero exit,
  where only the manager's unknown-label answer means that; and a service
  loaded with no running process is loaded again without an unload, or,
  by a foreground start, left loaded beside its loop (fleet-og989).
- **A render keys every row on the one project** the start found, replacing
  the project each row named, where a row should keep its own (fleet-013b).
- **A start with no project** runs the render with nothing to write and
  drops every agent row, where it should leave the rows as they are; and a
  render that drops rows because the policy names no agent seat does not
  count them (fleet-1ba5).
- **A foreground start** writes the service file, or rewrites it (fleet-zi1j).
- **An unconfirmed start** leaves the service loaded (fleet-nkgz).
- **The wait for another command's edit of `config.json`** has no bound
  (fleet-euij).
- **A relative `FLEET_SERVICE_BIN`** is read against the working directory
  and used. It should be refused as `FLEET_TMUX_BIN` is (fleet-bp1j).

Five shared gaps reach `fleet start` too, and the shared clauses carry them:

- A `config.json` that does not parse gets the no-fleet refusal, exit 1,
  where the start cannot tell and should exit 3 (shared.locations.R38,
  fleet-9sr5).
- The first write of `config.json` takes no lock, so two first starts on a
  fresh machine can both write it, and one write is lost
  (shared.locations.R17, fleet-rbwc).
- The telemetry edit of the policy file takes no lock, so it can lose an
  edit made by a command running beside it (shared.locations.R17,
  fleet-adol).
- A relative `HOME`, or on Linux a relative `XDG_STATE_HOME`, that decides
  the machine directory is read against the working directory, and the start
  writes the relative directory it gives into the service file
  (shared.locations.R30, fleet-8s9z).
- The defaults step rewrites `packs.lock` with no lock, so a start beside a
  `fleet pack add` can lose one of their entries (shared.locations.R17,
  fleet-z2qd).

## Behaviour

- **onboarding.start.R1**: WHEN `fleet start` is called, from any working
  directory (shared.locations.R28), fleet SHALL take as the fleet's policy
  file: where shared.locations.R40, shared.locations.R43, shared.locations.R44 or
  shared.locations.R46 finds a wingman project,
  the wingman directory's `fleet.toml`; where shared.locations.R23 or
  shared.locations.R24 finds a default fleet, its policy file; and
  otherwise the policy file
  `config.json` names (shared.locations.R11), where that file exists.
  Today: there is no wingman project to find; a standalone project's
  `.fleet/project.toml` is taken instead, with a `fleet.toml` beside it
  taken before it (fleet-1s1x.4).
- **onboarding.start.R2**: IF onboarding.start.R1 finds no policy file that
  exists, except as shared.locations.R38 says for a `config.json` that cannot
  be read or does not parse, THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line that names `fleet.toml`, `config.json`
  and `fleet create`, having written and loaded nothing.
- **onboarding.start.R3**: fleet SHALL take no argument and no flag but
  `--foreground`, `--project <name>` (shared.locations.R46 to
  shared.locations.R48), and `--help` or `-h`, for `fleet start`, exiting 2
  on any other (shared.exit-codes.R3, shared.exit-codes.R15), and SHALL write
  every line it writes on standard error and none on standard output.
  Today: `fleet start` takes no `--project` (fleet-1s1x.4).
- ~~**onboarding.start.R4**~~ retired 2026-10-13: it refused a
  `projects.toml` that would not read. `projects.toml` is gone with
  standalone projects (fleet-1s1x).
- **onboarding.start.R5**: WHERE `FLEET_SERVICE_BIN` is set to a value that is
  not empty or spaces only, fleet SHALL run the file at the absolute path it
  names for every service manager call `fleet start` makes, in place of the
  platform's manager (Interfaces), and otherwise SHALL treat it as unset.
- **onboarding.start.R6**: IF the service manager is unusable
  (`FLEET_SERVICE_BIN` is not an absolute path to an executable file, the
  platform's manager is not on the search path, or the status query cannot
  be run), or tmux is (`FLEET_TMUX_BIN` is not an absolute path to an
  executable file, or no `tmux` is on the search path), THEN fleet SHALL exit
  3 (shared.exit-codes.R4) on a line naming the variable or program and the
  search path, having written and loaded nothing.
  Today: a relative `FLEET_SERVICE_BIN` is read against the working
  directory and used (fleet-bp1j).
- **onboarding.start.R7**: WHEN `fleet start` is called, with or without
  `--foreground`, fleet SHALL make the service manager's status query for
  `dev.fleet.controller` (Interfaces) before it writes anything.
- **onboarding.start.R8**: IF the status query reports a pid, THEN fleet SHALL
  refuse with exit 1 (shared.exit-codes.R2) on a line naming that pid and
  when the projection was last published, or that none has been, having
  written and loaded nothing.
- **onboarding.start.R9**: IF the status query answers that the manager does
  not know `dev.fleet.controller` (Interfaces), THEN fleet SHALL take it that
  no service is loaded and go on with the start.
- **onboarding.start.R10**: IF the policy file does not parse, the agent
  adapter it names does not open or does not answer `capabilities` or
  `version`, its `version` is null, or the policy file does not read as the
  controller's policy, THEN fleet SHALL exit 3 (shared.exit-codes.R4) on a
  line naming why, and, where the adapter does not open because no installed
  pack carries it, the `fleet pack add` line that installs that pack, having
  written and loaded nothing.
- **onboarding.start.R11**: fleet SHALL run every program `fleet start` runs,
  and look for `tmux`, on the search path Interfaces lists for the platform,
  and never on the caller's `PATH`.
- **onboarding.start.R12**: WHEN every check onboarding.start.R38 orders
  passes, fleet SHALL do the first-run work, except as onboarding.start.R39
  says, making the machine directory where it is absent, before it loads
  anything, writing one line per step on standard error that starts
  `first run:`, and WHERE
  `--foreground` is given, SHALL write no service file and change none, and
  say on the service file's `first run:` line that it left the file as it
  is.
  Today: a foreground start writes the service file where it is absent, or
  rewrites it to name this copy of `fleet` (fleet-zi1j).
- **onboarding.start.R13**: WHEN the first run reaches the seat list, fleet
  SHALL write `config.json` naming the policy file onboarding.start.R1 took,
  with no rows, where no `config.json` exists, and otherwise leave the policy
  file it names as it is.
- **onboarding.start.R14**: WHEN fleet renders the seat rows, fleet SHALL key
  each row it adds on the project onboarding.start.R1 found (a wingman
  project, rooted at its `path`, or a default fleet's project); and SHALL
  leave a row that already names a project keyed on the project it names.
  Today: a render keys every row on that one project and replaces the
  project each row named before (fleet-013b).
  Today: a row is keyed first on a project registered in `projects.toml`
  whose root contains the policy file's directory (fleet-1s1x.4).
- **onboarding.start.R15**: IF onboarding.start.R14 gives no project, THEN
  fleet SHALL render no row, leave every row `config.json` holds as it is,
  and say on its `first run:` line that no row was rendered and that a start
  from inside one of the fleet's projects renders them.
  Today: the render still runs with nothing to write, and drops every agent
  row the seat list held without saying so (fleet-1ba5).
- **onboarding.start.R16**: WHEN onboarding.start.R14 gives a project, fleet
  SHALL render the policy file's agent seats into `config.json` as the seats
  area defines a seat row and its worktree, create no worktree directory, and
  say on a `first run:` line what it rendered, counting every row it dropped.
  Today: a render whose policy names no agent seat drops the rows without
  counting them (fleet-1ba5).
- **onboarding.start.R17**: WHEN fleet writes the service file, fleet SHALL
  make it run the `fleet` executable that ran this start, by its full path,
  with the one argument `observe`.
- **onboarding.start.R18**: WHEN fleet writes the service file, fleet SHALL
  make it set no `PATH`, and set `FLEET_DIR` only as shared.locations.R20
  says.
- **onboarding.start.R19**: WHEN a first-run step is already done, fleet SHALL
  repeat none of it, leaving its file untouched, and WHEN no step changed
  anything, SHALL say so on a last `first run:` line.
- **onboarding.start.R20**: fleet SHALL make no load call during the first
  run, and say on a `first run:` line that nothing was started and that
  `fleet start` loads `dev.fleet.controller` as its second act.
- **onboarding.start.R21**: WHEN the first run has written the service file,
  fleet SHALL ask the platform about it (Interfaces), report the answer on a
  `first run:` line, and go on whatever the answer, including none within 20
  seconds.
- **onboarding.start.R22**: WHERE the platform is Linux, fleet SHALL never
  turn lingering on.
- **onboarding.start.R23**: WHERE the policy file declares no `[telemetry]
  enabled`, fleet SHALL write `enabled = false` into it, appending a
  `[telemetry]` table or adding the key as the first line of the one already
  there, except where telemetry is declared other than by a `[telemetry]`
  header, where it writes nothing and says the key is the person's to write.
- **onboarding.start.R24**: fleet SHALL never write `enabled = true` into the
  policy file, and SHALL report the value `[telemetry] enabled` holds on a
  `first run:` line.
- **onboarding.start.R25**: IF the telemetry edit would leave the policy file
  holding text that does not parse as TOML, THEN fleet SHALL leave the file as
  it was and exit 3 (shared.exit-codes.R4), loading nothing.
- **onboarding.start.R26**: WHEN the first run is done, fleet SHALL install the
  defaults (shared.locations.R8) and say on a line starting `defaults:`
  whether it installed them, refreshed a set that differed, or found them
  already there, or why it could not, and go on with the start either way.
- **onboarding.start.R27**: WHEN the defaults step is done and `--foreground`
  was not given, fleet SHALL ask the service manager to load the service
  once, with the calls Interfaces lists for the platform, in that order.
- **onboarding.start.R28**: IF a load call exits non-zero or cannot be run,
  THEN fleet SHALL exit 3 (shared.exit-codes.R4) on a line naming the command,
  its exit and what it printed on standard error.
- **onboarding.start.R29**: WHEN the load has returned, fleet SHALL take as its
  confirmation only a `controller.started` event appended to the stream after
  the load was asked for, never one an earlier run left, and never the load
  command's exit.
- **onboarding.start.R30**: WHEN the confirmation arrives, fleet SHALL exit 0
  (shared.exit-codes.R1) after a line starting `started` that names
  `dev.fleet.controller`, the event's sequence and the machine directory.
- **onboarding.start.R31**: WHEN `--foreground` is given and the defaults step
  is done, fleet SHALL make no load call, write a line starting `running` that
  names the machine directory, run the controller's loop (the one
  `fleet observe` runs, which the seats area defines) in the calling process,
  and exit with the loop's exit.
- ~~**onboarding.start.R32**~~ retired 2026-10-12: folded into
  shared.locations.R38
- ~~**onboarding.start.R33**~~ retired 2026-10-13: it refused a declared
  project the machine had not registered. No project declares itself any
  more (fleet-1s1x).
- **onboarding.start.R34**: IF onboarding.start.R1 takes a default fleet's
  `fleet.toml` (shared.locations.R23) or a wingman directory's
  (shared.locations.R40, shared.locations.R43, shared.locations.R44,
  shared.locations.R46), and `config.json` names another
  policy file, THEN
  fleet SHALL refuse with exit 1 (shared.exit-codes.R2) on a line naming both
  policy files and how to move the machine to the fleet the start took,
  having written and loaded nothing.
  Today: the start renders this fleet's seats into the seat list
  `config.json` holds, writes the telemetry key into this fleet's file, and
  loads a controller that runs the policy `config.json` names (fleet-rlojy).
- **onboarding.start.R35**: IF the status query exits non-zero with any
  answer but that the manager does not know `dev.fleet.controller`, THEN
  fleet SHALL exit 3 (shared.exit-codes.R4) on a line naming the query's
  command and its answer, having written and loaded nothing.
  Today: any non-zero exit is read as no service loaded, and the start goes
  on (fleet-og989).
- **onboarding.start.R36**: WHEN fleet loads the service
  (onboarding.start.R27) and the status query reported it loaded with no
  running process, fleet SHALL first have the service manager unload it,
  once, with the unload onboarding.stop's Interfaces lists, and exit 3 as
  onboarding.start.R28 says if that unload exits non-zero or cannot be run.
  Today: fleet makes no unload and asks for the load again (fleet-og989).
- **onboarding.start.R37**: WHEN `--foreground` is given and the status query
  reported the service loaded with no running process, fleet SHALL have the
  service manager unload it, once, with the unload onboarding.stop's
  Interfaces lists, and write a line naming the service it unloaded, before
  it runs the loop (onboarding.start.R31). IF that unload exits non-zero or
  cannot be run, THEN fleet SHALL exit 3 as onboarding.start.R28 says and
  run no loop.
  Today: fleet leaves the service loaded and runs its loop beside it
  (fleet-og989).
- **onboarding.start.R38**: WHEN `fleet start` is called, with or without
  `--foreground`, fleet SHALL make its checks in this order, and end the call
  at the first that fails: the machine directory (shared.locations.R30 to
  shared.locations.R32); a fleet (onboarding.start.R2); the seat list
  (shared.locations.R38); the project (shared.locations.R41,
  shared.locations.R42, shared.locations.R45, shared.locations.R47,
  shared.locations.R48, shared.locations.R52, shared.locations.R53); one
  fleet per machine
  (onboarding.start.R34); the service manager (onboarding.start.R6's manager,
  and onboarding.start.R35); already running (onboarding.start.R8); the agent
  (onboarding.start.R10's policy parse and adapter); tmux
  (onboarding.start.R6's tmux); and the policy (onboarding.start.R10's
  policy reading).
- **onboarding.start.R39**: IF, during the first run and after the seat list
  is written (onboarding.start.R13), the seats area refuses one of the policy
  file's `[seats]` tables, THEN fleet SHALL exit 3 (shared.exit-codes.R4) on a
  line naming the table, keeping what the first run wrote and loading
  nothing.

## Non-functional

- **onboarding.start.N1**: IF no confirmation (onboarding.start.R29) arrives
  within 30 seconds of the load's return, THEN fleet SHALL have the service
  manager unload the service it loaded (onboarding.stop Interfaces), keep
  every file the start wrote, and exit 3 (shared.exit-codes.R4,
  shared.exit-codes.R9) on a line naming `controller.started`, the path of
  `events.jsonl` (shared.locations.R12), and where the service's output is
  read (shared.locations.R15, shared.locations.R16).
  Today: the service stays loaded and fleet makes no unload call
  (fleet-nkgz).
- **onboarding.start.N2**: IF a service manager call (the status query, an
  unload or a load) has not returned within 60 seconds, THEN fleet SHALL stop
  waiting on it and exit 3 (shared.exit-codes.R4) on a line naming the call.
- **onboarding.start.N3**: fleet SHALL bound each call of the agent's adapter
  as the agent area defines (20 seconds, or `FLEET_AGENT_TIMEOUT_MS`), and
  treat a call past its bound as one that did not answer
  (onboarding.start.R10).
- **onboarding.start.N4**: IF a start is killed at any point, THEN fleet SHALL
  leave `config.json`, the service file and the policy file each either as it
  was or as that start would have written it, never part-written.
- ~~**onboarding.start.N5**~~ retired 2026-10-13: restated
  shared.locations.R17 for `config.json`; shared.locations.R17 states it
  (fleet-1iby).
- **onboarding.start.N6**: IF fleet start has waited 30 seconds for another
  `fleet` command's edit of `config.json` (shared.locations.R17), THEN fleet
  SHALL stop waiting and exit 3 (shared.exit-codes.R4) on a line naming
  `config.json`, writing nothing more and loading nothing.
  Today: the wait has no bound (fleet-euij).

## Interfaces

**The service manager.** `fleet start` owns the label `dev.fleet.controller`
and the manager seam (onboarding.start.R5, R6, N2). These are the calls it
makes. A stand-in named by `FLEET_SERVICE_BIN` must answer them. Each call
runs with `PATH` set to the search path below, and each is bounded at 60
seconds. `<uid>` is the calling user's uid. onboarding.stop owns the unload,
which a start makes too (onboarding.start.R36, N1).

| Act | macOS: `launchctl` with | Linux: `systemctl` with | What fleet reads back |
| --- | --- | --- | --- |
| status query | `print gui/<uid>/dev.fleet.controller` | `--user show dev.fleet.controller.service --property=MainPID` | The manager's answer that it does not know the label means not loaded (onboarding.start.R9); any other non-zero exit means the query could not tell (onboarding.start.R35). On exit 0, a `pid = <n>` line (macOS) or `MainPID=<n>` with `<n>` not 0 (Linux) means running as `<n>`; neither means loaded and not running (onboarding.start.R36). |
| load | `bootstrap gui/<uid> <service file>` | `--user daemon-reload`, then `--user enable --now dev.fleet.controller.service` | Exit 0 accepts; anything else is onboarding.start.R28. |

Unset, empty or spaces only, `FLEET_SERVICE_BIN` leaves the platform's own
manager in place:
`launchctl` on macOS and `systemctl` on Linux, the first on the search path.
`fleet start` makes one status query. Unless it refuses or runs in the
foreground, it then makes one load: one call on macOS, two on Linux. Before
the load it makes one unload where the query found the service loaded and
not running (onboarding.start.R36), and after it one more where the
confirmation does not come (onboarding.start.N1).

**The search path** (onboarding.start.R11), in order:

| Platform | Directories |
| --- | --- |
| macOS | `/usr/bin`, `/bin`, `/usr/sbin`, `/sbin`, `/opt/homebrew/bin`, `/usr/local/bin`, `.local/bin` under `HOME` |
| Linux | `.local/bin` under `HOME`, `/usr/local/bin`, `/usr/bin`, `/bin` |

On macOS the system directories come first, so a broken copy of a program the
system also ships cannot shadow the system's copy. On Linux the list holds
neither the `sbin` directories nor a package manager's prefix: a systemd user
unit's own environment has neither, and the agent installs under `.local/bin`.

**The controller.** A loaded service runs `fleet observe`, and that process
writes `controller.started` on the stream as it comes up. The seats area
defines both. A stand-in manager must have something write that event after
a load, or the start unloads the service and exits 3 (onboarding.start.N1).

**The service file**, which the platform's manager reads. shared.locations.R18
and shared.locations.R19 give its path, shared.locations.R14 to
shared.locations.R16 where its output goes, and shared.locations.R20 its
`FLEET_DIR`.

| | macOS property list | Linux user unit |
| --- | --- | --- |
| Label or name | `Label` `dev.fleet.controller` | `dev.fleet.controller.service`, `[Unit] Description` |
| Runs | `ProgramArguments`: the full path of `fleet`, then `observe` | `[Service] Type=simple`, `ExecStart=<full path of fleet> observe` |
| Starts | `RunAtLoad` true: runs as soon as it is loaded | `[Install] WantedBy=default.target`: enabled for the user's default target |
| Restarts | `KeepAlive` true: whenever it exits | `Restart=always`: whenever it exits |
| Working directory | not set | `WorkingDirectory=<machine directory>` |
| Output | `StandardOutPath`, `StandardErrorPath` (shared.locations.R14, shared.locations.R15) | the user's journal (shared.locations.R16) |
| Environment | `EnvironmentVariables` `FLEET_DIR`, as shared.locations.R20 says, and nothing else | `Environment="FLEET_DIR=…"`, as shared.locations.R20 says, and nothing else |

**The platform's check** (onboarding.start.R21, R22) runs from the search
path and is bounded at 20 seconds:

| Platform | Call | Its line says |
| --- | --- | --- |
| macOS | `plutil -lint <service file>` | the file parses, does not parse, or was not checked (`plutil` not found) |
| Linux | `loginctl show-user --property=Linger` | lingering is on; is off, naming the `enable-linger` command; or could not be read |

**Other areas.**

- The seats area: seat ids and how a `[seats]` table is keyed; what a seat
  row carries; a seat's worktree and a project's worktrees directory; the
  rows a render leaves alone, transient rows among them; and the loop
  `fleet observe` runs, with the `controller.started` event it writes
  (onboarding.start.R15, R16, R31, R39). It has no feature spec yet.
- The agent area: how the adapter `[agent] adapter` names is opened, its
  `capabilities` and `version` answers, and its call bound
  (onboarding.start.R10, N3). It has no feature spec yet.
- The status area: the projection whose publish time the already-running
  refusal names (onboarding.start.R8).
- The packs area: the defaults and `packs.lock` (onboarding.start.R26).
- The controller's policy, which onboarding.start.R10 reads, has no feature
  spec yet. The shared area's map lists it.
- onboarding.create writes the fleet that `fleet start` starts, and
  onboarding.stop takes it down.
- Environment variables read: `FLEET_DIR`, `FLEET_HOME`, `HOME`,
  `XDG_STATE_HOME` and `XDG_CONFIG_HOME` (shared.locations), plus `FLEET_SERVICE_BIN`,
  `FLEET_TMUX_BIN` and `FLEET_AGENT_TIMEOUT_MS`. `FLEET_TEST_HERMETIC` is a
  seam for fleet's own tests and is not part of this contract.

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
