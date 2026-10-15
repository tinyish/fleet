# Agents working on fleet

Read `CONTRIBUTING.md` before your first item. It is how fleet is built, and
it binds you. The rules you are most likely to break:

- **Two documents, two names.** A *feature spec*
  (`brain/specs/<area>/<feature>.md`) says what fleet does and why. An
  *implementation plan* lives in an item's bead (its approach in the `design`
  field, its steps in the body or child beads) and says how that one item
  changes fleet. Planners write implementation plans from feature specs only;
  builders build from the implementation plan in their bead.
- **superpowers' "spec" is not a feature spec.** The design superpowers
  writes is the first half of an implementation plan: it goes in the bead's
  `design` field, never in `brain/specs/` or `docs/`.
- **`docs/` is never an input.** It is the user manual generated from the
  feature specs: never take intended behaviour from it, and never edit it
  outside the docs skill.
- **Declare the item's kind** (`behaviour`, `fix`, `internal` or `spec`) and
  cite the clause ids it touches. A `behaviour` change edits the feature spec
  first. A `spec` item changes no code.
- **A feature spec's story and clauses match row by row.** Before you hand
  in a spec, check every table row and every example in its story against a
  clause. Each exit, bound, order, stream, line or command the story names is
  stated by a clause, with the same value.
- **Two triggers that can both hold: the broader clause says "except as <id>
  says".** When you add or change a clause, search its spec and every spec
  it cites for a clause whose trigger can hold at the same time.
- **Never guess past a feature spec or a plan.** If it is silent, ambiguous
  or contradicts itself, say so on the item and stop that part of the work.
- **Planners and builders never open fleet-dtu**, the scenario repository,
  by any means. What you hear back from a scenario run is all you get.
- **Areas move over one at a time.** An area marked "not converted" in
  `CONTRIBUTING.md` still keeps the old rules: its docs are checked against
  the code, and its unit tests are its proof.
