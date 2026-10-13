# Exit codes

Uses: shared.output, shared.locations

## What it is for

Every `fleet` command ends with an exit code from one table of seven rows.
The table is shared so that whoever reads the outcome learns the same thing
from every verb:

- **A script** reads `$?` and branches on the number.
- **An agent or a tool** that asked for `--json` reads the document and
  branches on the row's name, which is the same row under a second spelling.
- **A person at a terminal** reads the line on standard error. A command that
  will not do what was asked says why there, under the verb's name.

You meet the table whenever you script fleet, and you meet a refusal whenever
fleet will not do what you asked.

## How it behaves

### The exit table

| Exit | Name | What it means | For example |
| --- | --- | --- | --- |
| 0 | `done` | The verb did what it was asked, and confirmed any change it made. | `fleet --version` |
| 1 | `refused` | The thing named is absent, is not in the state the verb needs, or already is so. Or a check the verb runs answered no. | `fleet stop` with nothing loaded; a verb run outside any fleet |
| 2 | `usage` | The call itself is wrong: a missing, unknown or invalid command, argument or flag; a name the verb takes from a fixed set it knows, such as a check name, that is not in that set; two flags that cannot go together; no command, or a family with no verb; or a question with no terminal to ask it on. | `fleet doctor --bogus`; `fleet doctor` given a check name no layer carries; `fleet create < /dev/null` |
| 3 | `could_not_tell` | Something the answer needs could not be read, so fleet cannot say what happened. It is never a success. | `fleet observe --once` with no seat list |
| 4 | `no_session` | The verb needs a seat's live session, and the seat has none. | `fleet seat attach` for a seat with no session |
| 5 | `no_collector` | No controller is consuming the event stream: there is no event stream, no projection, or a projection older than the verb allows. | `fleet status` with no projection; `fleet event show` with no stream |
| 6 | `wrong_seat_kind` | The seat is the wrong kind for the verb: a transient seat where a named one is needed, or the other way round. | `fleet event rest` on a transient seat; `fleet seat retire` on a named seat |

Rows 1 and 2 both cover a name that matches nothing, and which one applies
depends on where the name comes from. An item or a seat comes and goes, so a
named one that is absent is refused, exit 1: the call was right, and the
thing is not there. A name the verb takes from a fixed set it knows, such as
a check name for `fleet doctor`, can only be a typo when it is not in the
set, so the call itself is wrong, exit 2.

fleet exits with no other number. The one exception is `fleet seat attach`,
which hands your terminal to tmux. From then on the exit is tmux's, because
the terminal is yours and so is how long you stay, and tmux's own refusal
reaches you unchanged.

The name is the code that a `--json` refusal carries (see shared.output). The
number is already on `$?`, so the document names the row rather than
spelling the number a second time. Asking for `--json` never changes the
exit: the same refusal exits with the same number with the flag and without
it. `fleet status --json` is the exception, because it reads nothing beyond
the projection: once it prints the projection it exits 0, whatever the page
would exit (shared.output.R22).

The document never reads as a success that the exit does not back:
`fleet doctor --json` says `ok` is `true` only when it exits 0, and `false`
when it exits 1 or 3 (shared.output.R6, shared.output.R19).

A verb whose exit means something narrower, such as `fleet doctor` or
`fleet guard`, says so in its own spec. It still exits only with a number
from this table.

### How a refusal reads

A refusal is a verb that did not do what it was asked. It says why on a line
on standard error that starts with `fleet`, the verb as you typed it, and a
colon (`fleet stop:`, `fleet event show:`). The line names what the refusal
is about: the item, seat, event id, flag or service, or the file or directory
fleet looked at.

```sh
$ fleet event show 1
fleet event show: no event stream at <machine-dir>/events.jsonl
```

That call exits 5.

A check that answers no is not a refusal. `fleet pack check` finding a
defect did what it was asked, and its answer is a finding. A finding exits
1, the same row as a refusal, but it reports in its verb's own form and not
under the `fleet <verb>:` prefix.

### When the call is wrong

When the command line does not parse, fleet prints a block on standard error
whose first line starts with `error:`, and nothing on standard output. That
holds even under `--json`. The call exits 2.

