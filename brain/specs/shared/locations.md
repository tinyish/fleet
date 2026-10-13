# Locations

Uses: shared.exit-codes

## What it is for

fleet keeps everything about one machine in one directory, the machine
directory. That covers the installed packs and defaults, who this machine is,
the fleet it runs, the projects registered with it, the event stream and, on
macOS, what the controller's service printed. You meet it when you look for
one of those files, when a verb names a file it wrote, and when you point
fleet at another machine directory with an environment variable, for example
a scratch directory for a test.

The other half of this spec is how a call finds its fleet. Whenever you run a
verb inside a project, fleet looks for the fleet from the directory you ran it
in, or says there is none here.

## How it behaves

### Which directory is the machine directory

fleet reads four environment variables and takes the first row that applies:

| Env var | Wins when | Machine directory |
| --- | --- | --- |
| `FLEET_DIR` | it is set | the directory it names |
| `XDG_STATE_HOME` (Linux only) | `FLEET_DIR` is unset | `fleet` under it |
| `FLEET_HOME` | `FLEET_DIR` is unset, and on Linux `XDG_STATE_HOME` is unset too | `.fleet` under it |
| `HOME` | none of the above is set | `.fleet` under it |

A variable set to an empty value, or to spaces only, counts as unset.
`FLEET_HOME` stands in for your home and leaves the layout under it the same.
macOS has no state directory of the XDG kind, so it never reads
`XDG_STATE_HOME`. On Linux, `XDG_STATE_HOME` beats `FLEET_HOME`.

The machine directory is never relative to where you stand. Read against
the working directory, a relative path would give one machine a different
machine directory in every directory it runs in, and `fleet start` would
write that relative directory into the service file. So fleet refuses,
exit 1 (shared.exit-codes.R2), in two cases:

- the variable that decides the machine directory names a relative path:
  `FLEET_DIR`, on Linux `XDG_STATE_HOME`, `FLEET_HOME` or `HOME`, whichever
  wins in the table above;
- `HOME` is unset and nothing else names the machine directory: `FLEET_DIR`
  and `FLEET_HOME` are unset too, and on Linux so is `XDG_STATE_HOME`.

The refusal line names the variable and asks for an absolute path:

```sh
$ FLEET_DIR=scratch fleet status
fleet status: FLEET_DIR is scratch, a relative path — set FLEET_DIR to an absolute path
```

Only the variable that decides is checked. A relative `FLEET_HOME` under a
set `FLEET_DIR`, or on Linux under a set `XDG_STATE_HOME`, is never read,
so it refuses nothing. Every verb refuses this way, `fleet observe` and
`fleet prime` among them, whether or not it would have read the machine
directory, and before it reads or writes anything. `fleet --version`,
`fleet` with no command, and a command line that does not parse answer as
shared.exit-codes says.

Among the verbs, `fleet guard` is the one exception, on purpose. A guard
runs as a hook, and a hook that exits 1 lets the tool call it was asked to
judge go through unjudged. So a guard reads a relative deciding variable
against the working directory, and with `FLEET_DIR`, `FLEET_HOME` and `HOME`
all unset (and on Linux `XDG_STATE_HOME` unset) it uses `.fleet` under the
working directory.

Working out the machine directory creates nothing. A verb that only reads,
such as `fleet prime`, leaves a machine directory that does not exist as it
is.

### What you find in the machine directory

| File | What you find in it | Written by |
| --- | --- | --- |
| `packs/<name>/` | one installed pack's files, in a directory named after the pack | installing a pack (`fleet create`, `fleet pack add`) |
| `defaults/` | the defaults built into the `fleet` binary that last ran `fleet create` or `fleet start` | `fleet create`, `fleet start` |
| `packs.lock` | each installed pack and the defaults: the source it came from, its version, the commit that version resolved to, and when it was installed | installing a pack or the defaults |
| `identity.toml` | who acts on this machine when a verb is given no `--by` and no `FLEET_ACTOR`: a person's seat id, and a name if you add one | `fleet create` |
| `config.json` | the policy file of the fleet this machine runs, and the seats `fleet start` rendered from it | the first `fleet start`, or `fleet create --standalone --fleet <dir>` where `config.json` is absent |
| `events.jsonl` | the machine's event stream, one event per line, oldest first | every verb and the controller, by appending |
| `projects.toml` | each standalone project registered on this machine: its root and its name, once per root. A verb acts in a declared project only where this lists its root, or the primary checkout its declaration names | `fleet create --standalone` |
| `service.out.log`, `service.err.log` | macOS only: what the controller's service printed on standard output and on standard error | the service, while it runs |

