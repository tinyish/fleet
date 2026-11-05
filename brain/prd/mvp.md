# tinyfleet MVP

Status: approved

The first release of tinyfleet: what ships in it, how we know it is
ready, the milestones that build it, and what comes after. It is the one
source of truth for what is built first: the most bare-bones product that
proves the architecture works, a vertical slice. The principles and
success measures it draws on are in [seed](seed.md).

## What the MVP is

The MVP is the point where tinyfleet builds itself. Its one user is
Alberto, building tinyfleet: his own work on tinyfleet is handed out to
agents, worked, and landed through tinyfleet, day to day, including while
nobody is at the machine. Until then, nobody else is asked to use it.

The MVP proves:

- A solo builder can run their project's work through agents with
  tinyfleet, from the tracker to the trunk, without a terminal per agent or
  merging by hand.
- Agents can be left working unattended, with every stall noticed and
  standing work run on time.
- The primitives are enough: the way tinyfleet itself is built, roles and
  reviews included, is built from them, with nothing in tinyfleet to work
  around.

## What ships

- **Onboarding:** installing tinyfleet and checking the machine, putting a
  fleet on a project with one command, turning it off and on, and taking
  it off a project or the machine without a trace, all of it at a
  terminal or from a script with nobody there.
- **Delivery:** handing a task from the tracker to an agent that works
  apart from everyone else and lands it cleanly on the trunk; coworker
  agents with roles the person writes, which come back when they die;
  messages that reach an agent without being lost; seeing when an agent
  needs the person, and stepping in; agents that outlive a restart of the
  fleet; and everything a person does by hand open to a program, which can
  follow what happens as it happens.
- **Autonomy:** noticing when an agent has stalled, how and since when,
  and when the stall ends; telling a program the person writes about every
  stall, without tinyfleet acting on it itself; and routines that run on
  schedule while their fleet is on, never twice at once, and never repeat
  a step after a crash.

## Ready to ship when

- Every milestone is built, its evals pass, and its old specs are
  harvested.
- For one week, every tinyfleet item is handed out, worked and landed
  through tinyfleet, with no terminal opened per agent and no merge done by
  hand.
- In that week, at least one overnight run had every stall noticed, and a
  routine ran on time.

## Milestones

Each milestone builds one area from its PRD, written before this MVP
took its shape: the MVP takes that PRD's P0 requirements, and its P1s come
after the MVP. A milestone is built piece by piece, and is marked built
once its evals are written and its old specs are harvested.

### 1. Onboarding

Status: built

Putting a fleet on a project, and taking it off again: everything else
runs on a fleet. Its PRD is [onboarding](onboarding.md).

Done when a person goes from nothing to a fleet running on this machine and
project, at a terminal or from a script, and can take it off without a
trace.

### 2. Delivery

Status:

Handing work to agents and seeing it land. Its PRD is
[delivery](delivery.md).

Done when a task goes from the tracker to an agent and lands on the trunk,
every step traceable, with no copying, pasting or merging by hand.

### 3. Autonomy

Status:

Keeping agents working unattended. Its PRD is [autonomy](autonomy.md).

Done when agents left alone keep working, a stalled one is noticed and its
work moved on, and routines run on time.

## After the MVP

Next, the P1s of Onboarding, Delivery and Autonomy. Then, in this order,
each with a PRD of its own once the MVP is built:

1. **Visibility:** seeing what the fleet is doing.
2. **Safety:** keeping agents from doing damage.
3. **Customization:** making the fleet work the person's way.
4. **Integrations:** bringing the person's own tracker and agent.

A release for people other than Alberto comes after those, in no fixed
order yet.

## Assumptions and open questions

- **The tracker and the agent.** The MVP works with the tracker and agent
  tinyfleet is built with, beads and Claude Code; bringing others is
  Integrations, after the MVP.
- **Unattended without Safety.** Agents run overnight before tinyfleet
  refuses dangerous commands; until Safety ships, what an agent may run is
  left to the agent's own permissions.
- **Who else uses it, and when.** Which areas a first outside user needs
  is open; it is settled once Visibility and Safety have PRDs.
