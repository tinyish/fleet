# Locations

Uses: shared.exit-codes

## What it is for

fleet keeps everything about one machine in one directory, the machine
directory. That covers the installed packs and defaults, who this machine is,
the fleet it runs, the event stream and, on macOS, what the controller's
service printed. You meet it when you look for
one of those files, when a verb names a file it wrote, and when you point
fleet at another machine directory with an environment variable, for example
a scratch directory for a test.

The other half of this spec is how a call finds its fleet. Whenever you run a
verb inside a project, fleet looks for the fleet from the directory you ran it
in, or says there is none here. A fleet comes in two kinds:

- **A default fleet** keeps its policy file, `fleet.toml`, in the project it
  serves. A plain `fleet create` makes one.
- **A wingman fleet** leaves no trace in the projects it serves. Its files
  live in a directory of their own, the wingman directory: one shared
  `fleet.toml`, and one project file per project, `<name>.fleet.toml`,
  naming the project's checkout. You use one to try fleet on a project
  without teammates seeing it in committed files, or to run one fleet over
  several repositories. `fleet create --wingman` makes one.

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
| `defaults/` | the defaults built into the `fleet` binary that last installed them, with `fleet create` or `fleet start` | `fleet create`, `fleet start` |
| `packs.lock` | each installed pack and the defaults: the source it came from, its version, the commit that version resolved to, and when it was installed | installing a pack or the defaults |
| `identity.toml` | who acts on this machine when a verb is given no `--by` and no `FLEET_ACTOR`: a person's seat id, and a name if you add one | `fleet create` |
| `config.json` | the policy file of the fleet this machine runs, and the seats `fleet start` rendered from it | the first `fleet start`, or `fleet create --wingman` where `config.json` is absent or names a policy file that is gone |
| `events.jsonl` | the machine's event stream, one event per line, oldest first | every verb and the controller, by appending |
| `service.out.log`, `service.err.log` | macOS only: what the controller's service printed on standard output and on standard error | the service, while it runs |

`packs.lock` records the source as you typed it, because that is what a later
`fleet pack add` of the same source matches on. It records the commit as what
the version resolved to on the day.

`fleet create --wingman` writes `config.json` itself where it is absent, so
that adding a project to a wingman fleet is one step and needs no
`fleet start` before or after it. Where `config.json` names a policy file
that is gone, as after the wingman directory moved, `fleet create --wingman`
points it at the wingman directory it was given and keeps its seat rows. The
onboarding.create spec says how.

Two `fleet` commands that change the same file at the same moment never lose
each other's change. Without that, two `fleet seat add` calls that each read
the same `fleet.toml` would leave one seat. The promise covers the files in the
machine directory, a fleet's own `fleet.toml` wherever it sits, and a wingman
directory's project files: every project of one wingman fleet lists its
people in that fleet's one shared file, and none of them loses another's
edit. It holds among the commands that use the
same machine directory. Two commands under two machine directories, such as
one with a scratch `FLEET_DIR` and one without, are not kept from each other
on the same `fleet.toml`.

You may find empty files ending in `.lock` in the machine directory, and a
`locks/` directory of them. They hold nothing to read: fleet uses them to
keep that promise.

When fleet changes a file in your project's checkout, or in a wingman
directory, it leaves no other file there that you did not ask for: no lock
file, nothing git would list as untracked. So `fleet create` and
`fleet seat add` leave nothing in your checkout but the `fleet.toml` they
write, and `fleet create --wingman` leaves nothing in it at all.

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
store or its seats. Such a verb looks for its fleet in two ways at once:

- **The wingman lookup.** Where `config.json` names the `fleet.toml` of a
  wingman directory, fleet finds the git checkout the working directory is
  in, and, where that is a seat's linked worktree, the primary checkout it
  was added from. It then looks among the wingman directory's project files
  for the one whose `path` names that checkout. So a seat's worktree, which
  lives outside the project, still resolves to its project.
- **The walk.** It walks up from the working directory: first the directory
  itself, then each directory above it, up to the root of the filesystem,
  and stops at the first directory that holds a `fleet.toml`.

A directory is a wingman directory where it holds a `fleet.toml` beside at
least one file named `<name>.fleet.toml`. Each such file is one project's,
and `<name>` is the project's name:

    /xwing-fleet            a wingman directory
        fleet.toml          the shared policy
        acme.fleet.toml     path = "~/code/acme"
        foobar.fleet.toml   path = "/work/foobar"
    ~/code/acme             nothing fleet-related
    /work/foobar            nothing fleet-related

