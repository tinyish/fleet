# Delivery

Hand work to agents and see it land: give a piece of work to an agent and
have it delivered and landed on the trunk.

## Problem

A person has agents, such as Claude Code, and a tracker full of work.
Handing that work out today means opening a terminal per agent, pasting in
the task, checking back to see whether it is done or stuck on a question,
and merging the result by hand. Two agents in the same checkout trip over
each other, and afterwards nobody can say what an agent was told or what it
did.

Every person wants to work differently: one wants a reviewer agent on every
change, another a changelog written after every landing. tinyfleet doesn't
pick a way. It gives the pieces every way is built from: agents to bring
up, talk to and watch, a place for each to work apart, the tasks in the
person's own tracker, landing on the trunk, and a record of every step.

## Goals

- A piece of work goes from the tracker to the trunk without the person
  copying, pasting or merging by hand.
- Nothing an agent did is lost or unexplained: every step can be traced.
- Any way of working a person wants can be built from the pieces, with
  nothing in tinyfleet to work around.

We believe that giving each agent a place of its own to work, and landing
only what lands cleanly, will let a person run several agents on one
project at once, which we'll see in how many agents work on one project at
the same time.

We believe that if everything a person can do by hand a program can also
do, and follow as it happens, people will build their own ways of working
on tinyfleet rather than around it, which we'll see in how much of the
work is handed out by programs instead of by hand.

## Use cases

### Ana, 31, runs a small product on her own

Ana's tracker has a dozen tasks ready. Before a run she hands three of them
to three agents, each working apart from her own checkout and from each
other; no two ever take the same task. She keeps typing to one of them
while it works, to change its course, and can tell when it has received the
message and read its reply. One asks for a permission only she can give;
tinyfleet says so, and she watches it live and answers it directly. By the
time she's back, two tasks have landed on the trunk. The third would have
conflicted, so nothing changed, and she can see why. For each, she can
trace who took it, what the agent was told, and what landed.

### Raj, 38, keeps a release-notes agent

Raj has a coworker agent with a role he wrote: it writes the release notes,
on the agent and model he chose, and it's there whenever his fleet is on.
He upgrades tinyfleet and restarts his machine. Stopping didn't end any
agent's work: when it starts again, every agent still running is picked
up, and the release-notes agent, which ended with the restart, comes back
on its own, woken with its role. A one-off agent that ended in the meantime
is reported gone, not quietly replaced.

### Mei, 27, builds her own review loop

Mei wants every change reviewed by a second agent before it lands. She
writes a short program that hands a task to one agent, waits for its work,
hands that to a reviewer, and lands it once the reviewer is satisfied. It
follows each step as it happens, and can always tell what happened at each
one, including what tinyfleet couldn't confirm. tinyfleet imposed no role
or review process on her; the loop is hers.

## Requirements

### 1. Handing out work

1.1 A person shall be able to take a task from their tracker and give it to
    an agent. (P0)
1.2 No two agents shall ever take the same task. (P0)
1.3 Each agent shall work apart from the person's own checkout and from
    every other agent. (P0)
1.4 Finished work shall land on the trunk only cleanly: a landing that would
    conflict changes nothing and says why. (P0)
1.5 Every step shall be traceable afterwards: who took the task, what the
    agent was told, and what landed. (P0)

### 2. Coworker agents

2.1 A person shall be able to keep a named agent with a role, on the agent
    and model of their choosing, there whenever its fleet is on. (P0)
2.2 A coworker agent that dies, or is ended by a restart of the machine,
    shall come back on its own, woken with its role. (P0)
2.3 tinyfleet shall ship no roles of its own. (P0)

### 3. Talking to agents

3.1 A person shall be able to send an agent a message whether it is busy or
    not. (P0)
3.2 A message shall never be lost, and never land as the answer to a
    question the agent was asking. (P0)
3.3 The sender shall be able to tell whether the agent has received a
    message yet, and read its reply once it has answered. (P0)

### 4. Stepping in

4.1 An agent that needs the person, such as for a permission or a question,
    shall be shown as needing them, with why when that can be known. (P0)
4.2 When tinyfleet can't tell what an agent is doing, it shall say so
    instead of guessing. (P0)
4.3 A person shall be able to watch any agent live and type to it directly.
    (P0)

### 5. Restarting

5.1 Stopping the fleet or the supervisor shall never end an agent's work.
    (P0)
5.2 Starting again shall pick up every agent still running. (P0)
5.3 An agent that isn't a coworker and ended while the fleet was off shall
    be reported gone, never quietly replaced. (P0)

### 6. Building a way of working

6.1 Everything a person can do by hand, a program shall be able to do. (P0)
6.2 A program shall be able to follow what happens as it happens. (P0)
6.3 A program shall always be able to tell what happened at each step,
    including what tinyfleet could not confirm. (P0)
6.4 tinyfleet shall impose no role, workflow or review process to work
    around. (P0)

## Open questions

- The seed says Delivery's work is "delivered, reviewed and landed", while
  a person's approval before landing is a non-goal for now: is review here
  only what a person builds from the pieces?

## Success criteria

- A piece of work goes from the tracker to the trunk without the person
  copying, pasting or merging by hand.
- Nothing an agent did is lost or unexplained: every step can be traced.
- Any way of working a person wants can be built from the pieces, with
  nothing in tinyfleet to work around.

## Non-goals

- Deciding how work flows. Roles, review processes and reactions belong to
  the person, built or brought; tinyfleet ships none.
- A person's approval before work lands, for now.
- Noticing stuck agents, moving their work on, and standing duties. That is
  Autonomy.
- Reaching other agents, trackers and forges. That is Integrations.
- Being the tracker. The person's tracker stays the source of truth for its
  tasks.
- Carrying an agent's conversation across a restart. A coworker agent that
  comes back starts fresh; what it should know is the person's to give it.