`packs.lock` records the source as you typed it, because that is what a later
`fleet pack add` of the same source matches on. It records the commit as what
the version resolved to on the day.

`fleet create --standalone --fleet <dir>` writes `config.json` itself where
it is absent, so that declaring a project to a fleet is one step and needs no `fleet start`
before or after it. Registering the same project again adds no second
`projects.toml` entry.

Two `fleet` commands that change the same file at the same moment never lose
each other's change. Without that, two registrations that each read the same
`projects.toml` would leave one row. The promise covers the files in the
machine directory and a fleet's own `fleet.toml`, wherever it sits: every
project declared to one standalone fleet edits that fleet's one file, and
none of them loses another's edit. It holds among the commands that use the
same machine directory. Two commands under two machine directories, such as
one with a scratch `FLEET_DIR` and one without, are not kept from each other
on the same `fleet.toml`.

You may find empty files ending in `.lock` in the machine directory, and a
`locks/` directory of them. They hold nothing to read: fleet uses them to
keep that promise.

When fleet changes a file in your project's checkout, or in a standalone
fleet's own directory, it leaves no other file there that you did not ask
for: no lock file, nothing git would list as untracked. So
`fleet create --embedded` and `fleet seat add` leave nothing in your checkout
but the `fleet.toml` they write.

On Linux, the service manager collects what the service prints into the
user's journal instead of a file, so the machine directory holds no
`service.out.log` or `service.err.log`. Read the service's output with
`journalctl --user -u dev.fleet.controller.service`.

### The service file

`fleet start` installs the controller as a user service, in the directory
where the platform's service manager looks for a user's own services. That
directory never moves with the machine directory:

| Platform | Service file |
| --- | --- |
| macOS | `Library/LaunchAgents/dev.fleet.controller.plist` under `HOME` |
| Linux | `systemd/user/dev.fleet.controller.service` under `XDG_CONFIG_HOME` where it is set, else `.config/systemd/user/dev.fleet.controller.service` under `HOME` |

On Linux the user's service manager reads its units from under
`XDG_CONFIG_HOME` when that is set, so the unit goes there. A relative
`XDG_CONFIG_HOME` counts as unset, because the service manager ignores one
too. macOS has no such variable: its agents directory is always under `HOME`.

The service file always sets `FLEET_DIR` for the service, to the machine
directory the `fleet start` that wrote it resolved, whichever variable
produced it. The service manager starts the controller with its own
environment, not your shell's: it carries no `FLEET_HOME`, and on Linux it
may lack the `XDG_STATE_HOME` you set. Told the directory, the controller
reads and writes the one `fleet start` wrote `config.json` in, and never
resolves another.

A seat's session is handed `FLEET_DIR`, set to the machine directory the
controller resolved, whichever variable produced it. The verbs a seat runs in
its session then act on the controller's machine, as a run's command and a
routine's action do. The session is handed no `FLEET_HOME`: with `FLEET_DIR`
set, nothing would read it.

### Finding the project a call acts in

A verb that acts in a project is one that reads the project's policy, its
store or its seats. Such a verb walks up from the working directory: first the
directory itself, then each directory above it, up to the root of the
filesystem. It stops at the first directory that holds either file:

1. `.fleet/project.toml`: a standalone project rooted there. It acts under
   the fleet whose policy file `config.json` names. This file wins even where
   a `fleet.toml` sits beside it, because the declaration is that directory's
   own statement about itself, and the neighbouring file may belong to some
   other tool.
2. `fleet.toml`: an embedded fleet rooted there, with that file as its
   policy.