What the verb acts in:

| The lookup | The walk | The verb |
| --- | --- | --- |
| finds a project | stops at a `fleet.toml` that is not the wingman directory's own | refuses, exit 1, naming both files |
| finds a project | finds nothing, or the wingman directory's own `fleet.toml` | acts in that wingman project, rooted at its `path` |
| finds nothing | stops at a wingman directory, from a directory in no git checkout | acts in a project of that wingman directory, picked as below |
| finds nothing | stops at a `fleet.toml` that is not a wingman directory's | acts in the default fleet rooted there |
| finds nothing | finds nothing, or stops at a wingman directory from inside a checkout no project file names | falls back to `config.json`'s fleet, or refuses, as below |

A checkout claimed by a wingman project that also holds a `fleet.toml`, say
because the team later committed a default fleet, is refused rather than
settled for you: either answer would put work in a fleet you might not
mean. Remove one of the two. A wingman directory may itself sit above its
projects, as `~/code` holding `fleet.toml`, `acme.fleet.toml` and the
checkout `~/code/acme`. The walk from inside `~/code/acme` then stops at the
wingman directory's own `fleet.toml`, which is the same fleet, not a second.

Two project files that name one checkout are refused too, exit 1, naming
both: one path has one owner. fleet never picks between them.

A project file's `path` names a checkout's root: the project's primary
checkout. It is matched as a directory, not as a spelling, so two paths that
reach one directory are one. A `path` starting `~` is read against `HOME`,
never `FLEET_HOME`: it is your checkout, not the machine directory, and a
wingman directory synced between two machines resolves on both.

**In the wingman directory itself**, outside every checkout, there is no
checkout to match. Where the wingman directory holds one project file, the
verb acts in that project. Where it holds several:

- on a terminal, the verb asks which project, one row per project name in
  name order, and Enter alone takes the first row;
- `--project <name>` picks one without asking, for a script;
- with no terminal and no `--project`, the verb refuses, exit 2, naming
  `--project`.

`--project` naming no project file there is refused, exit 2, naming the
name and the wingman directory. Anywhere else, `--project` names the project
the verb found; one naming another is refused, exit 2.

**What a wingman project's policy is.** The shared `fleet.toml` is the
fleet's policy, and the project file may set, for its project alone, the
keys fleet reads per project, never those it reads once for the whole
fleet:

| A project file may set | Only the shared `fleet.toml` sets |
| --- | --- |
| `item_prefix`, `worktrees` | `[agent]` |
| `[guards] <class>.enabled`, `[guards.targets]` | `[seats]` |
| `[store] adapter` | `[telemetry]` |
| `[landing] ci_marker` | `[controller]` |
| `[permissions] tool_commands` | `[core.run] max_crashes` |
| `[core] reviewer`, `[core.run] max_open` | |
| `[core.flight] lanes`, `rerun_wait_seconds`, `[[core.flight.rules]]` | |
| `[packs.<pack>]` | |

Where the project file sets a key, its value is the project's. Where it
leaves one out, the shared file's value is. One controller runs the whole
fleet, so a key it reads once, such as the seats it renders or the agent it
opens, cannot differ by project. A project file carrying such a key, or any
key but `path` and those on the left, is refused for every call that acts in
that project, exit 1, naming the file and the key, rather than read and
quietly ignored. `worktrees` left out is `path` with `-worktrees` on the
end.

A project file that will not read or parse, or that has no `path`, is no
answer either way: fleet cannot tell whether it claims this checkout. Every
verb that makes the lookup exits 3 (shared.exit-codes.R4) on a line naming
that file, acting in, writing and loading nothing.

**A `config.json` that will not read** is the same. Every verb that acts in
a project reads it, for the lookup, a default fleet's verbs among them: a
default fleet's checkout could also be claimed by a wingman project. So a
`config.json` that is there but cannot be read or does not parse makes the
verb exit 3 on a line naming it. The same holds for `fleet start`, which
needs it to find the fleet it starts, and `fleet create`, which needs it to
learn which fleet a wingman project joins.

**Where no fleet is found.** The nearest `fleet.toml` wins. Take a
`fleet.toml` in `~/work/app` and another in `~/work/app/tools`. A verb run
in `~/work/app/tools/sub` acts in the fleet in `tools`.

