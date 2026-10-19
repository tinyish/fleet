# tinyfleet core architecture

The backbone every capability stands on: what runs, where state lives, how
tinyfleet reaches the tools it runs on, and the rules the code keeps. Drawn
for onboarding; later capabilities appear only as directional slots.

> **Directional.** Nothing below is built yet. Every part is directional
> until onboarding lands; the commit that makes a part real drops its mark.

## Components and lifetimes

```
 person or script
      │  fleet <verb>
      ▼
 ┌───────────┐  local socket  ┌───────────────────────┐   transactions  ┌────────────────┐
 │ fleet CLI │ ─────────────▶ │ supervisor (one/user) │ ──────────────▶ │ state (SQLite) │
 └───────────┘                └───────────────────────┘                 └────────────────┘
   │                              │                │
   │ service manager              │ multiplexer    │ git
   │ (start/stop the supervisor)  │                │
   ▼                              ▼                ▼
 launchd / systemd --user     tmux server on       the repos a fleet serves
 runs the supervisor          tinyfleet's socket
                              └─ agents' sessions

                              agent adapter · store adapter   (directional: defined later)
```

| Part | Does | Lives |
| --- | --- | --- |
| **fleet CLI** | Turns a command into a request to the supervisor and reports the answer. Writes nothing, except starting the supervisor through the service manager. | one command |
| **supervisor** | Replies with facts (a `protocol::Reply`: a `Response` or a `Status`), never sentences. The only writer of anything tinyfleet writes: registers fleets, writes their config file, turns projects on and off, launches and watches agents' sessions *(directional: arrives with the agent adapter)*, appends the record of every step. | from the supervisor's start to its stop; the service manager restarts it if it crashes, and it adopts the sessions still running |
| **state** | One SQLite file in the per-user state directory: the fleet registry, each project's on or off, its agents, the record of every step. | forever |
| **fleet directory** | Anywhere; holds only the person's config file. By default the project itself, left uncommitted; or a directory outside the repos, serving one or many. | the person's |
| **tmux server** *(directional: arrives with the agent adapter)* | Holds agents' sessions on tinyfleet's own socket, never the person's. The person can attach to watch an agent. | outlives the supervisor and any fleet's stop |

**When the supervisor is down,** a command that needs it (create, and later
commands) asks at a terminal whether to start it. With `--non-interactive`,
for CI or a script, it never asks: wherever it would, it takes the answer that
keeps going, so it starts the supervisor and says so. With neither, it refuses
with its own outcome, *supervisor not running*. Doctor never starts it: it
reports it as not running.

**Start and stop act on everything by default.** On and off belong to each
project, not to its fleet. Start brings the supervisor up, re-reads every
fleet's config and turns every project on; stop turns every project off and
brings the supervisor down. Acting on one project only is off the default
path, asked for with `--project`, one project at a time: in a wingman fleet,
stopping one project leaves the others as they were. A fleet whose config does
not read stays off, named with why, and the others still come on. Once
started, the supervisor comes back after a logout or a reboot, and every
project with it as it was, until stop. Once agents arrive with the agent
adapter (directional), stopping leaves agents' sessions running and starting
adopts the sessions still there.

## Outside tools

The core reaches every outside tool through a **tool interface**: a trait
defined in `core`, with one **driver** now, in its own crate, that runs the
real tool. The core never runs a tool itself.

| Tool interface | Driver now | Used by |
| --- | --- | --- |
| service manager | launchd (macOS), systemd user units (Linux) | CLI, to start and stop the supervisor; doctor |
| multiplexer | tmux, on tinyfleet's own socket *(directional: arrives with the agent adapter)* | supervisor; doctor |
| git | git | doctor; the supervisor *(directional: when work lands)* |
| agent | *directional: reached through an adapter, defined later* | supervisor |
| store | *directional: reached through an adapter, defined later* | supervisor |

**Adapters** *(directional: the adapter contract is defined later)*. Agents
and stores are reached through adapters, and users can write their own: for
agents such as Codex, Pi or Claude Code, for stores such as beads, Linear,
Jira or SQLite.

## Crates and dependency rules

One binary, `fleet`, plays both roles: the CLI, and the supervisor when the
service manager runs it as `fleet supervise`. Installing tinyfleet is one
file.

```
                ┌──────── fleet (bin) ────────┐
                │                              │
               cli                         supervisor ──── state
                │                              │
                └────────────► core ◄──────────┘
             rules + protocol + tool interfaces
                                ▲
                ┌───────────┬───┴───────┬────────────┐
             service       tmux        git     (agent, store: directional)
```

| Crate | Holds |
| --- | --- |
| `fleet` | the binary: picks the CLI or the supervisor role |
| `cli` | parsing, prompting, rendering outcomes, the socket client |
| `supervisor` | the socket server, and tending each project that is on |
| `core` (package `fleet-core`: `core` is Rust's own) | the rules (what a fleet is, when a create is refused, a fleet's lifecycle), the protocol the CLI and the supervisor speak (`core::protocol`: the requests and replies, and how they go on a stream), and the tool interfaces; no I/O except through them, and the protocol reads and writes only the stream it is handed |
| `state` | the SQLite schema and its queries |
| `service`, `tmux`, `git` | the drivers |

