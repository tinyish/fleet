# `fleet doctor`

Uses: shared.exit-codes, shared.output, shared.locations

## What it is for

`fleet doctor` asks the machine and the project whether what fleet runs on is
in order: the tmux the controller starts sessions in, the runtime a pack pins,
the agent and the store the fleet names, and whatever else a pack chooses to
look at. Each thing is a doctor check: a small script, carried by the binary's
defaults or by an installed pack, that looks at one thing and answers through
its exit code. `fleet doctor` runs the checks and prints one row per answer,
then a verdict over all of them.

You meet it right after `fleet create`, to see what is missing before the
first `fleet start`, and again after upgrading tmux, Claude Code, bd or a
pack. A script, an agent or a skill meets it through `--json` and its exit
code.

The doctor only looks. It writes nothing, starts no session, and asks the
adapters only questions that read. Proving that an adapter keeps the rest of
its contract is the job of `fleet store check` and `fleet agent check`: they
write a scratch store or spend a turn, and a doctor does neither.

## How it behaves

### Running it

```sh
$ fleet doctor                     # every check, then the two built-in rows
$ fleet doctor tmux-version        # only the checks named
$ fleet doctor --json              # one document on standard output
$ fleet doctor --packs-dir <DIR>   # read the packs from <DIR>
```

`fleet doctor` acts in a project, which it finds from the working directory
as shared.locations says (shared.locations.R21 to shared.locations.R24).
Where it finds no fleet, or the directory declares a project this machine
has not registered, it refuses before any check runs (shared.locations.R25,
shared.locations.R26, shared.locations.R35), with exit 1. Where
the machine's `config.json` or `projects.toml` is there but will not read,
it cannot tell which fleet it is in, and exits 3 without running a check
(shared.locations.R38). Like every verb but `fleet guard`, it refuses a
machine directory named by a relative path, or no home at all, before it
reads anything (shared.locations.R30 to shared.locations.R32). Every check
runs from the
project's root.

`--packs-dir <DIR>` reads the installed packs from `<DIR>` in place of
`packs/` in the machine directory, as shared.locations says. The packs area's
spec says what else it moves.

### Where the checks come from

A doctor check is a `doctor/<name>/` directory in one of the pack layers: the
defaults built into the `fleet` binary at the bottom, and each installed pack
above them. The directory's name is the check's name.

| File | What fleet reads from it |
| --- | --- |
| `doctor/<name>/doctor.toml` | `description`, one line on what the check is for, and `run`, the script to run, named relative to the entry. fleet reads no other key: judging the file is `fleet pack check`'s job. |
| `doctor/<name>/<script>` | the script `run` names, run with `sh` |

A pack above can replace a lower layer's check: its `doctor.toml`, its
script, or both, because each file resolves through the layers on its own.
A pack can also carry checks of its own under new names. The order of the
layers is the packs area's to state. The layer a row names is the one that
carries the check's `doctor.toml`, because that file is what says the check
exists.

The binary's defaults carry five checks:

| Check | What it looks at |
| --- | --- |
| `adopt-board` | whether the project's board already carries names or conventions of its own to map onto fleet's |
| `fleet-packs-version` | whether the packs installed from fleet-packs are at the tag this binary supports |
| `guards-installed` | whether each guard has the targets it needs configured |
| `runtime-version` | whether the runtime a pack pins answers, at the version it pins |
| `tmux-version` | whether tmux is installed at a release fleet supports, and whether fleet's own tmux server is running |

A check decides its own verdict. What each of these holds to is the check's
own business, as it is for a pack's checks, such as the claude-code pack's
`claude-code-version` and the bd pack's `bd-version`, whose pins belong to
fleet-packs.

### Which checks run, and in what order