When fleet is called with no command, or with a family and no verb (such as
`fleet pack`), it prints that level's help on standard error and nothing on
standard output, and exits 2. A caller that reached fleet meant something, so
the exit says the call was incomplete rather than a quiet 0.

A verb that would ask a question refuses instead when standard input is not
a terminal, so a prompt never waits on a script. The refusal line carries
the verb's one `fleet <verb>:` prefix and names every flag that answers the
question, and the call exits 2:

```sh
$ fleet create < /dev/null
fleet create: embedded or standalone? — stdin is not a terminal; answer it with --embedded or --standalone
```

### Could not tell, and refused

Exit 3 means fleet cannot say what happened, and it is never a success. A
verb that cannot say what happened is a third answer rather than a failure.

- **A verb that acted but cannot confirm that the act took effect** exits
  3, never 0. It may have done part of the work, so a script treats the
  outcome as unknown. The `fleet event` writers read their line back as the
  stream's last line, and exit 3 when it is not.
- **A file fleet reads that is not in the shape it must be** exits 3, though
  the call itself was right: `fleet event show` given an id that is on more
  than one line of the event stream, and `fleet seat spawn`, or
  `fleet dispatch` with no `--to`, finding that
  `[permissions] tool_commands` in `fleet.toml` is not a list of single
  command words.
- **When fleet stops on an error it did not turn into a refusal,** it writes
  a line starting `fleet:` that gives the error, then one indented line per
  cause that reads `caused by:` after its indent, and exits 3. Where the
  `fleet:` line would carry only the verb's name, fleet leaves it out: the
  verb's own `fleet <verb>:` line above it already names the verb, and the
  causes follow that line. Under `--json` (other than `fleet event tail`, see
  shared.output.R13) it writes the same lines and exits
  the same way, and also prints a refusal document whose code is
  `could_not_tell` on standard output (see shared.output).
- **A verb that cannot append a line of its own to the event stream** exits
  3. That holds for the `fleet event` writers (`woke`, `rest`, `handed-off`,
  `exited`, `clear-halt` and `step`), `fleet seat nudge`,
  `fleet seat attach --write`, and every other verb, `fleet observe --once`
  among them. `fleet observe` polling in a loop is the one exception: a loop
  is not a one-shot answer, so it says so on standard error and keeps
  polling. `fleet observe --once` polls once and ends, so it is a one-shot
  answer like any other verb.
- **A verb that cannot open or read the event stream it reads** exits 3:
  `fleet event tail`, given a stream that is there but will not open or
  read, cannot say what is in it, and an empty answer would read as a stream
  with no events.

For example, `fleet observe --once` with no seat list exits 3:

```sh
$ fleet observe --once
fleet observe: cannot read the seat list at <machine-dir>/config.json: …
  caused by: the seat list could not be read, so no poll ran
```

Exit 1 is different: fleet could tell, and the answer is no. A verb that
works on a project and finds no fleet for the working directory answers
so: nothing was unreadable, and the fleet the verb needs is absent, so it
refuses with exit 1, as `fleet start` always has. shared.locations says
where fleet looks for a fleet.

### When standard output closes early

When a verb's standard output is closed before the verb has finished writing
to it, as by `fleet prime | head -1`, the verb exits 0 and writes nothing
about it on standard error. Whoever reads the output chose to stop reading:
that is not a failure of the verb, and a line about it would only be noise in
a pipeline that worked as meant.

### Known gaps

Today's binary does not yet do all of the above. Each gap has an item that
closes it.

- **A `config.json` that does not parse (fleet-9sr5).** A verb looking for
  the fleet that finds the machine directory's `config.json` but cannot parse
  it refuses with exit 1, as though no fleet were named, rather than exit 3.
- **`fleet routine run` and a failed append (fleet-xboj).** It says so under
  a `fleet observe:` prefix and exits with the routine's outcome.
- **`fleet run` and a failed append where the run never started
  (fleet-36ab).** When `run.failed` does not reach the stream, it keeps the
  refusal's own exit.
