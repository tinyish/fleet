# Session life

Status: built. Bead: fleet-1xy3. Kind: internal. Touches: Delivery (sessions, messages, wait and events),
`state`. From the code-health review of 2026-11-06 (candidates C1 and C2).

## Why

Two rules are spread across modules that each re-derive them, and the last
three Delivery fixes (fleet-qjgn, fleet-p6c5, fleet-ojd3) each moved one
rule into one place without removing the shape that caused it:

- **A session's life.** A session ends two ways that look different in
  state: a seat's dead session is deleted at its revival and recorded
  `SessionGone`; a spawned one keeps its row and is recorded
  `SessionStateChanged{gone}`. Then every reader keeps its own rule for
  "live": a row exists (`names.rs`), the pane is alive now (message, end of
  a seat), the recorded state is not gone (wait, overview), and gone or
  ended for a session with no row is found by scanning the record.
  fleet-p6c5 was one reader missing one of the two ways.
- **A write's steps.** State drops what a change strands (fleet-qjgn), but
  the supervisor still builds the step that records the change, so each
  rule is half in each: the roster is diffed twice, a drop's reason is
  guessed from the variant of the step passed in, and "a seat is wanted"
  is written in three places.

## Design

`state` becomes the deep module for both. The supervisor tells it what
happened and asks it what is; it never builds a `Step` and never decides
whether a session is live.

**A session's life** is one of three, kept on its row, which now outlives
the session:

| Life | Means | How it comes |
| --- | --- | --- |
| live | launched, and neither gone nor ended | launch |
| gone | its pane ended | the tick reads it gone, records it once, and drops its messages |
| ended | someone or something said end | end: recorded `SessionEnded`, its messages dropped |

- One way to die: every session whose pane ends is read gone first. A seat's
  revival comes after, in the same tick, and only stops the old tmux
  session; its old session stays gone. The `SessionGone` step goes.
- Live is the record's word, not tmux's: a session is live while its row
  says so. Message, end, wait, status and names all read it from state;
  the tick settles it within a tick (about 2 s). Status still reads each
  live session's state as it is now. A
  pane that died since the last tick still counts as live, and what is sent
  to it then is dropped as its session gone when the tick reads it.
- A row is never deleted. Life is one lookup, and the record is never
  scanned to find a session.

**Every write records its own steps.** `Write::record` is private to
`state`. Each write takes what happened, reads what it needs in its
transaction, and records its steps and drops in it.

## Parts

Built in order, each reviewed; they land together as one commit.

1. [000-writes.md](000-writes.md): every state write records its own steps
   (C2).
2. [001-life.md](001-life.md): a session's life lives in state (C1), on top
   of 000's writes.

## What a person or a script sees change

- `fleet events` no longer shows a `SESSION_GONE` step: a seat's dead
  session shows its state changing to gone, as a spawned one's already
  does, before its seat's next `SESSION_LAUNCHED`.
- A message or an end sent within a tick of an agent dying is taken as for
  a live session: the message is dropped as its session gone when the tick
  reads it; the end ends it.
- Ending a seat whose fleet is off no longer says it comes back: it comes
  back only when its seat is wanted (000).
- A tick's reading of a session ended under it is no longer recorded.

## core.md, at landing

- § Primitives, States: say that a session's row keeps its life (live,
  gone, ended), that live is as state last recorded it, and that the tick
  alone reads tmux.
- § Primitives, Seats: a seat's dead session is recorded gone before its
  revival, and stays gone.
- § How a command flows, spawn / end: "a session already ended, found only
  in the record" becomes "found ended".
- Crates table, `state`: "the SQLite schema, its queries, and every write's
  steps and drops".
- Invariant 10: "the supervisor writes only through `State::write`'s
  writes, each recording its own steps".
- Terms: add *live*, *gone*, *ended* under session.

## Tests

- `state`'s inline tests, on a temp SQLite file: each write records these
  steps and drops these messages (000); one table of life cases, seat and
  spawned × died, ended, died then ended, revived (001).
- The supervisor's scenarios keep only what crosses modules: a message sent,
  its agent dying at a tick, the wait on it ending; a seat dying and coming
  back. Per-verb checks of gone and of drops that state's tests now cover
  are deleted, not kept beside them.