A `.fleet/project.toml` counts only once this machine has registered it:
`projects.toml` lists its directory as a project's root, and `config.json`
names a fleet whose policy file is there. A declaration written by hand, in
a directory where `fleet create --standalone` never ran, is refused, and so
is one on a machine whose `config.json` is absent or names no fleet. The
refusal names the directory, the machine's file that lacks it, and
`fleet create --standalone`, which registers the project:

```sh
$ fleet doctor
fleet doctor: /home/me/app declares a project that /home/me/.fleet/projects.toml does not list — `fleet create --standalone` registers it
```

A seat's worktree of a standalone project whose `.fleet/project.toml` is
committed carries its own copy of the file. The walk stops at that copy, and
the directory it names is the worktree, which `projects.toml` never lists.
So a declaration also counts where `projects.toml` lists the checkout its
`[project] primary` names: the project's primary checkout, which
`fleet create --standalone` writes there. A root is matched as a directory,
not as a spelling, so two paths that reach one directory are one root.

A `projects.toml` or a `config.json` that is there but will not read is no
answer either way. It does not say the project is unregistered or that no
fleet is named; the verb cannot tell, and exits 3 (shared.exit-codes.R4) on
a line that names the file, acting in, writing and loading nothing. The same
holds where the walk finds no file and the verb needs `config.json` to fall
back to a fleet (below), for `fleet start` where it needs `config.json` to
find the fleet it starts, and for `fleet create --standalone`, which reads
`config.json` to learn which fleet it declares the project to.

`fleet create` is not refused in such a directory, because it is the command
that registers. `fleet stop` acts from any directory and resolves no fleet
(see below), so it is not refused there. `fleet start` also acts from any
directory, but in a declared project the machine has not registered it is
refused like every verb that acts in a project: a start there would put the
fleet's seats on a project the machine does not know. `fleet store check` and
`fleet agent check` are not refused either:
they read the adapter settings that directory's project names, as they do in
a registered one. `fleet guard` reads such a directory as its own spec says.

The nearest directory wins. Take a `fleet.toml` in `~/work/app` and another in
`~/work/app/tools`. A verb run in `~/work/app/tools/sub` acts in the fleet in
`tools`.

If the walk finds neither file, there is one fallback. When the working
directory is inside a git checkout and `config.json` names a policy file that
exists, the verb acts in that fleet, with the nearest checkout as the
project's root. A checkout has a `.git` directory, or a `.git` file in a
linked worktree, at or above the working directory. The fallback covers a
seat's worktree cut from a project whose own `fleet.toml` is not committed: no
file sits above it, and the machine still names the fleet.

Outside every checkout, the verb refuses. There is no tree to act in, and a
guessed project would put a seat in the wrong one. The refusal names the
working directory and `fleet create`, and exits as shared.exit-codes.R23
says. `fleet prime` is such a verb. A seat's session runs it in the seat's
worktree, which is a checkout, so a seat is primed wherever it works,
provided the walk finds its fleet: a committed `fleet.toml` (R23); a
committed `.fleet/project.toml` whose primary checkout `projects.toml`
lists, while `config.json` names a policy file that exists (R22); or, with
no file, `config.json` naming a policy file that exists (R24). Otherwise it
refuses (R25, R26, R35), and a session outside every checkout is told there
is no fleet there. `fleet prime` exits as every verb does: 1 where it finds
no fleet or a declared project the machine has not registered
(shared.exit-codes.R2, shared.exit-codes.R23), and 3 where `config.json` or
`projects.toml` is there but will not read (R38, shared.exit-codes.R4).
`fleet create`, `fleet start`, `fleet stop` and `fleet guard` decide in
their own specs what they do where there is no fleet. `fleet store check`
and `fleet agent check` find their fleet as every verb does, the checkout
fallback (R24) included, and only where no fleet is found at all do they
read the defaults, without refusing.

The verbs differ where no file is found above the working directory:

| Verb | Where no file is found |
| --- | --- |
| the item verbs, `fleet doctor`, `fleet prime` | act in `config.json`'s fleet only inside a git checkout, and refuse outside one |
| `fleet start`, `fleet stop` | act on the machine from any directory, inside a checkout or not |
| `fleet store check`, `fleet agent check` | read the defaults where R21 to R24 find no fleet, and do not refuse |

