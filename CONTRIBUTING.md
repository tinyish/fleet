# Contributing to tinyfleet

How tinyfleet is built, and how work reaches `main`. Read all of it before
your first item, whether you are a person or an agent. The first half is the
method: the layered method, adopted 2026-10-15, which replaced the clause-id
spec method of 2026-10-12. The second half is branches, commits and releases.
The product is tinyfleet; its binary, crates and repository are named
`fleet`.

## How tinyfleet is built

Code is regenerable output. What lasts is the layers above it, each written at
the level it changes, so a rewrite can throw the code away and keep them:

| Layer | Holds | Changes | Lives in |
| --- | --- | --- | --- |
| PRD | the problem, who has it, journeys, what success looks like | rarely | `brain/prd/` |
| Architecture | the backbone: components, who talks to whom, boundaries, invariants | sometimes | `brain/architecture/overview.html`, pointing at contracts in code |
| Program design | how one change reshapes the code | every change | the item's bead |
| Evals | what any implementation must do: journeys and edge cases | grows with real use | fleet-dtu (hidden), `tests/cmd/` (visible) |
| Code | one implementation | freely | the crates |

Provenance, the why behind each layer, is deferred: beads is building it
(Memory Beads, Provenance V2). Until then commits keep their `Claude-Session:`
trailer and decisions stay in bead comments.

**People own intent; agents own code.** People write or approve the PRDs, the
overview and the journeys, and steer program design before code exists.
People never write code and never gate on it; they read it to learn. Planner
agents write program design from the PRD, the overview and the contracts.
Builder agents build from the program design. Neither ever sees a fleet-dtu
scenario.

**The loop.** A PRD's journey names what a person needs. An item declares its
kind and names the journey, boundary or eval case it touches. A planner writes
the program design into the item's bead, and Alberto steers it. A builder
writes the code. The visible evals run on every change; fleet-dtu runs the
journeys. The item lands.

**Where each truth lives:**

| Place | Holds | Written by |
| --- | --- | --- |
| `brain/prd/seed.md` | the product PRD: what tinyfleet is, who it is for, its capabilities, principles, success, non-goals | agents draft, Alberto approves |
| `brain/prd/<capability>.md` | one capability's PRD: its problem and journeys | agents draft, Alberto approves |
| `brain/architecture/overview.html` | the architecture: direction, the backbone, what makes it hard, boundaries, invariants, terms | agents draw, Alberto approves |
| the contracts in code | each boundary's authoritative form, listed in the overview's Boundaries table | builders, from program design |
| `tests/cmd/` | visible evals: edge cases and regressions at the CLI boundary | builders, from real use |
| fleet-dtu (sibling repository) | hidden evals: the journeys as scenarios, and the twins | agents draft, Alberto approves; planners and builders never see it |
| `docs/` | the user manual | the docs skill only; never a planner's or builder's input |
| `brain/metrics.md` | the code-health numbers, one row per release | `tools/metrics`, at each release's health pass |
| `brain/specs/` | the frozen clause-id specs: read-only material, archived after the pilot | nobody |
| beads | the work queue, and each item's program design | agents and people; a planner writes the program design |

## PRDs

**The product PRD** (`brain/prd/seed.md`) says what tinyfleet is, who it is
for, its capabilities, its principles, what success looks like and its
non-goals. Each capability's title is its PRD's heading and file name.

**A capability PRD** (`brain/prd/<capability>.md`) states the problem in the
user's terms, then its journeys. Each journey says what the person is trying
to do, what success looks like, and the verbs it passes through. Then what
success looks like for the capability, and its non-goals.