With no check named, every check the layers carry runs. Naming checks runs
only those, each once however often it is named. A name that no layer carries
and that is not a built-in row is a typo: running the rest would answer a
question nobody asked. So fleet refuses before any check runs, on a
`fleet doctor:` line that names the unknown name and every check the layers
carry. That call exits 2 (shared.exit-codes.R3): a check name is taken from
a fixed set the doctor knows, so a name outside it is a wrong call. Exit 1
would read as a finding.

The checks the layers carry run in ascending byte order of their names,
whatever order you typed them in. The two built-in rows come after them:
`store-adapter` first, then `agent-adapter`. Checks run one at a time, and
each row is printed as soon as its check finishes, before the next starts.

### How a check runs

fleet runs the script with `sh`, from the project's root. A check never waits
on a prompt: its standard input is at end of file, because a check has nobody
to answer one. fleet sets three variables, and everything else in the
caller's environment reaches the check unchanged, `PATH` included, so the
check finds `fleet` and its own tools where the person who typed
`fleet doctor` would. `runtime-version` is the one check run on another
`PATH` (below). The three variables:

| Variable | What it names |
| --- | --- |
| `FLEET_PACK_DIR` | the directory of the layer that carries the check's `doctor.toml`, whose manifest the check measures (`runtime-version` differs, below) |
| `FLEET_PROJECT` | the project's root |
| `FLEET_BIN` | the `fleet` binary that is running, so a check that calls back into fleet reaches the one the person ran. Every check the defaults carry calls back through it. |

A check that hangs never hangs the doctor. A check is given 60 s. One still
running then is killed, together with everything it started in its process
group, and could not tell. A check is a script somebody else wrote, and one
that hangs must not hang the verb that runs it. A healthy check answers in
well under a second, so one that runs for 60 s is waiting on something that
will not come.

A check stopped at its bound keeps what it printed before the stop, as any
other check that ran does: its last line is the row's line, and every line
it printed follows the row, indented. Those are the lines a person acts on,
and a timed-out check is no exception. Where it printed nothing, the row's
line names the bound.

### The verdict

A check's verdict is its exit code, never what it printed. What a check knows
is the check's own, and a parser in fleet would be a second opinion about it.

| The check | The row's verdict |
| --- | --- |
| exits 0 | `pass` |
| exits 1 | `finding` |
| exits any other code (3 among them), is killed by a signal, runs past 60 s, or cannot be started | `could not tell` |
| has no `doctor.toml`, a `doctor.toml` that does not parse, no `run` key, or a script no layer carries | `could not tell`, and nothing runs |

### What a row shows

```sh
$ fleet doctor <check> tmux-version
finding <check> (<layer>) — <the last line <check> printed>
  <every line <check> printed, in turn>
pass tmux-version (defaults) — <the last line tmux-version printed>
doctor 2 checks — 1 pass, 1 finding, 0 could not tell
```

That call exits 1.

A row starts with its verdict (`pass`, `finding` or `could not tell`), then
names the check and, in parentheses, its layer. Then, after ` — `, comes one
line from the check, with the whitespace around it trimmed: the last non-blank line it
printed on standard output, or on standard error where it printed nothing on
standard output. Where it printed nothing at all, the line says so and names
its exit, says it was killed by a signal, or names the 60 s bound it ran
past. Where nothing ran, the line says why.

A row that did not pass is followed by every non-blank line the check printed,
as it printed it, standard output's first and then standard error's, each
indented by two spaces. Those are the lines a person acts on, and the row's
own line is only the last of them.

After the last row comes one summary line,
`doctor <N> checks — <P> pass, <F> finding, <C> could not tell`, where `<N>`
is the number of rows (`doctor 1 check` for one) and the other three count
the rows with each verdict.

Without `--json`, the rows, their indented lines and the summary are printed
on standard output.

### runtime-version

`runtime-version` is run once for each installed pack whose `pack.toml`
declares a `[runtime]` table, in the layers' order, highest first. Its
`FLEET_PACK_DIR` names that pack, and it runs on the `PATH` `fleet run` gives
that pack's workflows, which the runs area owns, so a row here and a run's
refusal measure the same binary. Each row names the pack it measured as
`runtime-version for <pack>`.