`fleet stop` resolves no fleet at all. A stop that looked for a project first
would refuse from a bare directory while the service was loaded, and leave
the controller running.

Walking up creates nothing.

### Known gaps

Each clause below states what fleet is meant to do. Where today's binary does
something else, a `Today:` line follows the clause and names the item that
closes the gap.

- **Lost edits** (fleet-rbwc, fleet-adol, fleet-z2qd). A first write of
  `config.json`, `fleet start`'s telemetry edit of `fleet.toml`, and any
  change to `packs.lock` can each lose a change another command makes at the
  same moment.
- **A relative `HOME` or `XDG_STATE_HOME`** (fleet-8s9z). Either is read
  against the working directory where it decides the machine directory, and
  `fleet start` writes the relative directory it gives into the service file.
- **`fleet prime` and the machine directory** (fleet-bxch). Like
  `fleet guard`, it reads a relative `FLEET_DIR` or `FLEET_HOME` against the
  working directory, and with no `HOME` uses `.fleet` there, instead of
  refusing.
- **`fleet start` takes the neighbouring `fleet.toml`** (fleet-5qo1), in a
  directory that also holds a `.fleet/project.toml`.
- **A `config.json` that does not parse** (fleet-9sr5) gets the no-fleet
  refusal, exit 1, instead of exit 3, in three places: the verbs that find
  their fleet, `fleet start`, and `fleet create --standalone`.
- **`fleet event tail --seat` in an unregistered declared project**
  (fleet-e0fb) refuses on a `--seat` line instead of naming `projects.toml`.
- **`fleet start` in an unregistered declared project** (fleet-xjn7) is not
  refused. It starts the fleet `config.json` names and keys the seat rows on
  that declaration.

## Behaviour

- **shared.locations.R1**: IF `FLEET_DIR`, `FLEET_HOME`, `HOME`,
  `XDG_STATE_HOME` or `XDG_CONFIG_HOME` is set to an empty value or to spaces
  only, THEN fleet SHALL treat it as unset everywhere in this spec.
- **shared.locations.R2**: WHILE `FLEET_DIR` is set, fleet SHALL use the
  directory it names as the machine directory, whatever `FLEET_HOME`, `HOME`
  and `XDG_STATE_HOME` say.
- **shared.locations.R3**: WHILE `FLEET_DIR` is unset, fleet SHALL take the
  home the machine directory sits under from `FLEET_HOME` where it is set, and
  from `HOME` where it is not.
- **shared.locations.R4**: WHERE the platform is macOS, WHILE `FLEET_DIR` is
  unset, fleet SHALL use `.fleet` under the home R3 names as the machine
  directory, whatever `XDG_STATE_HOME` says.
- **shared.locations.R5**: WHERE the platform is Linux, WHILE `FLEET_DIR` is
  unset and `XDG_STATE_HOME` is set, fleet SHALL use `fleet` under
  `XDG_STATE_HOME` as the machine directory, whatever `FLEET_HOME` and `HOME`
  say.
- **shared.locations.R6**: WHERE the platform is Linux, WHILE `FLEET_DIR` and
  `XDG_STATE_HOME` are both unset, fleet SHALL use `.fleet` under the home R3
  names as the machine directory.
- **shared.locations.R7**: WHEN a pack is installed on this machine, fleet
  SHALL keep its files in a directory named after the pack under `packs/` in
  the machine directory.
- **shared.locations.R8**: WHEN `fleet create` or `fleet start` runs, fleet
  SHALL leave in `defaults/` in the machine directory the defaults built into
  the `fleet` binary that ran it.
- **shared.locations.R9**: WHEN a pack or the defaults are installed, fleet
  SHALL name each one installed in `packs.lock` in the machine directory, with
  the source it was installed from, its version, the commit that version
  resolved to, and when it was installed.
- **shared.locations.R10**: WHEN `fleet create` has run, fleet SHALL name in
  `identity.toml` in the machine directory the person who acts on this machine
  when a verb is given no `--by` and no `FLEET_ACTOR`: a human seat's id, and
  a name where one is added.