**The line rule.** A PRD line states a need or a success condition, and it
must survive a redesign of the CLI: no flags, exit codes, file names, output
lines or ordering. Those belong to the architecture's boundaries or to the
evals. Check a PRD with:

    grep -nE '(^|[^a-z])--[a-z]|exit [0-9]|`[^`]*\.(toml|json|md|lock)`' brain/prd/*.md

which prints nothing for a PRD that keeps the rule.

**A capability is a user problem, not a mechanism.** If its problem cannot be
stated without naming the mechanism, the mechanism is an architecture concept
serving another capability.

A PRD changes only when the problem does: a `behaviour` item that changes what
a journey promises edits the PRD first.

## Architecture

**The overview** (`brain/architecture/overview.html`) is one page, mostly
diagrams, made with the iterate-on-docs plugin's `make-a-project-overview` in
diagram-design's style and refined with Alberto through `iterate-on-a-document`.
It shows reality first; anything not yet true carries a visible
**directional** mark.

**It points, it never restates.** Each boundary in its table names the file
that holds the boundary's authoritative form: the clap definitions, the exit
enum, the `--json` envelope, the adapter schemas, the file formats. A complete
definition lives in code, decision history in git, task status on the board.

**It changes before the code it describes.** A change that moves a boundary or
an invariant updates the overview while it is planned, marked directional, so
Alberto sees the plan as a picture first. The commit that makes it real drops
the mark. A change that moves no boundary never touches the overview.

**It is written for people.** Builders meet the same backbone where an agent
obeys it: in the contracts, in the fitness tests that check the invariants,
and in `AGENTS.md`.

## Program design

Program design is the per-change plan superpowers' writing-plans skill writes:
types, signatures, call trees and file-tree diffs. It goes in the item's bead
(its approach in the `design` field, its steps in the body or child beads),
never in `brain/` or `docs/`. It is the level where Alberto reads and steers
how the code is shaped before any code exists. It is never a standing
document.

## Evals

The evals are the layer that remembers the edge cases: the code can be thrown
away, the evals are kept.

- **Journeys become fleet-dtu scenarios.** They are hidden from planners and
  builders and judge whether a journey succeeds end to end (§ Scenarios and
  fleet-dtu).
- **Edge cases and regressions become visible evals** at the CLI boundary:
  transcript cases under `tests/cmd/<verb>/`, run by `cargo test` on every
  change. They are black-box, so they survive a rewrite. *(Directional: the
  harness lands with fleet-20ti.8.)*
- **Unit tests are part of the code.** They test one implementation and may be
  regenerated with it. A unit test earns its place only for pure logic no eval
  reaches cheaply, such as parsers and the controller's decide step.
- **Edge cases enter from real use, never from reading a document.** A bug is
  fixed by first adding the eval case that fails on it.
- **Exact text where a change would matter, wildcards where it would not.** A
  case states only what a person or a script would notice if a rewrite changed
  it.

## Change kinds

Every item declares one kind, as a bead label, and names the journey, boundary
or eval case it touches:

| Kind | When | First edits | Evals |
| --- | --- | --- | --- |
| `behaviour` | what a journey or a boundary promises changes | the PRD or the overview (and the boundary's contract), before the code | added or updated for the changed promise |
| `fix` | the code breaks a promise that already exists | the eval case that fails on it, before the fix | the new case, and every other passes |
| `internal` | no promise moves: refactors, speed, dependencies, comments | the program design only | every eval passes unchanged |
| `spec` | a PRD or the overview is written or reworded to say what tinyfleet already does | the PRD or the overview; no code | added where a journey had none |

A fix that names no promise has found a gap in a PRD or the overview: it is a
`behaviour` item. An item mislabelled `internal` is caught when an eval fails.

## Scenarios and fleet-dtu

fleet-dtu is a sibling repository holding the scenarios
(`scenarios/<capability>/S-<capability>-<nnn>.md`), the twins and the runner.
**It is never mounted into a planner's or builder's seat.** Its own agents see
fleet's `brain/prd/`, `brain/architecture/` and the built binary, never fleet's
source, so scenarios test the journeys and not the implementation. Agents draft
scenarios from the journeys; Alberto approves each one.

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

**A scenario** is one Markdown file: a journey with checks.

    # S-delivery-007: a dispatched item survives a controller kill -9
    covers: delivery/hand-work-to-an-agent

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
LLM judges only output that is prose. `covers:` names the journeys the
scenario proves, as `<capability>/<journey>`.

**Running.** Each scenario gets its own `FLEET_DIR` and tmux server, so
scenarios run in parallel. Each runs three times and passes only if all three
pass, to catch races. A `behaviour` or `fix` item runs the scenarios covering
the journeys it names. An `internal` item runs every capability whose code it
touched. The full suite runs at least daily and at every release.

**What a builder hears back:** the verdict, the scenario's title, the
journeys it covers, and fleet's own output and event log from the run. Never
the scenario's text. A planner who finds a PRD or the overview contradicting
itself, or a builder who finds the program design contradicting them, says so
instead of guessing, and that becomes a `spec` item.

**The bridge.** `tools/scenarios` is fleet's only way into fleet-dtu. A
builder runs it with journeys or a capability. It builds fleet, has `dtu` run
the matching scenarios, and prints only what a builder hears back. With
`--coverage` it prints instead, for each journey, how many scenarios cover it
and the visible ones' titles. The gate and `tools/metrics` read that list.

fleet's Claude Code project settings deny every session in fleet's checkout
any read of fleet-dtu, and so every planner and builder seat and its
subagents. The file tools are denied by a `Read` deny rule. The shell is
denied by the sandbox's `denyRead`, with unsandboxed retries turned off. Only
the bridge is excluded from the sandbox. fleet-dtu's own settings deny its
sessions fleet's source the same way, leaving them `brain/prd/`,
`brain/architecture/` and the built binary.

**The holdout reserve.** About one scenario in five never reports to
builders: it runs only in the daily run and at release, whose results go to
Alberto. A failure there means the builders are being fitted to the visible
scenarios, and the journey or the suite needs work. A holdout says
`holdout: true` under its `covers:` line, and the bridge never runs it.

**The land gate** is `tools/gate <bead id>`, run by the reviewer before an
item is squash-landed. It:

- reads the item's kind label and the journeys, boundaries or eval cases it
  names;
- refuses an item with no kind, a named journey that no PRD holds, a
  `behaviour` item whose PRD or overview did not change in the item, a `fix`
  item that adds no eval case, or a `spec` item that changes code;
- stops an item that changes the bridge, the gate, the size list or the
  read-deny settings until Alberto approves it;
- runs the guards and the visible evals;
- runs the scenarios through the bridge: those covering the named journeys for
  `behaviour` and `fix` items, and those of every capability whose code it
  touched for an `internal` item. It refuses on any failure.

`fleet land` may call the same gate once fleet builds itself.

## Learning

**The bar:** every person on the project knows the overview: what each part
does, where its code lives, and how a request flows through it.

**Walkthroughs.** One sitting covers one capability. It starts from the
capability's PRD and the overview, and a trace of one real run read from
fleet's event log. From there it goes into the code itself, module by module,
as deep as the person wants. Reading code to learn is encouraged; what people
never do is write it or approve it. The sitting ends with the person
explaining the flow back, and its findings are filed as beads.

**When:**

- **Onboarding.** A new person walks every capability once.
- **After each release.** A short sitting covers each capability whose PRD or
  part of the overview moved, so familiarity keeps pace with the code.
- **Any time,** on request.

## Code health

- **Mechanical guards** are always on and fail the build:
  - fitness tests for the overview's invariants, generalising today's
    core-never-depends-on-controller test (`cli/tests/workspace.rs`); a
    crate edge or placement the overview does not allow fails them
    *(directional: fleet-20ti.9)*;
  - a pinned toolchain and clippy lint set, with warnings denied;
  - size budgets: at most 100 lines per function and 600 lines per file, not
    counting a file's inline tests. Code over budget when the guard landed is
    listed beside the guard at its size. A listed function or file may shrink
    but never grow, and leaves the list once it is within budget;
  - no dead code and no unused dependencies.

  Loosening a guard is a change Alberto approves, never a builder's call. That
  includes adding to the size list, and changing the scenario bridge, the land
  gate or a seat's read-deny (see § Scenarios and fleet-dtu).
- **Scheduled health passes** run at each release, before the docs pass.
  Agents look for duplication, dead code and simplification, security
  problems, and overview fidelity. Each finding is an `internal` item.
- **Walkthroughs** file what they find (see Learning). A part the overview
  cannot explain in one sentence is a finding.
- **Metrics, one row per release** in `brain/metrics.md`, printed by
  `python3 tools/metrics`:
  - build time, each test lane's tests and time, tests per crate;
  - lines per crate and the largest source file;
  - clippy allows and warnings;
  - journeys that have no scenario.

  They are read as trends: compare the new row with the ones above it and
  file a health item for what moved the wrong way. Thresholds that file items
  on their own are set once three rows exist. Duplication joins the table when
  the first health pass picks a tool for it.

## Moving to this method

tinyfleet moves to the layered method one capability at a time. This section
holds everything that is true only during the move; when the last capability
converts, it is deleted and nothing else in this file changes.

1. **The freeze.** `brain/specs/` is frozen as of 2026-10-15: nobody edits it,
   cites its clause ids in new work, or runs spec-cleanup sittings on it. Its
   area maps' `## Modules` lists are still read by `cli/tests/map.rs` until the
   archive.
2. **The pilot is onboarding, with shared** (epic fleet-20ti). Onboarding gets
   its PRD, its trycmd evals and its fleet-dtu journeys; shared's exit codes,
   output and locations become the overview's first boundaries and their
   contracts in code.
3. **The verdict.** The pilot passes when the onboarding PRD reads in one
   sitting and keeps the line rule, the overview's every boundary points at a
   contract or fitness test that exists, `fleet create`'s refusals are covered
   by visible evals, and `fleet create` regenerated from the layers alone
   passes them.
4. **The archive.** After the verdict, `brain/specs/` is archived. It stays
   readable as material for the capabilities still to convert.
5. **The rest**, one capability at a time, in the order below.

**Converting a capability looks at what tinyfleet does today, but writes down
only what is needed.** An agent drafts the PRD from the archived specs, the docs
and fleet's observable behaviour, keeping needs and dropping mechanisms, and
Alberto refines it. Edge cases are not copied from the archived specs; they
enter the evals from real use.

A capability not yet converted keeps the old rules: its docs are checked
against the code, and its unit tests are its proof. The land gate runs its
unit tests in place of its scenarios.

**Twins grow capability by capability.** A twin answers only the calls the
converted capabilities' scenarios make. While a twin is partial, its
faithfulness check runs only the checks for the calls it answers. The bd twin
must pass the whole `fleet store check` before Delivery converts. The claude
twin must pass the whole `fleet agent check` before Autonomy converts.

| Order | Capability | Status |
| --- | --- | --- |
| 1 | Onboarding | the pilot |
| 2 | Delivery | not converted |
| 3 | Autonomy | not converted |
| 4 | Visibility | not converted |
| 5 | Safety | not converted |
| 6 | Customization | not converted |
| 7 | Integrations | not converted |

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
