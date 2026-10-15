# Onboarding

Put a fleet on a project: get from nothing to a fleet running, on this
machine and this project, ready for agents.

## The problem

A user wants to try tinyfleet and get it running on their machine. The whole
process should take a couple of minutes at most. They run the install command
that is in the docs, then run one command that creates a config file to start
using tinyfleet. Everything else is taken care of, and no step commits
anything to their projects.

## Journeys

### First time, at a terminal

- **Trying to:** go from tinyfleet not installed to a fleet running on a
  project.
- **Success:** the whole path, install included, takes a couple of minutes at
  most. Before anything starts, the person knows what is missing on the
  machine and how to fix it. Anything written in the project is left
  uncommitted, for the person to commit or not.
- **Passes through:** install, doctor, create, start.

### A script or installer, with no terminal

- **Trying to:** set up a fleet unattended, from a dotfiles script, a
  provisioning step or another tool built on tinyfleet.
- **Success:** nothing ever waits for an answer. The script can always tell a
  fleet that was set up from one that was refused, and both from a result
  tinyfleet could not confirm.
- **Passes through:** create, start.

### Without a trace in the project

- **Trying to:** use tinyfleet on a repository without anyone else seeing it,
  for example before the team has agreed to it, or run one fleet across
  several repositories.
- **Success:** the repository is unchanged: nothing to commit, ignore or
  explain. One fleet serves every project it was given.
- **Passes through:** create.

### Running it again where a fleet already exists

- **Trying to:** run setup again, by habit, by a script that always runs it,
  or after an upgrade.
- **Success:** it never makes a second fleet or one nested inside another,
  never overwrites what is there, and says what it found.
- **Passes through:** create, start.

### Checking the machine after a change

- **Trying to:** know whether what tinyfleet runs on is still in order after
  upgrading a tool, an agent or a pack.
- **Success:** one look says what is fine, what is missing and what tinyfleet
  could not tell, and looking changes nothing.
- **Passes through:** doctor.

### Restarting after a change

- **Trying to:** change the fleet's config and have the fleet pick it up, or
  turn the fleet off for a while.
- **Success:** each step confirms what happened or says it could not tell.
- **Passes through:** stop, start.

## What success looks like

- From install to a running fleet takes a couple of minutes at most.
- Anything a person does at a terminal, a script can do with nobody there.
- No onboarding step leaves the person unsure whether it worked.

## Non-goals

- Installing the tools tinyfleet runs on. Onboarding checks them and says what
  is missing; installing them is the person's.
- Setting up the project's work tracker. Onboarding uses the one the project
  already has and never changes it.
