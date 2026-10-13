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

A machine directory is never relative to where you stand. Read against the
working directory, a relative path would give one machine a different
machine directory in every directory it runs in. So fleet refuses, exit 1
(shared.exit-codes.R2), in two cases:

- the variable that decides the machine directory, `FLEET_DIR` or
  `FLEET_HOME`, names a relative path;
- `HOME` is unset and nothing else names the machine directory: `FLEET_DIR`
  and `FLEET_HOME` are unset too, and on Linux so is `XDG_STATE_HOME`.

The refusal line names the variable and asks for an absolute path:

```sh
$ FLEET_DIR=scratch fleet status
fleet status: FLEET_DIR is scratch, a relative path — set FLEET_DIR to an absolute path
```

Only the variable that decides is checked. A relative `FLEET_HOME` under a
set `FLEET_DIR`, or on Linux under a set `XDG_STATE_HOME`, is never read,
so it refuses nothing. Every verb refuses this way, `fleet observe` among
them, whether or not it would have read the machine directory, and before
it reads or writes anything. `fleet --version`, `fleet` with no command,
and a command line that does not parse answer as shared.exit-codes says.
`fleet guard` and `fleet prime` are the exception: they read a relative
`FLEET_DIR` or `FLEET_HOME` against the working directory, and with
`FLEET_DIR`, `FLEET_HOME` and `HOME` all unset (and on Linux
`XDG_STATE_HOME` unset) use `.fleet` under the working directory (see Open
questions).

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
| `config.json` | the policy file of the fleet this machine runs, and the seats `fleet start` rendered from it | the first `fleet start`, or `fleet create --standalone --fleet <dir>` where no fleet is named yet |
| `events.jsonl` | the machine's event stream, one event per line, oldest first | every verb and the controller, by appending |
| `projects.toml` | each standalone project registered on this machine: its root and its name, once per root | `fleet create --standalone` |
| `service.out.log`, `service.err.log` | macOS only: what the controller's service printed on standard output and on standard error | the service, while it runs |
| `<file>.lock` | nothing to read: an empty file left beside `projects.toml`, `config.json` or a fleet's `fleet.toml` once fleet has changed it | the verb that changed that file |

`packs.lock` records the source as you typed it, because that is what a later
`fleet pack add` of the same source matches on. It records the commit as what
the version resolved to on the day.

`fleet create --standalone --fleet <dir>` writes `config.json` itself, so
that declaring a project to a fleet is one step and needs no `fleet start`
before or after it. Registering the same project again adds no second
`projects.toml` entry.

A `.lock` file exists so that two `fleet` processes changing the same file at
once do not overwrite each other: two registrations that each read the same
`projects.toml` would leave one row. fleet leaves the lock file in place.
Removing it would let a second process take a lock that guards nothing. A
project's own file gets its lock beside it too, so `fleet create --embedded`
leaves a `fleet.toml.lock` in your checkout, where git lists it as untracked.
That is changing (see below).

On Linux, the service manager collects what the service prints into the
user's journal instead of a file, so the machine directory holds no
`service.out.log` or `service.err.log`. Read the service's output with
`journalctl --user -u dev.fleet.controller.service`.

### The service file

`fleet start` installs the controller as a user service, in the directory
where the platform's service manager looks for a user's own services. That
directory is under `HOME`, whatever the machine directory is:

| Platform | Service file |
| --- | --- |
| macOS | `Library/LaunchAgents/dev.fleet.controller.plist` under `HOME` |
| Linux | `.config/systemd/user/dev.fleet.controller.service` under `HOME` |

The Linux path ignores `XDG_CONFIG_HOME`, even though a user service manager
with `XDG_CONFIG_HOME` set elsewhere does not look there.

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

A `.fleet/project.toml` counts whether or not `projects.toml` lists its root.
A declaration written by hand, in a directory where `fleet create --standalone`
never ran, is acted in all the same. So is one on a machine whose
`config.json` is absent or names no fleet.

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
says. `fleet create`, `fleet start`, `fleet stop`, `fleet prime` and
`fleet guard` decide in their own specs what they do where there is no fleet.

