Kind: maintainable

# The project

tinyfleet runs AI coding agents, such as Claude Code, on a solo builder's
own projects and machine, and is written in Rust. The product is tinyfleet;
its repository is named `fleet`. What it is and for whom lives in
`brain/prd/seed.md`; how it is set up and run, in `README.md`. How work is
done is the shared process, loaded in every session.

`brain/architecture/core.md` is kept in sync by hand, and changes before the
code it describes: a change that moves a boundary or an invariant updates it
while the change is planned, with a visible *directional* mark on what isn't
built yet, and the commit that makes it real drops the mark. A change that
moves no boundary or invariant never touches it.

# Rules of this project

- **Never look at the old code.** tinyfleet is being rebuilt from scratch.
  The implementation from before 2026-10-15 (its code, tools and user
  manual) lives only on older branches and in git history: `main`,
  `release/0.1.0`, `release/0.2.0`, and every branch or commit from before
  the deletion. Never check it out, `git show` it, `git log -p`, diff, blame
  or search it, open a worktree of it, or send a subagent to, unless Alberto
  names in this conversation what to read. Why: the old code was leaking into
  how the new one was designed.
- **The old specs are read only at harvest.** `brain/old-specs/` describes
  the old code. Once a milestone or a feature is built and its evals are
  written, read its files there with Alberto: a clause worth keeping
  becomes an eval, nothing else carries over, and the files are deleted.
- **One piece at a time.** The next piece starts only once Alberto has
  reviewed the one before and tested it on his machine.
- **Obey the contracts and the fitness tests.** `core.md` is drawn for
  people; what binds the code is the contract each boundary in its
  Boundaries table names, and the tests that check its invariants.
- **A PRD line survives a CLI redesign.** No flags, exit codes, file names,
  output lines or ordering in `brain/prd/`; those belong to the boundaries or
  the evals. Check with
  ``grep -nE '(^|[^a-z])--[a-z]|exit [0-9]|`[^`]*\.(toml|json|md|lock)`' brain/prd/*.md``,
  which prints nothing for a PRD that keeps the rule, on top of the
  process's own check.
- **Evals come after the build, then from real use.** A milestone's or a
  feature's evals are written once it is built
  (`brain/architecture/testing.md`). After that, never add an edge-case eval or a PRD line because a document or a
  sitting imagined it.
- **`docs/` is never an input.** It is the user manual: never take intended
  behaviour from it.
- **Loosening a guard is Alberto's call.** The lints, the size budgets and
  the fitness tests are never relaxed by a builder.

# Branches, commits and releases

- **Branches.** `main` takes only release and hotfix squashes and is never
  pushed to directly. `release/<x.y.z>` is cut from `main` before any work
  for that release starts, and its first commit sets the version to `x.y.z`
  (the workspace version in the root `Cargo.toml`). Work lands on the open
  release branch, one squash commit per item; only one release branch is
  open at a time, and work for a later version waits on its bead.
- **Releasing.** When the branch's work is done:
  1. The health pass: run every eval, and look for duplication, dead code
     and simplification, security problems, and `core.md`'s fidelity. Each
     finding is an `internal` item, landed on the branch or filed for the
     next release.
  2. The docs pass: bring `docs/` up to date with what the branch landed, as
     its last item, subject `<item id>: docs release pass for <x.y.z>`.
  3. Open the release PR from `release/<x.y.z>` to `main`, and squash-merge
     it with the subject `release: v<x.y.z>` (GitHub's ` (#N)` suffix may
     stay), keeping the body GitHub fills in: the release workflow reads the
     docs pass and the changelog from it.
  4. The release workflow (`.github/workflows/release.yml`) runs on that push
     to `main` *(directional: written, not yet run)*. It refuses if the tag
     already exists, if the version doesn't match the subject, or if the
     branch has no docs pass; then it builds the changelog from the item
     subjects, creates the tag and the GitHub release, and publishes `docs/`
     for that version (the renderer is not chosen yet). A release that failed
     part way is run again by hand, with the release commit's SHA.
  5. Delete the release branch, and cut the next one when work for it
     starts.
- **Hotfixes.** A fix that can't wait for the open release: cut
  `hotfix/<x.y.z+1>` from `main`, bump the patch version as its first commit,
  land the fix as an item squash, and squash-merge it into `main` as
  `release: v<x.y.z+1>`. Its docs pass still lands, saying so when nothing a
  user sees changed. Then cherry-pick the fix onto the open release branch.
