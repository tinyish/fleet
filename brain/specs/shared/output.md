# Output

Uses: shared.exit-codes

## What it is for

Most verbs a program drives take `--json`, which asks for the outcome as a
JSON document on standard output instead of lines a person reads. Every such
document has the same outer shape, the envelope: whether the verb did what it
was asked, which verb answered, and then either what it did or why it
refused. A program parses one document and learns what happened, rather than
parsing lines written for a person.

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
| `ok` | always | `true`: read `data`. `false`: read `refusal`. |
| `verb` | always | which verb answered: the verb as typed, without `fleet` and its arguments, such as `doctor`, `event show` or `seat add` |
| `data` | `ok` is `true` | the outcome; its shape is the verb's own spec |
| `refusal` | `ok` is `false` | an object with `code` and `why` |
| `refusal.code` | `ok` is `false` | the exit table's row name, such as `refused`, `usage`, `could_not_tell` or `no_collector`; a script branches on it |
| `refusal.why` | `ok` is `false` | the reason, as the refusal line on standard error says it |

The keys come in that order: `ok`, `verb`, then `data` or `refusal`. A
document never carries both `data` and `refusal`.

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
beyond the projection, so where the page exits 3 over a section it could not
fill, `fleet status --json` prints the projection and exits 0
(shared.exit-codes.R10). What the projection holds is the status area's
spec.

```sh
$ fleet status --json
{"ok":false,"verb":"status","refusal":{"code":"no_collector","why":"there is no projection at <machine-dir>/projection.json to print (No such file or directory (os error 2)) — run `fleet start`"}}
fleet status: there is no projection at <machine-dir>/projection.json to print (No such file or directory (os error 2)) — run `fleet start`
$ echo $?
5
```

### When there is no document

- **The command line does not parse.** fleet writes its `error:` block on
  standard error and nothing on standard output, and exits 2
  (shared.exit-codes.R15).
- **fleet stops on an error it did not turn into a refusal.** For a verb
  other than `fleet event tail`, fleet writes its `fleet:`
  and `caused by:` lines on standard error (shared.exit-codes.R17), nothing
  on standard output, and exits 3. This will print a refusal document
  (fleet-iox1).

### `fleet doctor`

Today `fleet doctor --json`, once its checks have run, prints a success
document whatever its checks said: `ok` is `true` and `data` carries every
row, while `$?` is 0, 1 or 3 (shared.exit-codes.R24). A ruling changes this
(fleet-accn): `ok` will be `true` only on exit 0, and a finding or a
could-not-tell will print `ok` as `false` with the exit's row name as the
code, keeping every row in `data`. A `fleet doctor` that refuses before any
check runs, such as for a check name no layer carries, prints a refusal
document like any verb.

### What is changing

Each of these is an approved change. Until its bead lands, the clauses state
what fleet does today, with a `Changing:` line after each one.

- **`ok` follows the exit** (fleet-accn). `fleet doctor --json` with a
  finding or a could-not-tell prints `ok` as `false`, with the exit's row
  name as the code, and keeps every row in `data`.
- **An untranslated error answers in the envelope** (fleet-iox1). Where fleet
  stops on an error it did not turn into a refusal, it prints a refusal
  document whose code is `could_not_tell` and whose `why` is the error's
  reason, still writes its `fleet:` and `caused by:` lines on standard error,
  and exits 3.

## Behaviour

- **shared.output.R1**: WHEN a verb other than `fleet event tail` is called
  with `--json` and its command line parses, fleet SHALL print exactly one
  JSON document on standard output, except where shared.output.R13 applies.
  Changing: fleet-iox1: the shared.output.R13 exception goes; that path
  prints one refusal document too.
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
- **shared.output.R6**: WHEN a verb called with `--json` does not refuse (a
  check's finding is not a refusal, shared.exit-codes.R2), fleet SHALL print
  a document carrying `ok` as `true` and a `data` key, whose content the
  verb's own spec states, and no `refusal` key. `fleet doctor` answers so
  once its checks have run, whatever its exit (shared.exit-codes.R24).
  Changing: fleet-accn: `ok` is `true` only on exit 0; `fleet doctor` with a
  finding or a could-not-tell answers as shared.output.R7 says, and still
  carries every row in `data`.
- **shared.output.R7**: IF a verb called with `--json` refuses, THEN fleet
  SHALL print a document carrying `ok` as `false` and a `refusal` key, and no
  `data` key.
  Changing: fleet-accn: `fleet doctor` with a finding or a could-not-tell
  also prints `ok` as `false`, with the exit's row name as its code
  (shared.exit-codes.R24), and keeps every row in `data`.
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
  under `--json` in the order `ok`, `verb`, then `data` or `refusal`.
- **shared.output.R12**: WHEN `fleet event tail` is called with `--json`,
  fleet SHALL print one document per event it prints (shared.output.R6), each
  on its own line, and no document when it prints no event.
- **shared.output.R13**: IF a verb other than `fleet event tail`, called with
  `--json`, stops on an error it did not turn into a refusal
  (shared.exit-codes.R17), THEN fleet SHALL print nothing on standard output.
  Changing: fleet-iox1: fleet prints a refusal document whose code is
  `could_not_tell` and whose `why` is the error's reason, keeps the lines
  shared.exit-codes.R17 names on standard error, and exits 3.
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

## Non-functional

None.

## Interfaces

- Scripts, the SDK, agents and the doctor checks read standard output as the
  envelope: they branch on `ok`, then read `data` or branch on the
  `refusal`'s `code` (shared.exit-codes.R11). `$?` beside the document is
  shared.exit-codes.R10's.
- The verbs that take `--json` today are `fleet dispatch`, `deliver`, `hold`,
  `clear`, `review` and `land`; `fleet item show` and `item list`;
  `fleet seat add`, `spawn`, `feed` and `retire`; `fleet doctor`;
  `fleet event tail` and `event show`; and `fleet status`. Each verb's spec
  says it takes the flag,
  states what its `data` holds, and cites these clauses rather than
  restating them.
- The SDK replays the event stream through `fleet event tail --json`
  (shared.output.R12).
- A doctor check the defaults carry, `adopt-board`, reads
  `fleet item list --ready --json`, so the defaults' own checks rely on the
  envelope too.

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
