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

## Capabilities

Each capability is a problem the person has. Its PRD holds the journeys that
solve it.

| Title | Description | User need | PRD |
| --- | --- | --- | --- |
| Onboarding | Put a fleet on a project | Get from nothing to a fleet running, on this machine and this project, ready for agents | [onboarding](onboarding.md) |
| Delivery | Hand work to agents and see it land | Give a piece of work to an agent or a person, and have it delivered, reviewed and landed on the trunk | [delivery](delivery.md) |
| Autonomy | Keep agents working unattended | Leave agents working, have stuck or idle ones noticed and moved on, and have routines run on their own | [autonomy](autonomy.md) |
| Safety | Keep agents from doing damage | Be sure an agent cannot run a dangerous command, whatever it was asked to do | not written yet |
| Visibility | See what the fleet is doing | Know at a glance what each agent is doing, and trace any item back through what happened to it | not written yet |
| Customization | Make the fleet work my way | Choose the roles, workflows and values a fleet runs with, and share them between projects | not written yet |
| Integrations | Bring my own tracker and agent | Keep the work tracker and the agent I already use | not written yet |

A capability's title is its PRD's heading and file name.

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
