# Contributing to fleet

How fleet is built, and how work reaches `main`. Read all of it before your
first item, whether you are a person or an agent. The first half is the
method, adopted 2026-10-12. The second half is branches, commits and
releases.

## How fleet is built

Four problems decide whether fleet stays buildable for years, and the method
answers each one:

| Problem | Answer |
| --- | --- |
| Saying what fleet does | **Specs**: the source people read and review |
| Proving it does it | **Scenarios** run against fleet from a separate, blind repository |
| Knowing how it works | **Learning**: walkthroughs led by the architecture map |
| Keeping the code sound | **Code health**: guards, scheduled passes, metrics |

**People own intent; agents own code.** People review, and may write, the
seed PRD, the specs, the architecture map and the scenarios. People never
write code and never gate on it; they read it only to learn, in
walkthroughs. Agents draft most of the intent too, but a person
approves every change to it. Planner agents write implementation plans from
the feature specs, and builder agents build from those plans; neither ever
sees a scenario. Reviewer agents still read code, against the code-health
rules; no person gates on it.

**The loop.** The seed PRD names the features. Each feature has a feature
spec made of a story and numbered clauses. An item (a bead) declares its kind
and cites the clauses it touches. A planner writes the item's implementation
plan into its bead from those feature specs. A builder writes the code from
the plan. fleet-dtu runs the scenarios that cite those clauses. The item
lands. Health passes and map sittings feed internal items back in.

**Where each truth lives:**

| Place | Holds | Written by |
| --- | --- | --- |
| `brain/specs/seed.md` | the seed PRD, one page: what fleet is, who it is for, its features, the limits it never crosses | agents draft, a person approves |
| `brain/specs/mod.md` | the map's overview: the crates, the areas, how the areas connect, which may depend on which | agents draft from the code, a person approves |
| `brain/specs/<area>/mod.md` | the area's part of the map: its feature specs, its modules and how work flows through them | agents draft from the code, a person approves |
| `brain/specs/<area>/<feature>.md` | a feature spec: what a feature does and why, the contract every implementation plan is written from | agents draft, a person approves |
| fleet-dtu (sibling repository) | the scenarios and the twins: the proof | agents draft, a person approves; planners and builders never see it |
| `docs/` | the user manual, generated from the feature specs | the docs skill only; never a planner's or builder's input |
| `brain/metrics.md` | the code-health numbers, one row per release | `tools/metrics`, at each release's health pass |
| the rest of `brain/` | why fleet is shaped the way it is | Alberto |
| beads | the work queue, and each item's implementation plan | agents and people; a planner writes the plan |

## Specs

**The seed PRD** (`brain/specs/seed.md`) is one page: what fleet is, who it
is for, the feature list with each feature's spec file, and the limits fleet
never crosses. It changes only when a feature is added or removed, and links
to the map rather than holding it.

**The specs folder mirrors the code.** Each area has its own folder, and
each folder's `mod.md` is its part of the architecture map, the way a Rust
`mod.rs` is a module's root. The areas started from the `docs/` areas and are
the folders under `brain/specs/`, listed in the map's overview
(`brain/specs/mod.md` § Areas).

    brain/specs/
      seed.md            the seed PRD
      mod.md             the map's overview
      <area>/
        mod.md           the area's map
        <feature>.md     one feature's spec

**The architecture map** is the `mod.md` files:

- `brain/specs/mod.md` is the overview: the crates, the areas and the folder for
  each, how a request crosses from one area to the next, and which crate
  and area may depend on which (the dependency guard reads this).
- `brain/specs/<area>/mod.md` lists the area's feature specs, and every code
  module the area owns with one sentence each: what it does and what it
  depends on. It says how work flows through those modules.

The map changes when the architecture moves. A test, `cli/tests/map.rs`,
fails the build on drift: a module no area's map names, a module named
twice, or a name with no module.

**A feature spec** (`brain/specs/<area>/<feature>.md`) does four jobs:

- it is the contract a planner writes an implementation plan from;
- it is what a person reads to understand the feature and why it behaves as
  it does;
- it is the source the user docs are generated from;
- it is where people own intent.

Agents read and write most of it, and people review its changes. It holds,
in this order:

1. **The story**, for readers. It covers:
   - what the feature is for, and who meets it and when;
   - every flow a user walks, with its inputs and outputs;
   - its errors and edge cases;
   - the reason behind each choice.

   Use tables, lists and examples wherever they read faster than prose: an
   exit table, a file layout, the order of lookups. The story may repeat
   what the clauses say, in the form a person reads fastest.
