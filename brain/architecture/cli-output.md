# How the CLI speaks

How tinyfleet turns what happened into what a person reads: the words, the
symbols at the start of each line, the colours, the prompts and the spinner.
It is one part of the architecture in [core.md](core.md). This page explains
how the pieces fit, and points at the files that hold them.

## The one rule

**Every word a command shows is the CLI's** (core.md, invariant 9). The
supervisor answers with **facts**: what happened, or why not. It never answers
with sentences, and neither do the drivers: a `ToolError` (in
`crates/core/src/tools.rs`) says what failed, not how to say it. The CLI turns
facts into words and colour. All of a command's wording, its help text
included, lives in one file, so it can be read, changed and kept consistent in
one place, and changing a sentence never touches the supervisor, a driver or
the protocol.

The supervisor's own log is the one exception: `fleet supervise` writes it,
not a command, so its lines are worded in `crates/supervisor/src/words.rs`,
the same way and in one place. What the operating system or a parser says
(a permission error, a JSON parse error) is passed through as it is.

```
 supervisor                     socket (JSON)                    CLI
 ──────────                     ─────────────                    ───
 decides, from the rules  ──▶   Reply::Response  ──▶  words.rs   turns it into a sentence
 (fleet-core)                   Reply::Status         output.rs  marks it, picks the stream, prints it
                                                      theme.rs   symbols and colours
                                                      ui.rs      prompts and spinners
```

## The layers

| Layer | Holds | File |
| --- | --- | --- |
| **The supervisor's reply** | `protocol::Reply`: a `Response` or a `Status`. `Response` is what it did, grouped by the command that asked (`Hello(response::Hello)`, `Create(response::Create)`, `Start(response::Start)`, `Stop(response::Stop)`); the CLI's `Request`s are grouped the same way (`request::Create::Fleet { .. }`, `request::Start::All`, …); `Status` is why not: a rule's verdict (`DirectoryNotRepoRoot { path, root }`, `AlreadyServed { project, fleet }`, …) or a problem with a path or file (`Missing { path }`, `StrayConfig { path }`, …). On the wire: `{"response":{"create":{"directory":…,"projects":[…],"created":true}}}` or `{"status":{"missing":{…}}}`. | `crates/protocol/src/lib.rs`; `Status` is defined in `crates/core/src/status.rs`, so the rules return it, and re-exported by the protocol |
| **How a command ends** | `Result<cli::Response, cli::Status>`. `Response` is what the command did (`Created { response, startup }`, `Started { response, startup }`, `SupervisorStopped`, `SupervisorWasNotRunning`); `Status` is why not, with its exit code: `Refused(protocol::Status)`, `Cannot(Problem)`, `NotRunning`, `CouldNotTell(Doubt)`. The person's `cli::Request` (`Create`, `Start`, `Stop`) goes in. | `crates/cli/src/ending.rs`, `crates/cli/src/lib.rs` |
| **Words** | Every sentence: `response(&Response, ink)`, `status(&Status, ink)`, `next(&Status, ink)`, the CLI's own lines (the prompt and its answers, the spinner text), and the help text clap shows | `crates/cli/src/words.rs` |
| **Output** | Where a line goes and how it starts: `Mark`, `line`, `report`, and `Ink` for colouring the parts of a sentence | `crates/cli/src/output.rs` |
| **Theme** | The symbols, the palette (`Tone`, `paint_on`), and the cliclack `Theme` that draws prompts and spinners | `crates/cli/src/theme.rs` |
| **Prompts and spinners** | `confirm(question, default)` and `spinner(message)`, drawn with cliclack in the theme | `crates/cli/src/ui.rs` |

## What a person sees, and what a script sees

tinyfleet is used both at a terminal and from scripts. A script must read the
same plain text whatever the terminal can do, so **anything decorative shows
only when its stream is a terminal**.

