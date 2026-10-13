# Output

Uses: shared.exit-codes

## What it is for

Most verbs a program drives take `--json`, which asks for the outcome as a
JSON document on standard output instead of lines a person reads. Every such
document has the same outer shape, the envelope: whether the verb did what it
was asked, which verb answered, and then what it did or why it refused, or,
for a `fleet doctor` whose checks did not all pass, both. A program parses
one document and learns what happened, rather than parsing lines written for
a person.

You meet it whenever something other than a person reads fleet's answer:

- a script you write, which runs a verb and branches on its answer;
- the SDK, which replays the event stream through `fleet event tail --json`;
- an agent that runs a verb and reads the answer back;
- the doctor checks the defaults carry, such as `adopt-board`, which reads
  `fleet item list --ready --json`.

What a verb puts inside the envelope is that verb's own spec. `$?` beside the
document is the exit table's row, the same with the flag as without it
(shared.exit-codes.R10).

## How it behaves

### A verb that did what it was asked

It prints one document on one line: `ok` is `true`, `verb` names the verb,
and `data` carries the outcome.

```sh
$ fleet event show 000000000000000018d2e9e7a6a1e001-00000001 --json
{"ok":true,"verb":"event show","data":{"actor":{"id":"01a13230-9867-75c2-b86e-7fefb1ce9cad","kind":"seat"},"id":"000000000000000018d2e9e7a6a1e001-00000001","kind":"seat.woke","payload":{},"seq":1,"ts":"2026-10-12T00:00:01.000Z"}}
$ echo $?
0
```

### A verb that refuses

It prints one document on one line: `ok` is `false`, `verb` names the verb,
and `refusal` carries the row's name as `code` and the reason as `why`. It
still writes its refusal line on standard error, because a person watching
the call reads that line. The `why` is that line's text after
`fleet <verb>: `, so the refusal is said once, in two places.

```sh
$ fleet event show nosuchid --json
{"ok":false,"verb":"event show","refusal":{"code":"refused","why":"no event nosuchid"}}
fleet event show: no event nosuchid
$ echo $?
1
```

The first line is standard output, the second standard error. The `code` is
the exit table's row name, not its number (shared.exit-codes.R11): the number
is already on `$?`, and a second spelling of it would be a second thing to
keep in step.

### The keys

| Key | Present when | What a script reads from it |
| --- | --- | --- |
| `ok` | always | `true`: read `data`. `false`: read `refusal`, and for `fleet doctor` once its checks have run, `data` too. |
| `verb` | always | which verb answered: the verb as typed, without `fleet` and its arguments, such as `doctor`, `event show` or `seat add` |
| `data` | `ok` is `true`, and in `fleet doctor`'s document once its checks have run | the outcome; its shape is the verb's own spec |
| `refusal` | `ok` is `false` | an object with `code` and `why` |
| `refusal.code` | `ok` is `false` | the exit table's row name, such as `refused`, `usage`, `could_not_tell` or `no_collector`; a script branches on it |
| `refusal.why` | `ok` is `false` | the reason, as the refusal line on standard error says it; for `fleet doctor`'s finding or could-not-tell, its summary line; for an error fleet did not turn into a refusal, the text of every line it writes on standard error, each without its `fleet: ` or `caused by: ` prefix, joined by `: ` (shared.output.R13) |

The keys come in that order: `ok`, `verb`, then `data` or `refusal`. Only
`fleet doctor`, once its checks have run and not all passed, prints a
document carrying both, with `refusal` before `data`.

### Standard output carries the document and nothing else

Under `--json`, standard output holds the document and no other line. Each
document is on one line of its own, ending in a newline. What happens to the
lines a verb prints on standard output without the flag depends on the verb:

| Verb | Under `--json`, its standard output lines |
| --- | --- |
| `fleet doctor`, `fleet dispatch`, `deliver`, `hold`, `clear`, `review`, `land`, `fleet seat feed`, `seat retire` | move to standard error |
| `fleet item show`, `item list`, `fleet event show`, `event tail`, `fleet seat add`, `spawn` | are printed nowhere: the document stands in for them, as they are the answer itself (the item, the events, the seat's id or name) |

### `fleet event tail`

`fleet event tail --json` prints one success document per event it prints,
one per line, and no document at all when it prints no event: it has done
what it was asked, so it exits 0 (shared.exit-codes.R1) with an empty
standard output. It refuses, if at all, before it prints any event, so a
refusal is the only document on standard output. The SDK replays the stream
from these documents; what each `data` holds is the event verbs' own spec.

### `fleet status`

`fleet status --json` prints one success document whose `data` is the
projection the controller last published, as the file holds it, on one line.
Where there is no projection it prints a refusal document whose code is
`no_collector`, and exits 5 (shared.exit-codes.R6). A projection that does
not parse, or is a version this binary does not read, is a refusal too,
whose code is `could_not_tell`, exiting 3. Under the flag it reads nothing
beyond the projection, so where the page exits 3 over a section it could
not fill, `fleet status --json` prints the projection and exits 0
(shared.exit-codes.R1, shared.output.R18). What the projection holds is the
status area's spec.

```sh
$ fleet status --json
{"ok":false,"verb":"status","refusal":{"code":"no_collector","why":"there is no projection at <machine-dir>/projection.json to print (No such file or directory (os error 2)) — run `fleet start`"}}
fleet status: there is no projection at <machine-dir>/projection.json to print (No such file or directory (os error 2)) — run `fleet start`
$ echo $?
5
```

### When fleet stops on an error of its own

Under `--json`, a verb other than `fleet event tail` that stops on an error
it did not turn into a refusal still answers in the envelope. It prints one
refusal document whose `code` is `could_not_tell` and whose `why` is the text
of every line it writes on standard error (shared.exit-codes.R17), each
without its `fleet: ` or `caused by: ` prefix, joined by `: `. It still
writes those lines and exits 3, as it does without the flag, so a script
reading the document and a person reading the lines learn the same thing.

### When there is no document

- **The command line does not parse.** fleet writes its `error:` block on
  standard error and nothing on standard output, and exits 2
  (shared.exit-codes.R15).

### `fleet doctor`

Once its checks have run, `fleet doctor --json` prints one document carrying
every row in `data`, whatever its checks said, and `ok` follows the exit:
`true` only on exit 0. A finding (exit 1) or a could-not-tell (exit 3)
prints `ok` as `false`, with a `refusal` before the rows: its `code` is the
exit's row name, `refused` or `could_not_tell`, and its `why` is the summary
line `fleet doctor` writes on standard error, after `doctor `.

```sh
$ fleet doctor --json
{"ok":false,"verb":"doctor","refusal":{"code":"refused","why":"7 checks — 6 pass, 1 finding, 0 could not tell"},"data":{"checks":[…],"counts":{"could_not_tell":0,"finding":1,"pass":6},"verdict":"finding"}}
$ echo $?
1
```

A script reads `ok` as the doctor's answer and `data` for which check said
what. A finding is still not a refusal (shared.exit-codes.R2): `fleet
doctor` writes no `fleet doctor:` line for it, only its rows and its summary,
which move to standard error under the flag. A `fleet doctor` that refuses
before any check runs, such as for a check name no layer carries, prints a
refusal document like any verb, with no `data`.

## Behaviour

- **shared.output.R1**: WHEN a verb other than `fleet event tail` is called
  with `--json` and its command line parses, fleet SHALL print exactly one
  JSON document on standard output.
- **shared.output.R2**: WHEN a verb is called with `--json`, fleet SHALL
  print on standard output nothing but the documents shared.output.R1,
  shared.output.R12 and shared.output.R14 name: no line the verb prints on
  standard output without the flag is printed there with it.
- **shared.output.R3**: fleet SHALL print each document it prints under
  `--json` on one line of its own, ending in a newline.
- **shared.output.R4**: fleet SHALL make each document it prints under
  `--json` a JSON object carrying `ok`, which is `true` or `false`, and
  `verb`.
- **shared.output.R5**: fleet SHALL set each document's `verb` to the verb as
  typed, without `fleet` and without its arguments or flags, such as
  `doctor`, `event show` or `seat add`: the same words its refusal line names
  after `fleet` (shared.exit-codes.R12).
- **shared.output.R6**: WHEN a verb called with `--json` exits 0
  (shared.exit-codes.R1), fleet SHALL print a document carrying `ok` as
  `true` and a `data` key, whose content the verb's own spec states, and no
  `refusal` key.
- **shared.output.R7**: IF a verb called with `--json` refuses (a check's
  finding is not a refusal, shared.exit-codes.R2), THEN fleet SHALL print a
  document carrying `ok` as `false` and a `refusal` key, and no `data` key.
- **shared.output.R8**: IF a verb called with `--json` refuses, THEN fleet
  SHALL make the document's `refusal` a JSON object carrying `code`, which
  shared.exit-codes.R11 fixes, and `why`.
- **shared.output.R9**: IF a verb called with `--json` refuses, THEN fleet
  SHALL set the `refusal`'s `why` to the text of its refusal line on standard
  error after `fleet <verb>: `, character for character
  (shared.exit-codes.R12, shared.exit-codes.R13).
- **shared.output.R10**: IF a verb called with `--json` refuses, THEN fleet
  SHALL still write its refusal line on standard error, as it does without
  the flag (shared.exit-codes.R12).
- **shared.output.R11**: fleet SHALL put the keys of each document it prints
  under `--json` in the order `ok`, `verb`, then `data` or `refusal`, and in
  a document carrying both (shared.output.R19), `refusal` before `data`.
- **shared.output.R12**: WHEN `fleet event tail` is called with `--json`,
  fleet SHALL print one document per event it prints (shared.output.R6), each
  on its own line, and no document when it prints no event.
- **shared.output.R13**: IF a verb other than `fleet event tail`, called with
  `--json`, stops on an error it did not turn into a refusal
  (shared.exit-codes.R17), THEN fleet SHALL print on standard output one
  document carrying `ok` as `false` and a `refusal` whose `code` is
  `could_not_tell` and whose `why` is the text of every line
  shared.exit-codes.R17 writes, each without its `fleet: ` or `caused by: `
  prefix, joined by `: `, and no `data` key.
- **shared.output.R14**: IF `fleet event tail` called with `--json` refuses,
  THEN fleet SHALL print its refusal document (shared.output.R7) and no
  event document.
- **shared.output.R15**: WHEN `fleet doctor`, `fleet dispatch`, `deliver`,
  `hold`, `clear`, `review` or `land` is called with `--json`, fleet SHALL
  write on standard error each line the verb writes on standard output
  without the flag.
- **shared.output.R16**: WHEN `fleet seat feed` or `fleet seat retire` is
  called with `--json`, fleet SHALL write on standard error each line the
  verb writes on standard output without the flag: the line naming the seat
  fed or retired, and the line saying the retired seat's work branch was
  deleted or kept.
- **shared.output.R17**: WHEN `fleet item show`, `item list`,
  `fleet event show`, `event tail`, `fleet seat add` or `spawn` is called
  with `--json`, fleet SHALL print what the verb prints on standard output
  without the flag on neither standard output nor standard error: the
  document stands in for it.
- **shared.output.R18**: WHEN `fleet status` is called with `--json` and a
  projection is published that it reads, fleet SHALL print a success
  document whose `data` is that projection's JSON value.
- **shared.output.R19**: WHEN `fleet doctor` called with `--json` has run its
  checks and exits 1 or 3, fleet SHALL print a document carrying `ok` as
  `false`, a `refusal` whose `code` is the name of the row it exits with
  (shared.exit-codes.R11), and a `data` key whose content is the same as on
  exit 0.
- **shared.output.R20**: WHEN fleet prints the document shared.output.R19
  names, fleet SHALL set its `refusal`'s `why` to the text of the summary
  line `fleet doctor` writes on standard error after `doctor `, character for
  character.

## Non-functional

None.

## Interfaces

- Scripts, the SDK, agents and the doctor checks read standard output as the
  envelope: they branch on `ok`, then read `data` or branch on the
  `refusal`'s `code` (shared.exit-codes.R11); `fleet doctor`'s document
  carries both on a finding or a could-not-tell (shared.output.R19). `$?`
  beside the document is shared.exit-codes.R10's.
- The verbs that take `--json` today are `fleet dispatch`, `deliver`, `hold`,
  `clear`, `review` and `land`; `fleet item show` and `item list`;
  `fleet seat add`, `spawn`, `feed` and `retire`; `fleet doctor`;
  `fleet event tail` and `event show`; and `fleet status`. Each verb's spec
  says it takes the flag, states what its `data` holds, and cites these
  clauses rather than restating them.
- The SDK replays the event stream through `fleet event tail --json`
  (shared.output.R12).
- A doctor check the defaults carry, `adopt-board`, reads
  `fleet item list --ready --json`, so the defaults' own checks rely on the
  envelope too.

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