Today the verbs differ where no file is found above the working directory:

| Verb | Where no file is found |
| --- | --- |
| the item verbs, `fleet doctor` | act in `config.json`'s fleet only inside a git checkout, and refuse outside one |
| `fleet prime` | acts under `config.json`'s fleet from any directory, inside a checkout or not |
| `fleet start`, `fleet stop` | act on the machine from any directory, inside a checkout or not |

`fleet stop` resolves no fleet at all. A stop that looked for a project first
would refuse from a bare directory while the service was loaded, and leave
the controller running.

Walking up creates nothing.

### What is changing

Each of these is an approved change. Until its bead lands, the clauses state
what fleet does today, with a `Changing:` line after each one.

- **A declared project must be registered** (fleet-np9q). A verb in a
  `.fleet/project.toml` that `projects.toml` does not list will refuse, and
  name the command that registers it. So will a verb in a declared project on
  a machine whose `config.json` is absent or names no fleet.
- **The fallback splits by kind of verb** (fleet-x1qu). Project verbs (the
  items, `fleet doctor`, `fleet prime`) fall back only inside a checkout, and
  refuse outside one. Machine verbs (`fleet start`, `fleet stop`) act from any
  directory. Only `fleet prime` changes.
- **No lock file in a checkout** (fleet-hn6e). The locks for a project's files
  live in the machine directory, keyed by the project.
- **On Linux, the unit follows `XDG_CONFIG_HOME`** (fleet-r0xz), where it is
  set.

## Behaviour

- **shared.locations.R1**: IF `FLEET_DIR`, `FLEET_HOME`, `HOME` or
  `XDG_STATE_HOME` is set to an empty value or to spaces only, THEN fleet
  SHALL treat it as unset everywhere in this spec.
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
  `fleet create --standalone --fleet <dir>` has run where no fleet was named
  yet, fleet SHALL name in `config.json` in the machine directory the policy
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
- **shared.locations.R17**: WHEN fleet has changed `projects.toml`,
  `config.json` or a fleet's `fleet.toml`, fleet SHALL leave an empty file
  beside that file, named after it with `.lock` appended (such as
  `projects.toml.lock`), which holds nothing a person reads.
  Changing: fleet-hn6e: the lock for a project's file is kept in the machine
  directory, keyed by the project, and no lock file appears in a checkout.
- **shared.locations.R18**: WHERE the platform is macOS, WHEN `fleet start`
  installs the controller's service, fleet SHALL write its file at
  `Library/LaunchAgents/dev.fleet.controller.plist` under `HOME`, whatever
  `FLEET_DIR` and `FLEET_HOME` say.
- **shared.locations.R19**: WHERE the platform is Linux, WHEN `fleet start`
  installs the controller's service, fleet SHALL write its file at
  `.config/systemd/user/dev.fleet.controller.service` under `HOME`, whatever
  `FLEET_DIR`, `FLEET_HOME`, `XDG_STATE_HOME` and `XDG_CONFIG_HOME` say.
  Changing: fleet-r0xz: where `XDG_CONFIG_HOME` is set, the file is
  `systemd/user/dev.fleet.controller.service` under `XDG_CONFIG_HOME`.
- **shared.locations.R20**: WHEN `fleet start` writes the service file, fleet
  SHALL set `FLEET_DIR` for the service to the machine directory that
  `fleet start` resolved, whichever of R2 to R6 produced it.
- **shared.locations.R21**: WHEN a verb that acts in a project (one that reads
  its policy, its store or its seats) is called, fleet SHALL look in the
  working directory and then in each directory above it, up to the root of
  the filesystem, and stop at the first that holds a `.fleet/project.toml` or
  a `fleet.toml`.