| | On a terminal | Piped (a script, a file, `\| less`) |
| --- | --- | --- |
| Done line (stdout) | `◆ created a fleet in /code/app -- welcome aboard` | `created a fleet in /code/app -- welcome aboard` |
| Refusal (stderr) | `■ /code/app/src is inside the repository at /code/app` | `fleet: /code/app/src is inside the repository at /code/app` |
| Could not tell (stderr) | `▲ the supervisor did not answer within 10s` | `fleet: the supervisor did not answer within 10s` |
| Hint (stderr), under the line it follows | `> run fleet create in /code/app, or pass --project for a wingman fleet` | `hint: run fleet create in /code/app, or pass --project for a wingman fleet` |
| Colour | the symbol, and every path, name and command in the sentence | none |
| Prompt | asked, with `>` and `Yes / No` | never asked: without `--non-interactive` the command ends *supervisor not running* (exit 3) |
| Spinner | drawn on stderr while waiting | not drawn |

**Streams:** what was done goes to **stdout**. Refusals, warnings, prompts and
spinners go to **stderr**. A script can capture the result alone with
`$(fleet create)`.

**Colour switches:** `console` decides per stream. It is on for a terminal and
off for a pipe. It honours `NO_COLOR`, `CLICOLOR=0` and `CLICOLOR_FORCE`.

## The look

The look follows Vite+'s CLI: frameless (no guide bars down the left), a blue
pointer for what is active, gray once a step is done.

**Symbols** (`theme.rs`):

| Symbol | Means | Where |
| --- | --- | --- |
| `◆` | done | the start of a done line |
| `■` | refused or failed | the start of an error line; a cancelled prompt |
| `▲` | warning, or could not tell | the start of a warning line; a prompt's validation error |
| `>` | active, or what to do next | the current prompt; the chosen option; the start of a hint line |
| `◇` | answered | a prompt once answered |
| `●` | information | cliclack's info log (no `Mark` uses it yet) |

**Palette** (`Tone` in `theme.rs`):

| Tone | For | Colour |
| --- | --- | --- |
| `Accent` | what to notice: paths, names, commands, the `>` pointer and hint | Vite+'s blue `#5892FF` on a true-colour terminal (`COLORTERM=truecolor`); otherwise the terminal's own blue |
| `Muted` | what is done or secondary: `◇` | gray |
| `Good` | `◆` | green |
| `Warn` | `▲` | yellow |
| `Bad` | `■`, and `fleet:` | red |

The spinner is the terminal's magenta, followed by its message and the elapsed
time, dimmed: `◒ starting the supervisor (2s)`. indicatif's spinner template
takes only named colours, so it cannot use Vite+'s true-colour magenta
(`#BB74F7`).

## How a sentence gets its colour

A sentence is built in `words.rs` with an `Ink`: one ink per stream, because
whether to colour depends on the stream the line will be written to. `OUT` is
for done lines (stdout); `ERR` is for refusals and warnings (stderr). Only the
variable parts are wrapped:

```rust
// words.rs
// response::Create is { directory, projects, created }; here, a new wingman fleet
format!(
    "created a wingman fleet in {} serving {}",
    ink.path(directory),          // accent, on the ink's stream
    ink.paths(projects),          // each path accented, joined with commas
)
```

A command never builds a sentence. It ends with data, and `output::report`
words it with the ink of the stream it ends on:

```rust
// create.rs: the client returns only create's kind of reply, or the Status
let response = client::create(&socket, request)?;   // Result<protocol::response::Create, Status>
Ok(Response::Created { response, startup })

// client.rs: one helper per command; another kind of reply is Doubt::OddReply,
// the supervisor's no is Status::Refused, a failed conversation is CouldNotTell

// output.rs: words it, marks it, picks the stream and the exit code
Ok(response) => line(Mark::Done, &words::response(response, OUT)),        // exit 0
Err(status)  => {
    line(mark_for(status), &words::status(status, ERR));                // status.exit_code()
    if let Some(next) = words::next(status, ERR) {
        line(Mark::Next, &next);                                         // the hint, if any
    }
}
```

The mark is `◆` for done, `■` for refused or cannot, and `▲` for could not tell.
A status that has something to do about it gets a hint line after it, marked
`>`: `words::next` says what, or `None`.