Where no installed pack declares a `[runtime]` table, `runtime-version` is one
passing row whose line says nothing is pinned, and nothing runs. A pack whose
`pack.toml` cannot be read or does not parse gets a could-not-tell row for
that pack, saying why, so you learn which pack could not be measured.

### The two built-in rows

Two rows are fleet's own, not a pack's. Each names `built-in` as its layer and
runs after every check the layers carry. Each opens its adapter the way every
other verb opens it and asks it `version` and `capabilities`, and nothing
else.

| Row | Asks | `pass` | `finding` | `could not tell` |
| --- | --- | --- | --- | --- |
| `store-adapter` | the store adapter the project's `[store] adapter` names | both answer and the capabilities validate | the capabilities do not validate | the adapter is not opened, or a call does not answer in time or answers something fleet cannot read |
| `agent-adapter` | the agent adapter the fleet's `[agent] adapter` names | both answer, a version is installed and the capabilities validate | `version` answers null (no agent installed for the adapter to drive), or the capabilities do not validate | the agent setting cannot be read, the adapter is not opened, or a call does not answer in time or answers something fleet cannot read |

Each call `store-adapter` makes is given 60 s. Each call `agent-adapter`
makes is given the agent's bound, 20 s unless `FLEET_AGENT_TIMEOUT_MS` sets
another; the agent area owns that variable.

Where the adapter otherwise passes, an installed agent version that is not
among the versions the adapter was measured against still passes. Its line names the installed version and the
measured ones, and says to re-measure. A finding there would gate whatever
consults the doctor.

No pack replaces a built-in row. A layer's `doctor/store-adapter/` or
`doctor/agent-adapter/` entry is not run, and the row stays the built-in one.
The check that a pack's own adapter answers is not that pack's to write.

### The exit

| Exit | When |
| --- | --- |
| 0 | every row passed (shared.exit-codes.R1) |
| 1 | at least one row is a finding and none could not tell (shared.exit-codes.R2) |
| 3 | at least one row could not tell, whatever the others said (shared.exit-codes.R4) |

3 wins over 1 because a set of checks nobody could read is not a set that
found nothing. A finding is not a refusal: it is reported in rows, not on a
`fleet doctor:` line.

### `--json`

`fleet doctor --json` prints the envelope shared.output describes. Once its
checks have run, the document on exit 0 is shared.output.R6's, and the
document on exit 1 or 3 is the one shared.output.R19 and shared.output.R20
describe, with every row in `data` either way (shared.output.R6,
shared.output.R19). The rows, their
indented lines and the summary move to standard error (shared.output.R15). A
refusal before any check runs prints a refusal document (shared.output.R7).
The exit is the same with the flag as without it (shared.exit-codes.R10).

`data` holds:

| Key | What it holds |
| --- | --- |
| `checks` | one object per row, in the order the rows were printed |
| `counts` | `pass`, `finding` and `could_not_tell`: how many rows had each verdict |
| `verdict` | the verdict over every row: `pass`, `finding` or `could_not_tell`, matching the exit |

Each object in `checks` holds:

| Key | What it holds |
| --- | --- |
| `name` | the check's name |
| `layer` | the layer that carries its `doctor.toml`, or `built-in` |
| `description` | the `description` its `doctor.toml` gives, the built-in row's own, or null |
| `measures` | for a `runtime-version` row, the pack it measured; otherwise null |
| `verdict` | `pass`, `finding` or `could_not_tell` |
| `exit` | the check's own exit code, or null where it gave none: nothing ran, it was killed, it ran past its bound, or the row is a built-in one |
| `line` | the row's one line |
| `stdout`, `stderr` | everything the check printed on each, up to its bound where it ran past it, or an empty string where nothing ran |