If neither the lookup nor the walk finds a fleet, there is one fallback.
When the working directory is inside a git checkout and `config.json` names
a default fleet's policy file that exists, the verb acts in that fleet, with
the nearest checkout as the project's root. A checkout has a `.git`
directory, or a `.git` file in a linked worktree, at or above the working
directory. The fallback covers a seat's worktree cut from a project whose
own `fleet.toml` is not committed: no file sits above it, and the machine
still names the fleet. A wingman fleet never falls back: it acts only in the
checkouts its project files name, so a checkout nobody added to it is never
taken for one of its projects.

Otherwise the verb refuses. There is no tree to act in, and a guessed
project would put a seat in the wrong one. The refusal names the working
directory and `fleet create`, and exits as shared.exit-codes.R23 says. It
names more where it can tell why:

- **The project moved.** Where the wingman directory holds a project file
  named after the checkout's directory whose `path` no longer exists, the
  refusal names that file and `fleet create --wingman`, which offers to
  point it here. Where that `path` still exists, as for a second clone,
  nothing is offered: a second clone never takes over the first one's
  project file.
- **The wingman directory moved.** Where `config.json` names a policy file
  that no longer exists, the refusal names `config.json`, that file, and
  `fleet create --dir <dir>`, which points the machine at the wingman
  directory's new place.

`fleet prime` is such a verb. A seat's session runs it in the seat's
worktree, which is a checkout, so a seat is primed wherever it works,
provided its fleet is found: a wingman project naming the seat's primary
checkout (R40); a committed `fleet.toml` (R23); or, with no file, a default
fleet `config.json` names (R24). Otherwise it refuses (R25), and a session
outside every checkout is told there is no fleet there. `fleet prime` exits
as every verb does: 1 where it finds no fleet (shared.exit-codes.R2,
shared.exit-codes.R23), and 3 where `config.json` or a project file is there
but will not read (R38, R53, shared.exit-codes.R4).
`fleet create`, `fleet start`, `fleet stop` and `fleet guard` decide in
their own specs what they do where there is no fleet. `fleet store check`
and `fleet agent check` find their fleet as every verb does, the checkout
fallback (R24) included, and only where no fleet is found at all do they
read the defaults, without refusing.

The verbs differ where no fleet is found:

| Verb | Where no fleet is found |
| --- | --- |
| the item verbs, `fleet doctor`, `fleet prime` | act in `config.json`'s default fleet only inside a git checkout, and refuse otherwise |
| `fleet start`, `fleet stop` | act on the machine from any directory, inside a checkout or not |
| `fleet store check`, `fleet agent check` | read the defaults, and do not refuse |

`fleet stop` resolves no fleet at all. A stop that looked for a project first
would refuse from a bare directory while the service was loaded, and leave
the controller running.

Looking for the fleet creates nothing.

### Known gaps

Each clause below states what fleet is meant to do. Where today's binary does
something else, a `Today:` line follows the clause and names the item that
closes the gap.

- **Wingman fleets** (fleet-1s1x.1, fleet-1s1x.2). Today's binary has no
  wingman fleet. In its place is a standalone project: a
  `.fleet/project.toml` in the project's own checkout, registered in a
  `projects.toml` in the machine directory, which the walk stops at before a
  `fleet.toml`. There is no lookup by `path`, no picking a project in a
  wingman directory, no overlay of a project file on a shared policy, and
  the checkout fallback takes `config.json`'s fleet whatever kind it is.
  `fleet create --wingman`, which writes `config.json` for a wingman fleet,
  does not exist yet (fleet-1s1x.3).
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
- **A `config.json` that does not parse** (fleet-9sr5) gets the no-fleet
  refusal, exit 1, instead of exit 3, in three places: the verbs that find
  their fleet, `fleet start`, and `fleet create`. A default fleet found by
  the walk does not read `config.json` at all, so acts where it will not
  read (fleet-1s1x.1).

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
  SHALL, except as onboarding.start.R26 says, leave in `defaults/` in the
  machine directory the defaults built into the `fleet` binary that ran it.
- **shared.locations.R9**: WHEN a pack or the defaults are installed, fleet
  SHALL name each one installed in `packs.lock` in the machine directory, with
  the source it was installed from, its version, the commit that version
  resolved to, and when it was installed.
- **shared.locations.R10**: WHEN `fleet create` has run, fleet SHALL name in
  `identity.toml` in the machine directory the person who acts on this machine
  when a verb is given no `--by` and no `FLEET_ACTOR`: a human seat's id, and
  a name where one is added.