- **shared.locations.R11**: WHEN `fleet start` has run, or
  `fleet create --standalone --fleet <dir>` has run where `config.json` was
  absent, fleet SHALL name in `config.json` in the machine directory the policy
  file of the fleet this machine runs, and the seats `fleet start` rendered
  from it.
- **shared.locations.R12**: WHEN fleet appends an event to the machine's event
  stream, fleet SHALL append it to `events.jsonl` in the machine directory, one
  event per line, after every line already there.
- **shared.locations.R13**: WHEN `fleet create --standalone` registers a
  project, fleet SHALL name that project's root and its name in
  `projects.toml` in the machine directory, once per root however often the
  project is registered.
- **shared.locations.R14**: WHERE the platform is macOS, WHEN the controller's
  service prints on standard output, fleet SHALL keep what it printed in
  `service.out.log` in the machine directory.
- **shared.locations.R15**: WHERE the platform is macOS, WHEN the controller's
  service prints on standard error, fleet SHALL keep what it printed in
  `service.err.log` in the machine directory.
- **shared.locations.R16**: WHERE the platform is Linux, WHEN the controller's
  service prints, fleet SHALL leave what it printed in the user's journal,
  read with `journalctl --user -u dev.fleet.controller.service`, and write no
  `service.out.log` or `service.err.log`.
- **shared.locations.R17**: IF two `fleet` commands that use the same machine
  directory change the same file in it, or the same fleet's `fleet.toml`, at
  the same moment, THEN fleet SHALL keep both commands' changes, and neither
  SHALL overwrite the other's.
  Today: a first write of `config.json` can lose a change another command
  makes to it at the same moment (fleet-rbwc), and so can `fleet start`'s
  telemetry edit of `fleet.toml` (fleet-adol). Two commands that change
  `packs.lock` at once, such as two `fleet pack add`s, or one racing
  `fleet start`'s defaults pin, can lose an entry (fleet-z2qd).
- **shared.locations.R18**: WHERE the platform is macOS, WHEN `fleet start`
  installs the controller's service, fleet SHALL write its file at
  `Library/LaunchAgents/dev.fleet.controller.plist` under `HOME`, whatever
  `FLEET_DIR` and `FLEET_HOME` say.
- **shared.locations.R19**: WHERE the platform is Linux, WHEN `fleet start`
  installs the controller's service, fleet SHALL write its file at
  `systemd/user/dev.fleet.controller.service` under `XDG_CONFIG_HOME` where it
  is set to an absolute path, and at
  `.config/systemd/user/dev.fleet.controller.service` under `HOME` where it is
  not (a relative `XDG_CONFIG_HOME` counts as not set, as the service manager
  ignores one), whatever `FLEET_DIR`, `FLEET_HOME` and `XDG_STATE_HOME` say.
- **shared.locations.R20**: WHEN `fleet start` writes the service file, fleet
  SHALL set `FLEET_DIR` for the service to the machine directory that
  `fleet start` resolved, whichever of R2 to R6 produced it.
- **shared.locations.R21**: WHEN a verb that acts in a project (one that reads
  its policy, its store or its seats) is called, fleet SHALL look in the
  working directory and then in each directory above it, up to the root of
  the filesystem, and stop at the first that holds a `.fleet/project.toml` or
  a `fleet.toml`.
- **shared.locations.R22**: WHEN the directory R21 stops at holds a
  `.fleet/project.toml`, `config.json` names a policy file that exists, and
  `projects.toml` lists that directory, or the checkout the declaration's
  `[project] primary` names, as a project's root (R13, matched as R36 says),
  fleet SHALL act in the standalone project rooted there, under the fleet
  whose policy file `config.json` names (R11), even where a `fleet.toml` sits
  beside it.
  Today: `fleet start` takes a `fleet.toml` beside the declaration as the
  fleet's policy file instead (fleet-5qo1).
- **shared.locations.R23**: WHEN the directory R21 stops at holds a
  `fleet.toml` and no `.fleet/project.toml`, fleet SHALL act in the embedded
  fleet rooted there, with that `fleet.toml` as its policy file.
- **shared.locations.R24**: WHEN R21 finds neither file, the working directory
  is inside a git checkout (a `.git` directory or file in it or in a directory
  above it), and `config.json` names a policy file that exists, fleet SHALL act
  in that fleet, with the nearest checkout at or above the working directory as
  the project's root.
