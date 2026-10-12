# Agents working on fleet

Read `CONTRIBUTING.md` before your first item. It is how fleet is built, and
it binds you. The rules you are most likely to break:

- **Build from `brain/specs/` only.** A spec says what fleet must do.
  `docs/` is the user manual generated from the specs: never take intended
  behaviour from it, and never edit it outside the docs skill.
- **Declare the item's kind** (`behaviour`, `fix` or `internal`) and cite the
  clause ids it touches. A `behaviour` change edits the spec first.
- **Never guess past a spec.** If it is silent, ambiguous or contradicts
  itself, say so on the item and stop that part of the work.
- **Builders never open fleet-dtu**, the scenario repository, by any means.
  What you hear back from a scenario run is all you get.
- **Areas move over one at a time.** An area marked "not converted" in
  `CONTRIBUTING.md` still keeps the old rules: its docs are checked against
  the code, and its unit tests are its proof.