### When fleet doctor refuses

A refusal comes before any check runs, on a `fleet doctor:` line
(shared.exit-codes.R12, shared.exit-codes.R13).

| Case | Exit |
| --- | --- |
| no fleet for the working directory | 1 (shared.locations.R25, shared.exit-codes.R23) |
| a declared project this machine has not registered, or a `config.json` that names no fleet for it | 1 (shared.locations.R26, shared.locations.R35) |
| `config.json` or `projects.toml` is there but will not read | 3 (shared.locations.R38) |
| a machine directory named by a relative path, or no home at all | 1 (shared.locations.R30 to shared.locations.R32) |
| a check name no layer carries | 2 (shared.exit-codes.R3) |
| the pack layers do not resolve, such as with no defaults directory | 3 (shared.exit-codes.R4) |
| the command line does not parse | 2 (shared.exit-codes.R3, shared.exit-codes.R15) |

### What the doctor writes

Nothing. `fleet doctor` changes no file in the machine directory or the
project, appends no event to the stream, and starts no seat's session. The
checks the defaults carry write nothing either. What a pack's check does is
that pack's business.

### Known gaps

Each gap is held by an open item, and the clause cited carries its
`Today:` line.

- **The defaults' checks call back through `FLEET_BIN`** (fleet-4yy). Today
  guards-installed runs the `fleet` first on `PATH`, so it can measure
  another build (onboarding.doctor.R16).
- **A `config.json` that does not parse** (fleet-9sr5). Today
  `fleet doctor` refuses there with exit 1, as though no fleet were named,
  instead of exit 3 (shared.locations.R38).
- **A check stopped at its 60 s bound keeps what it printed** (fleet-nhjb).
  Today its row shows none of it: its line names only the bound, no
  indented lines follow, and under `--json` its `stdout` and `stderr` are
  empty (onboarding.doctor.N3).
- **A relative `HOME`** (fleet-8s9z). Today a relative `HOME`, or on Linux
  `XDG_STATE_HOME`, is read against the working directory instead of being
  refused (shared.locations.R30).

## Behaviour

- **onboarding.doctor.R1**: IF `fleet doctor` finds no fleet for the working
  directory (shared.locations.R25), or the directory declares a project this
  machine has not registered (shared.locations.R26, shared.locations.R35),
  or a file that
  decides its fleet will not read (shared.locations.R38), THEN fleet SHALL
  run no check.
- **onboarding.doctor.R2**: IF the pack layers do not resolve, such as when
  the defaults directory is absent, THEN fleet SHALL refuse before any check
  runs and exit 3 (shared.exit-codes.R4), on a refusal line that says why.
- **onboarding.doctor.R3**: WHEN `fleet doctor` is called with
  `--packs-dir <DIR>`, fleet SHALL read the installed packs from `<DIR>` in
  place of `packs/` in the machine directory.
- **onboarding.doctor.R4**: fleet SHALL take as a doctor check each
  `doctor/<name>/` directory that any pack layer carries, the defaults or an
  installed pack, named by the directory's name.
- **onboarding.doctor.R5**: WHERE a pack above another layer carries a
  check's `doctor.toml`, its script, or both, fleet SHALL use the pack's
  copy of each file it carries in place of the lower layer's.
- **onboarding.doctor.R6**: fleet SHALL name as a check's layer the layer
  that carries its `doctor.toml`.
- **onboarding.doctor.R7**: fleet SHALL read from a check's `doctor.toml`
  only `description` and `run`, and ignore every other key.
- **onboarding.doctor.R8**: fleet SHALL carry in its defaults the doctor
  checks `adopt-board`, `fleet-packs-version`, `guards-installed`,
  `runtime-version` and `tmux-version`.
- **onboarding.doctor.R9**: WHEN `fleet doctor` is called with no check
  named, fleet SHALL run every check the layers carry and both built-in rows.