- **A closed standard output (fleet-wtpr).** A verb whose standard output is
  closed before it has written everything, as by `fleet prime | head -1`,
  panics, writes the panic on standard error and exits 101, a number outside
  the table, instead of exiting 0 quietly.
- **`fleet observe --once` and a failed append (fleet-ponk).** It says so on
  standard error and exits as its poll otherwise would, 0 where the poll
  went well, instead of exiting 3.
- **`fleet event tail` and a stream it cannot read (fleet-lmi8).** Where the
  event stream is there but does not open or read, it treats the stream as
  empty and exits 0 instead of 3.

## Behaviour

- **shared.exit-codes.R1**: WHEN a verb has done what it was asked and has
  confirmed any change it made, fleet SHALL exit 0, the row named `done`.
- **shared.exit-codes.R2**: IF the thing a verb names is absent, is not in the
  state the verb needs, or already is so, or a check the verb runs answers
  no, THEN fleet SHALL exit 1, the row named `refused`.
- **shared.exit-codes.R3**: IF the call itself is wrong (a missing, unknown or
  invalid command, argument or flag; a name the verb takes from a fixed set
  it knows, such as a check name, that is not in that set; two flags that
  cannot go together; no command, or a family with no verb; or a question
  fleet would have to ask with no terminal to ask it on), THEN fleet SHALL
  exit 2, the row named `usage`. A named item or seat that is absent is not
  such a name: shared.exit-codes.R2 refuses it.
- **shared.exit-codes.R4**: IF something the answer needs cannot be read, so
  fleet cannot say what happened, THEN fleet SHALL exit 3, the row named
  `could_not_tell`.
  Today: a verb looking for the fleet that cannot parse the machine
  directory's `config.json` refuses with exit 1, as though no fleet were
  named (fleet-9sr5).
  Today: `fleet event tail`, where the event stream is there but does not
  open or read, treats it as empty and exits 0 (fleet-lmi8).
- **shared.exit-codes.R5**: IF a verb needs a seat's live session and the seat
  has none, THEN fleet SHALL exit 4, the row named `no_session`.
