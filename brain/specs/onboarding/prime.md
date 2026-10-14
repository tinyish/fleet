# `fleet prime`

Uses: shared.exit-codes, shared.locations

## What it is for

`fleet prime` prints what a session is handed when it starts: which fleet
binary it is talking to, the pack layers and guards in force, the project's
store, the rules every seat works under, and the items assigned to the seat
whose worktree the session opened in.

Its caller is the claude-code pack's session-start hook. Every seat's session
loads the pack's plugin, and when the session starts the hook runs
`fleet prime` through the binary `FLEET_BIN` names. Claude Code hands what
`fleet prime` prints on standard output to the session as its opening
context. A person can also run it by hand to see what a session started in
that directory would be told.

It only reads: it creates no file and appends no event. It needs no
controller, takes no arguments and no `--json`, only `--help` and `-h`, and
never waits on standard input.

## How it behaves

### Which fleet it reads

`fleet prime` finds its fleet as every verb that acts in a project does
(shared.locations.R21 to shared.locations.R24, shared.locations.R39 to
shared.locations.R53): a wingman project whose project file names the
checkout, a default fleet's own `fleet.toml`, and, where neither is found,
the default fleet `config.json` names only inside a git checkout. A seat's
session runs `fleet prime` in the seat's worktree, which is a checkout. So a
seat is primed wherever it works, provided its fleet is found: a wingman
project whose `path` names the seat's primary checkout
(shared.locations.R40); a committed `fleet.toml` (shared.locations.R23); or,
with no file, a default fleet `config.json` names (shared.locations.R24).

Where it finds no fleet, or finding it refuses, it refuses as every such
verb does (shared.locations.R25, shared.locations.R41, shared.locations.R42,
shared.locations.R52): it says why on standard error, prints nothing on
standard output, and exits 1. A session outside every checkout is told there
is no fleet there. Where `config.json` or a project file is there but will
not read, so it cannot tell which fleet it acts in, it says so as every such
verb does (shared.locations.R38, shared.locations.R53), likewise prints
nothing on standard output, and exits 3. Today a `config.json` that does not
parse gets the no-fleet refusal and exit 1 instead (shared.locations.R38's
`Today:` line, fleet-9sr5).

Prime never waits on standard input, so it never asks which project. Run in
a wingman directory that holds several project files, it refuses as finding
no fleet, exit 1, on a line saying to run it in a project's checkout. It
takes no `--project`: seats never meet this, since they run prime in their
worktrees, and a hook has no flag to pass. A wingman directory with one
project file primes that project, as every verb does
(shared.locations.R43).

The machine directory prime reads `config.json` and the packs from is the one
shared.locations.R2 to shared.locations.R6 resolve. A seat's session is
handed the controller's (shared.locations.R29). Where the variable that
decides it names a relative path, or `HOME` is unset and nothing else names
it, prime refuses before it reads anything, as every verb but `fleet guard`
does (shared.locations.R30 to shared.locations.R32): exit 1, the reason on
standard error, and nothing on standard output.

### How it exits

Once it has found its fleet and its command line parses, `fleet prime` exits
0 (shared.exit-codes.R1), whatever it could or could not read. A session-start
hook that fails closed takes the session down with it. So a part that cannot
be read says so in its own place, and the parts after it still print. A
command line that does not parse exits 2: `fleet prime --json` or any other
argument but `--help` and `-h` (shared.exit-codes.R3, shared.exit-codes.R15).

Where it has no fleet to act in, prime exits as every verb does there: 1
where it finds no fleet or a declared project the machine has not
registered (shared.exit-codes.R2, shared.exit-codes.R23), and 3 where it
cannot tell which fleet it acts in (shared.locations.R38) or cannot read its
working directory (shared.exit-codes.R4). With no fleet it has no context
to hand the session, and a refusal is a refusal whoever called it. Its
refusal of a machine directory it cannot resolve exits 1 (above).

Everything is written on standard output, and nothing on standard error.
Where whatever reads standard output closes it before prime has finished
writing, prime stops writing, writes nothing on standard error and exits as
its answer gives, 0 wherever it finds its fleet (shared.exit-codes.R27,
onboarding.prime.R1): the reader chose to stop reading.
Today every verb, `fleet prime` among them, panics there and exits 101
(fleet-wtpr).

### What it prints, in order

In a default fleet `fleet create` made with the claude-code agent and
the bd store, from the worktree of a seat with two items open:

```text
fleet 0.1.0 — packs: bd, claude-code, ts; guards: shell-trap on, record on
store: bd 1.3.0 (adapter bd)
<the resolved assets/rules.md, as the file reads; its content is illustrative here>
item: fx-1 — first open item
item: fx-2 — one in progress
```

| Order | Part | What it says |
| --- | --- | --- |
| 1 | the first line | `fleet <version> —`, the version `fleet --version` prints; then `packs:` and the installed packs; then `; guards:` and the guard classes |
| 2 | the store line | `store:`, then the project's store by the name and version it answers with, and `(adapter <name>)` |
| 3 | the rules | the rules file the pack layers resolve, byte for byte |
| 4 | the item lines | where the working directory is a seat's worktree, one `item: <id> — <title>` line per item assigned to that seat that is open or in progress |

**The packs.** The installed packs, by name, in the order the pack layers
resolve, top first. The defaults are never named. With no pack installed the
layers still resolve, since the defaults are the bottom layer, and the part
reads `none installed`. When an installed pack imports a pack that is not
installed, the names are followed by one entry per missing import, in
parentheses and separated by `; `. Each entry names the importer and the
import, then the `fleet pack add` line that adds it where `packs.lock`
records the importer's source, and otherwise the source the importer's
manifest names. A session is told about a missing import at its start,
rather than by the first run it opens.

**The guards.** Each guard class the layers declare, each followed by `on` or
`off` as the fleet's policy file switches it. With the defaults alone that is
`shell-trap` and `record`.

**The store.** The project is the seat's worktree where the working directory
is one, so the store line and the item lines name one store. Otherwise it is
the wingman project's checkout where the fleet was found as a wingman
project, and else the directory the walk stopped at. The store is the one a verb run in that
project writes to. The adapter is named as `[store] adapter` names it: the
name, or the file name of an adapter named by path, or `bd` where nothing is
named. The version is shown whatever release it is: which release a store was
measured on is its own doctor check's to say. With no project (a directory
the checkout fallback admitted, with no fleet file of its own and no seat's
row), the line reads `store: none` and says there is no project here.

**The rules.** The file at `assets/rules.md` in the highest layer that
carries it, which is not always the highest layer, printed as it is, with a
newline added where the file lacks a last one.

**The items.** The seat is the one whose row in `config.json` names the
working directory itself as a worktree. A directory under that worktree
names no seat, because a working directory names a seat and proves nothing,
so the narrowest reading is taken. No environment variable names the seat:
`FLEET_ACTOR` is not read. The items are those the store holds assigned to
that seat's full id whose status is `open` or `in_progress`, in the order the
store lists them. A seat with none gets `item: none`. Where no row names the
working directory, no item line is printed.

### When a part cannot be read

| Part | When it cannot be read | What prints in its place |
| --- | --- | --- |
| the packs | the layers do not resolve | `packs: could not be resolved —` and the first reason; the rest are for `fleet pack check` |
| the guards | the fleet's policy file cannot be read or does not parse, whatever the layers do | `guards: could not be read —` and a reason naming the policy file |
| the guards | the layers do not resolve | `shell-trap` and `record` alone, each `on` or `off`: the classes no layer has to declare |
| the guards | an installed pack's manifest cannot say which classes it turns on | `guards: could not be read —` and the reason |
| the store | the adapter cannot be opened, refuses `version`, or does not answer within 2 seconds | `store: could not be read —` and the reason; an adapter no installed pack carries is named with the `fleet pack add` line that installs it |
| the rules | the layers do not resolve | nothing: the first line already says why |
| the rules | no layer carries a readable `assets/rules.md` | a line of its own saying so |
| the items | the store cannot be opened, refuses the listing, or does not answer within 5 seconds | `item: could not be read —` and the reason, never `item: none`, which would tell a seat it is free |

The two store calls are bounded on their own, well under a session-start
hook's own deadline: 2 seconds for the version, which reads no items, and 5
seconds for the listing. A store that never answers costs those two lines,
and never the session start.

A reason is folded onto the one line it belongs to, whatever line breaks it
carries, such as those of a pack manifest's TOML parse error. So the first
line is always one line, and the store line always the second.

### Known gaps

Each of these is an approved item. Until it lands, the clause states the
intent, and a `Today:` line says what fleet does now.