- **shared.locations.R25**: IF a verb that acts in a project finds no fleet by
  R21 to R24, THEN fleet SHALL refuse with the exit shared.exit-codes.R23
  states, on a refusal line (shared.exit-codes.R12, shared.exit-codes.R13)
  that names the working directory and `fleet create`. `fleet create`,
  `fleet start`, `fleet stop` and `fleet guard` are not bound by this clause: their own
  specs say what each does where there is no fleet. `fleet store check` and
  `fleet agent check` are not bound by it either: where R21 to R24 find no
  fleet they read the defaults and do not refuse.
- **shared.locations.R26**: IF the directory R21 stops at holds a
  `.fleet/project.toml` and `config.json` is absent, or reads and names no
  policy file that exists, THEN fleet SHALL refuse (shared.exit-codes.R2) on a refusal
  line (shared.exit-codes.R12, shared.exit-codes.R13) that names that
  directory, `config.json` and `fleet create --standalone`, the command that
  registers the project.
  `fleet create`, `fleet guard`, `fleet store check`, `fleet agent check` and
  the verbs R28 names are not bound by this clause.
- ~~**shared.locations.R27**~~ retired 2026-10-12: it stated `fleet prime`'s
  fallback from any directory, which fleet-x1qu ruled was not meant.
  `fleet prime` finds its fleet as every verb that acts in a project does,
  by R21 to R26 and R35: where no file is found, that is R24's fallback
  inside a checkout and R25's refusal outside one.
- **shared.locations.R28**: WHEN `fleet start` or `fleet stop` is called,
  fleet SHALL act from any working directory, inside a project or a git
  checkout or not, except that R35 refuses `fleet start` in a declared
  project the machine has not registered.
  Today: `fleet start` is not refused there (fleet-xjn7).
- **shared.locations.R29**: WHEN the controller starts a seat's session, fleet
  SHALL hand the session `FLEET_DIR` set to the machine directory the
  controller resolved (R2 to R6), and no `FLEET_HOME`.
- **shared.locations.R30**: IF the variable that decides the machine
  directory by R2 to R6 names a relative path, THEN fleet SHALL, except as
  R32 says, refuse (shared.exit-codes.R2) on a refusal line
  (shared.exit-codes.R12) that names that variable and asks for an absolute
  path.
  Today: a deciding `HOME`, or on Linux `XDG_STATE_HOME`, that names a
  relative path is read against the working directory, and `fleet start`
  writes the relative machine directory it gives into the service file
  (fleet-8s9z).
- **shared.locations.R31**: IF no variable that could decide the machine
  directory by R2 to R6 is set, THEN fleet SHALL, except as R32 says,
  refuse (shared.exit-codes.R2) on a refusal line (shared.exit-codes.R12)
  that names `HOME` and asks for an absolute path.
- **shared.locations.R32**: IF R30 or R31 refuses a call, THEN fleet SHALL
  refuse it before the verb reads or writes anything, whichever verb was
  called and whether or not it reads the machine directory, except for
  four calls. `fleet --version` (shared.exit-codes.R1), `fleet` with no
  command (shared.exit-codes.R16) and a command line that does not parse
  (shared.exit-codes.R15) each answer as that clause says. `fleet guard`
  reads the variable that decides the machine directory by R2 to R6 against
  the working directory where it names a relative path, and where R31 would
  refuse uses `.fleet` under the working directory, because a guard hook
  that exits 1 lets the tool call through unjudged.
  Today: `fleet prime` is exempt as well: it reads the machine directory as
  `fleet guard` does and does not refuse (fleet-bxch).
- ~~**shared.locations.R33**~~ retired 2026-10-12: it named where the lock on
  a fleet's `fleet.toml` is kept (`locks/` in the machine directory), which
  is how fleet keeps R17's and R34's promises, not a promise of its own.
- **shared.locations.R34**: WHEN fleet changes a file in a project's checkout
  or in a standalone fleet's own directory, fleet SHALL leave no other file
  there that the person did not ask for, a lock file among them.
