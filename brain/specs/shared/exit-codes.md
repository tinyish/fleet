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
| 0 | `done` | The verb did what it was asked, and any write it made reads back. | `fleet --version` |
| 1 | `refused` | The thing named is absent, is not in the state the verb needs, or already is so. Or a check the verb runs answered no. | `fleet stop` with nothing loaded |
| 2 | `usage` | The call itself is wrong: a missing, unknown or invalid command, argument or flag; two flags that cannot go together; no command, or a family with no verb; or a question with no terminal to ask it on. | `fleet doctor --bogus`; `fleet create < /dev/null` |
| 3 | `could_not_tell` | Something the answer needs could not be read, so fleet cannot say what happened. It is never a success. | `fleet observe --once` with no seat list |
| 4 | `no_session` | The verb needs a seat's live session, and the seat has none. | `fleet seat attach` for a seat with no session |
| 5 | `no_collector` | No controller is consuming the event stream: there is no event stream, no projection, or a projection older than the verb allows. | `fleet status` with no projection; `fleet event show` with no stream |
| 6 | `wrong_seat_kind` | The seat is the wrong kind for the verb: a transient seat where a named one is needed, or the other way round. | `fleet event rest` on a transient seat; `fleet seat retire` on a named seat |

fleet exits with no other number. The one exception is `fleet seat attach`,
which hands your terminal to tmux. From then on the exit is tmux's, because
the terminal is yours and so is how long you stay, and tmux's own refusal
reaches you unchanged.

The name is the code that a `--json` refusal carries (see shared.output). The
number is already on `$?`, so the document names the row rather than
spelling the number a second time. Asking for `--json` never changes the
exit: the same refusal exits with the same number with the flag and without
it. `fleet status --json` is the exception, because it reads nothing beyond
the projection; the status area's spec says what it exits.

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

- **A verb that acted but cannot read back that the act took effect** exits
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
  causes follow that line.
- **The verbs that write a line of their own to the event stream** and that
  R21 and R22 name exit 3 when they cannot append it: the `fleet event`
  writers (`woke`, `rest`, `handed-off`, `exited`, `clear-halt` and `step`),
  `fleet seat nudge` and `fleet seat attach --write`.

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

### Where today's exits do not fit their row

Some of today's exits do not match the meaning in the table. Each one has an
approved change, listed after it.

- **`fleet doctor --json` with a finding or a could-not-tell.** Today the
  document's `ok` is `true` whatever the checks said. `ok` will be `true`
  only on exit 0 (fleet-accn).

## Behaviour

- **shared.exit-codes.R1**: WHEN a verb has done what it was asked and any
  write it made reads back, fleet SHALL exit 0, the row named `done`.
- **shared.exit-codes.R2**: IF the thing a verb names is absent, is not in the
  state the verb needs, or already is so, or a check the verb runs answers
  no, THEN fleet SHALL exit 1, the row named `refused`.
- **shared.exit-codes.R3**: IF the call itself is wrong (a missing, unknown or
  invalid command, argument or flag; two flags that cannot go together; no
  command, or a family with no verb; or a question fleet would have to ask
  with no terminal to ask it on), THEN fleet SHALL exit 2, the row named
  `usage`.
- **shared.exit-codes.R4**: IF something the answer needs cannot be read, so
  fleet cannot say what happened, THEN fleet SHALL exit 3, the row named
  `could_not_tell`.
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
- **shared.exit-codes.R9**: IF a verb has acted but cannot read back that the
  act took effect, THEN fleet SHALL exit 3 and never 0.
- **shared.exit-codes.R10**: WHEN a verb that takes `--json` is called with
  it, fleet SHALL exit with the same number it exits with without the flag,
  except for `fleet status --json`, whose exit the status area's spec states.
- **shared.exit-codes.R11**: IF a verb refuses under `--json`, THEN fleet
  SHALL print a refusal document whose code is the name of the row it exits
  with (see shared.output).
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
  refusal, THEN fleet SHALL write on standard error a line that starts with
  `fleet:` and gives the error, unless that line would carry only the verb's
  name, followed by one indented line for each cause, which reads
  `caused by:` after its indent, and exit 3.
- **shared.exit-codes.R18**: IF `fleet event woke`, `rest`, `handed-off`,
  `exited`, `clear-halt` or `step` has written its line and the stream's last
  line is not that line, THEN fleet SHALL exit 3.
- **shared.exit-codes.R19**: IF `fleet event show` is given an id that is on
  more than one line of the stream, THEN fleet SHALL exit 3.
- **shared.exit-codes.R20**: IF `fleet seat spawn`, or `fleet dispatch` with no
  `--to`, finds that `[permissions] tool_commands` in `fleet.toml` is not a
  list of single command words, THEN fleet SHALL exit 3.
- **shared.exit-codes.R21**: IF `fleet event woke`, `rest`, `handed-off`,
  `exited`, `clear-halt` or `step` cannot append its line to the event
  stream, THEN fleet SHALL exit 3.
- **shared.exit-codes.R22**: IF `fleet seat nudge` or
  `fleet seat attach --write` cannot append its line to the event stream,
  THEN fleet SHALL exit 3.
- **shared.exit-codes.R23**: IF a verb that works on a project refuses
  because it finds no fleet for the working directory (shared.locations.R25
  says where it looks and which verbs refuse), THEN fleet SHALL exit 1, the
  row named `refused`.
- **shared.exit-codes.R24**: WHEN `fleet doctor --json` has run its checks,
  fleet SHALL print a document whose `ok` is `true` whether it exits 0, 1 or
  3 (see shared.output).
  Changing: fleet-accn: `ok` is `true` only on exit 0. On a finding or a
  could-not-tell, `ok` is `false`, the code is the exit's row name, and the
  rows stay in `data`.

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

- [NEEDS CLARIFICATION: should `fleet routine run`, `fleet run` where the run never started, and `fleet observe` also exit 3 when they cannot append to the event stream? Today each answers its own way: routine run says so and exits with the routine's outcome, run keeps the refusal's own exit, and observe says so and keeps polling (fleet-6dn4).]

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