2. **Behaviour clauses**, `<area>.<feature>.R<n>`, one EARS sentence each
   (below).
3. **Non-functional clauses**, `<area>.<feature>.N<n>`: recovery after a
   crash or `kill -9`, concurrency, timing, limits.
4. **Interfaces**: what other areas, the adapters and the operating system
   rely on.
5. **Open questions**: each `[NEEDS CLARIFICATION: …]`, or `None.`
6. **Definition of Done**: every clause has at least one scenario.

**Clauses are written in EARS**, the Easy Approach to Requirements Syntax.
It has five patterns, and they combine (`WHERE …, WHEN …, fleet SHALL …`):

| Pattern | Form | For |
| --- | --- | --- |
| Always | `fleet SHALL …` | what holds whatever happens |
| Event | `WHEN <trigger>, fleet SHALL …` | a command, a signal, an arriving event |
| State | `WHILE <state>, fleet SHALL …` | what holds as long as a state lasts |
| Unwanted | `IF <fault>, THEN fleet SHALL …` | a failure, a timeout, bad input |
| Optional | `WHERE <feature or platform>, fleet SHALL …` | `WHERE the platform is macOS` |

Each clause follows these rules:

- A clause states intended behaviour: what a person decided fleet does, not
  whatever the code happens to do. A spec never makes a bug look like design.
- A clause states the promise the behaviour serves, not the mechanism that
  keeps it: "two commands editing the same file never lose an edit", not
  which file gets a lock.
- One observable outcome per clause.
- Exact values: exit codes, event types, paths relative to the machine
  directory, bounds in seconds.
- A rule is stated by one clause. Every other clause, in this spec or
  another, cites that clause by its full id and never restates it.
- Where two clauses' triggers can both hold, the broader clause names the
  narrower one as its exception ("except as <id> says"), so every case meets
  exactly one clause.
- Only text a script or a person depends on is quoted, never a whole
  message.
- Where today's binary falls short of a clause, a line
  `Today: <what fleet does now> (<bead id>)` follows the clause, naming the
  `fix` or `behaviour` item that will close the gap. The item's spec change
  is deleting that line. Until then, the clause's scenarios are known
  failures, not regressions. The story lists the same gaps, in plain words,
  under a heading `### Known gaps`, so `grep -rn '^### Known gaps'
  brain/specs` finds every one.
- Every behaviour the story describes has a clause, and every clause appears
  in the story. An item that changes one changes the other. If they disagree,
  the clause is what fleet is held to, and the mismatch is a `spec` item.

A feature spec says what a user or a neighbouring component can observe,
never how the code does it. If a reader could only notice it by reading the
code, it is not in the feature spec. That keeps a feature spec as big as the
product's surface, not as big as the code. Where the code lives is the map's
job. How one item changes it is that item's implementation plan.

**A feature spec and an implementation plan are different documents.** Keep
the two names apart everywhere.

| | Feature spec | Implementation plan |
| --- | --- | --- |
| Says | what a feature does and why | how one item changes fleet: its approach and its steps |
| Lives | `brain/specs/<area>/<feature>.md`, for as long as the feature does | the item's bead: the approach in its `design` field (`bd update <id> --design-file <file>`), the steps in its body or child beads |
| Written by | agents draft, a person approves | a planner, with superpowers:brainstorming and superpowers:writing-plans |
| Read by | planners, people, the docs skill, fleet-dtu | the builder of that item |

superpowers calls the design it writes a "spec". In fleet that design is the
first half of an implementation plan, so it goes in the bead's `design`
field, never in `brain/specs/` or `docs/`.

**Zero clarification.** A planner must be able to write an implementation
plan from a feature spec without asking anything, and a builder must be able
to build from the plan without asking anything. An open question in a
feature spec is written `[NEEDS CLARIFICATION: …]` and blocks planning the
clauses it touches.

**Clause ids are never reused.** A removed clause stays in the file, marked
retired, so old items and scenarios that cite it still resolve.

## Change kinds

Every item declares one kind, as a bead label, and cites its clauses by id.
The land gate checks the kind:

| Kind | When | The feature spec | Scenarios |
| --- | --- | --- | --- |
| `behaviour` | something observable changes | changes first; the item cites the changed clauses, or deletes the `Today:` line a ruling left for it | fleet-dtu adds or updates the scenarios for those clauses |
| `fix` | the code broke a clause that already exists | does not change, except that the item deletes the `Today:` line naming it, if there is one; the item cites the broken clause | fleet-dtu adds a scenario that would have caught it |
| `internal` | comments, refactors, speed, dependencies | does not change | every scenario passes unchanged |
| `spec` | the feature spec is written or reworded to say what fleet already does | changes; the item cites the clauses it wrote or reworded | fleet-dtu adds or updates the scenarios for those clauses; the code does not change |

A fix with no clause to cite has found a gap in the feature spec: it is a
`behaviour` item. An item mislabelled `internal` is caught when its scenarios
fail.

## Scenarios and fleet-dtu

fleet-dtu is a sibling repository holding the scenarios
(`scenarios/<area>/S-<area>-<nnn>.md`), the twins and the runner. **It is
never mounted into a planner's or builder's seat.** Its own agents see
fleet's `brain/specs/` and the built binary, never fleet's source, so
scenarios test the feature specs and not the implementation. Agents draft
scenarios from the feature specs; a person approves each one.

**The twins** are fake `claude` and `bd` command-line tools. They sit behind
the real adapters from fleet-packs, with real git and real tmux on a private
server, so a scenario runs the whole stack a user runs. A twin is faithful
when `fleet agent check` or `fleet store check` passes against it exactly as
it does against the real tool. That check is re-run against the real tool
whenever its pin moves. Every answer a twin gives is recorded from the real
tool at its pin.

**How fleet-dtu is built.** It is a Rust workspace beside fleet
(`../fleet-dtu`). It has two binaries:

- `dtu`, the runner, which parses scenarios, builds each one's sandbox, runs
  it and checks it;
- one twin binary that answers as `claude`, `bd`, tmux or the service
  manager, depending on the name it is called by.

Scenarios run on the maintainer's Mac for now, and the full suite runs there
daily as a launchd job. Linux joins when fleet-dtu gets CI.

**The sandbox.** Each run of a scenario gets its own directory holding:

- its own `HOME` and `FLEET_DIR`, and a git project;
- a private tmux server: the tmux twin runs the real tmux on a socket inside
  the sandbox;
- a service-manager twin, named by `FLEET_SERVICE_BIN`. It runs the real
  `fleet observe` with the scenario's environment when fleet loads the
  service, and stops it with SIGTERM when fleet unloads it;
- the claude and bd twins, named by `FLEET_CLAUDE_BIN` and `FLEET_BD_BIN`;
- fleet-packs from a local checkout at the pinned tag, reached through the
  sandbox's git config, so nothing touches the network.

Nothing a scenario does reaches the real launchd, systemd, tmux server or
home directory.

**A scenario** is one Markdown file: a user story with checks.

    # S-items-007: a dispatched item survives a controller kill -9
    covers: items.dispatch.R3, seats.observe.N2

    ## Given
    - pack tiny · bd twin: open item t-1
    - claude twin: idle → on prompt: commit one file, run `fleet deliver`

    ## When
    $ fleet dispatch t-1
    ! kill -9 controller
    $ fleet start

    ## Then
    - within 30s: t-1 is delivered
    - the events include session.adopted for the same session
    - exactly one commit on the item's branch

`$` lines are fleet commands, `!` lines are faults, and every check under
`Then` is deterministic: exit codes, item states, events, commits, files. An
LLM judges only output that is prose.

**Running.** Each scenario gets its own `FLEET_DIR` and tmux server, so
scenarios run in parallel. Each runs three times and passes only if all three
pass, to catch races. A `behaviour` or `fix` item runs the scenarios citing
its clauses. An `internal` item runs every area whose modules it touched; the
map says which. The full suite runs at least daily and at every release.

**What a builder hears back:** the verdict, the scenario's title, the clauses
it cites, and fleet's own output and event log from the run. Never the
scenario's text. A planner who finds a feature spec contradicting itself,
or a builder who finds the plan contradicting the feature spec, says so
instead of guessing, and that becomes a `spec` item.

**The bridge.** `tools/scenarios` is fleet's only way into fleet-dtu. A
builder runs it with clause ids or an area. It builds fleet, has `dtu` run
the matching scenarios, and prints only what a builder hears back. With
`--coverage` it prints instead, for each clause id, how many scenarios cite it
and the visible ones' titles. The gate and `tools/metrics` read that list.