- **shared.locations.R35**: IF the directory R21 stops at holds a
  `.fleet/project.toml`, `config.json` names a policy file that exists,
  `projects.toml` reads, and it lists as a project's root neither that directory nor the
  checkout the declaration's `[project] primary` names, THEN fleet SHALL
  refuse (shared.exit-codes.R2) on a refusal line (shared.exit-codes.R12,
  shared.exit-codes.R13) that names that directory, `projects.toml` and
  `fleet create --standalone`, the command that registers it, where a root
  is matched as R36 says. `fleet create`, `fleet guard`, `fleet store check`,
  `fleet agent check` and `fleet stop` are not bound by this clause;
  `fleet start` is.
  Today: `fleet event tail --seat`, given a seat that only that fleet's
  `fleet.toml` names, refuses there on a `--seat` line that does not name
  `projects.toml` (fleet-e0fb).
  Today: `fleet start` is not refused there: it starts the fleet
  `config.json` names and keys the seat rows on that declaration
  (fleet-xjn7).
- **shared.locations.R36**: WHEN fleet decides by R22 or R35 whether
  `projects.toml` lists a directory, fleet SHALL count the two as one root
  where they name the same directory, however each is spelled.
- ~~**shared.locations.R37**~~ retired 2026-10-13: no older fleet runs
  anywhere to have left a `fleet.toml.lock`, so fleet promises nothing about
  one (Alberto).
- **shared.locations.R38**: IF a verb needs `config.json` or `projects.toml`
  to decide which fleet it acts in (R22, R24, R26, R35), which fleet
  `fleet start` starts, or which fleet `fleet create --standalone` declares a
  project to, and that
  file is there but cannot be read or does not parse, THEN fleet SHALL exit 3
  (shared.exit-codes.R4) on a line (shared.exit-codes.R12,
  shared.exit-codes.R13) that names that file, and act in, write or load
  nothing.
  Today: a `config.json` that does not parse gets the no-fleet refusal, exit
  1, in three places (fleet-9sr5): the verbs that find their fleet, with
  R26's refusal in a declared project and R25's where the walk finds no file
  inside a checkout; `fleet start`; and `fleet create --standalone`.

## Non-functional

- **shared.locations.N1**: WHEN fleet resolves the machine directory (R1 to
  R6, R30 to R32) or looks for the fleet a call acts in (R21 to R24, R26,
  R35, R36, R38), fleet SHALL create no file and no directory in doing so.

## Interfaces

- Every other feature spec names a file in the machine directory by its path
  relative to that directory (`config.json`, `packs/<name>/`), and cites the
  clause here that says what a person finds in it. A spec that keeps a further
  file there (the projection, the session list, a run's directory) names it
  in its own clauses.
- The operating system's service manager reads the service file (R18, R19)
  and, through it, the machine directory the service is told about (R20).
- A run's command and a routine's action are handed `FLEET_DIR` naming the
  machine directory; the runs and routines specs say so.
- A verb that takes `--packs-dir` reads packs from the directory it names in
  place of `packs/`; the packs area's spec says what else it moves.
- The guards, `fleet prime`, `fleet start` and the item verbs walk up the
  same way (R21 to R23). Every verb that acts in a project refuses a declared
  project the machine has not registered (R26, R35): the item verbs,
  `fleet prime`, `fleet doctor`, `fleet run`, the seat verbs that act in a
  project (`fleet seat add`, `spawn`, `feed`, `retire`, `nudge` and `attach`) and the
  `fleet routine` verbs among them, and `fleet start` is refused there too
  (R28, R35). `fleet create`, `fleet stop`, `fleet store check` and
  `fleet agent check` are not refused there, and the guards' spec says what
  a guard reads there. `fleet
  prime` and the item verbs then fall back to a checkout (R24) and, where
  that finds no fleet, refuse (R25). `fleet guard` follows its own spec where
  no file is found (R25). Where no file is found, `fleet start` acts on
  `config.json`'s fleet from any working directory; `fleet stop` resolves no
  fleet and acts from any working directory (R28).
- A seat's session is handed `FLEET_DIR` naming the machine directory (R29);
  the seats area's spec cites that clause.
- shared.exit-codes.R23 states the exit for a verb that finds no fleet (R25).

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
