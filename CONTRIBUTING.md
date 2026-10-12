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
approves every change to it. Builder agents work from the specs alone and
never see a scenario. Reviewer agents still read code, against the
code-health rules; no person gates on it.

**The loop.** The seed PRD names the features. Each feature has a spec made of
numbered clauses. An item (a bead) declares its kind and cites the clauses it
touches. A builder writes the code. fleet-dtu runs the scenarios that cite
those clauses. The item lands. Health passes and map sittings feed internal
items back in.

**Where each truth lives:**

| Place | Holds | Written by |
| --- | --- | --- |
| `brain/specs/seed.md` | the seed PRD, one page: what fleet is, who it is for, its features, the limits it never crosses | agents draft, a person approves |
| `brain/specs/mod.md` | the map's overview: the crates, the areas, how the areas connect, which may depend on which | agents draft from the code, a person approves |
| `brain/specs/<area>/mod.md` | the area's part of the map: its feature specs, its modules and how work flows through them | agents draft from the code, a person approves |
| `brain/specs/<area>/<feature>.md` | what a feature does: the only input a builder implements from | agents draft, a person approves |
| fleet-dtu (sibling repository) | the scenarios and the twins: the proof | agents draft, a person approves; builders never see it |
| `docs/` | the user manual, generated from the specs | the docs skill only; never a builder's input |
| the rest of `brain/` | why fleet is shaped the way it is | Alberto |
| beads | the work queue | agents and people |

## Specs

**The seed PRD** (`brain/specs/seed.md`) is one page: what fleet is, who it
is for, the feature list with each feature's spec file, and the limits fleet
never crosses. It changes only when a feature is added or removed, and links
to the map rather than holding it.

**The specs folder mirrors the code.** Each area has its own folder, and
each folder's `mod.md` is its part of the architecture map, the way a Rust
`mod.rs` is a module's root. The areas started from the `docs/` areas and are
the folders under `brain/specs/`, listed in the table under § Moving to this
method.

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

**A feature spec** (`brain/specs/<area>/<feature>.md`) holds:

- **Behaviour clauses**, `<area>.<feature>.R<n>`: `WHEN … fleet SHALL …`.
- **Non-functional clauses**, `<area>.<feature>.N<n>`: recovery after a
  crash or `kill -9`, concurrency, timing, limits.
- **Interfaces**: what other areas and the adapters rely on.
- **Definition of Done**: every clause has at least one scenario.

A spec says what a user or a neighbouring component can observe, never how
the code does it. If a reader could only notice it by reading the code, it is
not in the spec. That keeps a spec as big as the product's surface, not as
big as the code.

**Zero clarification.** A builder must be able to implement a spec without
asking anything. An open question is written `[NEEDS CLARIFICATION: …]` and
blocks building the clauses it touches.

**Clause ids are never reused.** A removed clause stays in the file, marked
retired, so old items and scenarios that cite it still resolve.

## Change kinds

Every item declares one kind, as a bead label, and cites its clauses by id.
The land gate checks the kind:

| Kind | When | The spec | Scenarios |
| --- | --- | --- | --- |
| `behaviour` | something observable changes | changes first; the item cites the changed clauses | fleet-dtu adds or updates the scenarios for those clauses |
| `fix` | the code broke a clause that already exists | does not change; the item cites the broken clause | fleet-dtu adds a scenario that would have caught it |
| `internal` | comments, refactors, speed, dependencies | does not change | every scenario passes unchanged |

A fix with no clause to cite has found a gap in the spec: it is a
`behaviour` item. An item mislabelled `internal` is caught when its scenarios
fail.

## Scenarios and fleet-dtu

fleet-dtu is a sibling repository holding the scenarios
(`scenarios/<area>/S-<area>-<nnn>.md`), the twins and the runner. **It is
never mounted into a builder's seat.** Its own agents see fleet's `brain/specs/`
and the built binary, never fleet's source, so scenarios test the spec and
not the implementation. Agents draft scenarios from the specs; a person
approves each one.

**The twins** are fake `claude` and `bd` command-line tools. They sit behind
the real adapters from fleet-packs, with real git and real tmux on a private
server, so a scenario runs the whole stack a user runs. A twin is faithful
when `fleet agent check` or `fleet store check` passes against it exactly as
it does against the real tool. That check is re-run against the real tool
whenever its pin moves.

**A scenario** is one Markdown file: a user story with checks.

    # S-items-007: a dispatched item survives a controller kill -9
    covers: items.dispatch.R3, seats.resume.N2

    ## Given
    - pack tiny · bd twin: open item t-1
    - claude twin: idle → on prompt: commit one file, run `fleet deliver`

    ## When
    $ fleet dispatch t-1
    ! kill -9 controller
    $ fleet start

    ## Then
    - within 30s: t-1 is delivered
    - the events include seat.resumed for the same session
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
scenario's text. A builder who finds the spec contradicting itself says so
instead of guessing, and that becomes a spec item.

**The holdout reserve.** About one scenario in five never reports to
builders and runs only at release. A failure there means the builders are
being fitted to the visible scenarios, and the spec or the suite needs work.

## Unit tests

Unit tests stay in fleet. New ones are written only for pure logic the
scenarios cannot reach cheaply, such as parsers and the controller's decide
step. Once an area's scenarios are complete, each of its unit tests is
triaged. A test stays if it catches a break that no scenario catches (a
mutant that the scenarios let through) and that break is worth pinning.
Otherwise it goes.

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
    core-never-depends-on-controller test;
  - a pinned clippy lint set, with warnings denied;
  - size budgets per file and per function, the numbers set in the pilot;
  - no dead code and no unused dependencies.

  Loosening a guard is a change a person approves, never a builder's call.
- **Scheduled health passes** run at each release, before the docs pass.
  Agents look for duplication, dead code and simplification, security
  problems, and map fidelity. Each finding is an `internal` item.
- **Map sittings** file what they find (see Learning). A module the map
  cannot explain in one sentence is a finding.
- **Metrics in every release's notes:**
  - lines per crate and module, and the largest files;
  - duplication and the clippy allow count;
  - build time, scenario-suite time and the unit test count;
  - clauses that have no scenario.

  They are read as trends; a jump past a set threshold files a health item.

## Moving to this method

fleet moves to this method one area at a time.

1. **Phase 0.** Write `brain/specs/seed.md`, the overview
   `brain/specs/mod.md`, and every area's folder with its `mod.md`, so the
   whole map exists before any spec does. Fix what now points the wrong way
   (README's links to the archived `brain/fleet-layers.md`, the rules in
   `docs/README.md`), and measure today's suite as the baseline.
2. **Pick the pilot area** together, after the seed PRD is written.
3. **The pilot.** Its feature specs, fleet-dtu with the twins and its
   scenarios, and the guards, measured against the baseline.
4. **The rest**, one area at a time.

An area is **converted** when every clause has a scenario, its scenarios pass
in fleet-dtu, its docs are regenerated from its spec, and its unit tests are
triaged. Until then the area keeps the old rules: its docs are checked
against the code, and its unit tests are its proof.

| Area | Status |
| --- | --- |
| onboarding | not converted |
| packs | not converted |
| items | not converted |
| runs | not converted |
| seats | not converted |
| guards | not converted |
| status | not converted |
| store | not converted |
| agent | not converted |
| routines | not converted |
| shared | not converted |

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
   reserve, and record the release's metrics. Each finding is an `internal`
   item, landed on the branch or filed for the next release.
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
