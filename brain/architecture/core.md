# tinyfleet core architecture

The backbone every capability stands on: what runs, where state lives, how
tinyfleet reaches the tools it runs on, and the rules the code keeps.
Onboarding is built; Delivery's primitives are drawn ahead of their code.

> **Directional marks.** A part not built yet carries a *directional* mark;
> the commit that makes it real drops the mark.

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

                              adapter programs: agent · task · forge   (directional)
```

| Part | Does | Lives |
| --- | --- | --- |
| **fleet CLI** | Turns a command into a request to the supervisor and reports the answer. Writes nothing, except starting the supervisor through the service manager. | one command |
| **supervisor** | Replies with facts (a `protocol::Reply`: a `Response` or a `Status`), never sentences. The only writer of anything tinyfleet writes: registers fleets, writes their config file, turns projects on and off, launches and watches agents' sessions, keeps every seat filled, holds each message until its agent is idle *(directional: arrives with Delivery)*, appends the record of every step. | from the supervisor's start to its stop; the service manager restarts it if it crashes, and it adopts the sessions still running |
| **state** | One SQLite file in the per-user state directory: the fleet registry, each project's on or off, each fleet's roles and seats as its config last read, agents' sessions, the record of every step. | forever |
| **fleet directory** | Anywhere; holds only the person's config file. By default the project itself, left uncommitted; or a directory outside the repos, serving one or many. | the person's |
| **tmux server** | Holds agents' sessions on tinyfleet's own socket, never the person's. The person can attach to watch an agent. | outlives the supervisor and any fleet's stop |
| **adapter programs** *(directional for task trackers and forges)* | One program per kind of agent, task tracker or forge. An agent's adapter says how to launch it and reads what state it is in; a tracker's or a forge's reaches that tool. | one request: the supervisor runs it through a driver, and it exits with its answer |
| **workspaces** *(directional: arrive with Delivery)* | Git worktrees, each on its own branch, for work kept apart from a project's checkout. | under the state directory, until whoever made one removes it |

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
project with it as it was, until stop. Stopping leaves agents' sessions
running, and starting adopts the sessions still there. When start brings the
supervisor up, it writes the person's `PATH` into the supervisor's unit, so
the supervisor, and the agents it launches, find the tools the person's shell
finds; a running supervisor keeps the `PATH` it started with.

## Outside tools

The core reaches every outside tool through a **tool interface**: a trait
defined in `core`, with one **driver** now, in its own crate, that runs the
real tool. The core never runs a tool itself.

| Tool interface | Driver now | Used by |
| --- | --- | --- |
| service manager | launchd (macOS), systemd user units (Linux) | CLI, to start and stop the supervisor; doctor |
| multiplexer | tmux, on tinyfleet's own socket | supervisor; doctor *(directional)* |
| git | git | doctor; the supervisor *(directional: when work lands)* |
| agent | an adapter program per kind of agent, named by its path or as a release of fleet-adapters, fetched | supervisor |
| task | *directional:* an adapter program per kind of task tracker | supervisor |
| forge | *directional:* plain git by default; an adapter program for GitHub, GitLab | supervisor |
| network | curl, run by its driver | supervisor, the first time it runs a named adapter; doctor |

**Adapters** *(built for agents, fetching included; directional for task
trackers and forges)*. Agents, task trackers and forges are reached through
adapters: external programs that the supervisor runs through a driver, once
per request. Each run is handed one request and gives one answer, then exits,
so an adapter is never a process to keep alive, revive or adopt. An adapter
says what to run and what its tool's signals mean; tinyfleet runs it,
launching every session through the multiplexer itself.

tinyfleet's own adapters (for agents such as Claude Code, Codex and Pi, for
task trackers such as beads, and plain git as the default forge) live together
in a repository of their own, `tinyish/fleet-adapters`, one directory per
kind (`agents/`, `trackers/`, `forges/`), each released like fleet as
prebuilt binaries. They are not installed with fleet: the first time one
is run, the supervisor fetches the release for this machine, checks it
against its SHA-256, and keeps it under the state directory. (Named today
by hand with `fleet debug`, and by a fleet's config for its roles.) This is the one thing tinyfleet reaches
the network for. Users can write their own adapters, in any language, for
other agents, for trackers such as Linear or Jira, and for forges such as
GitHub or GitLab, and name them by their path.

**Naming and fetching an adapter**. An adapter is named by its
path, or as `<name>@<version>`, its version its agent's: `claude-code@2.1.285`
is the adapter measured against Claude Code 2.1.285, and `2.1.285-2` a fix
of the adapter's own. A bare name is refused, naming the latest release.
Before keeping a fetched adapter, the supervisor checks the release's
manifest (the protocol it speaks, a build for this machine, that build's
SHA-256) and asks the binary itself which protocol it speaks; an adapter
named by its path is asked before each launch. An adapter speaking a
protocol this fleet does not is refused, saying whether fleet or the adapter
needs updating.

**The adapter SDK.** Their protocol, and a kit for writing an adapter in
Rust, is the crate `fleet-adapter-sdk` in this repository. It has a version
of its own, and adapters, tinyfleet's own included, take it by the tag
`adapter-sdk-v<version>`.

## Primitives

> **Directional,** drawn for Delivery. Built: roles and seats, kept filled by
> the supervisor and shown by `fleet status`; a debug build's `fleet debug`
> launches and stops a session by hand. Nothing else here is built yet.

tinyfleet gives a person the building blocks of their own metaharness, and
nothing that carries an opinion. Roles, workflows, reactions (such as "CI
failed: send it back to the agent"), notifications and triggers are built on
top of these by users, through the supervisor's socket.

| Primitive | What it is |
| --- | --- |
| role | an entry in a fleet's config saying how to run an agent: its label, which agent adapter, which model, the prompt the agent is woken with, and the directory it works in; tinyfleet ships no roles |
| seat | a named entry in a fleet's config naming a role: an agent the supervisor keeps one session of running |
| session | one agent running in tmux, on tinyfleet's socket |
| message | text for one session, typed in once its agent is idle |
| observe | a session's state and its last reply: asked for, waited on, or subscribed to |
| workspace | a git worktree on its own branch, kept under the state directory |
| task | an item in the project's own tracker, reached through a task adapter |
| land | putting a workspace's branch on a project's trunk, through a forge adapter |
| record | every step, kept in state; the events a subscriber follows are these steps |

**Seats, roles and sessions.** A seat is one individual, like a coworker:
the supervisor keeps exactly one session of it running while any project of
its fleet is on, revives it when it is gone, and adopts it on start. Every
revival starts fresh, with its role's wake-up prompt; what an agent remembers
between lives is its user's to arrange. It runs in the directory its role
names, by default the fleet directory. Off only stops the reviving: nothing
ends a seat's session but a person or a program telling it to stop. A seat
taken out of the config leaves its session running, never revived; a role
changed reaches a seat's session at its next revival. Nobody spawns a seat.

A role is also a template. Spawn makes a new session from it,
in the directory the caller gives (a project's root or a workspace), and
answers with the session's id. tinyfleet never ends a spawned session on its
own: it ends when it is told to stop, and one that dies is reported gone,
never revived.

**Messages.** A message goes to a seat by its name or to a spawned
session by its id, and is one-way: sending answers once the message is
queued. The supervisor types it in when the adapter reports the agent idle,
and records it queued, then delivered, with who sent it: a person, a script
or another agent's session. A message to a seat being revived
waits for its new session, and one to an agent that needs you waits until
someone answers it: nothing is ever typed at a prompt. Reading the answer is
observe's.

**States.** Observe reports one of six states for every session, whatever its
agent: *starting* (launched, not ready yet), *working* (in a turn), *idle*
(waiting for a message), *needs you* (blocked on what only a person can
answer), *gone* (its pane has ended) and *unknown* (tinyfleet could not
tell). Needs you carries its reason when the adapter knows it: a permission,
a question, logged out, or a usage limit. tinyfleet leaves a session it
cannot read alone.

The supervisor learns a state by polling. tmux says whether each session's
pane is alive; the agent's adapter reads the agent's own record of itself,
such as Claude Code's list of its sessions, never the screen and never
through hooks. The supervisor puts the two together, and when they disagree
the state is unknown. An adapter whose agent keeps no record of itself may
read the screen instead.

A caller asks for a state now, waits for one with a timeout, or subscribes
to a fleet's events: states changing, messages queued and delivered,
sessions spawned and gone, and what changed in a tracker or forge when the
supervisor last polled its adapter.

**Workspaces.** A workspace is made on a new branch from a base, the trunk
unless the caller names another. It lives under the state directory, never in
a project, so no project gains a path to ignore. It lasts until whoever made
it removes it, and removing one with uncommitted changes is refused unless
forced. Setting a workspace up, such as installing dependencies, is the
workflow's.

**Tasks.** A task adapter declares a schema of its tracker's fields, and
workflows read them by name. tinyfleet itself knows a task only by its id,
and claims one through the adapter, atomically, so two agents never take the
same task. The tracker stays the source of truth; tinyfleet records the
claim.

**Landing.** Land puts a workspace's branch on a project's trunk through a
forge adapter. The default, plain git, merges onto the local trunk, one land
per project at a time, and refuses a conflict, changing nothing. A GitHub or
GitLab adapter opens or merges a pull request instead, and reports CI and
review results, which become events.

**Open.** Not decided yet: how one role serves several projects of a
wingman fleet, whether a project can override it, and how people share
roles. With Customization. Decided: a seat is kept filled while any
project of its fleet is on.

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
                    │           ▲
                    │   ┌───────┴───┬───────────┬────────────┬─────────┐
                    │ service      tmux        git    adapter-host   curl
                    ▼
               adapter-sdk ◄───── adapter programs, with its kit
          the adapter protocols
```