fleet's Claude Code project settings deny every session in fleet's checkout
any read of fleet-dtu, and so every planner and builder seat and its
subagents. The file
tools are denied by a `Read` deny rule. The shell is denied by the sandbox's
`denyRead`, with unsandboxed retries turned off. Only the bridge is excluded
from the sandbox. fleet-dtu's own settings deny its sessions fleet's source the
same way, leaving them `brain/specs/` and the built binary.

**The holdout reserve.** About one scenario in five never reports to
builders: it runs only in the daily run and at release, whose results go to
a person. A failure there means the builders are being fitted to the visible
scenarios, and the feature spec or the suite needs work. A holdout says
`holdout: true` under its `covers:` line, and the bridge never runs it.

**The land gate** is `tools/gate <bead id>`, run by the reviewer before an
item is squash-landed. It:

- reads the item's kind label and the clauses it cites;
- refuses an item with no kind, a cited clause that does not exist, a
  `behaviour` item whose spec did not change in the item, or a `spec` item
  that changes code;
- stops an item that changes the bridge, the gate, the size list or the
  read-deny settings until a person approves it;
- runs the guards;
- runs the scenarios through the bridge. That is the cited clauses'
  scenarios for `behaviour` and `fix` items, and those of every area whose
  modules it touched for an `internal` item. It refuses on any failure,
  except a known failure of a clause that still has a `Today:` line, and on a
  cited clause no scenario covers.

`fleet land` may call the same gate once fleet builds itself.

## Unit tests

Unit tests stay in fleet. New ones are written only for pure logic the
scenarios cannot reach cheaply, such as parsers and the controller's decide
step. A unit test earns its place only by catching a break that no scenario
catches, and that break must be worth pinning.

## Learning

**The bar:** every person on the project knows the architecture map: what
each area does, where its code lives, and how a request flows through it.

**Walkthroughs.** One sitting covers one area. It starts from the area's
folder, its `mod.md` and feature specs, and a trace of one real run
read from fleet's event log. From there it goes into the code itself, module by
module, as deep as the person wants. Reading code to learn is encouraged;
what people never do is write it or approve it. The sitting ends with the
person explaining the flow back, and its findings are filed as beads.

**When:**

- **Onboarding.** A new person walks every area once.
- **After each release.** A short sitting covers each area whose spec or map
  moved, so familiarity keeps pace with the code.
- **Any time,** on request.

## Code health

- **Mechanical guards** are always on and fail the build:
  - dependency rules from the map, generalising today's
    core-never-depends-on-controller test: a test reads § Allowed
    dependencies in `brain/specs/mod.md`, and a crate or area edge the map
    does not list fails it;
  - a pinned toolchain and clippy lint set, with warnings denied;
  - size budgets: at most 100 lines per function and 600 lines per file, not
    counting a file's inline tests. Code over budget when the guard landed is
    listed beside the guard at its size. A listed function or file may shrink
    but never grow, and leaves the list once it is within budget;
  - no dead code and no unused dependencies.

  Loosening a guard is a change a person approves, never a builder's call.
  That includes adding to the size list, and changing the scenario bridge, the
  land gate or a seat's read-deny (see § Scenarios and fleet-dtu).
- **Scheduled health passes** run at each release, before the docs pass.
  Agents look for duplication, dead code and simplification, security
  problems, and map fidelity. Each finding is an `internal` item.
- **Map sittings** file what they find (see Learning). A module the map
  cannot explain in one sentence is a finding.
- **Metrics, one row per release** in `brain/metrics.md`, printed by
  `python3 tools/metrics`:
  - build time, each test lane's tests and time, tests per crate;
  - lines per crate and the largest source file;
  - clippy allows and warnings;
  - clauses that have no scenario.

  They are read as trends: compare the new row with the ones above it and
  file a health item for what moved the wrong way. Thresholds that file items
  on their own are set once three rows exist. Duplication joins the table when
  the first health pass picks a tool for it.

## Moving to this method

fleet moves to this method one area at a time. This section holds
everything that is true only during the move. When the last area converts,
the whole section is deleted and nothing else in this file changes
(fleet-ptd9).

1. **Phase 0.** Write `brain/specs/seed.md`, the overview
   `brain/specs/mod.md`, and every area's folder with its `mod.md`, so the
   whole map exists before any spec does. Fix what now points the wrong way
   (README's links to the archived `brain/fleet-layers.md`, the rules in
   `docs/README.md`), and measure today's suite as the baseline.
