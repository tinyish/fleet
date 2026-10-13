# `fleet create`

Uses: shared.exit-codes, shared.locations, shared.output

## What it is for

`fleet create` turns the directory you stand in into a fleet's project. It is
the first verb a person runs in a project, after `fleet doctor` and before
`fleet start`. It asks three questions: whether the fleet is embedded or
standalone, which store its items live in, and which agent its seats run on.
Then it installs the packs you picked, if any, writes the file for the mode,
and lists you as the fleet's first seat.

- **Embedded**: the fleet and the project are one directory. `fleet create
  --embedded` writes the fleet's policy file, `fleet.toml`, at the project's
  root.
- **Standalone**: the fleet lives in a directory of its own, and other
  projects are declared to it. `fleet create --standalone` writes the
  project's declaration, `.fleet/project.toml`, and registers the project on
  this machine. A standalone fleet is itself an embedded fleet in its own
  directory, made first with `--embedded`.

You meet it once per project. A script or an installer meets it with every
answer passed as a flag and no terminal attached.

## How it behaves

### Where it writes

`fleet create` writes into the working directory alone. It works inside a
git checkout or outside one. First it walks up from the working directory as
every verb that acts in a project does (shared.locations.R21). Where that
walk stops at a directory above the working directory, you stand inside an
existing fleet: a fleet written here would be a second, nested one, and every
verb run below it would act in it instead. So the call refuses, exit 1, on a
line naming the `fleet.toml` or `.fleet/project.toml` the walk stopped at,
and writes nothing. shared.locations.R25 leaves this spec to say what
`fleet create` does where no fleet is found. It is the verb that makes one,
so it never refuses for that.

### The three questions

On a terminal, each question is a list you pick from, and Enter alone takes
the first row. Where standard input is not a terminal, nothing is asked: the
mode must come from a flag (`--fleet` implies standalone, below), and the
store and the agent take `none`.

| Question | Asked when | Flag | Rows, in order | No terminal, no flag |
| --- | --- | --- | --- | --- |
| Embedded or standalone? | no `--embedded`, no `--standalone` and no `--fleet` | `--embedded`, `--standalone`; `--fleet` implies standalone | embedded, standalone | refused, exit 2 |
| Which store? | no `--store` | `--store none`, `--store bd` | none, bd | none |
| Which agent? | embedded, and no `--agent` | `--agent none`, `--agent claude-code` | none, claude-code | none |

They are asked in that order. `none` is the default of the store and the
agent alike: the first row, so Enter alone takes it, and what a call with no
terminal and no flag gets. `fleet create` installs nothing you did not pick.
Each pack is a fetch from the network and a choice of tool, and either pack
can be added later. `bd`, `none` and `claude-code` are the only spellings
the two flags take. Another spelling is refused like any unknown name.

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

