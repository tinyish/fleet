# Autonomy

Keep agents working unattended: leave agents working, have stuck or idle ones
noticed so they can be moved on, and have standing work happen on its own.

## The problem

A user hands work to agents and walks away: to sleep, to a meeting, to other
work. Today they come back to find one agent waited all night on a permission
nobody saw, another has been circling the same failing test for an hour,
a third finished long ago and sat idle, and a fourth hit its usage limit and
never started again. Nothing told them, so the only way to know is to keep
looking. The work that should happen every day, such as triaging the tracker
or handing out the next task, happens only when they remember to do it.

What to do about a stuck agent is personal: one user wants a nudge, another
wants the task handed to a different agent, another wants a message on their
phone. tinyfleet does not pick. It notices when an agent has stopped moving
its work, and it runs the user's own routines on time; what happens next is
the user's to build.

## Journeys

### Leaving agents working

- **Trying to:** leave agents working overnight, or for an afternoon, and come
  back to the work moved on rather than to agents that stopped an hour in.
- **Success:** nothing that stopped while the user was away went unnoticed.
  On coming back, the user can see which agents stalled, how, since when,
  and what was done about it.
- **Passes through:** start, observe.

### Noticing an agent that has stopped

- **Trying to:** learn, without watching, when an agent has stopped moving its
  work: it has waited on a person too long, it has been working a long time
  with nothing to show, it has sat idle, or it has hit its usage limit.
- **Success:** every such stall is noticed, saying which of these it is and
  since when. How long is too long is the user's to set, for a fleet or for
  one role, or to turn off; a new fleet starts with values that
  work without setting anything. When tinyfleet cannot tell whether an agent
  is moving, it says so instead of guessing. A stall's end is noticed too:
  the question answered, the agent moving again, the limit lifted.
- **Passes through:** observe.

### Moving stalled work on my way

- **Trying to:** decide what happens to a stalled agent: nudge it, stop it and
  hand its task to another, wait for its limit to lift, or reach the user
  wherever they are.
- **Success:** a program the user writes learns of every stall as it
  happens, and can act on it with everything the user can do by hand.
  tinyfleet itself never acts on a stall: it never nudges, stops, replaces or
  reassigns an agent because it stalled.
- **Passes through:** observe, message, stop, spawn, claim.

### Keeping standing work going

- **Trying to:** have work happen on its own on a schedule, such as triaging
  the tracker every morning, handing the oldest task to an agent and landing
  what it delivers, or writing up the day's landings each evening.
- **Success:** a routine, a workflow the user writes, runs on its schedule
  while its fleet is on, and never while it is off; turning a fleet off lets
  a run already going finish. A routine never has two runs going at once: a
  time that comes while its last run is still going is skipped, and the skip
  is recorded. A routine can do anything the user can do by hand, do several
  things at once, wait for hours or until a set time, remember what it did in
  earlier runs, and wait for the user's answer before going on. A run that
  dies part way, because the machine restarted or it crashed, never repeats
  what it already did: it waits for the user to resume it from where it
  stopped or to abandon it, and the routine's next times still run. Every run
  is recorded: when it ran, each step it took, and how it ended. tinyfleet
  ships no routines of its own.
- **Passes through:** routine, start, stop.

## What success looks like

- No agent stalls unnoticed, whether or not anyone is watching.
- Standing work happens on time without the user remembering it.
- What happens to a stalled agent is always the user's choice, built from the
  same pieces as everything else, with nothing in tinyfleet to work around.

## Non-goals

- Acting on a stall. tinyfleet notices; nudging, restarting, reassigning and
  notifying are the user's to build. Bringing back a coworker agent that died
  is Delivery's, not a reaction to a stall.
- Reaching the person. A desktop notification or a message to a phone is
  something the user builds on the stalls tinyfleet notices.
- Routines set off by anything but their schedule. A routine started by a
  stall, by a change in a session, a tracker or a forge, or by hand comes
  later.
- Running a missed routine late.
- Shipping routines. Like roles, they belong to the user, built or brought.
- Running anything while the machine is off or asleep.