| Crate | Holds |
| --- | --- |
| `fleet` | the binary: picks the CLI or the supervisor role |
| `cli` | parsing, prompting, rendering outcomes, the socket client |
| `supervisor` | the socket server, and tending each project that is on |
| `core` (package `fleet-core`: `core` is Rust's own) | the rules (what a fleet is, when a create is refused, a fleet's lifecycle), the protocol the CLI and the supervisor speak (`core::protocol`: the requests and replies, and how they go on a stream), and the tool interfaces; no I/O except through them, and the protocol reads and writes only the stream it is handed |
| `adapter-sdk` (package `fleet-adapter-sdk`) | the adapter protocols (agents now; task trackers and forges with theirs), which `core` re-exports to the rest of tinyfleet; and, behind its `kit` feature, which tinyfleet never turns on, what anyone writing an adapter in Rust needs: the run loop, helpers, a test kit and the conformance checks |
| `state` | the SQLite schema and its queries |
| `service`, `git` | the drivers |
| `tmux` | the multiplexer's driver |
| `adapter-host` | the driver that runs adapter programs: one request on stdin, one answer on stdout, with a time limit |
| `curl` | the network's driver: downloads with `curl` |

Fitness tests fail the build when:

- any crate but `supervisor` depends on `state` (so the CLI cannot write state);
- `core` depends on a crate of the workspace other than `fleet-adapter-sdk`,
  or `fleet-adapter-sdk` on any;
