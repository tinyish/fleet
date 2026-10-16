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
| **supervisor** | The only writer of anything tinyfleet writes: registers fleets, writes their config file, turns fleets on and off, launches and watches agents' sessions *(directional: arrives with the agent adapter)*, appends the record of every step. | from the supervisor's start to its stop; the service manager restarts it if it crashes, and it adopts the sessions still running |
| **state** | One SQLite file in the per-user state directory: the fleet registry, each fleet's on/off, its agents, the record of every step. | forever |
| **fleet directory** | Anywhere; holds only the person's config file. By default the project itself, left uncommitted; or a directory outside the repos, serving one or many. | the person's |
| **tmux server** *(directional: arrives with the agent adapter)* | Holds agents' sessions on tinyfleet's own socket, never the person's. The person can attach to watch an agent. | outlives the supervisor and any fleet's stop |

**When the supervisor is down,** a command that needs it (create, and later
commands) asks at a terminal whether to start it. With the opt-in flag, for
CI or a script, it starts the supervisor without asking and says so. With
neither, it refuses with its own outcome, *supervisor not running*. Doctor
never starts it: it reports it as not running.

**Start and stop act on everything by default.** Start brings the
supervisor up and turns every fleet on; stop turns every fleet off and brings
the supervisor down. Acting on one fleet only is off the default path, and is
asked for explicitly. Once started, the supervisor comes back after a logout
or a reboot, and every fleet with it as it was, until stop. Starting a fleet
re-reads its config. Once agents arrive with the agent adapter (directional),
stopping leaves agents' sessions running and starting adopts the sessions
still there.

## Outside tools

The core reaches every outside tool through a **tool interface**: a trait
defined in `core`, with one **driver** now, in its own crate, that runs the
real tool. The core never runs a tool itself.

| Tool interface | Driver now | Used by |
| --- | --- | --- |
| service manager | launchd (macOS), systemd user units (Linux) | CLI, to start and stop the supervisor; doctor |
| multiplexer | tmux, on tinyfleet's own socket *(directional: arrives with the agent adapter)* | supervisor; doctor |
| git | git | supervisor; doctor |
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
               cli ──────── protocol ──────── supervisor ──── state
                │                              │
                └────────────► core ◄──────────┘
                    rules + tool interfaces
                                ▲
                ┌───────────┬───┴───────┬────────────┐
             service       tmux        git     (agent, store: directional)
```

| Crate | Holds |
| --- | --- |
| `fleet` | the binary: picks the CLI or the supervisor role |
| `cli` | parsing, prompting, rendering outcomes, the socket client |
| `protocol` | the requests, answers and outcomes between CLI and supervisor |
| `supervisor` | the socket server, the loop that tends each fleet that is on |
| `core` | the rules (what a fleet is, when a create is refused, a fleet's lifecycle) and the tool interfaces; no I/O except through them |
| `state` | the SQLite schema and its queries |
| `service`, `tmux`, `git` | the drivers |

Fitness tests fail the build when:

- any crate but `supervisor` depends on `state` (so the CLI cannot write state);
- `core` depends on another crate of the workspace;
- a driver depends on anything in the workspace but `core`;
- any crate but a driver starts an outside program.

## How a command flows

**create**, in a directory: the CLI sends the directory to the supervisor. The
supervisor refuses if the registry already has a fleet there or in a
directory above or below it, and says what it found. Otherwise it writes the
config file, registers the fleet, records the step and answers. Nothing is
committed.

**start / stop**, the default: start enables and loads the supervisor's unit
through the service manager, waits for the supervisor to answer on the
socket, then asks it to turn every fleet on. Stop asks the supervisor to turn
every fleet off, then stops and disables the unit. Either says it could not
tell when the supervisor does not answer in time.

**start / stop** one fleet, off the default path: the supervisor marks that
fleet on or off. On, it reads the config; off, it stops tending the fleet.
*(Directional, with the agent adapter: on, it launches or adopts each agent's
session in tmux; off, it leaves the sessions alone.)*

**doctor**: the CLI asks each driver to check its tool and reports
each as fine, missing or could not tell, and whether the supervisor is
running. It changes nothing.

**Every command ends in one outcome:** done, refused (with a reason),
supervisor not running, or could not tell. How each outcome reaches a script
is the CLI contract's (see Boundaries).

## Invariants

1. Only the supervisor writes state or any file in a fleet directory. The CLI's
   one write is the supervisor's service unit.
2. No onboarding step commits to a repository.
3. A directory holds at most one fleet, and no fleet directory is inside
   another's.
4. Stopping a fleet or the supervisor never ends an agent's session *(directional: arrives with the agent adapter)*.
5. Nothing waits for an answer when there is no terminal.
6. tinyfleet uses only its own tmux socket *(directional: arrives with the agent adapter)*.
7. Only drivers start outside programs.
8. Doctor changes nothing.

## Where things are on disk

| What | Where |
| --- | --- |
| state, and the supervisor's socket | `$XDG_STATE_HOME/fleet/` (default `~/.local/state/fleet/`) on Linux; `~/Library/Application Support/fleet/` on macOS |
| a fleet's config file | `fleet.toml` in the fleet directory |
| the supervisor's service unit | `~/Library/LaunchAgents/` on macOS; `~/.config/systemd/user/` on Linux |
| tmux socket | tmux's socket named `fleet` (`tmux -L fleet`) |

## Boundaries

Each boundary's authoritative form lives in code; this table points at it.
None exists yet.

| Boundary | Between | Authoritative form |
| --- | --- | --- |
| CLI | person or script ↔ `fleet` | *directional:* the transcript evals in `tests/cmd/` |
| protocol | CLI ↔ supervisor | *directional:* `crates/protocol/src/lib.rs` |
| state schema | supervisor ↔ state | *directional:* `crates/state/` migrations |
| config file | person ↔ supervisor | *directional:* `crates/core/` config types |
| tool interfaces | core ↔ drivers | *directional:* `crates/core/src/tools/` |
| install | person ↔ the GitHub release | *directional:* `install.sh` at the repository root |

## Install

A one-line shell script downloads the prebuilt `fleet` binary for the
machine's OS and CPU from the GitHub release and puts it on the `PATH`. No
Rust toolchain is needed. The release workflow builds the binaries: macOS and
Linux, on arm64 and x86_64.

## Toolchain

- **Rust**, stable, pinned in `rust-toolchain.toml`; one Cargo workspace,
  crates under `crates/`.
- The version is the workspace version in the root `Cargo.toml`: a release
  branch's first commit sets it, and the release workflow reads it there.
- **clippy** with a pinned lint set, warnings denied.
- Visible evals are trycmd transcript cases under `tests/cmd/<verb>/`, run by
  `cargo test`.
- Fitness tests read the workspace's dependency graph (`cargo metadata`).

## Terms

| Term | Means |
| --- | --- |
| fleet | a set of agents working on one or more repositories, defined by one fleet directory |
| fleet directory | the directory holding a fleet's config file |
| supervisor | the one long-running tinyfleet process per user |
| state | everything tinyfleet writes about its fleets, in one SQLite file |
| store | where task items live, reached through an adapter (defined later) |
| adapter | what tinyfleet talks to an agent or a store through; users can write their own (contract defined later) |
| session | a tmux session on tinyfleet's socket, running one agent |
| tool interface / driver | the trait in `core` for one kind of outside tool / the code behind it that runs the real tool |
| outcome | how a command ended: done, refused, supervisor not running, could not tell |