2. **The pilot is onboarding.** Its feature specs come first, with `shared`'s
   exit codes, locations and output. Then come fleet-dtu with the twins, the
   bridge and the land gate, onboarding's scenarios, and the guards, all
   measured against the baseline.
3. **The rest**, one area at a time, in the order below.

**Areas convert in the order a person meets them** using fleet for the first
time: setting it up, adding a seat, working an item, watching it, being
refused by a guard, and only later customising packs, workflows and
routines or writing adapters. The specs then read as the path a new user
walks. `shared` is not converted on its own: each of its features is specced
with the first area that needs it (the exit codes, locations and output with
onboarding).

**Converting an area looks at what fleet does today, but writes down only
what was meant.** An agent drafts each feature spec from fleet's observable
behaviour: it runs fleet, and reads the area's docs and code. It writes only
what a user or a neighbouring component can observe, as the promise each
behaviour serves. It then sorts every clause by the evidence that the
behaviour was intended:

- **evidenced**: a code comment giving the reason, a `brain/` note, a test
  that asserts it on purpose, or a person's earlier ruling;
- **apparent**: consistent and plausible, but nothing says why;
- **doubtful**: looks accidental, inconsistent or wrong. It is written
  `[NEEDS CLARIFICATION: …]`, never kept silently.

The sorting, with each clause's evidence, goes in a review note on the spec
item's bead, never in the spec. The person confirms the apparent clauses,
rules on each doubtful one, and reads and edits the feature spec before it
is committed. A clause nobody can vouch for stays out. Where a ruling or a
found bug leaves today's binary short of a clause, the clause states the
intent and carries a `Today:` line naming the `behaviour` or `fix` item that
will close the gap.

An area is **converted** when every clause has a scenario, its scenarios pass
in fleet-dtu, its docs are regenerated from its feature specs, and its unit
tests are triaged. Until then the area keeps the old rules: its docs are checked
against the code, and its unit tests are its proof. The land gate then runs
the area's unit tests in place of its scenarios.

**Triaging an area's unit tests** happens once its scenarios are complete.
An agent does it from inside fleet, so it never sees a scenario's text. It
reads each of the area's unit tests against the area's specs and the titles
and clauses of its scenarios, from the bridge's coverage list. It decides
whether each test stays or goes by the rule in § Unit tests, and writes one
report giving the reason for every test it kept and every test it removed. A
person reads the report before the removals land.

**Twins grow area by area.** A twin answers only the calls the converted
areas' scenarios make. While a twin is partial, its faithfulness check runs
only the checks for the calls it answers. The bd twin must pass the whole
`fleet store check` before items converts. The claude twin must pass the
whole `fleet agent check` before seats converts.

| Order | Area | Status |
| --- | --- | --- |
| 1 | onboarding | not converted (the pilot) |
| 2 | seats | not converted |
| 3 | items | not converted |
| 4 | status | not converted |
| 5 | guards | not converted |
| 6 | packs | not converted |
| 7 | runs | not converted |
| 8 | routines | not converted |
| 9 | store | not converted |
| 10 | agent | not converted |
| — | shared | specced feature by feature, with the area that first needs each |

## Branching, commits and releases

Kept simple on purpose. Three kinds of branch, one commit shape, and one way
work reaches `main`.

## Branches