- **onboarding.doctor.R10**: WHEN `fleet doctor` is called with check names,
  fleet SHALL run only the checks and built-in rows named, each once however
  often it is named.
- **onboarding.doctor.R11**: IF a name given to `fleet doctor` is neither a
  check the layers carry nor a built-in row, THEN fleet SHALL refuse before
  any check runs and exit 2 (shared.exit-codes.R3), on a refusal line that
  names the unknown name and every check the layers carry, the built-in rows
  included. A check name is taken from the fixed set the layers and the
  built-in rows make, so a name outside it is a wrong call, not an absent
  thing.
- **onboarding.doctor.R12**: fleet SHALL print the rows of the checks the
  layers carry in ascending byte order of their names, whatever order they
  were named in, and then `store-adapter`, then `agent-adapter`.
- **onboarding.doctor.R13**: WHEN fleet runs a check, fleet SHALL run the
  script its `doctor.toml`'s `run` names with `sh`, from the project's root.
- **onboarding.doctor.R14**: WHEN fleet runs a check, fleet SHALL give it
  standard input at end of file, so that a check never waits on a prompt.
- **onboarding.doctor.R15**: WHEN fleet runs a check other than
  `runtime-version`, fleet SHALL set `FLEET_PACK_DIR` to the directory of the
  layer that carries the check's `doctor.toml`.
- **onboarding.doctor.R16**: WHEN fleet runs a check, fleet SHALL set
  `FLEET_PROJECT` to the project's root and `FLEET_BIN` to the path of the
  `fleet` binary that is running, and every check the defaults carry SHALL
  call back into fleet through `FLEET_BIN`, so that it measures the binary
  the person ran.
  Today: guards-installed runs the fleet first on `PATH` (fleet-4yy).
- **onboarding.doctor.R17**: WHEN fleet runs a check, fleet SHALL hand it
  every other variable of the caller's environment unchanged, `PATH`
  included, except the `PATH` onboarding.doctor.R22 gives `runtime-version`.
- **onboarding.doctor.R18**: WHEN a check exits 0, fleet SHALL give its row
  the verdict `pass`, whatever it printed.
- **onboarding.doctor.R19**: WHEN a check exits 1, fleet SHALL give its row
  the verdict `finding`, whatever it printed.
- **onboarding.doctor.R20**: IF a check exits with any code other than 0 or
  1, is killed by a signal, or cannot be started, THEN fleet SHALL give its
  row the verdict `could not tell`, whatever it printed. A check past its
  bound is onboarding.doctor.N2's.
- **onboarding.doctor.R21**: IF a check's entry has no `doctor.toml`, a
  `doctor.toml` that does not parse as TOML, no `run` key, or a script that no
  layer carries, THEN fleet SHALL run nothing for it and give its row the
  verdict `could not tell`, with a line that says why.
- **onboarding.doctor.R22**: WHEN `fleet doctor` runs `runtime-version`,
  fleet SHALL run it once for each installed pack whose `pack.toml` declares a
  `[runtime]` table, with `FLEET_PACK_DIR` set to that pack's directory, on
  the `PATH` `fleet run` gives that pack's workflows, which the runs area
  owns.
- **onboarding.doctor.R23**: WHEN `fleet doctor` runs `runtime-version` for
  more than one pack, fleet SHALL print their rows in the layers' order,
  highest first.
- **onboarding.doctor.R24**: WHEN `fleet doctor` runs `runtime-version` for a
  pack, fleet SHALL name that pack in the row, as `runtime-version for <pack>`.
- **onboarding.doctor.R25**: IF no installed pack declares a `[runtime]`
  table, THEN fleet SHALL print one `runtime-version` row with the verdict
  `pass`, whose line says nothing is pinned, and run nothing for it.
- **onboarding.doctor.R26**: IF an installed pack's `pack.toml` cannot be
  read or does not parse, THEN fleet SHALL print a `runtime-version` row for
  that pack with the verdict `could not tell`, whose line says why.
