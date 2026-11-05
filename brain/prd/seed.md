# tinyfleet

tinyfleet runs AI coding agents on your projects. It hands items of work to
agents or people, sees each item through delivery and review to a landing on
the trunk, refuses dangerous commands before they run, and keeps a record of
every step.

## Who it is for

A solo builder who runs several agents, such as Claude Code, on their own
projects, on their own macOS or Linux machine with the subscriptions they
already pay for, and wants those agents to work while nobody watches without
losing track of what each one did.

## What it does

- Puts a fleet on a project in moments, on this machine, ready for agents,
  and takes it off again without a trace.
- Hands work to agents or people, and sees it delivered, reviewed and
  landed on the trunk.
- Keeps agents working unattended: stuck or idle ones are noticed and
  moved on, and routines run on their own.
- Keeps agents from doing damage: a dangerous command is refused, whatever
  an agent was asked to do.
- Shows at a glance what each agent is doing, and traces any item back
  through what happened to it.
- Lets a person choose the roles, workflows and values a fleet runs with,
  share them between projects, and keep the tracker and agent they
  already use.

## Principles

- **Local-first.** Most orchestration tools are heading where the money is:
  cloud platforms for enterprise teams. tinyfleet is for you and your own
  projects. It runs on your machine, with the agent subscriptions you already
  have.
- **A boring, durable core.** tinyfleet does a few things and does them well. It
  is documented, pleasant to use, fast and light on memory, and you are up and
  running in minutes.
- **Primitives, not opinions.** Anything that carries an opinion, such as a
  role, a workflow or a value, lives outside tinyfleet. tinyfleet gives you the
  building blocks to build whatever metaharness you want.

## What success looks like

- Setting up a fleet takes seconds, and the first workflow runs within minutes.
- tinyfleet runs reliably and fast on whatever machine it is on, however long
  it is left running.
- tinyfleet fits the workflow and tools a person already uses, with no
  workarounds.
- One person working with tinyfleet gets done what used to take a team.
- Other tools and services can be built on top of tinyfleet.

## Non-goals

- Being a work tracker or an agent. tinyfleet works with the ones the person
  brings.
- A hosted, cloud or team orchestration platform. tinyfleet runs on one person's
  machine.
