# Contributing to tinyfleet

How tinyfleet is built, and how work reaches `main`. Read all of it before
your first item, whether you are a person or an agent. The first half is the
method; the second half is branches, commits and releases. The product is
tinyfleet; its binary, crates and repository are named `fleet`.

tinyfleet is being built again from scratch, as a greenfield project, one
capability at a time. The implementation that existed until 2026-10-15 was
deleted from this line of history, and nobody building the new one reads it
(§ The old code).

## The old code

The old implementation (its crates, its tools and its user manual) survives
only on older branches and in git history: `main`, `release/0.1.0`,
`release/0.2.0` and every other branch or commit from before the deletion,
and any worktree checked out from one.

**Agents never look at it** unless Alberto names, in that conversation, what
to read. That means no checkout, `git show`, `git log -p`, diff, blame,
worktree, or search of an older branch or commit, and no subagent sent to do
it. Alberto's permission covers what he names and only for that
conversation.

The same holds for `brain/specs/`: it describes the old code, so it is read
only at a capability's harvest step (§ Building a capability).

**Why:** the old code was leaking into how the new one was being designed. A
design that starts from the PRD and the architecture doc, and from nothing
older, is the one the rewrite is for.

## How tinyfleet is built

Code is regenerable output. What lasts is the layers above it, each written at
the level it changes, so a rewrite can throw the code away and keep them:

| Layer | Holds | Changes | Lives in |
| --- | --- | --- | --- |
| PRD | the problem, who has it, journeys, what success looks like | rarely | `brain/prd/` |
| Architecture | the backbone: components, who talks to whom, boundaries, invariants | sometimes | `brain/architecture/`, once one is needed, pointing at contracts in code |
| Program design | how one change reshapes the code | every change | the item's bead |
| Evals | what any implementation must do: journeys, invariants and edge cases | grows with real use | `tests/cmd/` (visible), fleet-dtu (hidden) |
| Code | one implementation | freely | the crates |

Provenance, the why behind each layer, is deferred: beads is building it
(Memory Beads, Provenance V2). Until then commits keep their `Claude-Session:`
trailer and decisions stay in bead comments.

**People own intent; agents own code.** People write or approve the PRDs, the
architecture doc and the journeys, and steer program design before code exists.
People never write code; they read it to learn. Planner agents write program
design from the PRD, the architecture doc and the contracts. Builder agents
build from the program design. Neither ever sees a fleet-dtu scenario.

**Alberto reviews and tests every piece.** Nothing moves on to the next piece
until Alberto has reviewed the one before it and tested it on his machine.

**Where each truth lives:**

| Place | Holds | Written by |
| --- | --- | --- |
| `brain/prd/seed.md` | the product PRD: what tinyfleet is, who it is for, its capabilities, principles, success, non-goals | agents draft, Alberto approves |
| `brain/prd/<capability>.md` | one capability's PRD: its problem and journeys | agents draft, Alberto approves |
| `brain/architecture/` | the architecture doc, once a capability needs one: the backbone, what makes it hard, boundaries, invariants, terms | agents draw, Alberto approves |
| the contracts in code | each boundary's authoritative form, listed in the architecture doc's Boundaries table | builders, from program design |
| `tests/cmd/` | visible evals at the CLI boundary | builders, from the journeys, the invariants and real use |
| fleet-dtu (sibling repository) | hidden evals: the journeys as scenarios | agents draft, Alberto approves; planners and builders never see it |
| `docs/` | the user manual, written again as capabilities are built; never a planner's or builder's input | not decided yet |
| `brain/specs/` | the old clause-id specs, read only at a capability's harvest step and deleted after it | nobody |
| beads | the work queue, and each item's program design | agents and people; a planner writes the program design |

## Building a capability

Capabilities are built one at a time, in the order below. Each goes through
the same loop:

1. **Start from its PRD.** If the capability's shape needs one, draw its part
   of the architecture first, and Alberto approves it.