- **onboarding.doctor.R27**: fleet SHALL compute two rows itself,
  `store-adapter` and `agent-adapter`, each naming `built-in` as its layer,
  and run no layer's `doctor/store-adapter/` or `doctor/agent-adapter/`
  entry in their place.
- **onboarding.doctor.R28**: WHEN fleet computes `store-adapter` or
  `agent-adapter`, fleet SHALL open the adapter that the project's
  `[store] adapter` or the fleet's `[agent] adapter` names, as every verb
  opens it, and ask it `version` and `capabilities` and nothing else.
- **onboarding.doctor.R29**: WHEN the store adapter answers `version` and
  `capabilities` and its capabilities validate, fleet SHALL give
  `store-adapter` the verdict `pass`.
- **onboarding.doctor.R30**: IF the store adapter's capabilities do not
  validate, THEN fleet SHALL give `store-adapter` the verdict `finding`, with
  a line that says why.
- **onboarding.doctor.R31**: WHEN the agent adapter answers a non-null
  `version` and capabilities that validate, fleet SHALL give `agent-adapter`
  the verdict `pass`.
- **onboarding.doctor.R32**: WHEN the agent adapter answers a non-null
  `version` and capabilities that validate, and its installed version is not
  among the versions its capabilities say it was measured against, fleet
  SHALL still give `agent-adapter` the verdict `pass`, with a line that names
  the installed version and the measured ones and says to re-measure.
- **onboarding.doctor.R33**: IF the agent adapter answers `version` with a
  null version, or capabilities that do not validate, THEN fleet SHALL give
  `agent-adapter` the verdict `finding`, with a line that says why.
- **onboarding.doctor.R34**: IF the fleet's agent setting cannot be read, or
  an adapter is not opened, or a call to it does not answer within its bound
  (onboarding.doctor.N5) or answers something fleet cannot read, THEN fleet
  SHALL give that adapter's row the verdict `could not tell`, with a line that
  says why.
- **onboarding.doctor.R35**: fleet SHALL start each row with its verdict
  (`pass`, `finding` or `could not tell`), followed by the check's name and,
  in parentheses, its layer, then ` — ` before the row's line.
- **onboarding.doctor.R36**: fleet SHALL end each row of a check that ran,
  after its ` — `, with the last non-blank line the check printed on
  standard output, or, where it printed none there, the last on standard
  error, with the whitespace around it trimmed.
- **onboarding.doctor.R37**: IF a check that ran printed no non-blank line,
  THEN fleet SHALL end its row with a line that says so and names its exit
  code, or says it was killed by a signal. A check stopped at its bound is
  onboarding.doctor.N3's.
- **onboarding.doctor.R38**: IF a row's verdict is not `pass`, THEN fleet
  SHALL print after it every non-blank line the check printed, untrimmed,
  those on standard output first and then those on standard error, each
  indented by two spaces.
- **onboarding.doctor.R39**: WHEN every row has been printed, fleet SHALL
  print one summary line of the form
  `doctor <N> checks — <P> pass, <F> finding, <C> could not tell`, where
  `<N>` is the number of rows (`doctor 1 check — …` for one row) and `<P>`,
  `<F>` and `<C>` count the rows with each verdict.
- **onboarding.doctor.R40**: WHEN `fleet doctor` is called without `--json`,
  fleet SHALL print the rows, their indented lines and the summary on
  standard output.
- **onboarding.doctor.R41**: WHEN every row passed, fleet SHALL exit 0
  (shared.exit-codes.R1).
- **onboarding.doctor.R42**: WHEN at least one row is a finding and none could
  not tell, fleet SHALL exit 1 (shared.exit-codes.R2).
- **onboarding.doctor.R43**: WHEN at least one row could not tell, fleet SHALL
  exit 3 (shared.exit-codes.R4), whatever the other rows say.
