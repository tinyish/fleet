# Onboarding

Put a fleet on a project: get from nothing to a fleet running, on this
machine and this project, ready for agents.

## Problem

A person who wants to try tinyfleet wants it running on their machine and
one of their projects in a couple of minutes, not an afternoon. Tools like
it usually ask for a pile of setup first: a service to configure, tools
found missing one at a time as each step fails, and files committed into
the project before anyone has decided to keep using it.

The same setup is also run by things with nobody at the keyboard: a
dotfiles script, a provisioning step, another tool built on tinyfleet. A
setup that stops to ask a question hangs them, and one whose result can't
be read leaves them guessing whether it worked.

And people change their minds. They take a fleet off a project, or
tinyfleet off the machine, and expect nothing left behind; or they run
setup again by habit and expect it not to make a mess of what's there.

## Goals

- A person goes from tinyfleet not installed to a fleet running on a
  project in a couple of minutes at most.
- Anything a person does at a terminal, a script can do with nobody there,
  and tell what happened.
- No onboarding step changes what the person didn't ask it to: nothing
  committed, nothing overwritten, nothing left behind on the way out.

We believe that saying what's missing on the machine before anything
starts will get most people to a running fleet on their first try, which
we'll see in how rarely a first setup is followed by a second within the
hour.

We believe that if a script can always tell a fleet that was set up from
one that was refused, and both from one tinyfleet couldn't confirm, people
will build tinyfleet into their own setup scripts, which we'll see in
setups run with no terminal.

## Use cases

### Inés, 34, freelance developer

Inés has read about tinyfleet and has ten minutes before a call. She runs
the install command from the docs, then asks tinyfleet to check her
machine: it tells her one tool it runs on is missing, and how to get it.
She installs it, creates a fleet on the project she's working on, and the
fleet is running before her call starts. The config file it wrote sits
uncommitted in her project, for her to commit or not. A week later, after
upgrading her agent, she asks tinyfleet to check the machine again: one
look tells her everything is still in order, and looking changed nothing.

### Tomás, 45, keeps his machines in a dotfiles repo

Tomás sets up every new laptop with one script. He adds tinyfleet to it:
install, then create a fleet on each of his three projects. The script runs
with nobody watching and never waits for an answer. On the second run,
after an upgrade, it finds the fleets already there, says so, and changes
nothing. When one project's setup is refused, the script can tell, and
prints why.

### Leah, 29, on a team that hasn't agreed to agents yet

Leah wants to try tinyfleet on her team's repositories without anyone else
seeing it. She creates one fleet that serves two of them, and the
repositories are unchanged: nothing to commit, ignore or explain. A month
later the trial is over. She takes the fleet off, keeping its config file,
then takes tinyfleet off her machine: nothing of it runs or comes back
after a reboot, and nothing it kept is left.

## Requirements

### 1. Installing and checking the machine

1.1 A person shall be able to install tinyfleet with one command from its
    docs. (P0)
1.2 From install to a fleet running on a project shall take a couple of
    minutes at most, for a person whose machine has what tinyfleet runs
    on. (P0)
1.3 Before anything starts, a person shall be able to learn what tinyfleet
    needs that the machine is missing, and how to fix each. (P0)
1.4 A check of the machine shall say what is fine, what is missing, and
    what tinyfleet could not tell, in one look. (P0)
1.5 Checking the machine shall change nothing. (P0)

### 2. Creating a fleet

2.1 A person shall be able to put a fleet on a project with one command
    that writes its config file and starts it. (P0)
2.2 No onboarding step shall commit anything to a repository; anything
    written in a project is left uncommitted. (P0)
2.3 A person shall be able to create a fleet that leaves the repository
    unchanged, with nothing to commit, ignore or explain. (P1)
2.4 One fleet shall be able to serve several projects. (P1)
2.5 Setup run where a fleet already exists shall never make a second
    fleet, nor one nested inside another, never overwrite what is there,
    and say what it found. (P0)
2.6 A config file that no fleet owns, left by a removed fleet or written by
    hand, shall be taken up as it is. (P1)

### 3. With no terminal

3.1 No onboarding step shall wait for an answer when there is no terminal.
    (P0)
3.2 A script shall be able to tell a step that succeeded from one that was
    refused, and both from one whose result tinyfleet could not confirm.
    (P0)
3.3 Everything a person can do in onboarding at a terminal, a script shall
    be able to do with nobody there. (P0)

### 4. Stopping and starting

4.1 A person shall be able to turn a fleet off for a while and on again, and
    have a changed config picked up when it starts. (P0)
4.2 Each stop and start shall confirm what happened, or say it could not
    tell. (P0)

### 5. Taking it off

5.1 A person shall be able to take a fleet off a project, including one
    whose directory has since been moved or deleted. (P0)
5.2 Once a fleet is removed, starting the fleets that remain shall no
    longer mention it. (P0)
5.3 Removing a fleet shall delete its config file only when the person says
    so; with nobody there to ask, the file stays. Nothing else in the
    project shall change. (P0)
5.4 Removing a fleet that isn't there shall say so and change nothing. (P1)
5.5 A person shall be able to take tinyfleet off the machine, after which
    nothing of it runs or comes back after a reboot, and nothing it keeps in
    its own places is left: what it remembered, its log, and the program
    itself. (P0)
5.6 Uninstalling shall leave the config files in the person's projects
    alone. (P0)
5.7 Uninstalling shall never happen by accident; a script that says so
    shall be able to uninstall with nobody there. (P0)

## Open questions

None.

## Success criteria

- From install to a running fleet takes a couple of minutes at most.
- Anything a person does at a terminal, a script can do with nobody there.
- No onboarding step leaves the person unsure whether it worked.

## Non-goals

- Installing the tools tinyfleet runs on. Onboarding checks them and says
  what is missing; installing them is the person's.
- Setting up the project's work tracker. Onboarding uses the one the project
  already has and never changes it.