- **prime refuses a machine directory it cannot resolve** (fleet-bxch,
  carried by shared.locations.R32's `Today:` line). Today, like
  `fleet guard`, it reads a relative `FLEET_DIR` or `FLEET_HOME` against the
  working directory, and with no `HOME` uses `.fleet` there, and goes on to
  print and exit as though that were the machine directory.
- **A missing rules file says so** (fleet-mkbn). Today nothing prints in the
  rules' place.
- **The store line names the store the project's verbs use** (fleet-10uu).
  Today, in a seat's worktree with no fleet file of its own, prime opens the
  bd pack's store rather than the one the fleet's policy file names.
- **A closed standard output ends prime quietly** (fleet-wtpr, carried by
  shared.exit-codes.R27's `Today:` line). Today prime, like every verb,
  panics, writes the panic on standard error and exits 101.
- **Prime in a wingman directory of several projects refuses** (fleet-1s1x.5).
  Today fleet knows no wingman directory.
- **An unreadable policy file says so in the guards part** (fleet-uo1u).
  Today every class reads as on, without a word.
- **An unreadable working directory exits 3** (fleet-fd5g). Today prime
  refuses as though outside every checkout, with exit 1, on a line naming an
  empty directory.
- **A reason is folded onto its line** (fleet-373f). Today a reason with
  line breaks spreads the first line over several, and the store line is no
  longer the second.

## Behaviour

- **onboarding.prime.R1**: WHEN `fleet prime` has found its fleet and its
  command line parses, fleet SHALL exit 0 (shared.exit-codes.R1), whether or
  not each part below could be read.
- **onboarding.prime.R2**: IF `fleet prime` is given any argument or flag
  other than `--help` or `-h`, `--json` included, THEN fleet SHALL treat the command
  line as one that does not parse (shared.exit-codes.R3,
  shared.exit-codes.R15).
- **onboarding.prime.R3**: fleet SHALL never wait on standard input during
  `fleet prime`.
- **onboarding.prime.R4**: WHILE `fleet prime` has found its fleet, fleet
  SHALL write everything it prints on standard output, and nothing on
  standard error.
- **onboarding.prime.R5**: WHEN `fleet prime` is called, fleet SHALL find the
  fleet it acts under as every verb that acts in a project finds it
  (shared.locations.R21 to shared.locations.R24, shared.locations.R39 to
  shared.locations.R53), except as onboarding.prime.R37 says.
- **onboarding.prime.R6**: IF `fleet prime` finds no fleet for the working
  directory (shared.locations.R25), finding it refuses
  (shared.locations.R41, shared.locations.R42, shared.locations.R52), or it
  cannot tell which fleet it acts in (shared.locations.R38,
  shared.locations.R53), THEN fleet SHALL say why on a line on standard error
  as those clauses word it, print nothing on standard output, and exit as
  those clauses say: 1 under shared.locations.R25, shared.locations.R41,
  shared.locations.R42 and shared.locations.R52 (shared.exit-codes.R2,
  shared.exit-codes.R23), and 3 under shared.locations.R38 and
  shared.locations.R53 (shared.exit-codes.R4).
- **onboarding.prime.R7**: IF the fleet's policy file cannot be read or does
  not parse, THEN fleet SHALL make the guards part read `could not be read —`
  followed by a reason naming the policy file, in place of what
  onboarding.prime.R13 to R15 would name.
  Today: prime reads every guard class as on, and prints nothing saying so
  (fleet-uo1u).
- **onboarding.prime.R8**: WHEN `fleet prime` has found its fleet, fleet SHALL
  print as its first line `fleet <version> —`, where the version is the one
  `fleet --version` prints, then `packs:` and the packs part, then
  `; guards:` and the guards part.
- **onboarding.prime.R9**: WHEN the pack layers resolve, fleet SHALL name in
  the packs part each installed pack, in the order the layers resolve, top
  first, and never the defaults.
- **onboarding.prime.R10**: WHEN the pack layers resolve and no pack is
  installed, fleet SHALL make the packs part read `none installed`.
- **onboarding.prime.R11**: WHEN an installed pack imports a pack that is not
  installed, fleet SHALL follow the packs part's names with one entry per
  missing import, in parentheses and separated by `; `, each naming the
  importer and the import, then the `fleet pack add` line that adds it where
  `packs.lock` records the importer's source, and otherwise the source the
  importer's manifest names.
- **onboarding.prime.R12**: IF the pack layers do not resolve, THEN fleet
  SHALL make the packs part read `could not be resolved —` followed by the
  first reason only.
- **onboarding.prime.R13**: WHEN the pack layers resolve, fleet SHALL name in
  the guards part each guard class the layers declare, each followed by `on`
  or `off` as the fleet's policy file switches it.
- **onboarding.prime.R14**: IF the pack layers do not resolve, THEN fleet
  SHALL name in the guards part `shell-trap` and `record` alone, each
  followed by `on` or `off`.
- **onboarding.prime.R15**: IF an installed pack's manifest cannot say which
  guard classes it turns on, THEN fleet SHALL make the guards part read
  `could not be read —` followed by the reason.
- **onboarding.prime.R16**: WHEN `fleet prime` has printed its first line,
  fleet SHALL print as its second line a line starting `store:` about the
  project's store, where the project is the seat's worktree that R25 finds,
  or else the `path` of the wingman project shared.locations.R40 or
  shared.locations.R43 finds, or else the directory the walk stopped at.
  Today: fleet knows no wingman project, so the project is the seat's
  worktree or the directory the walk stopped at (fleet-1s1x.5).
- **onboarding.prime.R17**: WHEN the project's store answers its version,
  fleet SHALL make the store line name the store by the name and version it
  answers with, whatever release that version is, followed by
  `(adapter <name>)`.
- **onboarding.prime.R18**: WHEN fleet names the adapter on the store line,
  fleet SHALL name it as the project's `[store] adapter` does: the name, the
  file name of an adapter named by path, or `bd` where nothing is named.
- **onboarding.prime.R19**: WHEN `fleet prime` reads the project's store for
  the store line and the item lines, fleet SHALL read the store a verb run in
  that project writes to.
  Today: in a seat's worktree with no fleet file of its own, prime opens the
  bd pack's store, not the store the fleet's policy file names (fleet-10uu).
- **onboarding.prime.R20**: IF there is no project by R16, THEN fleet SHALL
  make the store line read `store: none` and say there is no project here.
- **onboarding.prime.R21**: IF the project's store cannot be opened or does
  not answer its version, THEN fleet SHALL make the store line read
  `store: could not be read —` followed by the reason, and where no
  installed pack carries the store's adapter, name the `fleet pack add` line
  that installs it.
- **onboarding.prime.R22**: WHEN the pack layers resolve and a layer carries
  a readable `assets/rules.md`, fleet SHALL print after the store line the
  file from the highest layer that carries it, byte for byte, ending in a
  newline whether or not the file does.
- **onboarding.prime.R23**: IF the pack layers do not resolve, THEN fleet
  SHALL print nothing in the rules' place.
- **onboarding.prime.R24**: IF the pack layers resolve and no layer carries a
  readable `assets/rules.md`, THEN fleet SHALL print in the rules' place a
  line of its own saying the rules file could not be read and naming
  `assets/rules.md`.
  Today: it prints nothing in the rules' place (fleet-mkbn).
- **onboarding.prime.R25**: WHEN `fleet prime` looks for the seat, fleet SHALL
  take the seat whose row in `config.json` names the working directory itself
  as a worktree, and no seat for a directory under a worktree.
- **onboarding.prime.R26**: fleet SHALL find the seat of `fleet prime` by R25
  alone, whatever `FLEET_ACTOR` or any other environment variable says.
- **onboarding.prime.R27**: WHEN R25 finds a seat, fleet SHALL print after the
  rules one line per item the store holds assigned to that seat's full id
  whose status is `open` or `in_progress`, in the order the store lists them,
  each starting `item:` and giving that item's id and title.
- **onboarding.prime.R28**: WHEN R25 finds a seat that has no such item,
  fleet SHALL print the one line `item: none`.
- **onboarding.prime.R29**: IF R25 finds a seat and the store cannot be
  opened or does not answer the listing, THEN fleet SHALL print the one line
  `item: could not be read —` followed by the reason, and never
  `item: none`.
- **onboarding.prime.R30**: WHEN no row in `config.json` names the working
  directory as a worktree, fleet SHALL print no item line.
- ~~**onboarding.prime.R31**~~ retired 2026-10-13: it printed an item line
  for an unreadable `config.json` in an embedded fleet. Every verb that acts
  in a project now reads `config.json` to find its fleet, a default fleet's
  among them, and exits 3 where it will not read (shared.locations.R38)
  (fleet-1s1x).
- **onboarding.prime.R32**: fleet SHALL print the same lines from
  `fleet prime` whether or not a controller is running.
- **onboarding.prime.R33**: IF `fleet prime` cannot read its working
  directory, THEN fleet SHALL say on a line on standard error
  (shared.exit-codes.R12, shared.exit-codes.R13) that the working directory
  could not be read, print nothing on standard output, and exit 3
  (shared.exit-codes.R4).
  Today: prime goes on as though the walk found neither file there, outside
  every checkout, and refuses with exit 1 on the no-fleet line naming an
  empty directory (fleet-fd5g).
- **onboarding.prime.R34**: WHEN `fleet prime` names a guard class `on` or
  `off` (R13, R14), fleet SHALL name it as `fleet guard` run in the same
  directory treats that class.
- ~~**onboarding.prime.R35**~~ retired 2026-10-12: restated
  shared.locations.R32
- **onboarding.prime.R36**: WHEN `fleet prime` prints a reason on a line
  (onboarding.prime.R7, R12, R15, R21, R29), fleet SHALL fold any line
  break the reason carries, so that the reason stays on that one line and
  the store line is always the second line.
- **onboarding.prime.R37**: IF shared.locations.R21 stops at a wingman
  directory (shared.locations.R39), the working directory is inside no git
  checkout, and the wingman directory holds more than one project file, THEN
  fleet SHALL refuse `fleet prime` as shared.locations.R25 says, exit 1
  (shared.exit-codes.R2, shared.exit-codes.R23), on a line saying to run it
  in a project's checkout, and ask nothing, whatever standard input is.
  Today: fleet knows no wingman directory (fleet-1s1x.5).
  Today: a reason that carries line breaks, such as a pack manifest's TOML
  parse error, spreads its line over several, and the store line is no
  longer the second (fleet-373f).

## Non-functional

- **onboarding.prime.N1**: IF the project's store does not answer its version
  within 2 seconds, THEN fleet SHALL make the store line read
  `store: could not be read —` with a reason naming the 2-second bound.
- **onboarding.prime.N2**: IF the project's store does not answer the seat's
  listing within 5 seconds, THEN fleet SHALL print
  `item: could not be read —` with a reason naming the 5-second bound.
- **onboarding.prime.N3**: fleet SHALL create no file and no directory, and
  append nothing to `events.jsonl`, during `fleet prime`
  (shared.locations.N1 covers finding the fleet).

## Interfaces

- **The claude-code pack's session-start hook** runs `fleet prime` through
  the plugin's `bin/fleet`, which runs the binary `FLEET_BIN` names and no
  other. fleet sets `FLEET_BIN` on every seat's session it starts; the seats
  area's spec says so. Where `FLEET_BIN` is unset, not absolute or not an
  executable file, the plugin runs no fleet and exits 127 itself, and the
  session starts without its context: that is the pack's behaviour, not
  fleet's.
- **What the hook relies on:** standard output is the session's opening
  context (R4); the exit is 0 wherever prime finds its fleet (R1), so a
  session in a fleet never fails to start on fleet, a hook that closes
  standard output early included (shared.exit-codes.R27); where it finds
  none, prime refuses on standard error (R6), prints no context, and the
  hook sees exit 1, as it does where prime cannot resolve the machine
  directory (shared.locations.R30 to shared.locations.R32); where it cannot
  tell which fleet it acts in (R6, under shared.locations.R38) or cannot read
  its working directory (R33), the hook sees exit 3; nothing is read from
  standard input (R3); and the two store calls are bounded (N1, N2), so the hook
  waits at most those two bounds on the store.
- **The store adapter** is asked two calls of the store contract: `version`
  (R17, N1) and `list` filtered to the seat as assignee (R27, N2). The bd
  pack's adapter answers them with `bd version` and
  `bd list --assignee=<seat id> --all --json -n 0`; that argv is the pack's,
  not fleet's. How an adapter name resolves to an adapter is the store
  area's spec.
- **The packs area** states the order the layers resolve in and how a file
  path resolves to the highest layer that carries it (R9, R22). **The guards
  area** states which classes the layers declare and how the policy file
  switches one (R13, R14).
- **The seats area** writes the rows of `config.json` (shared.locations.R11)
  that R25 reads. A seat's session is handed the controller's machine
  directory (shared.locations.R29), so prime in a seat's session reads the
  `config.json` and packs the controller does.
- `fleet guard` reads the guard switches off the same policy file R5 finds,
  so the two verbs agree on which classes are on (R34).

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