- a driver depends on anything in the workspace but `core`;
- any crate but a driver starts an outside program (the SDK's `kit` is not
  part of tinyfleet: its conformance checks run an adapter in that adapter's
  own tests).

## How a command flows

**create**, in a directory: the CLI sends the directory, and any `--project`
paths, to the supervisor. With no `--project` the directory must be a git
repository's root and is the fleet's one project: a default fleet. With
`--project` it is a wingman fleet, outside the repositories it serves. The
supervisor refuses a project that is not a repository's root, a wingman fleet
inside one of its own projects, a fleet in a directory above or below another
fleet's, and a project another fleet already serves. The same fleet asked for
again is done, saying what it found. A config file already in the directory
that no fleet owns is adopted: the fleet is the one that file describes, read
as start reads it, and the file is left as it is. With `--project`, the
projects must be the ones the file lists, or create refuses and changes
nothing. Otherwise, in one transaction, it registers the fleet, writes the
config file (unless it adopted one) without overwriting anything, records the
step and answers; the new fleet's projects come up on. Nothing is committed.

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
Start also copies each fleet's roles and seats into state as its config
reads, and the supervisor keeps every seat of a fleet with any project on
filled, adopting the sessions still running; off, it leaves the sessions
alone. Only state is read to do it, so a reboot needs no config.

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

**uninstall**: the CLI asks first (`--non-interactive` answers yes; with no
terminal and no `--non-interactive` it does nothing and exits 3), then stops
the supervisor and removes its unit exactly as stop does. Only once the
supervisor is down does it delete the files tinyfleet keeps in its state
directory (the adapters it fetched; its workspaces too, landed or not: *directional, with
Delivery*), that directory if it is then empty (a `.DS_Store` left alone by
Finder goes too), and the running `fleet` binary. A stop that fails or cannot tell leaves everything; a delete that
fails names what is left, and running uninstall again finishes. The
`fleet.toml` files in projects stay.