| Branch | Cut from | What lands on it | How it leaves |
| --- | --- | --- | --- |
| `main` | — | release squashes and hotfix squashes, nothing else | never |
| `release/<x.y.z>` | `main` | one squash commit per item, for that release | squash-merged into `main` |
| `hotfix/<x.y.z>` | `main` | the fix, as one or more item squashes | squash-merged into `main` |
| work branches (a seat's worktree, `worktree-agent-*`) | the open release branch | anything | squashed onto the release branch, then deleted |

- **Cut the release branch first.** Before any work for a release starts,
  `release/<x.y.z>` is cut from `main`, and its first commit sets the version
  in every crate's `Cargo.toml` to `x.y.z`.
- **Work lands on the release branch, never on `main`.** A builder's branch is
  cut from the open release branch, and the reviewer squash-lands it there.
- **One release branch open at a time.** Anything for a later version waits
  on its bead until the next release branch is cut.
- **Nobody pushes to `main` directly.** `main` changes only by the squash
  merge of a release or hotfix branch.

## Commits

Every commit on a release or hotfix branch is **one item, squashed**:

    <full item id>: <what changed, in the present tense>

    <why, and anything a reviewer needs; wrapped at 72>

    Co-Authored-By: …

- The subject starts with the item's **full id** (`fleet-cq6`, never `cq6`).
  The release changelog is built from these subjects, so write them for a
  reader of the changelog.
- One item per commit. No merge commits, no "fix review comments" commits:
  the squash hides the work branch's history.
- The item's own bead records the verdict and the landing; the commit does
  not repeat them.
- Board state (`.beads/`) is committed on the branch the work is on, as its
  own commit (`beads: …`).

## Supported versions

The releases of the tools fleet runs on that it supports are declared beside
what they date:

| Tool | Constant | Where it is declared | Checked by |
| --- | --- | --- | --- |
| fleet-packs | `PINNED_PACKS_SOURCE` and `PINNED_PACKS` (the tag) | `core/src/supported.rs` | `doctor/fleet-packs-version`, over every pack `packs.lock` pins from the source; `fleet create` installs the agent's pack and the store's pack at the tag |

Claude Code is a pack's, not the binary's: the claude-code pack
(`adapters/agent/claude-code` in the fleet-packs repository) pins the release
its adapter was measured against, declares it as `measured` in its
capabilities — which the controller's `substrate.moved` and
`fleet doctor agent-adapter` compare the installed one with — and carries
the `doctor/claude-code-version` check that measures it. A re-pin is that
repository's work: the pack moves, its lessons are re-measured beside its
fixtures, and it reaches this binary as a `PINNED_PACKS` move below. Deno is
a pack's too: the ts pack's `[runtime]` table (`runtimes/ts/pack.toml` in
the fleet-packs repository), measured by `doctor/runtime-version`. So is bd:
the bd pack pins it and carries the `doctor/bd-version` check that measures
it (`adapters/store/bd` in the fleet-packs repository), and a move of it is
that repository's. git carries no pin.

**A supported-version move is one constant plus a re-check**, as one item:

1. Change the constant, and the one copy of it in its doctor check's `run.sh`
   (`PINNED=`; fleet-packs' check also copies the source as `SOURCE=`). The
   core suite fails until the copies agree.
2. Re-check the packs at the new tag against this binary: `fleet store check
   --adapter` over each store adapter, `fleet agent check --adapter` over
   each agent adapter, and the `fleet create` suites. Change the code where a
   behaviour moved.
3. The version is also in the "What fleet runs on" table of
   `docs/getting-started.md`, which only the docs skill edits: ask for the
   edit on the item's bead, or the release pass catches it up.

## Releasing

1. **The health pass.** When the branch's work is done, run the scheduled
   health passes (see Code health), the full scenario suite with the holdout
   reserve, and append the row `python3 tools/metrics` prints to
   `brain/metrics.md`. Each finding is an `internal` item, landed on the
   branch or filed for the next release.
2. **The docs pass.** Then run the docs skill's
   release pass (`.claude/skills/docs/SKILL.md` § 8) over the items landed on
   the branch. It lands as the branch's last item, subject
   `<item id>: docs release pass for <x.y.z>`.
3. **Open the release PR** from `release/<x.y.z>` to `main`.
4. **Squash-merge it.** The squash commit's subject is `release: v<x.y.z>`.
5. **The release workflow runs on that push to `main`**
   (`.github/workflows/release.yml`). It:
   - reads the version from `Cargo.toml` and refuses if the tag `v<x.y.z>`
     already exists or the version does not match the merged branch's name;
   - refuses if the branch carries no docs release pass;
   - tags `v<x.y.z>`;
   - builds the changelog from the release branch's commit subjects, grouped
     by item, and creates the GitHub release with it;
   - publishes `docs/` for that version (the renderer is not chosen yet).
6. **Delete the release branch**, and cut the next one when work for it
   starts.

## Hotfixes

A fix that cannot wait for the open release:

1. Cut `hotfix/<x.y.z+1>` from `main`, bump the patch version as its first
   commit, and land the fix as an item squash.
2. Squash-merge it into `main` as `release: v<x.y.z+1>`. The release workflow
   runs the same way (a hotfix's docs pass may be a no-op when no behaviour a
   user sees has changed; it still lands, saying so).
3. Cherry-pick the fix onto the open release branch, so the next release does
   not undo it.