- **shared.locations.R11**: WHEN `fleet start` has run, or
  `fleet create --wingman` has run where `config.json` was absent or named a
  policy file that is gone, fleet SHALL name in `config.json` in the machine
  directory the policy file of the fleet this machine runs, and the seats
  `fleet start` rendered from it.
  Today: `fleet create --wingman` does not exist; `fleet create --standalone
  --fleet <dir>` writes `config.json` where it is absent, and nothing
  re-points one that names a policy file that is gone (fleet-1s1x.3).
- **shared.locations.R12**: WHEN fleet appends an event to the machine's event
  stream, fleet SHALL append it to `events.jsonl` in the machine directory, one
  event per line, after every line already there.
- ~~**shared.locations.R13**~~ retired 2026-10-13: `projects.toml` is gone
  with standalone projects. A wingman project is named by its project file
  in the wingman directory (shared.locations.R39), so there is nothing to
  register on the machine (fleet-1s1x).
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
  directory change the same file in it, the same fleet's `fleet.toml`, or the
  same project file in a wingman directory, at the same moment, THEN fleet SHALL keep both commands' changes, and neither
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
  the filesystem, and stop at the first that holds a `fleet.toml`.
  Today: the walk also stops at a directory holding a `.fleet/project.toml`,
  and takes it before a `fleet.toml` beside it (fleet-1s1x.1).
- ~~**shared.locations.R22**~~ retired 2026-10-13: it stated the standalone
  project a `.fleet/project.toml` declared. Wingman projects replace
  standalone ones (shared.locations.R39, R40), and nothing in a project's
  checkout declares it (fleet-1s1x).
- **shared.locations.R23**: WHEN the directory R21 stops at is not a wingman
  directory (R39), and R40 finds no project, fleet SHALL act in the default
  fleet rooted there, with that `fleet.toml` as its policy file.
  Today: a `fleet.toml` beside at least one `<name>.fleet.toml` is taken as
  a default fleet's (fleet-1s1x.1).
- **shared.locations.R24**: WHEN R40 finds no project, R21 stops at no
  `fleet.toml` or at a wingman directory's (R39), the working directory is
  inside a git checkout (a `.git` directory or file in it or in a directory
  above it), and `config.json` names a policy file that exists and is not in
  a wingman directory, fleet SHALL act in that fleet, with the nearest
  checkout at or above the working directory as the project's root.
  Today: it falls back to the policy file `config.json` names whatever
  directory holds it (fleet-1s1x.1).
- **shared.locations.R25**: IF a verb that acts in a project finds no fleet by
  R21 to R24 and R40 to R46, THEN fleet SHALL refuse with the exit shared.exit-codes.R23
  states, on a refusal line (shared.exit-codes.R12, shared.exit-codes.R13)
  that names the working directory and `fleet create`. `fleet create`,
  `fleet start`, `fleet stop` and `fleet guard` are not bound by this clause: their own
  specs say what each does where there is no fleet. `fleet store check` and
  `fleet agent check` are not bound by it either: where no fleet is found
  they read the defaults and do not refuse. R54 and R55 say what more the
  refusal line names.
- ~~**shared.locations.R26**~~ retired 2026-10-13: it refused a declared
  project where `config.json` named no fleet. No project declares itself any
  more (fleet-1s1x).
- ~~**shared.locations.R27**~~ retired 2026-10-12: it stated `fleet prime`'s
  fallback from any directory, which fleet-x1qu ruled was not meant.
  `fleet prime` finds its fleet as every verb that acts in a project does,
  by R21 to R26 and R35: where no file is found, that is R24's fallback
  inside a checkout and R25's refusal outside one.
- **shared.locations.R28**: WHEN `fleet start` or `fleet stop` is called,
  fleet SHALL act from any working directory, inside a project or a git
  checkout or not.
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
  or in a wingman directory, fleet SHALL leave no other file
  there that the person did not ask for, a lock file among them.
- ~~**shared.locations.R35**~~ retired 2026-10-13: it refused a declared
  project `projects.toml` did not list. Both are gone with standalone
  projects (fleet-1s1x).
- ~~**shared.locations.R36**~~ retired 2026-10-13: it matched a declaration
  against `projects.toml`. A project file's `path` is matched as R49 says
  (fleet-1s1x).