**doctor**: the CLI checks, and reports each as fine, missing or could not
tell: the service manager (it answers), git (on the `PATH`, its version), curl (likewise), the supervisor (whether it
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

1. Only the supervisor writes state or any file in a fleet directory. The CLI
   writes only the supervisor's service unit, and, for uninstall once the
   supervisor is stopped and its unit removed, deletes tinyfleet's own files:
   the state directory's and the binary.
2. No onboarding step commits to a repository.
3. A directory holds at most one fleet, no fleet directory is inside
   another's, and a repository belongs to at most one fleet.
4. Stopping a fleet or the supervisor never ends an agent's session *(directional: arrives with the agent adapter)*.
5. Nothing waits for an answer when there is no terminal.
6. tinyfleet uses only its own tmux socket.
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
11. tinyfleet never removes a workspace on its own: only whoever made it
    does, or uninstall, which takes every workspace with it, landed or not
    *(directional: arrives with Delivery)*.
12. A message is typed into a session only while its agent is idle
    *(directional: arrives with Delivery)*.
13. A seat has at most one live session.

## Where things are on disk

| What | Where |
| --- | --- |
| state, and the supervisor's socket | `$XDG_STATE_HOME/fleet/` (default `~/.local/state/fleet/`) on Linux; `~/Library/Application Support/fleet/` on macOS |
| a fleet's config file | `fleet.toml` in the fleet directory |
| the installed binary | `~/.local/bin/fleet`, or `FLEET_INSTALL_DIR` |
| the supervisor's service unit | `~/Library/LaunchAgents/` on macOS; `~/.config/systemd/user/` on Linux |
| tmux socket | `tmux.sock` in the state directory (`tmux -S <state directory>/tmux.sock`), so a sandbox or an eval has a tmux server of its own |
| workspaces *(directional)* | under the state directory |
| fetched adapters | `adapters/<name>/<version>/fleet-<name>` in the state directory |

## Boundaries

Each boundary's authoritative form lives in code; this table points at it.

| Boundary | Between | Authoritative form |
| --- | --- | --- |
| CLI | person or script ↔ `fleet` | the transcript evals in `tests/cmd/` |
| protocol | CLI ↔ supervisor: a `Request`, then a `Reply` (a `Response` or a `Status`) | `crates/core/src/protocol.rs` |
| state schema | supervisor ↔ state | the migration in `crates/state/src/lib.rs` |
| config file | person ↔ supervisor (and doctor, which reads it) | `crates/core/src/config.rs`, and `crates/core/src/seats.rs` for roles and seats, read through `fleets::read_fleet` |
| tool interfaces | core ↔ drivers | `crates/core/src/tools.rs` |
| adapter protocol | supervisor, through a driver ↔ adapter programs | `crates/adapter-sdk/src/agent.rs` for agents, and `crates/adapter-sdk/src/lib.rs` for what every adapter answers to `protocol`; *directional:* tasks and forges |
| adapter releases | fleet-adapters' releases ↔ the supervisor | `crates/core/src/adapters.rs` |
| install | person ↔ the GitHub release | `install.sh` at the repository root |
| release | a release squash on `main` ↔ the GitHub release | `.github/workflows/release.yml` *(directional: written, not yet run)* |

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
without features, so none has the CLI's specimen or its debug verb: the
workflow refuses a release whose binary accepts `fleet specimen` or `fleet
debug`.

## Toolchain

- **Rust**, stable, pinned in `rust-toolchain.toml`; one Cargo workspace,
  crates under `crates/`.
- The version is the workspace version in the root `Cargo.toml`: a release
  branch's first commit sets it, and the release workflow reads it there.
- **clippy** with a pinned lint set, warnings denied.
- clippy and the tests also run with `--all-features`, so the CLI's
  specimen ([cli-output.md](cli-output.md)) builds and stays complete, and
  `fleet debug` builds. `tools/try` builds with the `debug` feature.
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
| task | an item of work, kept in the tracker the project already uses and reached through a task adapter |
| adapter | an external program tinyfleet reaches an agent, a task tracker or a forge through, run once per request; users can write their own; its protocol is `fleet-adapter-sdk`'s |
| forge | where a branch lands: plain git onto the local trunk, or a host such as GitHub or GitLab |
| primitive | a building block tinyfleet gives users for their own metaharness; it carries no opinion |
| role | an entry in a fleet's config saying how to run an agent: its label, its adapter, its model, its wake-up prompt and its directory |
| seat | a named agent of a fleet, running as its role says, that the supervisor keeps one session of running, revived fresh when it is gone |
| session | a tmux session on tinyfleet's socket, running one agent; a spawned session ends only when told to stop |
| workspace | a git worktree on its own branch, under the state directory |
| event | a recorded step, as a subscriber sees it |
| tool interface / driver | the trait in `core` for one kind of outside tool / the code behind it that runs the real tool |
| outcome | how a command ended: done, refused, supervisor not running, could not tell |