**Naming:** the two sides share the words `Request`, `Response` and `Status`,
and the module says which side: `protocol::Response` is what the supervisor
did, and the CLI's own `Response` is how a command ended. Inside the CLI, the
supervisor's types are always written with `protocol::`, never imported bare.

## Recipes

**A new sentence for the CLI's own step.** Add a `const` or a function to
`words.rs`, and call it from the command. Wrap paths and commands with
`ink.path` and `ink.accent`. Never write a sentence anywhere else; a check for
that is to grep the CLI for English strings outside `words.rs`.

**Help text.** Add a `const` to `words.rs` and point clap at it with
`#[command(about = words::X)]` or `#[arg(help = words::X)]`. The clap types in
`lib.rs` take plain comments, not doc comments, because clap turns doc
comments into help text.

**A new way a driver can fail.** Add a variant to `ToolError` (or a new
step to `FileStep`) in `crates/core/src/tools.rs`, return it from the driver,
and word it in `words::tool`. A driver never formats a sentence.

**A new thing the supervisor can say.** Add a variant to the command's own
`protocol::response::…` enum (or a new command's enum, plus its `Request` and
`Response` variants and a `client` helper) for something it did, or a
`Status` variant (in `crates/core/src/status.rs`) for a reason it says no. Field names follow the AIPs (core.md § Naming). Then:
- the supervisor returns the variant;
- `words.rs` gets one match arm for it (the compiler points at the missing arm);
- a protocol test round-trips it.

The supervisor never builds the sentence.

**A new way for a command to end.** Add a variant to the CLI's `Response` or
`Status` in `ending.rs` (a new `Problem` or `Doubt` for a new reason). A
`Status` variant also gives its exit code in `Status::exit_code`.

**A hint after a status.** Add a match arm to `words::next` for that
`Status`; `report` prints it under the status line. A status line says what
is wrong; what to do about it goes in the hint, never after a semicolon.

**A new kind of line** (say, `● info: …`). Add a variant to
`Mark` in `output.rs`, with its symbol and tone in `Mark::symbol`, its
piped prefix in `Mark::piped_prefix`, and say
which stream it belongs to in `Mark::on_stderr`. Add the symbol to `theme.rs`
if it is new. Unused variants are dead code and fail the build, so a mark
arrives with the first line that uses it.

**A new prompt.** Call `ui::confirm(words::YOUR_QUESTION, default)`. Only ask
when the command's `Ask` is `Terminal`. With `--non-interactive`, take the
answer that keeps going; with neither, end without asking (core.md
§ Components).

**Changing the look.** Symbols, palette and prompt rendering all live in
`theme.rs`. The cliclack `Theme` methods there (`format_header`,
`format_confirm`, `format_progress_start`, …) are the hooks.

## Seeing every line

`cargo run -p fleet --features specimen -- specimen` prints every line the
CLI can print, through the same code a command uses: the help text, the
start prompt in each state, the spinners, and every way a command can end
with its hint and exit code. Pipe it (`… 2>&1 | cat`) to see what a script
sees. The `specimen` feature is off by default, so a release binary has no
such command.

It cannot fall behind: every enum a command can end with has a kind twin
(strum's `EnumDiscriminants`) under that feature, and the specimen builds a
sample for each kind in an exhaustive `match`. A new variant does not
compile with `--all-features` until it has a sample in
`crates/cli/src/specimen.rs`. What the compiler cannot check is a choice
of values rather than a variant: the shapes of `protocol::response::Create`
(a struct), wording that depends on a field's value (a tool that failed
saying nothing), which combinations of nested facts are shown, and whether
a new status should have a hint (`words::next` falls back to none).

## Why it is built this way

- **Facts, not sentences, on the socket**: the wording can change without
  touching the supervisor or the protocol, and a protocol test checks
  meaning, not text.
- **One words file**: the whole voice of the CLI is readable in one sitting,
  and consistent by construction.
- **Decoration only on a terminal**: scripts and the visible evals
  (`tests/cmd/`) see stable plain text, while a person gets symbols and
  colour.
- **cliclack, restyled**: it draws prompts and spinners on the same
  `console` and `indicatif` the rest of the CLI uses, and its `Theme` trait is
  the one place the look is set.