- **shared.locations.R22**: WHEN the directory R21 stops at holds a
  `.fleet/project.toml`, fleet SHALL act in the standalone project rooted
  there, under the fleet whose policy file `config.json` names (R11), even
  where a `fleet.toml` sits beside it, and whether or not `projects.toml`
  lists that project's root.
  Changing: fleet-np9q: where `projects.toml` does not list that root, fleet
  refuses (shared.exit-codes.R2) on a line that names
  `fleet create --standalone`, the command that registers it.
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
  states, on a refusal line (shared.exit-codes.R12, R13) that names the
  working directory and `fleet create`. `fleet create`, `fleet start`,
  `fleet stop`, `fleet prime` and `fleet guard` are not bound by this clause:
  their own specs say what each does where there is no fleet.
  Changing: fleet-x1qu: `fleet prime` is bound by this clause too.
- **shared.locations.R26**: WHEN the directory R21 stops at holds a
  `.fleet/project.toml` and `config.json` is absent or names no policy file
  that exists, fleet SHALL still act in the standalone project rooted there.
  Changing: fleet-np9q: fleet refuses (shared.exit-codes.R2) on a line that
  names `fleet create --standalone`, the command that registers it.
- **shared.locations.R27**: WHEN `fleet prime` is called, R21 finds neither
  file, and `config.json` names a policy file that exists, fleet SHALL act
  under that fleet whether or not the working directory is inside a git
  checkout.
  Changing: fleet-x1qu: `fleet prime` falls back only inside a checkout, as
  R24 says, and refuses outside one, as R25 says.
- **shared.locations.R28**: WHEN `fleet start` or `fleet stop` is called,
  fleet SHALL act from any working directory, inside a project or a git
  checkout or not.
- **shared.locations.R29**: WHEN the controller starts a seat's session, fleet
  SHALL hand the session `FLEET_DIR` set to the machine directory the
  controller resolved (R2 to R6), and no `FLEET_HOME`.
- **shared.locations.R30**: IF `FLEET_DIR` names a relative path, or
  `FLEET_DIR` is unset, `FLEET_HOME` names a relative path and R5 does not
  apply, THEN fleet SHALL refuse (shared.exit-codes.R2) on a refusal line
  (shared.exit-codes.R12) that names that variable and asks for an absolute
  path.
- **shared.locations.R31**: IF `FLEET_DIR`, `FLEET_HOME` and `HOME` are all
  unset, and on Linux `XDG_STATE_HOME` is unset too, THEN fleet SHALL refuse
  (shared.exit-codes.R2) on a refusal line (shared.exit-codes.R12) that
  names `HOME` and asks for an absolute path.
- **shared.locations.R32**: IF R30 or R31 refuses a call, THEN fleet SHALL
  refuse it before the verb reads or writes anything, whichever verb was
  called and whether or not it reads the machine directory, except `fleet
  guard` and `fleet prime`, which read a relative `FLEET_DIR` or
  `FLEET_HOME` against the working directory, and with `FLEET_DIR`,
  `FLEET_HOME` and `HOME` all unset (and on Linux `XDG_STATE_HOME` unset)
  use `.fleet` under the working directory (see Open questions).

## Non-functional

- **shared.locations.N1**: WHEN fleet resolves the machine directory (R1 to
  R6, R30 to R32) or looks for the fleet a call acts in (R21 to R24, R26, R27),
  fleet SHALL create no file and no directory in doing so.

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
- The guards, `fleet prime` and the item verbs walk up the same way (R21 to
  R23). They differ only where no file is found (R24, R27, R28).
- A seat's session is handed `FLEET_DIR` naming the machine directory (R29);
  the seats area's spec cites that clause.
- shared.exit-codes.R23 states the exit for a verb that finds no fleet (R25).

## Open questions

- [NEEDS CLARIFICATION: should `fleet guard` and `fleet prime` also refuse a relative `FLEET_DIR` or `FLEET_HOME`, or no `HOME` (shared.locations.R30, R31)? A guard hook that exits 1 lets the tool call through unjudged, and `fleet prime`'s help promises exit 0, so both are exempt until ruled (fleet-5yd9).]

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
