# Conventions

How tinyfleet is designed and built. Each convention has a key, its
heading, that specs, items and agents cite it by, such as
`conventions.md#<key>`, and a very brief description: a line or two, not
a document. Whoever designs a feature reads Design; whoever builds one
reads Implementation.

Most are corrections Alberto made once and wants never to make again, so
each says where it came from. Add one whenever a review corrects something
that could come up again; change or drop one only with Alberto.

# Design

None yet.

# Implementation

## change-dont-wrap

When a function needs a new parameter or a second result, change it and
update every caller; never add a twin and turn the original into a wrapper
to leave call sites alone. Two functions for one job is the thing to avoid,
however small. *(fleet-sibp.3: `client::ask_within` beside
`client::ask_for`, merged; `fleets::read_projects` as a wrapper of
`read_fleet`, removed.)*

## size-budgets

At most 100 lines per function and 600 lines per file, not counting a
file's inline tests. No dead code and no unused dependencies. The build
fails on these.

## query-index

A state query that runs every tick, or holds a correlated subquery, comes
with an index that serves it, checked with `EXPLAIN QUERY PLAN`; rows that
are never deleted make an unserved scan grow without end. *(fleet-1xy3:
`sessions()`'s filter scanned every session row twice a tick.)*