Fitness tests fail the build when:

- any crate but `supervisor` depends on `state` (so the CLI cannot write state);
- `core` depends on another crate of the workspace;
- a driver depends on anything in the workspace but `core`;
- any crate but a driver starts an outside program.

## How a command flows

**create**, in a directory: the CLI sends the directory, and any `--project`
paths, to the supervisor. With no `--project` the directory must be a git
repository's root and is the fleet's one project: a default fleet. With
`--project` it is a wingman fleet, outside the repositories it serves. The
supervisor refuses a project that is not a repository's root, a wingman fleet
inside one of its own projects, a fleet in a directory above or below another
fleet's, a project another fleet already serves, and a config file already in
the directory that no fleet owns. The same fleet asked for again is done,
saying what it found. Otherwise, in one transaction, it registers the fleet,
writes the config file without overwriting anything, records the step and
answers; the new fleet's projects come up on. Nothing is committed.

**start / stop**, the default: start enables and loads the supervisor's unit
through the service manager, waits for the supervisor to answer on the
socket, then asks it to re-read every fleet's config and turn every project
on. Stop asks the supervisor to turn every project off, then stops and
disables the unit. Either says it could not
tell when the supervisor does not answer in time.

**start / stop** one project, off the default path (`--project`): start
brings the supervisor up if it is down, re-reads every fleet's config so a
project just added is found, and turns that one project on; stop turns it off.
The other projects of its fleet keep their own on or off. Re-reading never
changes an existing project's on or off: a project added comes in off, and
one removed from its config is dropped. A fleet whose directory no longer
exists is forgotten by the re-read, and start says so; one whose directory
cannot be looked at is not gone, and stays off.
*(Directional, with the agent adapter: on, it launches or adopts each agent's
session in tmux; off, it leaves the sessions alone.)*

**remove**: the CLI asks the supervisor to remove the fleet whose directory
is the current one, or, with `--project`, to take those projects out of a
wingman fleet. The request says what to do with the fleet's `fleet.toml`:
delete it, keep it, or ask. At a terminal it says ask. Then, when a whole
registered fleet would go and its `fleet.toml` is there, the supervisor
changes nothing and asks back; the CLI asks the person and sends the request
again with the answer. Anywhere else the file is kept. Under its lock the
supervisor removes the fleet, deleting its `fleet.toml` when told to, or edits
the wingman's `fleet.toml` in place without the taken projects. The file is
deleted or rewritten inside the same state write, so a file that cannot be
changed leaves the registry as it was. Taking every project removes the
fleet; a directory with no fleet is nothing to remove, and says so.