- **onboarding.doctor.R44**: WHEN `fleet doctor --json` has run its checks,
  fleet SHALL carry in its document's `data` a `checks` array with one object
  per row, in the order the rows were printed, each holding the keys `name`,
  `layer`, `description`, `measures`, `verdict`, `exit`, `line`, `stdout` and
  `stderr` as the `--json` table above says.
- **onboarding.doctor.R45**: WHEN `fleet doctor --json` has run its checks,
  fleet SHALL carry in `data` a `counts` object whose `pass`, `finding` and
  `could_not_tell` give how many rows had each verdict, and a `verdict` of
  `pass`, `finding` or `could_not_tell` matching the exit
  onboarding.doctor.R41 to R43 give.
- **onboarding.doctor.R46**: WHEN `fleet doctor` runs, fleet SHALL change no
  file in the machine directory or the project, append no event to the
  stream and start no seat's session, the checks its defaults carry
  included; what a pack's check does is that pack's.

## Non-functional

- **onboarding.doctor.N1**: IF a check has not exited within 60 s, THEN
  fleet SHALL stop it, and everything it started in its process group, so
  that no check outlives its bound or holds up the doctor.
- **onboarding.doctor.N2**: IF a check is stopped at its 60 s bound, THEN
  fleet SHALL give its row the verdict `could not tell`.
- **onboarding.doctor.N3**: IF a check is stopped at its 60 s bound, THEN
  fleet SHALL keep what it printed before the stop, as for any other check
  that ran: the row's line is its last non-blank line
  (onboarding.doctor.R36), or, where it printed none, a line that names the
  bound; the indented lines follow the row (onboarding.doctor.R38); and under
  `--json` its `stdout` and `stderr` hold what it printed.
  Today: the row's line names only the bound, no indented lines follow, and
  `stdout` and `stderr` are empty under `--json` (fleet-nhjb).
- **onboarding.doctor.N4**: fleet SHALL run one check at a time, and print
  each check's row before the next check starts.
- **onboarding.doctor.N5**: WHEN fleet computes a built-in row, fleet SHALL
  give each call to the store adapter 60 s, and each call to the agent
  adapter the agent's bound: 20 s unless `FLEET_AGENT_TIMEOUT_MS` sets
  another, as the agent area says.

## Interfaces

- A pack's author relies on R4 to R7 and R13 to R21: a `doctor/<name>/`
  entry with a `doctor.toml` and a script, run with `sh` from the project's
  root, with `FLEET_PACK_DIR`, `FLEET_PROJECT` and `FLEET_BIN` set, and judged
  by its exit alone. The packs area's spec states the order of the layers,
  the defaults' shadow registry that says which of the defaults' files a pack
  may replace, and what `--packs-dir` moves.
- `fleet run` runs a pack's `runtime-version` before it opens a run, on the
  same `PATH` (R22) and with the same reading of its exit (R18 to R20); the
  runs area's spec says what that `PATH` is and what a run does with the
  verdict.
- The two built-in rows open the store and agent adapters as every verb does
  and ask only `version` and `capabilities` (R28). The store and agent areas'
  specs say what those calls answer. `fleet store check` and
  `fleet agent check` prove the rest of each contract.
- Scripts, agents and skills read the exit (R41 to R43) and the `--json`
  document's `data` (R44, R45), inside the envelope shared.output states.
- `fleet doctor` is one of the verbs that shared.output.R6, shared.output.R7,
  shared.output.R15, shared.output.R19 and shared.output.R20,
  shared.exit-codes.R23, and shared.locations.R25, shared.locations.R26,
  shared.locations.R30 to shared.locations.R32, shared.locations.R35 and
  shared.locations.R38 cover. shared.locations.R30 and shared.locations.R38
  carry the `Today:` lines that reach it (fleet-8s9z, fleet-9sr5).

## Open questions

None.

## Definition of Done

Every clause above has at least one scenario in fleet-dtu.
