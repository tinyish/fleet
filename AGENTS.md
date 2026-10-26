# Agents working on tinyfleet

Read `CONTRIBUTING.md` before your first item. It is how tinyfleet is built,
and it binds you. Read `brain/conventions.md` too, before you design or write
code: it holds the corrections Alberto has made once and wants never to make
again. The rules you are most likely to break:

- **Never look at the old code.** tinyfleet is being rebuilt from scratch.
  The implementation from before 2026-10-15 lives only on older branches and
  in git history. Never check it out, `git show` it, diff against it, search
  it, open a worktree of it, or send a subagent to, unless Alberto names in
  this conversation what to read. `brain/specs/` describes the old code too:
  read it only at a capability's harvest step, with Alberto.
- **Intent lives in layers, not in code.** A PRD (`brain/prd/`) says what a
  person needs. The architecture doc (`brain/architecture/core.md`) says how
  the system is shaped and points at the contracts in code. Program design, in an item's bead, says how that one item
  changes the code. Planners write program design from the PRD, the architecture
  doc and the contracts; builders build from the program design in their bead.
- **superpowers' "spec" is program design.** The design superpowers writes
  goes in the bead's `design` field, never in `brain/` or `docs/`.
- **Obey the contracts and the fitness tests.** The architecture doc is drawn
  for people; what binds your code is the contract each boundary names and the
  tests that check the invariants.
- **One piece at a time.** Alberto reviews each piece and tests it on his
  machine before the next one starts. Leave the piece as unstaged changes on
  the branch it lands on: no commit per task or per review fix, even when a
  skill says to commit. It becomes one commit when he says to land it.
- **Declare the item's kind** (`behaviour`, `fix`, `internal` or `spec`) and
  name the journey, boundary or eval case it touches. A `behaviour` change edits
  the PRD or the architecture doc first. A `fix` adds its failing eval case
  first. A `spec` item changes no code.
- **A PRD line survives a CLI redesign.** No flags, exit codes, file names,
  output lines or ordering in `brain/prd/`; those belong to the boundaries or
  the evals.
- **Evals come after the build, then from real use.** A capability's evals
  are written once it is built, from its journeys, the invariants found while
  building it, and the spec clauses Alberto keeps. After that, never add an
  edge-case eval or a PRD line because a document or a sitting imagined it.
- **`docs/` is never an input.** It is the user manual: never take intended
  behaviour from it.
- **Never guess past a PRD, the architecture doc or a plan.** If one is silent,
  ambiguous or contradicts itself, say so on the item and stop that part of
  the work.
