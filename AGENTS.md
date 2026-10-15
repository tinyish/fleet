# Agents working on tinyfleet

Read `CONTRIBUTING.md` before your first item. It is how tinyfleet is built,
and it binds you. The rules you are most likely to break:

- **Intent lives in layers, not in code.** A PRD (`brain/prd/`) says what a
  person needs. The overview (`brain/architecture/overview.html`) says how
  the system is shaped and points at the contracts in code. Program design,
  in an item's bead, says how that one item changes the code. Planners write
  program design from the PRD, the overview and the contracts; builders build
  from the program design in their bead.
- **superpowers' "spec" is program design.** The design superpowers writes
  goes in the bead's `design` field, never in `brain/` or `docs/`.
- **Obey the contracts and the fitness tests.** The overview is drawn for
  people; what binds your code is the contract each boundary names and the
  tests that check the invariants.
- **Declare the item's kind** (`behaviour`, `fix`, `internal` or `spec`) and
  name the journey, boundary or eval case it touches. A `behaviour` change
  edits the PRD or the overview first. A `fix` adds its failing eval case
  first. A `spec` item changes no code.
- **A PRD line survives a CLI redesign.** No flags, exit codes, file names,
  output lines or ordering in `brain/prd/`; those belong to the boundaries or
  the evals.
- **Edge cases come from real use.** Never add an edge-case eval, or a PRD
  line, because a document or a sitting imagined it. Journeys become scenarios;
  edge cases enter only from real use.
- **`docs/` is never an input.** It is the user manual: never take intended
  behaviour from it, and never edit it outside the docs skill.
- **`brain/specs/` is frozen material.** Never edit it or cite its clause
  ids, and never run `/spec-cleanup`.
- **Never guess past a PRD, the overview or a plan.** If one is silent,
  ambiguous or contradicts itself, say so on the item and stop that part of
  the work.
- **Planners and builders never open fleet-dtu**, the scenario repository,
  by any means. What you hear back from a scenario run is all you get.
- **Capabilities move over one at a time.** A capability marked "not
  converted" in `CONTRIBUTING.md` still keeps the old rules: its docs are
  checked against the code, and its unit tests are its proof.