A standalone project is never asked which agent, and nothing is installed
for one. It runs on the agent of the fleet it is declared to, which that
fleet's own `fleet create` installed. An `--agent` naming that agent is
accepted. One naming another agent is refused. What a standalone
`--agent none` does is not settled yet ([Open questions](#open-questions)).

`--fleet` names the fleet a standalone project is declared to, and means
nothing to an embedded fleet. So `--fleet` alone chooses standalone, and the
mode question is not asked. `--embedded` with `--fleet` is refused as a
usage error.

Without a terminal, a mode flag or `--fleet`, the call is refused. The line
carries the one `fleet create:` prefix and names `--embedded` and
`--standalone`, the flags that answer (shared.exit-codes.R14). A question the
terminal could not put, such as an interrupted prompt, exits 3, on a line
under the same one prefix.

**When refusals come.** A refusal that holds whatever the remaining answers
are comes before the first question. A person should not answer two prompts
and then meet a refusal the call could have carried from the start. A
standalone project's own refusals need the mode, so they come as soon as it
is known: before any question where the mode comes from a flag (`--fleet`
among them), and straight after the mode question where it is answered
there.

### What each mode writes

| | Embedded | Standalone |
| --- | --- | --- |
| The file written | `fleet.toml` in the working directory | `.fleet/project.toml` in the working directory, unless one is there already |
| The fleet's own policy file | that `fleet.toml` | the `fleet.toml` that `config.json` names, or that `--fleet` names |
| Packs installed | the store's, unless the store is `none`, and the agent's, unless the agent is `none` | the store's, unless the store is `none` |
| `config.json` | not written | written where it is absent and `--fleet` names the fleet |
| `projects.toml` | not written | the project's root and name |
| `events.jsonl` | nothing appended | a `project.registered` event, when the root was not registered yet |
| Your seat | a human table in the `fleet.toml` written | a human table in the fleet's own `fleet.toml` |

Both modes leave `identity.toml` (minted where absent), `defaults/` and its
line in `packs.lock`. Every machine-directory file is the one
shared.locations names.

**The embedded `fleet.toml`** is the smallest file that runs. A comment at
its top names the command that wrote it. It carries:

- `[guards]` with `shell-trap.enabled = true` and `record.enabled = true`;
- `[telemetry]` with `enabled = false`;
- `[agent]` with `adapter = "claude-code"`, absent where the agent is
  `none`;
- `[store]` with `adapter = "bd"`, absent where the store is `none`;
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

**The standalone `.fleet/project.toml`** opens with a comment naming the
command that wrote it, then `[project]` with:

- `name`: the working directory's own name;
- `item_prefix`: the prefix the project's store answers for its ids. Where
  the store cannot be opened or names no prefix, the line is left commented
  out for the person to fill in. A guessed prefix is one the store's own ids
  would disagree with. The store area's spec bounds how long the store is
  asked;
- `primary`: the working directory;
- `worktrees`: a sibling directory, the working directory's path with
  `-worktrees` on the end.

The two directories are written out because a standalone fleet's own file
sits elsewhere, so nothing is there to derive them from. A `[store]` table
follows, as in the embedded file. Then comes a `[landing]` table with its
`ci_marker` line commented out, because a guessed marker would skip a
pipeline nobody chose to skip.

**A `.fleet/project.toml` already there** is a declared project, not a
collision. `fleet create --standalone` writes nothing over it and registers
it. It reads every key (`name`, `item_prefix`, `primary`, `worktrees`), not
only the one the register is keyed on. A file this verb did not write is the
only one whose keys nobody has checked. The call refuses such a file, and
registers nothing, when the file:

- does not parse;
- lacks `name`, `primary` or `worktrees`;
- lacks `item_prefix` where the project's store answers a prefix for its ids;
- names a `primary` that is not a directory on this machine;
- or names a `worktrees` that is a file.

The `worktrees` directory need not exist yet. A prefix is needed only where a
store needs one. So a declaration whose `item_prefix` is still commented out,
as this verb writes it where the store answers none, is registered as it
stands: a second `fleet create --standalone` never refuses the file the first
one wrote.

Until it is registered, a declared project refuses every verb that acts in
it, and the refusal names `fleet create --standalone` (shared.locations.R26,
R35). `fleet create` is not refused there, because it is the command that
registers.

### Which fleet a standalone project is declared to

| `config.json` | `--fleet <dir>` | Outcome |
| --- | --- | --- |
| names a `fleet.toml` that exists | absent, or the directory holding that file | declared to the fleet `config.json` names |
| names a `fleet.toml` that exists | any other directory | refused, exit 1, naming both fleets, and nothing written |
| names a `fleet.toml` that is not there | anything | refused, exit 1 |
| is there but cannot be read or does not parse | anything | exit 3, as shared.locations.R38 says, and nothing written |
| absent | absent, or a directory holding no `fleet.toml` | refused, exit 1 |
| absent | a directory holding a `fleet.toml` | `config.json` written naming that file, then declared to it |

`--fleet` is read against the working directory. Otherwise `config.json` is
written by a fleet's first `fleet start`, and requiring that would put a
start between the two creates for no reason a person could see. So
`--fleet` names the embedded fleet's directory instead. The `config.json`
`fleet create` writes names the fleet's file and lists no seats, as a first
start does. The next `fleet start` renders the seats.

Where `config.json` already names a fleet, a `--fleet` that agrees with it is
accepted, and one that names another fleet is refused rather than ignored:
the project would otherwise be declared to a fleet the person did not name,
without a word.

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
pack refuses it, exit 2, and writes and installs nothing: standalone with the
store `none`, or embedded with both the store and the agent `none`. With no
terminal, that includes a call that passes `--packs-from` and neither
`--store` nor `--agent`, because both then take `none`. The refusal comes as
soon as the answers it turns on are known: before any question where the
mode comes from a flag and the store and the agent from flags or from no
terminal, and straight after the last question that answers one of them
otherwise.

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
installs a pack, so a defaults refusal leaves no fleet file, no
`config.json`, no `projects.toml` row, no `identity.toml` and no
`project.registered` event behind. A machine directory that cannot be written
is met there, on the defaults' own refusal line, before anything else is
tried.

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
| Standalone, the fleet's file cannot take the table | project still declared and registered; the `seat:` line says `not listed`, why, and names `fleet seat add --human`; exit 0 |
| Embedded, the table cannot be written, or `identity.toml` cannot be read or minted | exit 3, naming `fleet seat add --human`; the fleet file stays written |

In standalone mode, a fleet file that cannot take the table gets a line
rather than a refusal. The project is declared and registered, and the
fleet's file is somebody else's to put right.

### Your project directory, and edits that are never lost

`fleet create` leaves in a project's directory only the file you asked for:
`fleet.toml`, or `.fleet/project.toml`. Listing you in a standalone fleet's
own `fleet.toml` leaves no other file in that fleet's directory either
(shared.locations.R34). Two fleet commands writing the same file at
once never lose an edit. Two `fleet create --standalone` calls registering at
once leave both rows in `projects.toml`. Two calls minting this machine's
identity at once leave one identity, and both list it
(shared.locations.R17).

### The project's store

`fleet create` never initialises, writes into or rewrites the project's
store. It makes no beads board. The bd pack's README says how to make one.

### What it prints

Lines go to standard error, each starting with a word a person scans for:

| Line starts with | When |
| --- | --- |
| `store:`, `agent:` | for each pack installed, imported or found already there; and, where the store or the embedded fleet's agent is `none`, naming the `fleet pack add` command that installs it |
| `machine:` | standalone, when `config.json` was written |
| `declared:` | standalone, when a `.fleet/project.toml` was already there |
| `registered` | standalone |
| `created` | on success, naming the mode and the file |
| `defaults:` | on success |
| `seat:`, `identity:` | as [Your seat](#your-seat) says |
| `next:` | last, naming `fleet start` |

The `config.json` line starts `machine:`, not `fleet:`, because `fleet:`
starts the line of an error fleet did not turn into a refusal
(shared.exit-codes.R17), and a script keying on it must be able to tell the
two apart.

### Refusals

Each refusal writes a line under `fleet create:` (shared.exit-codes.R12),
naming what it is about (shared.exit-codes.R13).

| The case | Exit | When |
| --- | --- | --- |
| the variable that decides the machine directory names a relative path, or nothing names it (shared.locations.R30, R31) | 1 | before anything (shared.locations.R32) |
| a flag `fleet create` does not take, or `--embedded` with `--standalone` or `--fleet` | 2 | before anything |
| no mode flag, no `--fleet` and no terminal | 2 | instead of the first question |
| `--agent` or `--store` names an answer `fleet create` does not take; `--packs-from` names no directory | 2 | before any question |
| a `fleet.toml` or a `.fleet` is here, and no `.fleet/project.toml` | 1 | before any question |
| the walk up from here stops at a directory above it (shared.locations.R21) | 1 | before any question |
| `--packs-from` where the call installs no pack: standalone with the store `none`, embedded with the store and the agent `none` | 2 | once the mode and those answers are known: before any question where flags, or no terminal, give them all |
| standalone: an `--agent` that is not the fleet's | 2 | once the mode is known |
| standalone: no fleet to declare to | 1 | once the mode is known |
| standalone: `--fleet` names a fleet other than the one `config.json` names | 1 | once the mode is known |
| standalone: `config.json` is there but cannot be read or does not parse (shared.locations.R38) | 3 | once the mode is known |
| embedded: a `.fleet/project.toml` is here, with or without a `fleet.toml` beside it | 1 | once the mode is known |
| standalone: a `.fleet/project.toml` already there is refused | 1 | once the mode is known |
| a pack's install fails | 1 | see [The packs](#the-packs) |
| a pack's fetch runs past 120 s | 3 | see [The packs](#the-packs) |
| `defaults/` cannot be written, or is unrecorded or edited | 1 | see [The defaults](#the-defaults) |
| a file it writes after the installs cannot be written | 3 | what was written before stays |

A declared project wins at its own level. A directory carrying
`.fleet/project.toml` is read as a standalone project, whatever sits beside
it (shared.locations.R22). So `--embedded` is refused wherever a declaration
is, with or without a `fleet.toml` beside it, because no verb run there would
act in the fleet it wrote. `--standalone` registers the declaration.

### Known gaps

Each clause states what fleet is meant to do. Where today's binary falls
short, a `Today:` line under the clause names the item that closes the gap.

- **The line for a question the terminal could not put** carries one
  `fleet create:` prefix. Today it reads `fleet create: fleet: …`
  (fleet-130b).
- **Refusals that hold whatever the answers are come before any question.**
  Today two of them come late, after the questions and the pack installs,
  which stay installed (fleet-b42k):
  - `--embedded` beside both a `fleet.toml` and a declaration;
  - a declaration already there that is refused.
- **No lost edits.** Today the first write of `config.json` takes no lock, so
  two calls writing it at once can lose one (fleet-rbwc).
- **A `config.json` that does not read or parse** refuses with exit 1, where
  the call cannot tell which fleet it declares the project to and should
  exit 3 (fleet-9sr5).
- **Inside an existing fleet, the call refuses.** Today it looks in the
  working directory alone, and writes a second, nested fleet there
  (fleet-6zpk).
- **`--fleet` implies standalone, and one naming another fleet than
  `config.json`'s is refused** (fleet-coog). Today `--fleet` with no mode
  flag still asks the mode question, or is refused with no terminal, and an
  embedded answer drops it without a word. A `--fleet` naming another fleet
  is ignored, and the project is declared to `config.json`'s, exit 0.
- **`--embedded` beside a lone declaration is refused** (fleet-okaw). Today
  it writes a `fleet.toml` there, which no verb run there resolves to.
- **A declaration this verb wrote is registered as it stands** (fleet-eho2).
  Today one whose `item_prefix` is commented out is refused for lacking it,
  so a second `--standalone` refuses the first one's file.
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

- **onboarding.create.R1**: WHEN `fleet create` is called, fleet SHALL write
  the fleet's file into the working directory, whether or not the working
  directory is inside a git checkout, and only where no directory above it
  holds a fleet (onboarding.create.R49).
  Today: it writes a second, nested fleet below an existing one
  (fleet-6zpk).
- **onboarding.create.R49**: IF the walk shared.locations.R21 makes from the
  working directory stops at a directory above it, THEN fleet SHALL refuse
  with exit 1 (shared.exit-codes.R2) on a line naming the `fleet.toml` or
  `.fleet/project.toml` the walk stopped at, and write nothing.
  Today: it looks in the working directory alone, and writes a second,
  nested fleet there (fleet-6zpk).

### Clauses: the questions

- **onboarding.create.R2**: WHEN `fleet create` is called with none of
  `--embedded`, `--standalone` and `--fleet`, and standard input is a
  terminal, fleet SHALL ask first whether the fleet is embedded or
  standalone, offering embedded then standalone, with embedded selected
  before a key is pressed.
  Today: `--fleet` with no mode flag still asks, and an embedded answer drops
  `--fleet` without a word (fleet-coog).
- **onboarding.create.R3**: WHILE standard input is a terminal, fleet SHALL
  ask which store where no `--store` is given (offering `none` then `bd`),
  then, where the mode is embedded and no `--agent` is given, which agent
  (offering `none` then `claude-code`), each with its first row, `none`,
  selected before a key is pressed.
  Today: the store question offers `bd` then `none`, and the agent question
  offers `claude-code` alone, so Enter alone takes `bd` and `claude-code`
  (fleet-284h).
- **onboarding.create.R4**: WHERE the mode is standalone, fleet SHALL ask no
  agent question and install no agent's pack, and SHALL accept an `--agent`
  that names the agent the fleet's own `fleet.toml` names in
  `[agent] adapter` (`claude-code` where it names none).
- **onboarding.create.R5**: IF `fleet create` is called with none of
  `--embedded`, `--standalone` and `--fleet`, and standard input is not a
  terminal, THEN fleet SHALL refuse with exit 2 (shared.exit-codes.R3, R14)
  on a line that carries one `fleet create:` prefix and names `--embedded`
  and `--standalone`, and write nothing.
  Today: `--fleet` with no mode flag is refused here too (fleet-coog).
- **onboarding.create.R6**: WHILE standard input is not a terminal, fleet
  SHALL take `none` where no `--store` is given and, where the mode is
  embedded, `none` where no `--agent` is given, without asking.
  Today: it takes `bd` and `claude-code`, and installs their packs
  (fleet-284h).
- **onboarding.create.R7**: IF a question cannot be put on the terminal,
  THEN fleet SHALL exit 3 (shared.exit-codes.R4) on a line that carries the
  one `fleet create:` prefix (shared.exit-codes.R12).
  Today: the line reads `fleet create: fleet: …`, a second prefix
  (fleet-130b).
- **onboarding.create.R8**: fleet SHALL make every refusal that holds
  whatever the questions still to come are answered before asking any of
  them, and before installing anything: onboarding.create.R9 to R12 and R49
  before any question; onboarding.create.R14 to R17, R35, R48, R50 and R52
  as soon as the mode is known (before any question where the mode comes
  from a flag, `--fleet` among them (onboarding.create.R51), and straight
  after the mode question where it is answered there); and
  onboarding.create.R13 as soon as the mode, the store and, where the mode is
  embedded, the agent are known.
  Today: the refusals of onboarding.create.R17 and R35 come only after the
  questions the mode asks and the packs' installs (fleet-b42k).
- **onboarding.create.R51**: WHEN `fleet create` is called with `--fleet` and
  neither `--embedded` nor `--standalone`, fleet SHALL take the mode as
  standalone and ask no mode question.
  Today: the mode question is still asked, or, with no terminal, the call is
  refused as onboarding.create.R5 says (fleet-coog).

### Clauses: refused answers and directories

- **onboarding.create.R9**: IF `--embedded` is given with `--standalone` or
  with `--fleet`, THEN fleet SHALL refuse as shared.exit-codes.R15 says, with
  exit 2 (shared.exit-codes.R3), and write nothing.
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
- **onboarding.create.R12**: IF the working directory holds a `fleet.toml`
  (shared.locations.R23) or a `.fleet`, and no `.fleet/project.toml`
  (shared.locations.R22), THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming the one it found, and write
  nothing.
- **onboarding.create.R13**: IF `--packs-from` is given and the call installs
  no pack, the store being `none` and, where the mode is embedded, the agent
  being `none`, by a flag, a question or onboarding.create.R6, THEN fleet
  SHALL refuse with exit 2 (shared.exit-codes.R3) on a line naming
  `--packs-from`, and write nothing.
  Today: only a standalone call given `--store none` as a flag is refused; a
  standalone call that takes `none` at the question is not, and an embedded
  call cannot answer `none` for its agent (fleet-284h).
- **onboarding.create.R14**: WHERE the mode is standalone, IF `--agent` names
  an agent other than the one onboarding.create.R4 accepts, THEN fleet SHALL
  refuse with exit 2 (shared.exit-codes.R3) on a line naming the fleet's
  `fleet.toml` and its agent, and write nothing.
- **onboarding.create.R15**: WHERE the mode is standalone, IF `config.json`
  (shared.locations.R11) is absent, and no `--fleet` is given or the
  directory `--fleet` names holds no `fleet.toml`, THEN fleet SHALL refuse
  with exit 1 (shared.exit-codes.R2) on a line naming `config.json` and
  `--fleet` or that directory, and write nothing.
- **onboarding.create.R16**: WHERE the mode is standalone, IF `config.json`
  (shared.locations.R11) names a policy file that is not there, THEN fleet
  SHALL refuse with exit 1 (shared.exit-codes.R2) on a line naming
  `config.json`, and write nothing. A `config.json` that is there but cannot
  be read or does not parse is onboarding.create.R48's.
- **onboarding.create.R17**: WHERE the mode is embedded, IF the working
  directory holds both a `fleet.toml` and a `.fleet/project.toml`
  (shared.locations.R22), THEN fleet SHALL refuse with exit 1
  (shared.exit-codes.R2) on a line naming that `fleet.toml`, and write
  nothing.
  Today: the defaults and the packs this call installed before the refusal
  stay installed and pinned (fleet-b42k).
- **onboarding.create.R52**: WHERE the mode is embedded, IF the working
  directory holds a `.fleet/project.toml` (shared.locations.R22) and no
  `fleet.toml`, THEN fleet SHALL refuse with exit 1 (shared.exit-codes.R2) on
  a line naming that declaration, and write nothing.
  Today: it writes a `fleet.toml` beside the declaration, which no verb run
  there resolves to (fleet-okaw).

### Clauses: which fleet a standalone project is declared to

- **onboarding.create.R18**: WHERE the mode is standalone, WHILE `config.json`
  (shared.locations.R11) names a policy file that exists, and `--fleet` is
  absent or names the directory that holds that file, fleet SHALL declare the
  project to that fleet.
  Today: it declares the project there whatever `--fleet` names
  (fleet-coog).
- **onboarding.create.R19**: WHERE the mode is standalone, WHEN `config.json`
  is absent and `--fleet` names a directory holding a `fleet.toml`, fleet
  SHALL write `config.json` (shared.locations.R11) naming that `fleet.toml`
  by its full path, read against the working directory, with no seats, and
  print a line starting `machine:` that names that file and `config.json`.
  Today: the line starts `fleet:` (fleet-qb4y).
- **onboarding.create.R50**: WHERE the mode is standalone, IF `config.json`
  (shared.locations.R11) names a policy file that exists and `--fleet` names
  a directory other than the one that holds it, THEN fleet SHALL refuse with
  exit 1 (shared.exit-codes.R2) on a line naming both fleets, the policy file
  `config.json` names and the directory `--fleet` names, and write nothing.
  Today: the `--fleet` is ignored without a word, and the project is
  declared to the fleet `config.json` names, exit 0 (fleet-coog).

### Clauses: the packs

- **onboarding.create.R20**: WHEN `fleet create` installs the store's pack
  (the store is `bd`) or the agent's (the mode is embedded and the agent is
  `claude-code`), fleet SHALL
  install `<source>//adapters/store/bd` or
  `<source>//adapters/agent/claude-code` at the tag this binary pins, into
  `packs/bd/` or `packs/claude-code/` (shared.locations.R7), pinned in
  `packs.lock` (shared.locations.R9), unless onboarding.create.R22 applies.
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
- **onboarding.create.R53**: WHERE the mode is embedded, WHEN the agent is
  `none`, fleet SHALL install no agent pack, and print a line starting
  `agent:` naming the `fleet pack add` command, with
  `<source>//adapters/agent/claude-code`, `<source>` taken as
  onboarding.create.R21 takes it, and `--version` at the tag this binary
  pins, that installs one later.
  Today: the agent cannot be `none`: `--agent none` is refused, and the
  question offers no `none` row (fleet-284h).
- **onboarding.create.R25**: IF a pack cannot be installed, THEN fleet SHALL
  refuse with exit 1 (shared.exit-codes.R2) on a line naming the install's
  reason and, for the store's pack, `fleet create --store none`, or, for the
  agent's pack, the `fleet pack add` command that installs it. It SHALL write
  no `fleet.toml`, `.fleet/project.toml`, `config.json`, `projects.toml` row
  (shared.locations.R11, R13) or `identity.toml` (shared.locations.R10). A
  fetch past its bound is onboarding.create.N1's.
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

- **onboarding.create.R31**: WHERE the mode is embedded, fleet SHALL write
  `fleet.toml` (shared.locations.R23) in the working directory. The file
  SHALL open with a comment naming the command that wrote it, and carry:
  `[guards]` with `shell-trap.enabled = true` and `record.enabled = true`;
  `[telemetry]` with `enabled = false`; `[agent]` with `adapter` naming the
  agent where the agent is `claude-code`; `[store]` with `adapter = "bd"`
  where the store is `bd`; and `[seats]`, under a comment showing an agent
  seat's table. It SHALL carry no other key but the seat's
  (onboarding.create.R41).
  Today: the file always carries `[agent]`, because the agent cannot be
  `none` (fleet-284h).
- **onboarding.create.R32**: WHERE the mode is standalone, WHEN the working
  directory holds no `.fleet/project.toml`, fleet SHALL write
  `.fleet/project.toml` (shared.locations.R22). The file SHALL open with a
  comment naming the command that wrote it, and carry `[project]` with:
  `name` (the working directory's own name); `item_prefix` (the prefix the
  project's store answers, or a commented-out line where it cannot be opened
  or answers none); `primary` (the working directory's full path); and
  `worktrees` (that path with `-worktrees` on the end). It SHALL then carry
  `[store]` with `adapter = "bd"` where the store is `bd`, and `[landing]`
  with its `ci_marker` line commented out.
- **onboarding.create.R33**: IF a file `fleet create` writes after its
  installs (the file for its mode, `config.json`, its `projects.toml` row or
  its `project.registered` event) cannot be written, THEN fleet SHALL exit 3
  (shared.exit-codes.R4) on a line naming what could not be written.
- **onboarding.create.R34**: WHERE the mode is standalone, WHEN the working
  directory already holds a `.fleet/project.toml`, fleet SHALL write nothing
  over it, and print a line starting `declared:` naming the project and the
  file.
- **onboarding.create.R35**: IF a `.fleet/project.toml` already there does not
  parse, lacks `name`, `primary` or `worktrees` under `[project]`, lacks
  `item_prefix` where the project's store answers a prefix for its ids, names
  a `primary` that is not a directory, or names a `worktrees` that is a file,
  THEN fleet SHALL refuse with exit 1 (shared.exit-codes.R2) on a line naming
  the file and the key, and write nothing. A declaration whose `item_prefix`
  is commented out where the store answers none, as onboarding.create.R32
  writes it, SHALL not be refused for it.
  Today: the defaults and the store's pack this call installed before the
  refusal stay installed and pinned (fleet-b42k).
  Today: a declaration with `item_prefix` commented out is refused for
  lacking it, whatever the store answers, so a second `--standalone` refuses
  the file the first one wrote (fleet-eho2).
- **onboarding.create.R36**: WHERE the mode is standalone, fleet SHALL
  register the working directory's full path and the declaration's `name` in
  `projects.toml` (shared.locations.R13), and print a line starting
  `registered` naming the name, the root and `projects.toml`.
- **onboarding.create.R37**: WHEN `fleet create` registers a root that
  `projects.toml` did not list, fleet SHALL append to `events.jsonl`
  (shared.locations.R12) a `project.registered` event whose payload carries
  the root and the name. It SHALL append none for a root already listed. The
  status area's specs say what an event carries beyond its payload.
- **onboarding.create.R38**: WHEN `fleet create` writes the file for its mode
  or lists the person, fleet SHALL leave no file the person did not ask for
  (shared.locations.R34) in the project's directory or in the fleet's own:
  in the project's, only the `fleet.toml` or `.fleet/project.toml` that
  onboarding.create.R31 and R32 write.
- **onboarding.create.R39**: WHEN two fleet commands write the same
  `projects.toml`, `config.json`, `identity.toml` or fleet's `fleet.toml` at
  once, fleet SHALL lose neither's edit (shared.locations.R17).
  Today: `fleet create --standalone --fleet`'s first write of `config.json`
  takes no lock, so two such writes at once can lose one (fleet-rbwc).
- **onboarding.create.R40**: fleet SHALL never initialise, write into or
  rewrite the project's store.

### Clauses: your seat

- **onboarding.create.R41**: WHEN `fleet create` has written or read the file
  for its mode, fleet SHALL list the person who ran it in the fleet's own
  `fleet.toml` as a table under `[seats]`. The table SHALL be keyed by the
  id in `identity.toml` (shared.locations.R10) and carry `kind = "human"`,
  with a name only where `identity.toml` carries one. fleet SHALL ask for
  no name.
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
- **onboarding.create.R44**: WHERE the mode is standalone, IF the fleet's own
  `fleet.toml` cannot take the person's table, THEN fleet SHALL still declare
  and register the project. It SHALL print a `seat:` line saying the person
  is not listed and why, naming `fleet seat add --human`, and exit 0
  (shared.exit-codes.R1).
- **onboarding.create.R45**: IF `identity.toml` cannot be read or minted, or,
  where the mode is embedded, the `fleet.toml` just written cannot take the
  person's table, THEN fleet SHALL exit 3 (shared.exit-codes.R4) on a line
  naming `fleet seat add --human`, and leave the fleet file it wrote
  written.

### Clauses: outcome

- **onboarding.create.R46**: WHEN `fleet create` has done what it was asked,
  fleet SHALL print on standard error a line starting `created` naming the
  mode and the file written or read, and exit 0 (shared.exit-codes.R1).
- **onboarding.create.R47**: WHEN `fleet create` succeeds, fleet SHALL print
  on standard error, as its last line, a line starting `next:` that names
  `fleet start`.

### Clauses: a seat list that cannot be read

- **onboarding.create.R48**: WHERE the mode is standalone, IF `config.json`
  is there but cannot be read or does not parse, THEN fleet SHALL answer as
  shared.locations.R38 says, and write nothing: writing a fresh `config.json`
  over it would replace the machine's seat rows.
  Today: it refuses with exit 1 (fleet-9sr5).

## Non-functional

- **onboarding.create.N1**: IF a pack's fetch has not finished within 120 s,
  THEN fleet SHALL stop it, keep nothing of that pack, and exit 3
  (shared.exit-codes.R4) on a line naming the pack and the bound.
  Today: a fetch has no bound, and one that stalls on the network waits
  indefinitely (fleet-pwwl).

The store area's spec bounds the ask for the prefix (onboarding.create.R32).
onboarding.create.R39 holds the promise for two calls at once.

## Interfaces

- shared.locations.R25 leaves to this spec what `fleet create` does where no
  fleet is found: onboarding.create.R1, R49.
- `fleet start` reads what `fleet create` wrote: the embedded `fleet.toml`, or
  `config.json` and `projects.toml`. It renders the seats `fleet create` left
  unrendered (onboarding.create.R19). Its own spec says how.
- The verbs that act in a project find the files written here by
  shared.locations.R21 to R23. `fleet create --standalone` is the command
  that registers a declaration: the verbs that act in a declared project the
  machine has not registered refuse and name it (shared.locations.R26, R35),
  and `fleet create` is not bound by those clauses.
- Like every verb but `fleet guard`, `fleet create` is refused before it
  reads or writes anything where the machine directory would be relative or
  is named by nothing (shared.locations.R30 to R32).
- The packs area: the install is the one `fleet pack add <source> --version
  <tag>` does, including a pack's imports, and the defaults are its spec.
  Whether `fleet pack add` takes the 120 s bound of onboarding.create.N1 is
  the packs area's to say.
  `fleet doctor`'s `fleet-packs-version` check compares the installed packs
  with the same pinned tag.
- The seats area: `fleet seat add --human` writes the person's table the same
  way, and is the command onboarding.create.R44 and R45 name.
  `identity.toml` is the seats area's identity spec.
- The store area: the store adapter answers the prefix (onboarding.create.R32).
  The agent area: the agent adapter names the default model the embedded
  file's example row carries. A fleet whose file names no `[agent] adapter`
  opens `claude-code`, so a fleet made with the agent `none`
  (onboarding.create.R53) runs on it once its pack is installed. Until then,
  `fleet start` refuses as onboarding.start.R10 says, and `fleet doctor`
  rates `agent-adapter` as onboarding.doctor.R34 says.
- The status area: the event stream carries `project.registered`
  (onboarding.create.R37).
- git fetches each pack, is told never to prompt for credentials, and is
  given 120 s (onboarding.create.N1).
- `fleet create` takes no `--json`. shared.output lists the verbs that do.

## Open questions

- [NEEDS CLARIFICATION: what a standalone `--agent none` does. The ruling
  of 2026-10-13 adds `none` to the agent question, which a standalone
  project is never asked. As onboarding.create.R4 and R14 read, `none` names
  no agent, the fleet's own file runs `claude-code` where it names none, and
  so a standalone `--agent none` is refused with exit 2. It could instead be
  accepted, as a standalone project installs no agent pack anyway. Blocks
  onboarding.create.R4 and R14 for the answer `none` only.]

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