- **shared.exit-codes.R6**: IF a verb needs a controller consuming the event
  stream and none is (there is no event stream, no projection, or a
  projection older than the verb's own spec allows), THEN fleet SHALL exit 5,
  the row named `no_collector`.
- **shared.exit-codes.R7**: IF a verb is given a seat of the wrong kind (a
  transient seat where it needs a named one, or a named seat where it needs a
  transient one), THEN fleet SHALL exit 6, the row named `wrong_seat_kind`.
- **shared.exit-codes.R8**: fleet SHALL exit with one of the numbers 0 to 6
  and no other, except that once `fleet seat attach` has handed the terminal
  to tmux, the exit is tmux's.
  Today: a verb whose standard output is closed early, as by
  `fleet prime | head -1`, panics and exits 101 (fleet-wtpr).
- **shared.exit-codes.R9**: IF a verb has acted but cannot confirm that the
  act took effect, THEN fleet SHALL exit 3 and never 0.
- **shared.exit-codes.R10**: WHEN a verb that takes `--json` is called with
  it, fleet SHALL exit with the same number it exits with without the flag,
  except as shared.output.R22 says for `fleet status --json`.
- **shared.exit-codes.R11**: WHEN a verb called with `--json` prints a
  refusal document, fleet SHALL set its `code` to the name of the row the
  verb exits with.
- **shared.exit-codes.R12**: IF a verb refuses after its arguments parse (a
  refusal, not a check's finding), THEN fleet SHALL write a line on standard
  error that starts with `fleet`, the verb as typed without its arguments,
  and a colon, such as `fleet event show:` or `fleet stop:`.
- **shared.exit-codes.R13**: IF a verb refuses (a refusal, not a check's
  finding), THEN fleet SHALL name in its line on standard error what the
  refusal is about: the item, seat, event id, flag, service, or the file or
  directory it looked at.
- **shared.exit-codes.R14**: IF a verb would ask a question and standard input
  is not a terminal, THEN fleet SHALL refuse with one line, carrying the one
  `fleet <verb>:` prefix shared.exit-codes.R12 names, that names every flag
  that answers the question.
- **shared.exit-codes.R15**: IF the command line does not parse (a missing,
  unknown or invalid argument, an unknown command or flag, or two flags that
  cannot go together), THEN fleet SHALL write a block on standard error whose
  first line starts with `error:`, and nothing on standard output.
- **shared.exit-codes.R16**: IF `fleet` is called with no command, or with a
  family and no verb (such as `fleet pack`), THEN fleet SHALL print that
  level's help on standard error and nothing on standard output.
- **shared.exit-codes.R17**: IF fleet stops on an error it did not turn into a
  refusal, THEN fleet SHALL write on standard error, with or without
  `--json`, a line that starts with `fleet:` and gives the error, unless that
  line would carry only the verb's name, followed by one indented line for
  each cause, which reads `caused by:` after its indent, and exit 3.
- ~~**shared.exit-codes.R18**~~ retired 2026-10-13: it was
  shared.exit-codes.R9 applied to the `fleet event` writers, which
  shared.exit-codes.R9 already binds (fleet-mjni).
- **shared.exit-codes.R19**: IF `fleet event show` is given an id that is on
  more than one line of the stream, THEN fleet SHALL exit 3.
- **shared.exit-codes.R20**: IF `fleet seat spawn`, or `fleet dispatch` with no
  `--to`, finds that `[permissions] tool_commands` in `fleet.toml` is not a
  list of single command words, THEN fleet SHALL exit 3.
- ~~**shared.exit-codes.R21**~~ retired 2026-10-13: shared.exit-codes.R25
  gives every verb exit 3 on a failed append, the `fleet event` writers among
  them (fleet-mjni).
- ~~**shared.exit-codes.R22**~~ retired 2026-10-13: shared.exit-codes.R25
  gives every verb exit 3 on a failed append, `fleet seat nudge` and
  `fleet seat attach --write` among them (fleet-mjni).
- **shared.exit-codes.R23**: IF a verb that works on a project refuses
  because it finds no fleet for the working directory (shared.locations.R25
  says where it looks and which verbs refuse), THEN fleet SHALL exit 1, the
  row named `refused`.
- ~~**shared.exit-codes.R24**~~ retired 2026-10-13: it restated
  shared.output.R6 and shared.output.R19, which state `fleet doctor --json`'s
  `ok` (fleet-vd0k).
- **shared.exit-codes.R25**: IF a verb cannot append a line of its own to
  the event stream, other than `fleet observe` polling in a loop
  (shared.exit-codes.R26), THEN fleet SHALL exit 3, the row named
  `could_not_tell`. `fleet observe --once` is bound by this clause.
  Today: `fleet observe --once` says so on standard error and exits as its
  poll otherwise would, 0 where the poll went well (fleet-ponk).
  Today: `fleet routine run` says so and exits with the routine's outcome
  (fleet-xboj).
  Today: `fleet run`, where the run never started and `run.failed` does not
  reach the stream, keeps the refusal's own exit (fleet-36ab).
- **shared.exit-codes.R26**: WHILE `fleet observe` polls in a loop (without
  `--once`), IF it cannot append a line to the event stream, THEN fleet SHALL
  say so on standard error and keep polling rather than exit.
- **shared.exit-codes.R27**: IF a verb's standard output is closed before the
  verb has finished writing to it, THEN fleet SHALL exit 0, the row named
  `done`, and write nothing about it on standard error.
  Today: the verb panics, writes the panic on standard error and exits 101
  (fleet-wtpr).

## Non-functional

None.

## Interfaces

- Scripts and the adapters' hooks read `$?`, and rely on R1 to R7 meaning the
  same thing from every verb.
- Every feature spec cites these rows by id rather than restating them. A
  verb whose exit means something narrower (`fleet doctor`, `fleet guard`,
  `fleet prime`, `fleet run`, `fleet routine check`, `fleet status`) says so
  in its own spec, citing the rows it narrows. It still exits only with a
  number from the table (R8).
- The `--json` document's refusal code is the row's name from R1 to R7 (see
  shared.output).

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
