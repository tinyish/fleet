# Decisions

What was decided about tinyfleet, and why. Each decision has a key, its
heading, that items, docs and agents cite it by, such as
`decisions.md#<key>`. An entry is a short description: what was decided,
when, and why, in a few lines. It is not a long document; what it changes
lands in the PRD or the architecture doc, and the entry says where.

## shared-process

2026-11-01. tinyfleet moved onto Alberto's shared process as a
maintainable project (caught up on 2026-11-05, when the process traded
sizes for kinds and capabilities for an MVP with milestones).
CONTRIBUTING.md is gone: its method is the process's, and what is
tinyfleet's own moved to `AGENTS.md` (the old-code rule, harvest,
branches and releases), `architecture/testing.md` (evals, guards) and
`prd/mvp.md` (the milestone order); walkthroughs were dropped. The
capability PRDs stay as feature PRDs, and the MVP takes their P0s. Why:
one process for every project, so it never drifts between them. In:
`AGENTS.md`, `architecture/`, `prd/`.

## specs-in-brain

2026-11-01. Program design moves from a bead's design field to
`brain/spec/<feature>/`, from the next feature on; Delivery's in-flight
beads keep their design fields until it lands. The old-code specs were
renamed `brain/old-specs/` so they can't be mistaken for `brain/spec/`.
Why: the process keeps specs as files. In: `brain/old-specs/`.