- ~~**shared.locations.R37**~~ retired 2026-10-13: no older fleet runs
  anywhere to have left a `fleet.toml.lock`, so fleet promises nothing about
  one (Alberto).
- **shared.locations.R38**: IF `config.json` is there but cannot be read or
  does not parse, THEN fleet SHALL exit 3 (shared.exit-codes.R4) on a line
  (shared.exit-codes.R12, shared.exit-codes.R13) that names that file, and
  act in, write or load nothing, for every verb that acts in a project (R21),
  a default fleet's among them, for `fleet start`, and for `fleet create`.
  `fleet stop` and `fleet guard` are not bound by this clause.
  Today: a `config.json` that does not parse gets the no-fleet refusal, exit
  1, in three places (fleet-9sr5): the verbs that find their fleet, where the
  walk finds no file inside a checkout; `fleet start`; and
  `fleet create --standalone`.
  Today: a verb that finds a default fleet's `fleet.toml` by the walk does
  not read `config.json`, and acts where it will not read (fleet-1s1x.1).
- **shared.locations.R39**: fleet SHALL take a directory as a wingman
  directory where it holds a `fleet.toml` and at least one file named
  `<name>.fleet.toml`, each such file as one project's project file, and
  `<name>` as that project's name.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R40**: WHERE `config.json` names the `fleet.toml` of a
  wingman directory (R39), WHEN a verb that acts in a project is called from
  inside a git checkout, fleet SHALL act in the project whose project file's
  `path` names that checkout's primary checkout (the checkout itself, or the
  one a linked worktree was added from), with that `path` as the project's
  root, except as R41 and R42 refuse.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R41**: IF R40 finds a project, and R21 stops at a
  directory holding a `fleet.toml` other than the wingman directory's own,
  THEN fleet SHALL refuse with exit 1 (shared.exit-codes.R2) on a refusal
  line (shared.exit-codes.R12, shared.exit-codes.R13) naming the project
  file and that `fleet.toml`.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R42**: IF two project files in the wingman directory
  name, as R49 matches them, the checkout R40 looks for, THEN fleet SHALL
  refuse with exit 1 (shared.exit-codes.R2) on a refusal line
  (shared.exit-codes.R12, shared.exit-codes.R13) naming both files.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R43**: WHEN R21 stops at a wingman directory (R39), the
  working directory is inside no git checkout, and the wingman directory
  holds one project file, fleet SHALL act in that file's project.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R44**: WHEN R21 stops at a wingman directory (R39), the
  working directory is inside no git checkout, the wingman directory holds
  more than one project file, no `--project` is given, and standard input is
  a terminal, fleet SHALL ask which project, offering one row per project
  name in name order, with the first selected before a key is pressed, and
  act in the one picked.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R45**: IF R21 stops at a wingman directory (R39), the
  working directory is inside no git checkout, the wingman directory holds
  more than one project file, no `--project` is given, and standard input is
  not a terminal, THEN fleet SHALL refuse with exit 2 (shared.exit-codes.R3)
  on a refusal line (shared.exit-codes.R12) naming `--project`.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R46**: WHEN R21 stops at a wingman directory (R39) and
  the working directory is inside no git checkout, WHERE `--project <name>`
  is given and the wingman directory holds `<name>.fleet.toml`, fleet SHALL
  act in that project without asking.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R47**: IF R21 stops at a wingman directory (R39), the
  working directory is inside no git checkout, and `--project <name>` is
  given where the wingman directory holds no `<name>.fleet.toml`, THEN fleet
  SHALL refuse with exit 2 (shared.exit-codes.R3) on a refusal
  line (shared.exit-codes.R12) naming the name and the wingman directory.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R48**: IF `--project <name>` is given where R46 does not
  apply, and the project fleet finds (R23, R24, R40) is named other than
  `<name>`, a wingman project as R39 names it and a default fleet's project
  (R23, R24) by the name of the directory that is its root, THEN fleet SHALL
  refuse with exit 2 (shared.exit-codes.R3) on a
  refusal line (shared.exit-codes.R12) naming both names.
  Today: only the transient verbs take `--project`, and it checks the name
  without picking a project (fleet-1s1x.1).
