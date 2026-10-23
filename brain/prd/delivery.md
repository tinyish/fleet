# Delivery

Hand work to agents and see it land: give a piece of work to an agent and
have it delivered and landed on the trunk.

## The problem

A user has agents, such as Claude Code, and a tracker full of work. Handing
that work out today means opening a terminal per agent, pasting in the task,
checking back to see whether it is done or stuck on a question, and merging
the result by hand. Two agents in the same checkout trip over each other, and
afterwards nobody can say what an agent was told or what it did.

Every user wants to work differently: one wants a reviewer agent on every
change, another a changelog written after every landing. tinyfleet does not
pick a way. It gives the pieces every way is built from: agents to bring up,
talk to and watch, a place for each to work apart, the tasks in the user's
own tracker, landing on the trunk, and a record of every step.

## Journeys

### Handing a task to an agent

- **Trying to:** take a task from the tracker, give it to an agent, and get
  the finished work onto the trunk.
- **Success:** the agent works apart from the user's own checkout and from
  every other agent. No two agents ever take the same task. The work lands on
  the trunk only cleanly: a landing that would conflict changes nothing and
  says why. Afterwards, every step can be traced: who took the task, what the
  agent was told, and what landed.
- **Passes through:** claim, workspace, spawn, message, observe, land.

### Keeping a coworker agent

- **Trying to:** have a named agent with a role, on the agent and model of
  the user's choosing, always there to talk to, such as one that writes the
  release notes.
- **Success:** the agent is there whenever its fleet is on. If it dies, or
  the machine restarts, it comes back on its own, woken with its role. The
  user chooses its agent, its model and its role; tinyfleet ships no roles of
  its own.
- **Passes through:** start, message, observe.

### Talking to an agent while it works

- **Trying to:** give an agent more to do, change its course, or ask it
  something, whether it is busy or not.
- **Success:** a message is never lost, and never lands as the answer to a
  question the agent was asking. The sender can tell whether the agent has
  received it yet, and can read the agent's reply once it has answered.
- **Passes through:** message, observe.

### Stepping in when an agent needs you

- **Trying to:** notice when an agent is waiting on something only the user
  can answer, such as a permission or a question, and answer it.
- **Success:** an agent that needs the user says so, and why when that can be
  known. When tinyfleet cannot tell what an agent is doing, it says so instead
  of guessing. The user can watch any agent live and type to it directly.
- **Passes through:** observe, attach.

### Restarting without losing agents

- **Trying to:** stop the fleet or the supervisor for a while, upgrade
  tinyfleet, or restart the machine, and come back to the agents as they were.
- **Success:** stopping never ends an agent's work. Starting again picks up
  every agent still running. A coworker agent that ended in the meantime comes
  back on its own; any other agent that ended is reported gone, never quietly
  replaced.
- **Passes through:** stop, start, observe.

### Building my own way of working

- **Trying to:** write a workflow, a script or a tool that hands out work the
  user's way: roles, review loops, reactions to a failed check.
- **Success:** everything a user can do by hand, a program can do, and follow
  as it happens. tinyfleet imposes no role, workflow or review process to work
  around. A program can always tell what happened at each step, including
  what tinyfleet could not confirm.
- **Passes through:** every verb above.

## What success looks like

- A piece of work goes from the tracker to the trunk without the user
  copying, pasting or merging by hand.
- Nothing an agent did is lost or unexplained: every step can be traced.
- Any way of working a user wants can be built from the pieces, with nothing
  in tinyfleet to work around.

## Non-goals

- Deciding how work flows. Roles, review processes and reactions belong to
  the user, built or brought; tinyfleet ships none.
- A person's approval before work lands. It comes later.
- Noticing stuck agents, moving their work on, and standing duties. That is
  Autonomy.
- Reaching other agents, trackers and forges. That is Integrations.
- Being the tracker. The user's tracker stays the source of truth for its
  tasks.
- Carrying an agent's conversation across a restart. A coworker agent that
  comes back starts fresh; what it should know is the user's to give it.