**doctor**: the CLI checks, and reports each as fine, missing or could not
tell: the service manager (it answers), git (on the `PATH`, its version), the supervisor (whether it
answers; whether its version matches this binary's; not running is a
warning, not a failure), the state (only the supervisor opens it, so it is
reported only through a running supervisor's hello) and the project the
current directory is in (its fleet's config reads; its projects exist;
nothing is said outside a fleet directory). It asks the drivers and the
supervisor, reads the fleet's config, and starts and writes nothing; it runs
even when the state directory cannot be worked out. It exits 1 when anything
is missing, otherwise 4 when anything could not be told, otherwise 0.

**Every command ends in one outcome:** done, refused (with a reason),
supervisor not running, or could not tell. A start that turned some projects
on but left a fleet off ends refused, saying both what came on and which
fleets stayed off and why. How each outcome reaches a script is the CLI
contract's (see Boundaries).

## Invariants

1. Only the supervisor writes state or any file in a fleet directory. The CLI's
   one write is the supervisor's service unit.
2. No onboarding step commits to a repository.
3. A directory holds at most one fleet, no fleet directory is inside
   another's, and a repository belongs to at most one fleet.
4. Stopping a fleet or the supervisor never ends an agent's session *(directional: arrives with the agent adapter)*.
5. Nothing waits for an answer when there is no terminal.
6. tinyfleet uses only its own tmux socket *(directional: arrives with the agent adapter)*.
7. Only drivers start outside programs.
8. Doctor changes nothing.
9. Every word a command shows is the CLI's. The supervisor and the drivers
   answer with facts (what happened, or why not), never with sentences; the
   CLI writes all of a command's wording and colour, help text included, in
   one module, `words`. The supervisor words only its own log, in one file of
   its own.
   How that works: [cli-output.md](cli-output.md).
10. Every change to state commits in one transaction with the steps that
    record it: the supervisor writes only inside `State::write`, so a change
    and its record never part, and a failure part way changes nothing.

## Where things are on disk

| What | Where |
| --- | --- |
| state, and the supervisor's socket | `$XDG_STATE_HOME/fleet/` (default `~/.local/state/fleet/`) on Linux; `~/Library/Application Support/fleet/` on macOS |
| a fleet's config file | `fleet.toml` in the fleet directory |
| the installed binary | `~/.local/bin/fleet`, or `FLEET_INSTALL_DIR` |
| the supervisor's service unit | `~/Library/LaunchAgents/` on macOS; `~/.config/systemd/user/` on Linux |
| tmux socket | tmux's socket named `fleet` (`tmux -L fleet`) |

## Boundaries

Each boundary's authoritative form lives in code; this table points at it.
None exists yet.

| Boundary | Between | Authoritative form |
| --- | --- | --- |
| CLI | person or script ↔ `fleet` | *directional:* the transcript evals in `tests/cmd/` |
| protocol | CLI ↔ supervisor: a `Request`, then a `Reply` (a `Response` or a `Status`) | *directional:* `crates/core/src/protocol.rs` |
| state schema | supervisor ↔ state | *directional:* `crates/state/` migrations |
| config file | person ↔ supervisor (and doctor, which reads it) | *directional:* `crates/core/src/config.rs`, read through `fleets::read_projects` |
| tool interfaces | core ↔ drivers | *directional:* `crates/core/src/tools/` |
| install | person ↔ the GitHub release | *directional:* `install.sh` at the repository root |
| release | a release squash on `main` ↔ the GitHub release | *directional:* `.github/workflows/release.yml` |

**Until tinyfleet's first release, the state schema stays at version 1.** No
state file exists outside development, so a schema change edits the one
version-1 migration in place rather than adding a version; after such a change,
delete your local state file (or point `FLEET_STATE_DIR` at a fresh
directory). This holds until Alberto says otherwise.

## Naming

Names at every boundary (the state schema's tables and columns, the
protocol's fields, the config file's keys) follow Google's API Improvement
Proposals, the AIPs at [google.aip.dev](https://google.aip.dev). The one that
comes up most is AIP-142: a point in time ends in `_time`, with its verb in
the imperative (`create_time`, never `created_at`), and a time stored as an
integer adds its unit as a final suffix, one of `seconds`, `millis`, `micros`
or `nanos` (`occur_time_millis`).

## Install

`install.sh`, at the repository root, is run as
`curl -fsSL https://raw.githubusercontent.com/tinyish/fleet/main/install.sh | sh`.
It downloads the prebuilt `fleet` for the machine's OS and CPU from the
GitHub release and checks it against its SHA-256 file. It then installs it to
`~/.local/bin` (`FLEET_INSTALL_DIR` overrides it) by writing a new file and
renaming it over the old one, so a running `fleet` is never half replaced.
It never asks anything and changes no shell file: when the directory is not
on the `PATH`, it prints the line to add. No Rust toolchain is needed, and
`FLEET_VERSION` pins a release.

**The release binaries** are built by the release workflow on a release
squash to `main`. There are four: macOS on arm64 and on x86_64, and Linux on
arm64 and on x86_64, statically linked against musl. Each is
`fleet-<target>.tar.gz` with its `.sha256` beside it. The names carry no
version, so the latest release's download links never change. They are built
without features, so none has the CLI's specimen: the workflow refuses a
release whose binary accepts `fleet specimen`.

## Toolchain

- **Rust**, stable, pinned in `rust-toolchain.toml`; one Cargo workspace,
  crates under `crates/`.
- The version is the workspace version in the root `Cargo.toml`: a release
  branch's first commit sets it, and the release workflow reads it there.
- **clippy** with a pinned lint set, warnings denied.
- clippy and the tests also run with `--all-features`, so the CLI's
  specimen ([cli-output.md](cli-output.md)) builds and stays complete.
- Visible evals are trycmd transcript cases under `tests/cmd/<verb>/`, run by
  `cargo test`.
- Fitness tests read the workspace's dependency graph (`cargo metadata`).
- The `Makefile` at the root wraps these commands; `make check` runs every
  gate.

## Terms

| Term | Means |
| --- | --- |
| fleet | a set of agents working on one or more repositories, defined by one fleet directory |
| fleet directory | the directory holding a fleet's config file |
| project | a git repository a fleet serves, named by its root; each has its own on or off |
| default fleet | a fleet whose directory is its one project's repository; its config file is left uncommitted there |
| wingman fleet | a fleet whose directory is outside the repositories it serves, so it leaves no trace in them |
| supervisor | the one long-running tinyfleet process per user |
| state | everything tinyfleet writes about its fleets, in one SQLite file |
| store | where task items live, reached through an adapter (defined later) |
| adapter | what tinyfleet talks to an agent or a store through; users can write their own (contract defined later) |
| session | a tmux session on tinyfleet's socket, running one agent |
| tool interface / driver | the trait in `core` for one kind of outside tool / the code behind it that runs the real tool |
| outcome | how a command ended: done, refused, supervisor not running, could not tell |