- **shared.locations.R49**: WHEN fleet decides whether a project file's
  `path` names a checkout (R40, R42), fleet SHALL count the two as one where
  they name the same directory, however each is spelled.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R50**: WHEN a project file's `path` starts with `~`,
  fleet SHALL read `~` as the directory `HOME` names, whatever `FLEET_HOME`
  says.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R51**: WHILE fleet acts in a wingman project (R40, R43,
  R44, R46), fleet SHALL take each of `item_prefix`, `worktrees`,
  `[guards] <class>.enabled`, `[guards.targets]`, `[store] adapter`,
  `[landing] ci_marker`, `[permissions] tool_commands`, `[core] reviewer`,
  `[core.run] max_open`, `[core.flight] lanes`, `[core.flight]
  rerun_wait_seconds`, `[[core.flight.rules]]` and `[packs.<pack>]` from the
  project file where it sets it, and from the wingman directory's
  `fleet.toml` where it does not, and every other key from that
  `fleet.toml`.
  Today: a standalone project takes the project keys from its
  `.fleet/project.toml` alone and the guard switches from the fleet's file
  alone, with no fallback between the two (fleet-1s1x.2).
- **shared.locations.R52**: IF a project file carries a key other than `path`
  and those R51 takes from it, THEN fleet SHALL refuse every verb that acts
  in that project with exit 1 (shared.exit-codes.R2) on a refusal line
  (shared.exit-codes.R12, shared.exit-codes.R13) naming the file and the key.
  Today: fleet knows no project file (fleet-1s1x.2).
- **shared.locations.R53**: IF a project file in the wingman directory R40
  reads cannot be read, does not parse, or has no `path`, THEN fleet SHALL
  exit 3 (shared.exit-codes.R4) on a line (shared.exit-codes.R12,
  shared.exit-codes.R13) naming that file, and act in, write or load
  nothing.
  Today: fleet knows no project file (fleet-1s1x.2).
- **shared.locations.R54**: WHEN R25 refuses inside a git checkout, and the
  wingman directory `config.json` names holds a project file named after the
  checkout's primary checkout's directory whose `path` names no directory
  that exists, fleet SHALL name on the refusal line that project file and
  `fleet create --wingman`.
  Today: fleet knows no wingman directory (fleet-1s1x.1).
- **shared.locations.R55**: WHEN R25 refuses and `config.json` names a policy
  file that does not exist, fleet SHALL name on the refusal line
  `config.json`, that policy file, and `fleet create --dir`.
  Today: the line names the working directory and `fleet create` alone
  (fleet-1s1x.1).
- **shared.locations.R56**: WHILE fleet acts in a wingman project whose
  project file sets no `worktrees`, and the wingman directory's `fleet.toml`
  sets none, fleet SHALL take the project's worktrees directory as its
  `path` with `-worktrees` on the end.
  Today: fleet knows no project file (fleet-1s1x.2).

## Non-functional

- **shared.locations.N1**: WHEN fleet resolves the machine directory (R1 to
  R6, R30 to R32) or looks for the fleet a call acts in (R21, R23, R24, R38
  to R50, R53), fleet SHALL create no file and no directory in doing so.

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
- The guards, `fleet prime`, `fleet start` and the item verbs find their
  fleet the same way (R21, R23, R24, R39 to R50): the item verbs,
  `fleet prime`, `fleet doctor`, `fleet run`, the seat verbs that act in a
  project (`fleet seat add`, `spawn`, `feed`, `retire`, `nudge` and
  `attach`) and the `fleet routine` verbs among them. Each but `fleet prime`
  takes `--project <name>` (R46 to R48); `fleet prime` takes none
  (onboarding.prime.R2). In a wingman project they read the policy
  R51 composes; the guards' spec says what a guard reads where no fleet is
  found. `fleet prime` and the item verbs fall back to a checkout (R24) and,
  where that finds no fleet, refuse (R25). `fleet guard` follows its own
  spec where no fleet is found (R25). Where no fleet is found, `fleet start`
  acts on `config.json`'s fleet from any working directory; `fleet stop`
  resolves no fleet and acts from any working directory (R28).
- `fleet create` writes what R39 to R56 read: a default fleet's
  `fleet.toml`, or a wingman directory's `fleet.toml` and project files, and
  `config.json` (R11). The onboarding.create spec says how, and how it
  repairs a moved project's `path` (R54) and re-points a moved wingman
  directory (R55).
- The policy keys R51 lists belong to their own areas' specs (guards, store,
  items, packs, seats). This spec says only which file a wingman project
  reads each from.
- A seat's session is handed `FLEET_DIR` naming the machine directory (R29);
  the seats area's spec cites that clause.
- shared.exit-codes.R23 states the exit for a verb that finds no fleet (R25).

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
