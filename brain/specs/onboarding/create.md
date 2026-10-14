# `fleet create`

Uses: shared.exit-codes, shared.locations, shared.output

## What it is for

`fleet create` makes a fleet for the project you stand in. It is the first
verb a person runs in a project, after `fleet doctor` and before
`fleet start`. It installs the packs you picked, if any, writes the fleet's
files, and lists you as the fleet's first seat. It makes one of the two kinds
of fleet shared.locations describes:

- **A default fleet**: a plain `fleet create`. It asks which store the
  fleet's items live in and which agent its seats run on, and writes the
  fleet's policy file, `fleet.toml`, in the working directory. Nobody is
  asked to choose the kind.
- **A wingman project**: `fleet create --wingman`, or `--dir <dir>`. It leaves
  no trace in the project. It asks first which directory holds the fleet,
  the wingman directory, and writes there: the project's own file,
  `<name>.fleet.toml`, and, where the wingman directory is new, the fleet's
  shared `fleet.toml`. One wingman fleet serves every project that has a
  file in its directory.

You meet it once per project. A script or an installer meets it with every
answer passed as a flag and no terminal attached.

## How it behaves

### Where it writes

A plain `fleet create` writes the fleet's file into the working directory.
It works inside a git checkout or outside one. First it walks up from the
working directory as every verb that acts in a project does
(shared.locations.R21). Where that walk stops at a directory above the
working directory, you stand inside an existing fleet: a fleet written here
would be a second, nested one, and every verb run below it would act in it
instead. So the call refuses, exit 1, on a line naming the `fleet.toml` the
walk stopped at, and writes nothing. Where the checkout you stand in is
already a wingman project's (shared.locations.R40), it refuses too, exit 1,
naming that project file: a `fleet.toml` here would make every verb in the
checkout refuse (shared.locations.R41).

`fleet create --wingman` writes into the wingman directory, never into the
project. The project is the git checkout you stand in, and, where that is a
seat's linked worktree, the primary checkout behind it. Outside every
checkout it refuses, exit 1: a project file names a checkout. Where the walk
from the working directory stops at a `fleet.toml` other than the wingman
directory's own, it refuses, exit 1, naming that file, for the same reason
as above.

shared.locations.R25 leaves this spec to say what `fleet create` does where
no fleet is found. It is the verb that makes one, so it never refuses for
that.

### The questions

On a terminal, a question with rows is a list you pick from, and Enter alone
takes the first row. Where standard input is not a terminal, nothing is
asked: the store and the agent take `none`, and a wingman call must name its
directory with `--dir`.

| Question | Asked when | Flag | Rows, in order | No terminal, no flag |
| --- | --- | --- | --- | --- |
| Which directory holds the fleet? | `--wingman` and no `--dir` | `--dir` | free text | refused, exit 2 |
| Which store? | no `--store` | `--store none`, `--store bd` | none, bd | none |
| Which agent? | a default fleet, or a wingman project whose wingman directory has no `fleet.toml` yet; and no `--agent` | `--agent none`, `--agent claude-code` | none, claude-code | none |

They are asked in that order. `--dir` alone makes a wingman project:
naming a directory for the fleet means nothing to a default fleet, so no
`--wingman` is needed beside it.

The directory question takes a path, read against the working directory,
with a leading `~` read against `HOME`. Where `config.json` already names
the `fleet.toml` of a wingman directory, that directory is offered, and
Enter alone takes it: a machine runs one fleet, so it is nearly always the
answer.

`none` is the default of the store and the agent alike: the first row, so
Enter alone takes it, and what a call with no terminal and no flag gets.
`fleet create` installs nothing you did not pick. Each pack is a fetch from
the network and a choice of tool, and either pack can be added later. `bd`,
`none` and `claude-code` are the only spellings the two flags take. Another
spelling is refused like any unknown name.

`none` means "install later", for the store and the agent alike:

- You can create the fleet now and add the pack later. The `store:` or
  `agent:` line names the `fleet pack add` command that installs it
  ([The packs](#the-packs)).
- Where the store's pack cannot be fetched, with no network or a checkout
  without the tag, `--store none` lets the fleet be created anyway. That is
  why the refusal for a failed store install names it.
- A fleet whose file names no agent opens the default adapter,
  `claude-code`, once its pack is installed. So `--agent none` is not a
  fleet whose seats run on nothing: it is the claude-code pack, installed
  later. Until it is, a verb that opens the agent refuses and names the
  `fleet pack add` line that installs it (onboarding.start.R10).

**The store is the project's, the agent is the fleet's.** A wingman
project's store goes in its own project file, so it is asked every time. Its
agent goes in the shared `fleet.toml`, which one controller runs, so it is
asked only where this call writes that file. Where the shared file is
already there, nothing is asked and no agent pack is installed: the project
runs on the fleet's agent. An `--agent` that agrees with the shared file is
accepted, and one that does not is refused. A file naming no agent agrees
with both `none` and `claude-code`, since it opens `claude-code` once its
pack is installed.

A question the terminal could not put, such as an interrupted prompt, exits
3, on a line under the one `fleet create:` prefix.

**When refusals come.** A refusal that holds whatever the remaining answers
are comes before the first question. A person should not answer two prompts
and then meet a refusal the call could have carried from the start. A
wingman project's refusals about its wingman directory need the directory,
so they come as soon as it is known: before any question where it comes from
`--dir`, and straight after the directory question where it is answered
there.

### What it writes

| | Default fleet | Wingman project |
| --- | --- | --- |
| Where | the working directory | the wingman directory, made where it is absent |
| The fleet's policy file | `fleet.toml`, written | the shared `fleet.toml`, written where absent |
| The project's file | none: `fleet.toml` is both | `<name>.fleet.toml`, written unless one already names this checkout |
| Packs installed | the store's, unless the store is `none`, and the agent's, unless the agent is `none` | the store's, unless the store is `none`; the agent's only where this call writes the shared `fleet.toml` and the agent is not `none` |
| `config.json` | not written | written where absent, or re-pointed where it names a policy file that is gone |
| `events.jsonl` | nothing appended | a `project.registered` event, when it writes a project file |
| Your seat | a human table in the `fleet.toml` written | a human table in the shared `fleet.toml` |

Both leave `identity.toml` (minted where absent), `defaults/` and its line
in `packs.lock`. Every machine-directory file is the one shared.locations
names. `<name>` is the name of the project's primary checkout's directory,
or the name `--name` gives.

**The `fleet.toml` written** is the smallest file that runs, the same for a
default fleet and a wingman directory's shared file. A comment at its top
names the command that wrote it. It carries:

- `[guards]` with `shell-trap.enabled = true` and `record.enabled = true`;
- `[telemetry]` with `enabled = false`;
- `[agent]` with `adapter = "claude-code"`, absent where the agent is
  `none`;
- `[store]` with `adapter = "bd"`, for a default fleet only, absent where the
  store is `none`. A wingman project's store goes in its project file;
- `[seats]` with your table in it, under a comment showing what an agent
  seat's table looks like. The agent area's spec says what that comment's
  model line carries.

Every key the controller defaults is left out, because a written default is
a value nobody chose that a reader has to check anyway. The guards are
written because they are opt-out, and an opt-out has to be visible to be
taken. Telemetry is written because it is off, and the file says so.
`[agent]` is written even though a missing key opens the same adapter,
because the file records the choice. Where the agent is `none`, nobody chose
one, and the missing key opens `claude-code` once its pack is installed.

**The project file, `<name>.fleet.toml`,** opens with a comment naming the
command that wrote it, then:

- `path`: the project's primary checkout. Where it sits under `HOME`, it is
  written `~/…`, so a wingman directory synced to another machine still
  resolves there (shared.locations.R50);
- `item_prefix`: the prefix the project's store answers for its ids. Where
  the store cannot be opened or names no prefix, the line is left commented
  out for the person to fill in. A guessed prefix is one the store's own ids
  would disagree with. The store is given 60 s to answer, and one that has
  not answered by then names no prefix;
- `[store]` with `adapter = "bd"`, absent where the store is `none`;
- `[landing]` with its `ci_marker` line commented out, because a guessed
  marker would skip a pipeline nobody chose to skip.

No `worktrees` is written: left out, it is `path` with `-worktrees` on the
end (shared.locations.R56), outside the project as every wingman file is.
The other keys a project file may set are shared.locations.R51's, and none
is written: each is the shared file's until the person sets it here.

### Running it again in a wingman project

Where a project file in the wingman directory already names this checkout
(as shared.locations.R49 matches), the call writes nothing over it. It reads
it first: a file this verb did not write is the only one whose keys nobody
has checked. A file that carries a key shared.locations.R51 does not take
from a project file is refused, exit 1, naming the file and the key, and
nothing is written. A project file there that cannot be read, does not
parse, or has no `path` cannot say which checkout it names: the call exits
3 naming it, as every verb that makes the lookup does
(shared.locations.R53), and writes nothing. A good one gets a
`project:` line saying it already names this checkout. The call goes on to
list you, and exits 0. So `fleet create --wingman` run twice changes nothing
the second time.

`--name` naming a file other than the one that already names this checkout
is refused, exit 1, naming both: one path has one owner. So are two project
files already naming it, exit 1, naming both.

### Two projects with one name

Where no project file names this checkout yet, and `<name>.fleet.toml` is
already there and names another checkout:

| That file's `path` | On a terminal | No terminal |
| --- | --- | --- |
| exists | refused, exit 1, naming the file, its path and `--name` | the same |
| exists no more | asks whether to point that file here, with no as the default; yes rewrites `path` and nothing else, on a `project:` line; no refuses as the row above | refused, exit 1, naming the file to edit |

A `path` that still exists is another project, such as a second repository
called `app`, or a second clone of this one: `--name` gives this one its own
file, and a second clone never takes over the first one's link. A `path`
that exists no more is most likely this project, moved. Only then is the
repair offered, and only on asking, because a wrong yes would hand one
project's file to another. Every other verb run in a moved project refuses
and names this command (shared.locations.R54).

### Which fleet the machine runs

A machine runs one fleet: `config.json` names its policy file
(shared.locations.R11).

| `config.json` | The wingman directory's `fleet.toml` | Outcome |
| --- | --- | --- |
| absent | any | `config.json` written naming it, with no seats, on a `machine:` line |
| names it | there | nothing changed |
| names another policy file that exists | any | refused, exit 1, naming both, and nothing written |
| names a policy file that is gone | any | re-pointed at it, the seat rows kept, on a `machine:` line naming the old file and the new |
| is there but cannot be read or does not parse | any | exit 3, as shared.locations.R38 says, and nothing written |

`config.json` is otherwise written by a fleet's first `fleet start`, and
requiring that would put a start between a fleet's creates for no reason a
person could see. The `config.json` `fleet create` writes names the fleet's
file and lists no seats, as a first start does. The next `fleet start`
renders the seats.

A policy file that is gone is most often a wingman directory that moved. Its
seat rows name worktrees, which did not move, so they are kept. The
directory given is the person's own answer, so nothing more is asked.

A default `fleet create` reads `config.json` only to learn whether its
checkout is a wingman project's, and never writes it.

### The packs

Packs are installed before any fleet file is written, so a fetch that fails
is refused with no fleet file written.

The store's pack is installed first, then the agent's, each from
`<source>//adapters/store/<name>` or `<source>//adapters/agent/<name>`, at the
tag this binary pins:

- `<source>` is the fleet-packs source this binary pins
  (`https://github.com/bembot90/fleet-packs` today). It is fetched with git,
  so `fleet create` needs the network;
- or, with `--packs-from <dir>`, it is the checkout at `<dir>`, read against
  the working directory and recorded by its full path. The checkout's git
  history is read at that tag, not its working tree.

A pack the installed one imports, such as `ts` for both the bd and the
claude-code pack, comes with it and gets a line of its own. The packs area's
spec says how imports are installed.

A pack whose directory is already under `packs/` is left as it stands,
whatever it was installed from. It is not fetched again, and `packs.lock`
is not touched. Every fleet on a machine shares one packs directory, so a
second fleet reads the pack the first installed. Replacing it is
`fleet pack remove`, then `fleet pack add`.

A `none` answer installs no pack for its question. The `store:` line then
names the `fleet pack add` command that installs the bd pack later, and the
`agent:` line the one that installs the claude-code pack, each from the same
source at the same tag.

`--packs-from` names where the packs come from, so a call that installs no
pack refuses it, exit 2, and writes and installs nothing: the store `none`,
and the agent `none` or not asked. With no terminal, that includes a call
that passes `--packs-from` and neither `--store` nor `--agent`, because both
then take `none`. The refusal comes as soon as the answers it turns on are
known: before any question where they come from flags or from no terminal,
straight after the directory question where only that decides whether the
agent is asked, and straight after the last question that answers one of
them otherwise.

Each pack's fetch is given 120 s. One still running then is stopped, nothing
of that pack is kept, and the call exits 3 (shared.exit-codes.R4): fleet
cannot tell whether the pack would have come. git is told never to prompt,
but a fetch that stalls on the network would otherwise wait for ever. The
packs go in before any fleet file, so none is written.

A failed install is refused, exit 1, naming the reason, and no fleet file is
written. A failed store install names `fleet create --store none` as the way
to create the fleet without it. A failed agent install names the
`fleet pack add` command that installs the pack. What was installed before
the failure stays installed: the defaults, and the store's pack when the
agent's install is the one that failed.

### The defaults

Every call that runs to its end installs the defaults built into this binary
and pins them. The `defaults:` line says which of three happened: installed
fresh, refreshed because the copy there held another set, or already there.
A replacement a person cannot see is the same to them as no replacement at
all. A `defaults/` that cannot be written, has no line in `packs.lock`, or
was edited since its line was written is refused, and not written over. The
packs area's spec owns the defaults. This spec says only what `fleet create`
shows of them.

The defaults go in before any file is written, whether or not the call
installs a pack, so a defaults refusal leaves no fleet file, no project
file, no `config.json`, no `identity.toml` and no `project.registered` event
behind. A machine directory that cannot be written is met there, on the
defaults' own refusal line, before anything else is tried.

### Your seat

The person who ran `fleet create` becomes the fleet's first seat. The seat
is a human one, keyed by this machine's identity, with `kind = "human"`. It
has no name unless `identity.toml` carries one, and nobody is asked for one.
Where `identity.toml` is absent it is minted, and an `identity:` line says
so, whichever step of the call mints it.

| Case | Outcome |
| --- | --- |
| The table is written | `seat:` line naming the table and the fleet's file |
| The fleet's file already lists this identity | file left as it was, and the `seat:` line says `already listed` |
| Wingman, a shared `fleet.toml` already there cannot take the table | project file still written; the `seat:` line says `not listed`, why, and names `fleet seat add --human`; exit 0 |
| A `fleet.toml` this call wrote cannot take the table | exit 3, naming `fleet seat add --human`; the files written stay written |
| `identity.toml` cannot be read or minted | exit 3, naming `fleet seat add --human`; the files written stay written |

A shared `fleet.toml` that was already there and cannot take the table gets
a line rather than a refusal. The project is added, and the shared file is
the fleet's to put right, not this project's.

### Your project directory, and edits that are never lost

A default `fleet create` leaves in the project only the `fleet.toml` you
asked for. A wingman `fleet create` leaves nothing in the project at all,
and nothing in the wingman directory but the files it names above
(shared.locations.R34). Two fleet commands writing the same file at once
never lose an edit. Two `fleet create --wingman` calls listing two people in
one shared `fleet.toml` at once leave both tables. Two calls minting this
machine's identity at once leave one identity, and both list it
(shared.locations.R17).

### The project's store

`fleet create` never initialises, writes into or rewrites the project's
store. It makes no beads board. The bd pack's README says how to make one.

### What it prints

Every line goes to standard error, and nothing goes to standard output. Each
line starts with a word a person scans for:

| Line starts with | When |
| --- | --- |
| `store:`, `agent:` | for each pack installed, imported or found already there; and, where the store or an asked agent is `none`, naming the `fleet pack add` command that installs it |
| `machine:` | wingman, when `config.json` was written or re-pointed |
| `project:` | wingman: the project file written, already naming this checkout, or re-pointed here |
| `created` | on success, naming the kind (`default fleet` or `wingman project`) and the file |
| `defaults:` | on success |
| `seat:`, `identity:` | as [Your seat](#your-seat) says |
| `next:` | last, naming `fleet start` |

The `config.json` line starts `machine:`, not `fleet:`, because `fleet:`
starts the line of an error fleet did not turn into a refusal
(shared.exit-codes.R17), and a script keying on it must be able to tell the
two apart.

### Refusals

Each refusal writes a line under `fleet create:` (shared.exit-codes.R12),
naming what it is about (shared.exit-codes.R13). Where two hold at the same
moment, a `config.json` or a project file that cannot be read or does not
parse is the one made, and otherwise the usage error (exit 2)
(onboarding.create.R8).

| The case | Exit | When |
| --- | --- | --- |
| the variable that decides the machine directory names a relative path, or nothing names it (shared.locations.R30, shared.locations.R31) | 1 | before anything (shared.locations.R32) |
| a flag `fleet create` does not take (`--embedded`, `--standalone` and `--fleet` among them), or `--name` without `--wingman` or `--dir` | 2 | before anything |
| `--agent` or `--store` names an answer `fleet create` does not take; `--packs-from` names no directory; `--dir` names a file | 2 | before any question |
| `config.json` is there but cannot be read or does not parse (shared.locations.R38) | 3 | before any question |
| default: the working directory holds a `fleet.toml` | 1 | before any question |
| default: the walk up from here stops at a directory above it (shared.locations.R21) | 1 | before any question |
| default: the checkout here is a wingman project's (shared.locations.R40) | 1 | before any question |
| wingman: no terminal and no `--dir` | 2 | instead of the first question |
| wingman: the working directory is in no git checkout | 1 | before any question |
| wingman: the walk stops at a `fleet.toml` other than the wingman directory's own | 1 | once the directory is known |
| wingman: the directory holds a `fleet.toml` and no project file: a default fleet's root | 1 | once the directory is known |
| wingman: `config.json` names another policy file that exists | 1 | once the directory is known |
| wingman: a project file naming this checkout carries a key it may not, `--name` names another, or two name it | 1 | once the directory is known |
| wingman: a project file there cannot be read, does not parse, or has no `path` (shared.locations.R53) | 3 | once the directory is known |
| wingman: no project file names this checkout, `<name>.fleet.toml` names another, and the person does not repoint it | 1 | once the directory is known, or after the question |
| wingman: an `--agent` that is not the shared `fleet.toml`'s | 2 | once the directory is known |
| `--packs-from` where the call installs no pack | 2 | once the answers it turns on are known (see [The packs](#the-packs)) |
| a pack's install fails | 1 | see [The packs](#the-packs) |
| a pack's fetch runs past 120 s | 3 | see [The packs](#the-packs) |
| `defaults/` cannot be written, or is unrecorded or edited | 1 | see [The defaults](#the-defaults) |
| a file it writes after the installs cannot be written | 3 | what was written before stays |

A wingman directory that holds a `fleet.toml` and no project file cannot be
told from a default fleet's root, so it is refused rather than turned into a
wingman directory. Turning a default fleet into a wingman fleet is not
something `fleet create` does.

### Known gaps

Each clause states what fleet is meant to do. Where today's binary falls
short, a `Today:` line under the clause names the item that closes the gap.

- **Wingman projects, and a create with no mode** (fleet-1s1x.3). Today
  `fleet create` asks whether the fleet is embedded or standalone, and
  takes `--embedded`, `--standalone` and `--fleet`. A standalone project
  writes `.fleet/project.toml` into the project and registers it in
  `projects.toml`; there is no `--wingman`, `--dir` or `--name`, no project
  file in a wingman directory, no repair of a moved project and no re-point
  of a moved fleet. A default create does not check whether its checkout is
  a wingman project's.
- **The line for a question the terminal could not put** carries one
  `fleet create:` prefix. Today it reads `fleet create: fleet: …`
  (fleet-130b).
- **No lost edits** (shared.locations.R17). Today the first write of
  `config.json` takes no lock, so two calls writing it at once can lose one
  (fleet-rbwc).
- **Inside an existing fleet, the call refuses.** Today it looks in the
  working directory alone, and writes a second, nested fleet there
  (fleet-6zpk).
- **The defaults go in first, pack or not** (fleet-eofi9). Today, where the
  call installs no pack (standalone with store `none`), they go in last, and
  a defaults refusal leaves the declaration, the registration,
  `project.registered` and the listing behind.
- **Every mint of the identity is said** (fleet-tlw). Today a standalone call
  that mints it as it registers a new root prints no `identity:` line.
- **The `config.json` line starts `machine:`** (fleet-qb4y). Today it starts
  `fleet:`.
- **A pack's fetch is bounded at 120 s** (fleet-pwwl). Today it has no bound.
- **`none` answers both questions, and is both questions' default**
  (fleet-284h). Today:
  - the store question lists `bd` first, and the agent question lists
    `claude-code` alone, so Enter alone and a call with no terminal take
    `bd` and `claude-code`;
  - `--agent none` is refused as an unknown agent, and the embedded file
    always carries `[agent]`;
  - `--packs-from` with nothing to install is refused only for a standalone
    call given `--store none` as a flag.

## Behaviour

### Clauses: where it acts

- **onboarding.create.R1**: WHEN `fleet create` is called with neither
  `--wingman` nor `--dir`, fleet SHALL write the fleet's file into the
  working directory, whether or not the working directory is inside a git
  checkout.
- **onboarding.create.R49**: WHERE neither `--wingman` nor `--dir` is given,
  IF the walk shared.locations.R21 makes from the working directory stops at
  a directory above it, THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming the `fleet.toml` the walk stopped
  at, and write nothing.
  Today: it looks in the working directory alone, and writes a second,
  nested fleet there (fleet-6zpk).
- **onboarding.create.R55**: WHEN `fleet create` is called with `--dir`,
  fleet SHALL make a wingman project, as with `--wingman`, and ask no
  directory question.
  Today: there is no `--dir` (fleet-1s1x.3).
- **onboarding.create.R56**: WHERE `--wingman` or `--dir` is given, IF the
  working directory is inside no git checkout, THEN fleet SHALL refuse with
  exit 1 (shared.exit-codes.R2) on a line naming the working directory, and
  write nothing.
  Today: there is no `--wingman` (fleet-1s1x.3).
- **onboarding.create.R57**: WHERE `--wingman` or `--dir` is given, WHEN
  fleet names the project, fleet SHALL take as the project's checkout the
  git checkout the working directory is in or, where that is a linked
  worktree, the primary checkout it was added from, and as its name that
  checkout's directory name, or the name `--name` gives.
  Today: there is no `--wingman` (fleet-1s1x.3).
- **onboarding.create.R58**: WHERE neither `--wingman` nor `--dir` is given,
  IF the checkout the working directory is in is a wingman project's
  (shared.locations.R40), THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming that project file, and write
  nothing.
  Today: the call does not look for a wingman project (fleet-1s1x.3).
- **onboarding.create.R59**: WHERE `--wingman` or `--dir` is given, IF the
  walk shared.locations.R21 makes from the working directory stops at a
  `fleet.toml` other than the wingman directory's own, THEN fleet SHALL
  refuse with exit 1 (shared.exit-codes.R2) on a line naming that
  `fleet.toml`, and write nothing.
  Today: there is no `--wingman` (fleet-1s1x.3).

### Clauses: the questions

- ~~**onboarding.create.R2**~~ retired 2026-10-13: it asked whether the
  fleet is embedded or standalone. A plain `fleet create` makes a default
  fleet, and `--wingman` or `--dir` a wingman project; nobody is asked the
  kind (fleet-1s1x).
- **onboarding.create.R3**: WHILE standard input is a terminal, fleet SHALL
  ask which store where no `--store` is given (offering `none` then `bd`),
  then, where onboarding.create.R60 says the agent is asked and no `--agent`
  is given, which agent (offering `none` then `claude-code`), each with its
  first row, `none`, selected before a key is pressed.
  Today: the store question offers `bd` then `none`, and the agent question
  offers `claude-code` alone, so Enter alone takes `bd` and `claude-code`
  (fleet-284h).
- **onboarding.create.R4**: WHERE `--wingman` or `--dir` is given, and the
  wingman directory already holds a `fleet.toml`, fleet SHALL ask no agent
  question and install no agent's pack, and SHALL accept an `--agent` that
  agrees with that file: the agent its `[agent] adapter` names, or, where it
  names none, `none` or `claude-code`.
  Today: a standalone call accepts `claude-code` alone where the fleet's
  file names no agent, and refuses `--agent none` (fleet-1s1x.3).
- ~~**onboarding.create.R5**~~ retired 2026-10-13: it refused a call with no
  mode flag and no terminal. There is no mode to choose (fleet-1s1x).
- **onboarding.create.R6**: WHILE standard input is not a terminal, fleet
  SHALL take `none` where no `--store` is given and, where
  onboarding.create.R60 says the agent is asked, `none` where no `--agent`
  is given, without asking.
  Today: it takes `bd` and `claude-code`, and installs their packs
  (fleet-284h).
- **onboarding.create.R7**: IF a question cannot be put on the terminal,
  THEN fleet SHALL exit 3 (shared.exit-codes.R4) on a line that carries the
  one `fleet create:` prefix (shared.exit-codes.R12).
  Today: the line reads `fleet create: fleet: …`, a second prefix
  (fleet-130b).
- **onboarding.create.R8**: fleet SHALL make every refusal that holds
  whatever the questions still to come are answered before asking any of
  them, and before installing anything: shared.locations.R38 and
  onboarding.create.R10 to R12, R49, R56, R58, R62 and R63 before any
  question; R14, R50, R59, R65 and R67
  to R71 as soon as the wingman directory is known (before any question
  where it comes from `--dir`, and straight after the directory question
  where it is answered there); and R13 as soon as the store and, where
  onboarding.create.R60 says it is asked, the agent are known. Where two
  refusals hold at the same moment, fleet SHALL make shared.locations.R38's
  or shared.locations.R53's (onboarding.create.R67) before any other, and
  otherwise the one that exits 2 (shared.exit-codes.R3).
  Today: there is no wingman project, and so none of its refusals
  (fleet-1s1x.3).
- **onboarding.create.R60**: fleet SHALL ask the agent question, or take
  `--agent` as the agent to install, only where the call makes a default
  fleet, or makes a wingman project whose wingman directory holds no
  `fleet.toml` yet.
  Today: a standalone call is never asked, and an embedded one always is
  (fleet-1s1x.3).
- **onboarding.create.R61**: WHERE `--wingman` is given and no `--dir`,
  WHILE standard input is a terminal, fleet SHALL ask first which directory
  holds the fleet, taking a path read against the working directory, with a
  leading `~` read against `HOME`. Where `config.json` names the
  `fleet.toml` of a wingman directory (shared.locations.R39), fleet SHALL
  offer that directory, taken by Enter alone.
  Today: there is no `--wingman` (fleet-1s1x.3).
- **onboarding.create.R62**: WHERE `--wingman` is given and no `--dir`, IF
  standard input is not a terminal, THEN fleet SHALL refuse with exit 2
  (shared.exit-codes.R3, shared.exit-codes.R14) on a line that carries one
  `fleet create:` prefix and names `--dir`, and write nothing.
  Today: there is no `--wingman` (fleet-1s1x.3).
- ~~**onboarding.create.R51**~~ retired 2026-10-13: it made `--fleet` imply
  standalone. `--fleet` is gone; `--dir` implies a wingman project
  (onboarding.create.R55) (fleet-1s1x).

### Clauses: refused answers and directories

- ~~**onboarding.create.R9**~~ retired 2026-10-13: it refused `--embedded`
  with `--standalone` or `--fleet`. Those flags are gone, and a usage error
  like any unknown flag (shared.exit-codes.R15) (fleet-1s1x).
- **onboarding.create.R10**: IF `--agent` names anything other than
  `claude-code` or `none`, or `--store` names anything other than `bd` or
  `none`, each spelled exactly so, THEN fleet SHALL refuse with exit 2
  (shared.exit-codes.R3) on a line naming the value given and the values
  that flag takes, and write nothing.
  Today: `--agent none` is refused, and the line names `claude-code` alone
  (fleet-284h).
- **onboarding.create.R11**: IF `--packs-from` names a path that is not a
  directory, THEN fleet SHALL refuse with exit 2 (shared.exit-codes.R3) on a
  line naming the path, and write nothing.
- **onboarding.create.R12**: WHERE neither `--wingman` nor `--dir` is given,
  IF the working directory holds a `fleet.toml` (shared.locations.R23), THEN
  fleet SHALL refuse with exit 1 (shared.exit-codes.R2) on a line naming it,
  and write nothing.
- **onboarding.create.R13**: IF `--packs-from` is given and the call installs
  no pack, the store being `none` and the agent being `none` or not asked
  (onboarding.create.R60), by a flag, a question or onboarding.create.R6,
  THEN fleet SHALL refuse with exit 2 (shared.exit-codes.R3) on a line
  naming `--packs-from`, and write nothing.
  Today: only a standalone call given `--store none` as a flag is refused; a
  standalone call that takes `none` at the question is not, and an embedded
  call cannot answer `none` for its agent (fleet-284h).
- **onboarding.create.R14**: WHERE `--wingman` or `--dir` is given, IF the
  wingman directory already holds a `fleet.toml` and `--agent` names an
  agent onboarding.create.R4 does not accept, THEN fleet SHALL refuse with
  exit 2 (shared.exit-codes.R3) on a line naming that `fleet.toml` and its
  agent, and write nothing.
  Today: there is no wingman directory; a standalone call refuses an agent
  other than its fleet's, `none` among them (fleet-1s1x.3).
- ~~**onboarding.create.R15**~~ retired 2026-10-13: it refused a standalone
  call with no fleet to declare to. A wingman call names its directory, and
  writes the fleet's `fleet.toml` there where it is absent (fleet-1s1x).
- ~~**onboarding.create.R16**~~ retired 2026-10-13: it refused where
  `config.json` names a policy file that is gone. A wingman call now
  re-points it (onboarding.create.R66) (fleet-1s1x).
- ~~**onboarding.create.R17**~~ retired 2026-10-13: it refused an embedded
  fleet beside a `fleet.toml` and a `.fleet/project.toml`. No project
  declares itself any more (fleet-1s1x).
- ~~**onboarding.create.R52**~~ retired 2026-10-13: it refused an embedded
  fleet beside a lone `.fleet/project.toml`, for the same reason
  (fleet-1s1x).
- **onboarding.create.R63**: IF `--dir` names a path that is a file, or
  `--name` is given with neither `--wingman` nor `--dir`, THEN fleet SHALL
  refuse with exit 2 (shared.exit-codes.R3) on a line naming the flag, and
  write nothing.
  Today: there is no `--dir` or `--name` (fleet-1s1x.3).
- **onboarding.create.R65**: WHERE `--wingman` or `--dir` is given, IF the
  directory given holds a `fleet.toml` and no `<name>.fleet.toml` of any
  name, THEN fleet SHALL refuse with exit 1 (shared.exit-codes.R2) on a line
  naming that `fleet.toml` as a default fleet's, and write nothing.
  Today: there is no wingman directory (fleet-1s1x.3).

### Clauses: which fleet the machine runs

- ~~**onboarding.create.R18**~~ retired 2026-10-13: it declared a standalone
  project to the fleet `config.json` names. A wingman project joins the
  fleet of the directory it names (fleet-1s1x).
- **onboarding.create.R19**: WHERE `--wingman` or `--dir` is given, WHEN
  `config.json` is absent, fleet SHALL write `config.json`
  (shared.locations.R11) naming the wingman directory's `fleet.toml` by its
  full path, with no seats, and print a line starting `machine:` that names
  that file and `config.json`.
  Today: a standalone call with `--fleet` writes it, on a line starting
  `fleet:` (fleet-qb4y, fleet-1s1x.3).
- **onboarding.create.R50**: WHERE `--wingman` or `--dir` is given, IF
  `config.json` (shared.locations.R11) names a policy file that exists other
  than the wingman directory's `fleet.toml`, THEN fleet SHALL refuse with
  exit 1 (shared.exit-codes.R2) on a line naming both, and write nothing.
  Today: there is no wingman directory; a standalone call ignores a
  `--fleet` naming another fleet and declares the project to the fleet
  `config.json` names, exit 0 (fleet-1s1x.3).
- **onboarding.create.R66**: WHERE `--wingman` or `--dir` is given, WHEN
  `config.json` names a policy file that does not exist, fleet SHALL point
  `config.json` at the wingman directory's `fleet.toml`, keep every seat row
  it holds, and print a line starting `machine:` naming the old policy file
  and the new.
  Today: a standalone call refuses there, exit 1 (fleet-1s1x.3).
- ~~**onboarding.create.R48**~~ retired 2026-10-13: it held a standalone
  call to shared.locations.R38, which now binds every `fleet create`
  itself (fleet-1s1x).

### Clauses: the project file

- **onboarding.create.R67**: WHERE `--wingman` or `--dir` is given, IF a
  project file in the wingman directory already names the project's
  checkout (as shared.locations.R49 matches) and carries a key
  shared.locations.R51 does not take from a project file, THEN fleet SHALL
  refuse with exit 1 (shared.exit-codes.R2) on a line naming the file and
  the key, and write nothing. A project file there that cannot be read,
  does not parse, or has no `path` SHALL be refused as shared.locations.R53
  says, exit 3, and nothing written.
  Today: there is no project file (fleet-1s1x.3).
- **onboarding.create.R68**: WHERE `--wingman` or `--dir` is given, IF two
  project files in the wingman directory name the project's checkout, or
  one does and `--name` names another, THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming both, and write nothing.
  Today: there is no project file (fleet-1s1x.3).
- **onboarding.create.R69**: WHERE `--wingman` or `--dir` is given, IF no
  project file in the wingman directory names the project's checkout,
  `<name>.fleet.toml` is there, and its `path` names
  another directory that exists, THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming the file, its `path` and
  `--name`, and write nothing.
  Today: there is no project file (fleet-1s1x.3).
- **onboarding.create.R70**: WHERE `--wingman` or `--dir` is given, WHEN no
  project file in the wingman directory names the project's checkout,
  `<name>.fleet.toml` is there, and its `path` names no directory that
  exists, fleet SHALL, on a terminal, ask whether to point
  that file at the project's checkout, with no selected before a key is
  pressed. A yes SHALL rewrite its `path` and nothing else of the file, on a
  line starting `project:`; a no SHALL refuse as onboarding.create.R69 says.
  Today: there is no project file (fleet-1s1x.3).
- **onboarding.create.R71**: WHERE `--wingman` or `--dir` is given, IF no
  project file in the wingman directory names the project's checkout,
  `<name>.fleet.toml` names a `path` that is no directory that exists, and
  standard input is not a terminal, THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming the file to edit, and write
  nothing.
  Today: there is no project file (fleet-1s1x.3).
- **onboarding.create.R72**: WHERE `--wingman` or `--dir` is given, WHEN a
  project file in the wingman directory already names the project's
  checkout and onboarding.create.R67 and R68 refuse nothing, fleet SHALL
  write nothing over it, and print a line starting `project:` naming the
  file and that it already names this checkout.
  Today: there is no project file (fleet-1s1x.3).

### Clauses: the packs

- **onboarding.create.R20**: WHEN `fleet create` installs the store's pack
  (the store is `bd`) or the agent's (onboarding.create.R60 says the agent
  is asked, and it is `claude-code`), fleet SHALL install
  `<source>//adapters/store/bd` or `<source>//adapters/agent/claude-code` at
  the tag this binary pins, into `packs/bd/` or `packs/claude-code/`
  (shared.locations.R7), pinned in `packs.lock` (shared.locations.R9), unless
  onboarding.create.R22 applies. Where it installs both, it SHALL install
  the store's pack before the agent's.
- **onboarding.create.R21**: WHEN `fleet create` installs a pack, fleet SHALL
  take `<source>` as the fleet-packs source this binary pins, or, where
  `--packs-from <dir>` is given, as the full path of `<dir>` read against the
  working directory. It SHALL read that source's commit at the tag, whatever
  a checkout's working tree holds.
- **onboarding.create.R22**: WHILE a directory named after the pack is already
  under `packs/` (shared.locations.R7), fleet SHALL leave it and `packs.lock`
  as they stand, fetch nothing, and print a line saying the pack was already
  installed, naming its tag and source where `packs.lock` has its line.
- **onboarding.create.R23**: WHEN `fleet create` installs a pack, or a pack it
  imports, fleet SHALL print a line starting `store:` or `agent:` naming the
  pack, its tag, the commit it resolved to, and its directory.
- **onboarding.create.R24**: WHEN the store is `none`, fleet SHALL install no
  store pack, and print a line starting `store:` naming the `fleet pack add`
  command, with `<source>//adapters/store/bd`, `<source>` taken as
  onboarding.create.R21 takes it, and `--version` at the tag this binary
  pins, that installs one later.
- **onboarding.create.R53**: WHERE onboarding.create.R60 says the agent is
  asked, WHEN the agent is `none`, fleet SHALL install no agent pack, and
  print a line starting `agent:` naming the `fleet pack add` command, with
  `<source>//adapters/agent/claude-code`, `<source>` taken as
  onboarding.create.R21 takes it, and `--version` at the tag this binary
  pins, that installs one later.
  Today: the agent cannot be `none`: `--agent none` is refused, and the
  question offers no `none` row (fleet-284h).
- **onboarding.create.R25**: IF a pack cannot be installed, THEN fleet SHALL
  refuse with exit 1 (shared.exit-codes.R2) on a line naming the install's
  reason and, for the store's pack, `fleet create --store none`, or, for the
  agent's pack, the `fleet pack add` command that installs it. It SHALL write
  no `fleet.toml`, project file, `config.json` (shared.locations.R11) or
  `identity.toml` (shared.locations.R10). A fetch past its bound is
  onboarding.create.N1's.
- **onboarding.create.R26**: IF a pack's install fails (onboarding.create.R25),
  THEN fleet SHALL leave installed and pinned the defaults, and every pack
  the call installed before the failure.

### Clauses: the defaults

- **onboarding.create.R27**: WHEN `fleet create` runs to its end, fleet SHALL
  install the defaults this binary carries in `defaults/`
  (shared.locations.R8) and pin them in `packs.lock` (shared.locations.R9).
- **onboarding.create.R28**: WHEN `fleet create` has installed the defaults,
  fleet SHALL print a line starting `defaults:` saying whether it installed
  them, refreshed a copy that held another set, or found them already there,
  with the version.
- **onboarding.create.R29**: IF `defaults/` (shared.locations.R8) cannot be
  written, is there with no line in `packs.lock`, or holds files changed
  since its line was written, THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming `defaults/`, and not write over
  it. The packs area's spec owns the defaults.
- **onboarding.create.R30**: IF onboarding.create.R29 refuses, a machine
  directory that cannot be written among its cases, THEN fleet SHALL have
  written none of the files onboarding.create.R25 names and appended no
  `project.registered` event, whether or not the call installs a pack.
  Today: where the call installs no pack, the defaults go in last, after the
  declaration, the registration, `project.registered` and the listing, which
  a defaults refusal leaves behind (fleet-eofi9).

### Clauses: the files written

- **onboarding.create.R31**: WHEN `fleet create` writes a `fleet.toml`, the
  working directory's for a default fleet or the wingman directory's where
  it is absent, fleet SHALL open it with a comment naming the command that
  wrote it, and carry: `[guards]` with `shell-trap.enabled = true` and
  `record.enabled = true`; `[telemetry]` with `enabled = false`; `[agent]`
  with `adapter` naming the agent where the agent is `claude-code`; for a
  default fleet only, `[store]` with `adapter = "bd"` where the store is
  `bd`; and `[seats]`, under a comment showing an agent seat's table. It
  SHALL carry no other key but the seat's (onboarding.create.R41).
  Today: the file always carries `[agent]`, because the agent cannot be
  `none` (fleet-284h).
  Today: there is no wingman directory to write it in (fleet-1s1x.3).
- ~~**onboarding.create.R32**~~ retired 2026-10-13: it wrote a standalone
  project's `.fleet/project.toml`. A wingman project's file is
  onboarding.create.R73's (fleet-1s1x).
- **onboarding.create.R64**: WHERE `--wingman` or `--dir` is given, WHEN the
  wingman directory does not exist, fleet SHALL make it.
  Today: there is no wingman directory (fleet-1s1x.3).
- **onboarding.create.R73**: WHERE `--wingman` or `--dir` is given, WHEN no
  project file in the wingman directory names the project's checkout and
  onboarding.create.R69 to R71 refuse nothing, fleet SHALL, except as
  onboarding.create.R70 says, write `<name>.fleet.toml`
  (onboarding.create.R57) in the wingman directory. The
  file SHALL open with a comment naming the command that wrote it, and
  carry: `path`, the checkout's full path, written with `~` for `HOME`
  where it sits under `HOME`; `item_prefix`, the prefix the project's store
  answers, or a commented-out line where the store cannot be opened or
  answers none; `[store]` with `adapter = "bd"` where the store is `bd`; and
  `[landing]` with its `ci_marker` line commented out. It SHALL carry no
  other key, and print a line starting `project:` naming the file.
  Today: there is no project file (fleet-1s1x.3).
- **onboarding.create.R33**: IF a file `fleet create` writes after its
  installs (a `fleet.toml`, a project file, `config.json` or its
  `project.registered` event) cannot be written, THEN fleet SHALL exit 3
  (shared.exit-codes.R4) on a line naming what could not be written.
- ~~**onboarding.create.R34**~~ retired 2026-10-13: it printed a
  `declared:` line for a `.fleet/project.toml` already there.
  onboarding.create.R72 says what a project file already there prints
  (fleet-1s1x).
- ~~**onboarding.create.R35**~~ retired 2026-10-13: it checked a
  `.fleet/project.toml` already there. onboarding.create.R67 checks a
  project file already there (fleet-1s1x).
- ~~**onboarding.create.R36**~~ retired 2026-10-13: it registered a
  standalone project in `projects.toml`, which is gone with standalone
  projects (fleet-1s1x).
- **onboarding.create.R37**: WHEN `fleet create` writes a project file,
  fleet SHALL append to `events.jsonl` (shared.locations.R12) a
  `project.registered` event whose payload carries the project's `path` and
  its name. It SHALL append none where a project file already named the
  checkout, or where it re-points one (onboarding.create.R70). The status
  area's specs say what an event carries beyond its payload.
  Today: the event is appended when a standalone call registers a root
  `projects.toml` did not list (fleet-1s1x.3).
- ~~**onboarding.create.R38**~~ retired 2026-10-13: it restated
  shared.locations.R34, which the story cites.
- ~~**onboarding.create.R39**~~ retired 2026-10-13: it restated
  shared.locations.R17, which the story cites.
- **onboarding.create.R40**: fleet SHALL never initialise, write into or
  rewrite the project's store.

### Clauses: your seat

- **onboarding.create.R41**: WHEN `fleet create` has written or read the
  files it makes, fleet SHALL list the person who ran it in the fleet's own
  `fleet.toml` (the default fleet's, or the wingman directory's) as a table
  under `[seats]`. The table SHALL be keyed by the id in `identity.toml`
  (shared.locations.R10) and carry `kind = "human"`, with a name only where
  `identity.toml` carries one. fleet SHALL ask for no name.
- **onboarding.create.R42**: WHEN `fleet create` lists the person, fleet SHALL
  print a line starting `seat:` naming the seat and the fleet's own
  `fleet.toml`. Where that file already lists this machine's identity, fleet
  SHALL leave the file as it was, and the line SHALL say the person is
  already listed.
- **onboarding.create.R43**: WHEN `fleet create` mints `identity.toml`
  (shared.locations.R10), whichever step of the call mints it, fleet SHALL
  print a line starting `identity:` naming it.
  Today: a standalone call that mints it as it registers a new root prints
  no `identity:` line (fleet-tlw).
- **onboarding.create.R44**: WHERE `--wingman` or `--dir` is given, IF a
  wingman directory's `fleet.toml` that was there before the call cannot
  take the person's table, THEN fleet SHALL still write the project file. It
  SHALL print a `seat:` line saying the person is not listed and why, naming
  `fleet seat add --human`, and exit 0 (shared.exit-codes.R1).
  Today: there is no wingman directory; a standalone call answers so for
  the fleet's file (fleet-1s1x.3).
- **onboarding.create.R45**: IF `identity.toml` cannot be read or minted, or
  a `fleet.toml` this call wrote cannot take the person's table, THEN fleet
  SHALL exit 3 (shared.exit-codes.R4) on a line naming
  `fleet seat add --human`, and leave the files it wrote written.

### Clauses: outcome

- **onboarding.create.R54**: fleet SHALL write every line `fleet create`
  writes on standard error, and none on standard output.
- **onboarding.create.R46**: WHEN `fleet create` has done what it was asked,
  fleet SHALL print a line starting `created` naming the kind of fleet,
  `default fleet` or `wingman project`, and the file written or read, and
  exit 0 (shared.exit-codes.R1).
  Today: the line names the mode, `embedded` or `standalone`
  (fleet-1s1x.3).
- **onboarding.create.R47**: WHEN `fleet create` succeeds, fleet SHALL print,
  as its last line, a line starting `next:` that names `fleet start`.

## Non-functional

- **onboarding.create.N1**: IF a pack's fetch has not finished within 120 s,
  THEN fleet SHALL stop it, keep nothing of that pack, and exit 3
  (shared.exit-codes.R4) on a line naming the pack and the bound.
  Today: a fetch has no bound, and one that stalls on the network waits
  indefinitely (fleet-pwwl).
- **onboarding.create.N2**: IF the project's store has not answered its
  prefix within 60 s, THEN fleet SHALL stop asking and take the store as
  answering none (onboarding.create.R73).

shared.locations.R17 holds the promise for two calls at once.

## Interfaces

- shared.locations.R25 leaves to this spec what `fleet create` does where no
  fleet is found: onboarding.create.R1, R49, R56.
- `fleet start` reads what `fleet create` wrote: a default fleet's
  `fleet.toml`, or `config.json` and the wingman directory. It renders the
  seats `fleet create` left unrendered (onboarding.create.R19). Its own spec
  says how.
- The verbs that act in a project find the files written here as
  shared.locations.R21 to R24 and R39 to R56 say. `fleet create --wingman`
  is the command that repairs a moved project's `path` and re-points a moved
  wingman directory: the verbs that cannot find their fleet name it
  (shared.locations.R54, shared.locations.R55).
- Like every verb but `fleet guard`, `fleet create` is refused before it
  reads or writes anything where the machine directory would be relative or
  is named by nothing (shared.locations.R30 to shared.locations.R32), and
  exits 3 where `config.json` will not read (shared.locations.R38).
- The packs area: the install is the one `fleet pack add <source> --version
  <tag>` does, including a pack's imports, and the defaults are its spec.
  Whether `fleet pack add` takes the 120 s bound of onboarding.create.N1 is
  the packs area's to say.
  `fleet doctor`'s `fleet-packs-version` check compares the installed packs
  with the same pinned tag.
- The seats area: `fleet seat add --human` writes the person's table the same
  way, and is the command onboarding.create.R44 and R45 name.
  `identity.toml` is the seats area's identity spec.
- The store area: the store adapter answers the prefix (onboarding.create.R73),
  given 60 s (onboarding.create.N2).
  The agent area: the agent adapter names the default model the written
  file's example row carries. A fleet whose file names no `[agent] adapter`
  opens `claude-code`, so a fleet made with the agent `none`
  (onboarding.create.R53) runs on it once its pack is installed. Until then,
  `fleet start` refuses as onboarding.start.R10 says, and `fleet doctor`
  rates `agent-adapter` as onboarding.doctor.R34 says.
- The status area: the event stream carries `project.registered`
  (onboarding.create.R37).
- git fetches each pack, is told never to prompt for credentials, and is
  given 120 s (onboarding.create.N1).
- `fleet create` takes no `--json`, so shared.output.R1 does not bind it.
  shared.output lists the verbs that do.

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
