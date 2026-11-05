# fleet

The repository of tinyfleet, which runs AI coding agents on your own projects.

tinyfleet is being built again from scratch, one milestone at a time. What it
must do, and how it is built, are written down rather than read out of the
code:

- `brain/prd/` holds the PRDs: `seed.md` for the product, `mvp.md` for
  the first release and its milestones, and one file per feature.
- `brain/architecture/core.md` is the architecture doc: how tinyfleet is
  shaped, in what language, and the rules its code keeps. Beside it are
  the conventions and how tinyfleet is tested.
- `AGENTS.md` holds tinyfleet's own rules, and its branches and releases.

The implementation from before 2026-10-15 is on older branches only, and
nobody building the new one reads it (`AGENTS.md`, "Never look at the old
code").