2. **Build it piece by piece.** Each piece is an item with its program design
   on the bead. A builder writes the code.
3. **Alberto reviews each piece and tests it on this machine** before the
   next one starts.
4. **Write the evals** once the capability is built: from its journeys, and
   from the invariants found while building it (§ Evals).
5. **Harvest the old specs.** Read the capability's files in `brain/specs/`
   with Alberto. A clause worth keeping becomes an eval; nothing else carries
   over. Then delete the spec files.
6. **Move on** to the next capability.

| Order | Capability | Status |
| --- | --- | --- |
| 1 | Onboarding | building: install tinyfleet and create a new project first |
| 2 | Delivery | not started |
| 3 | Autonomy | not started |
| 4 | Visibility | not started |
| 5 | Safety | not started |
| 6 | Customization | not started |
| 7 | Integrations | not started |

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

**The architecture doc** lives in `brain/architecture/`. None exists yet: it
is drawn when the first capability's shape needs one, and Alberto approves
it. It is one page, mostly diagrams, made with the iterate-on-docs plugin's
`make-a-project-overview` in diagram-design's style and refined with Alberto
through `iterate-on-a-document`. It shows reality first; anything not yet true
carries a visible **directional** mark.

**It points, it never restates.** Each boundary in its table names the file
that holds the boundary's authoritative form. A complete definition lives in
code, decision history in git, task status on the board.

**It changes before the code it describes.** A change that moves a boundary or
an invariant updates the architecture doc while it is planned, marked
directional, so Alberto sees the plan as a picture first. The commit that
makes it real drops the mark. A change that moves no boundary never touches
it.

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

The evals are the layer that remembers what any implementation must do: the
code can be thrown away, the evals are kept.

- **A capability's evals are written once it is built**, from three sources:
  its journeys, the invariants found while building it, and the old spec
  clauses Alberto keeps at the harvest step.
- **What one command can show is a visible eval**: a transcript case under
  `tests/cmd/<verb>/` at the CLI boundary, run by `cargo test` on every
  change. A script with no terminal never waiting, or a second setup never
  nesting a fleet, is one command's result.
- **A journey that needs several steps or faults is a fleet-dtu scenario**,
  hidden from planners and builders (§ Scenarios and fleet-dtu).
- **After that, edge cases enter from real use**, never from reading a
  document. A bug is fixed by first adding the eval case that fails on it.
- **Unit tests are part of the code.** They test one implementation and may be
  regenerated with it. A unit test earns its place only for pure logic no eval
  reaches cheaply, such as a parser.
- **Exact text where a change would matter, wildcards where it would not.** A
  case states only what a person or a script would notice if a rewrite changed
  it.

## Change kinds

Every item declares one kind, as a bead label, and names the journey, boundary
or eval case it touches:

| Kind | When | First edits | Evals |
| --- | --- | --- | --- |
| `behaviour` | what a journey or a boundary promises changes | the PRD or the architecture doc (and the boundary's contract), before the code | added or updated for the changed promise |
| `fix` | the code breaks a promise that already exists | the eval case that fails on it, before the fix | the new case, and every other passes |
| `internal` | no promise moves: refactors, speed, dependencies, comments | the program design only | every eval passes unchanged |
| `spec` | a PRD or the architecture doc is written or reworded to say what tinyfleet already does | the PRD or the architecture doc; no code | added where a journey had none |

A fix that names no promise has found a gap in a PRD or the architecture doc: it
is a `behaviour` item. An item mislabelled `internal` is caught when an eval
fails.

## Scenarios and fleet-dtu

*(Directional: fleet-dtu is not built yet. It is built when the first
capability's multi-step journeys need it.)*

fleet-dtu is a sibling repository holding the scenarios
(`scenarios/<capability>/S-<capability>-<nnn>.md`), the fakes of the tools
tinyfleet runs on, and the runner. **It is never mounted into a planner's or
builder's seat.** Its own agents see fleet's `brain/prd/`,
`brain/architecture/` and the built binary, never fleet's source, so
scenarios test the journeys and not the implementation. Agents draft
scenarios from the journeys; Alberto approves each one.

**The fakes** stand in for the agent, the work tracker and the service
manager, with real git and a private terminal multiplexer, so a scenario runs
the whole stack a user runs. A fake is faithful when it answers as the real
tool does at its pinned release. Nothing a scenario does reaches the real
service manager, terminal sessions or home directory.

**A scenario** is one Markdown file: a journey with checks. `$` lines are
tinyfleet commands, `!` lines are faults, and every check under `Then` is
deterministic: exit codes, files, commits, recorded events. An LLM judges
only output that is prose. A `covers:` line names the journeys the scenario
proves, as `<capability>/<journey>`.

**Running.** Each scenario runs in its own sandbox, so scenarios run in
parallel. Each runs three times and passes only if all three pass, to catch
races. A `behaviour` or `fix` item runs the scenarios covering the journeys it
names. The full suite runs at least daily and at every release.

**What a builder hears back:** the verdict, the scenario's title, the journeys
it covers, and fleet's own output from the run. Never the scenario's text. A
planner who finds a PRD or the architecture doc contradicting itself, or a
builder who finds the program design contradicting them, says so instead of
guessing, and that becomes a `spec` item.

**The read-deny.** fleet's Claude Code project settings deny every session in
fleet's checkout, and its subagents, any read of fleet-dtu. fleet-dtu's own
settings deny its sessions fleet's source the same way.

**The holdout reserve.** About one scenario in five never reports to
builders: it runs only in the daily run and at release, whose results go to
Alberto. A failure there means the builders are being fitted to the visible
scenarios, and the journey or the suite needs work.

## Learning

**The bar:** every person on the project knows the architecture doc: what each
part does, where its code lives, and how a request flows through it.

**Walkthroughs.** One sitting covers one capability. It starts from the
capability's PRD and the architecture doc, and a trace of one real run. From
there it goes into the code itself, module by module, as deep as the person
wants. Reading code to learn is encouraged; what people never do is write it.
The sitting ends with the person explaining the flow back, and its findings are
filed as beads.

**When:**

- **Onboarding.** A new person walks every capability once.
- **After each release.** A short sitting covers each capability whose PRD or
  part of the architecture doc moved, so familiarity keeps pace with the code.
- **Any time,** on request.

## Code health

- **Mechanical guards** are always on and fail the build:
  - fitness tests for the architecture doc's invariants: a crate edge or
    placement the architecture doc does not allow fails them;
  - a pinned toolchain and clippy lint set, with warnings denied;
  - size budgets: at most 100 lines per function and 600 lines per file, not
    counting a file's inline tests;
  - no dead code and no unused dependencies.

  Loosening a guard is a change Alberto approves, never a builder's call.
- **Scheduled health passes** run at each release, before the docs pass.
  Agents look for duplication, dead code and simplification, security
  problems, and the architecture doc's fidelity. Each finding is an
  `internal` item.
- **Walkthroughs** file what they find (see Learning). A part the architecture
  doc cannot explain in one sentence is a finding.

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

## Releasing

1. **The health pass.** When the branch's work is done, run the scheduled
   health passes (see Code health) and the full scenario suite with the
   holdout reserve. Each finding is an `internal` item, landed on the branch
   or filed for the next release.
2. **The docs pass.** Then bring `docs/` up to date with the items landed
   on the branch. It lands as the branch's last item, subject
   `<item id>: docs release pass for <x.y.z>`.
3. **Open the release PR** from `release/<x.y.z>` to `main`.
4. **Squash-merge it.** The squash commit's subject is `release: v<x.y.z>`.
5. **The release workflow runs on that push to `main`** *(directional: not
   written yet)*. It:
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
