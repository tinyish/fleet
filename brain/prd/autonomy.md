# Autonomy

Keep agents working unattended: leave agents working, have stuck or idle
ones noticed so they can be moved on, and have standing work happen on its
own.

## Problem

A person hands work to agents and walks away: to sleep, to a meeting, to
other work. Today they come back to find one agent waited all night on a
permission nobody saw, another has been circling the same failing test for
an hour, a third finished long ago and sat idle, and a fourth hit its usage
limit and never started again. Nothing told them, so the only way to know
is to keep looking. The work that should happen every day, such as triaging
the tracker or handing out the next task, happens only when they remember
to do it.

What to do about a stuck agent is personal: one person wants a nudge,
another wants the task handed to a different agent, another wants a message
on their phone. tinyfleet doesn't pick. It notices when an agent has
stopped moving its work, and it runs the person's own routines on time;
what happens next is the person's to build.

## Goals

- No agent stalls unnoticed, whether or not anyone is watching.
- Standing work happens on time without the person remembering it.
- What happens to a stalled agent is always the person's choice, built from
  the same pieces as everything else.

We believe that noticing every stall, and telling a program about it as it
happens, will let people leave agents working overnight and come back to
the work moved on, which we'll see in how much work lands while nobody is
at the machine.

We believe that routines that survive a crash without repeating what they
did will let people hand their daily chores to tinyfleet, which we'll see
in routines kept running for weeks.

## Use cases

### Carmen, 36, leaves agents working overnight

Carmen hands four tasks to agents before bed. At 2 a.m. one hits its usage
limit, and a program she wrote learns of it as it happens and waits for the
limit to lift before nudging it on. Another waits on a permission past the
time she set for her fleet; her program hands its task to a different
agent. In the morning she can see which agents stalled, how, since when,
and what was done about each. One agent tinyfleet couldn't read is shown as
unknown, not guessed at.

### Olu, 42, has the tracker triaged every morning

Olu writes a routine that triages his tracker at eight every morning, hands
the oldest task to an agent, waits for it to deliver, and lands it. One
morning his machine restarts halfway through a run. The run doesn't repeat
what it already did; it waits for him to resume it from where it stopped,
and the next morning's run still happens. The day his fleet is off, the
routine doesn't run, and he can see every run it did: when, each step, and
how it ended.

## Requirements

### 1. Noticing stalls

1.1 tinyfleet shall notice when an agent has stopped moving its work: it
    has waited on a person too long, worked a long time with nothing to
    show, sat idle, or hit its usage limit. (P0)
1.2 Each stall noticed shall say which of these it is, and since when. (P0)
1.3 A stall's end shall be noticed too: the question answered, the agent
    moving again, the limit lifted. (P0)
1.4 A person shall be able to set how long is too long, for a fleet or for
    one role, or turn a kind of stall off. (P1)
1.5 A new fleet shall start with stall times that work without setting
    anything. (P0)
1.6 When tinyfleet can't tell whether an agent is moving, it shall say so
    instead of guessing. (P0)
1.7 A person coming back shall be able to see which agents stalled while
    they were away, how, since when, and what was done about it. (P0)

### 2. Acting on stalls

2.1 A program the person writes shall learn of every stall as it happens.
    (P0)
2.2 That program shall be able to act on a stall with everything the
    person can do by hand. (P0)
2.3 tinyfleet itself shall never nudge, stop, replace or reassign an agent
    because it stalled. (P0)

### 3. Routines

3.1 A person shall be able to write a routine, a workflow that runs on a
    schedule. (P0)
3.2 A routine shall run on its schedule while its fleet is on, and never
    while it is off; turning a fleet off lets a run already going finish.
    (P0)
3.3 A routine shall never have two runs going at once: a time that comes
    while its last run is still going is skipped, and the skip is
    recorded. (P0)
3.4 A routine shall be able to do anything the person can do by hand, and
    several things at once. (P0)
3.5 A routine shall be able to wait for hours or until a set time, remember
    what it did in earlier runs, and wait for the person's answer before
    going on. (P1)
3.6 A run that dies part way, because the machine restarted or it crashed,
    shall never repeat what it already did: it waits for the person to
    resume it from where it stopped or abandon it. (P0)
3.7 A routine's next times shall still run while one of its runs waits to
    be resumed. (P0)
3.8 Every run shall be recorded: when it ran, each step it took, and how it
    ended. (P0)
3.9 tinyfleet shall ship no routines of its own. (P0)

## Open questions

None.

## Success criteria

- No agent stalls unnoticed, whether or not anyone is watching.
- Standing work happens on time without the person remembering it.
- What happens to a stalled agent is always the person's choice, with
  nothing in tinyfleet to work around.

## Non-goals

- Acting on a stall. tinyfleet notices; nudging, restarting, reassigning and
  notifying are the person's to build. Bringing back a coworker agent that
  died is Delivery's, not a reaction to a stall.
- Reaching the person. A desktop notification or a message to a phone is
  something the person builds on the stalls tinyfleet notices.
- Routines set off by anything but their schedule, for now: by a stall, by
  a change in a session, a tracker or a forge, or by hand.
- Running a missed routine late.
- Shipping routines. Like roles, they belong to the person, built or
  brought.
- Running anything while the machine is off or asleep.
