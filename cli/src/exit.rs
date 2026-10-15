//! The one exit vocabulary every command shares.
//!
//! A script reading `$?` learns the same thing from every verb, so the table
//! lives in one type and `main` is its only reader. A subcommand returns the
//! row it means; an `Err` is could-not-tell and nothing else, because a verb
//! that cannot say what happened is a third answer rather than a failure.

use anyhow::{bail, Result};

/// The exit table, one variant per row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// done, and the write (if any) read back
    Done = 0,
    /// refused on the record: the thing named is absent, held, or already so
    Refused = 1,
    /// usage: a missing or unknown argument, with the usage line printed
    Usage = 2,
    /// could not tell: an instrument the answer needs was unreadable
    CouldNotTell = 3,
    /// the seat has no live session
    NoSession = 4,
    /// no collector is consuming the stream
    NoCollector = 5,
    /// the seat is the wrong kind for the verb, in either direction: a
    /// transient seat where a named one is required, or the other way round
    WrongSeatKind = 6,
}

impl Exit {
    pub fn code(self) -> u8 {
        self as u8
    }

    /// The row's own name, in snake case: the `code` the JSON envelope's
    /// refusal carries (`envelope.rs`). A caller reading a document branches on
    /// the name where a caller reading `$?` branches on the number, and both
    /// read this one table.
    pub fn class(self) -> &'static str {
        match self {
            Exit::Done => "done",
            Exit::Refused => "refused",
            Exit::Usage => "usage",
            Exit::CouldNotTell => "could_not_tell",
            Exit::NoSession => "no_session",
            Exit::NoCollector => "no_collector",
            Exit::WrongSeatKind => "wrong_seat_kind",
        }
    }

    /// A status a library handed back, read into the table.
    ///
    /// The controller and core answer in `u8`, which is a wider set than the
    /// table: a status outside it is a defect in the caller's own mapping and
    /// not a row, so it is an error here rather than a guessed row.
    pub fn from_status(status: u8) -> Result<Exit> {
        Ok(match status {
            0 => Exit::Done,
            1 => Exit::Refused,
            2 => Exit::Usage,
            3 => Exit::CouldNotTell,
            4 => Exit::NoSession,
            5 => Exit::NoCollector,
            6 => Exit::WrongSeatKind,
            other => bail!("exit {other} is not a row of the exit table"),
        })
    }

    /// A library states a stop's exit as a number from the same table this
    /// enum holds. A number outside it is that library's own defect and reads
    /// as could-not-tell here rather than as a status this binary invented.
    pub fn of(status: u8) -> Exit {
        Exit::from_status(status).unwrap_or(Exit::CouldNotTell)
    }
}

/// A refusal said once: the human's line on stderr always, the envelope on
/// stdout when the caller asked for the document, and the exit table's own row
/// either way.
pub fn refuse(verb: &str, exit: Exit, why: &str, json: bool) -> Exit {
    eprintln!("fleet {verb}: {why}");
    if json {
        println!("{}", crate::envelope::refusal(verb, exit, why));
    }
    exit
}

/// A stop's refusal: the person's line on stderr, the document on stdout
/// where one was asked for, and the row on `$?`.
///
/// THE ROW IS READ ONCE AND USED FOR BOTH, so the `code` a caller branches on
/// and the number it reads from `$?` cannot come apart — including on a status
/// outside the exit table, which lands on could-not-tell in both places.
pub fn refuse_stop(verb: &str, stop: &fleet_core::item::Stop, json: bool) -> Exit {
    refuse(verb, Exit::of(stop.code), &stop.message, json)
}

/// A line on stderr under the verb's name, and the row it answers.
pub fn say(verb: &str, exit: Exit, why: impl std::fmt::Display) -> Exit {
    eprintln!("fleet {verb}: {why}");
    exit
}

/// The verb as typed, without `fleet` and its arguments (`event show`,
/// `seat attach`), and whether the call asked for `--json`: what a refusal
/// made before any verb's own code runs reads off the parsed line.
pub fn typed_verb(matches: &clap::ArgMatches) -> (String, bool) {
    let mut words = Vec::new();
    let mut leaf = matches;
    while let Some((name, sub)) = leaf.subcommand() {
        words.push(name);
        leaf = sub;
    }
    // A verb with no `--json` flag answers an error here, which is "not asked".
    let json = matches!(leaf.try_get_one::<bool>("json"), Ok(Some(true)));
    (words.join(" "), json)
}

/// The verb this call asked to answer in the envelope: the verb as typed,
/// where it was given `--json`. `main` names it in the could-not-tell document
/// an error no verb turned into a refusal is answered with
/// (shared.output.R13), which `fleet event tail` does not print.
pub fn asked(matches: &clap::ArgMatches) -> Option<String> {
    let (verb, json) = typed_verb(matches);
    (json && verb != "event tail").then_some(verb)
}

/// The verb an error stopped, put on the error as its outermost context.
///
/// A `fleet:` line that would carry this and nothing more is left out
/// (shared.exit-codes.R17): the verb has already said its own
/// `fleet <verb>:` line, and the causes follow that line.
#[derive(Debug)]
pub struct Stopped(String);

impl Stopped {
    /// The verb as typed, without `fleet`: `observe`, `event rest`.
    pub fn verb(name: impl Into<String>) -> Stopped {
        Stopped(name.into())
    }
}

impl std::fmt::Display for Stopped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fleet {}", self.0)
    }
}

/// Whether the error's outermost message is a [`Stopped`] and nothing else.
fn verb_only(err: &anyhow::Error) -> bool {
    err.downcast_ref::<Stopped>()
        .is_some_and(|stopped| stopped.to_string() == err.to_string())
}

/// The lines an error no verb turned into a refusal is said in: `fleet:` and
/// the error, unless it names only the verb, then one `caused by:` line per
/// cause.
pub(crate) fn untranslated_lines(err: &anyhow::Error) -> Vec<String> {
    let mut lines = Vec::new();
    if !verb_only(err) {
        lines.push(format!("fleet: {err}"));
    }
    lines.extend(
        err.chain()
            .skip(1)
            .map(|cause| format!("  caused by: {cause}")),
    );
    lines
}

/// An error no verb turned into a refusal: its lines on stderr, the
/// could-not-tell document on stdout where the call asked `verb` for `--json`
/// (shared.output.R13), and the row it answers, which is could-not-tell and
/// never 0.
pub fn untranslated(err: &anyhow::Error, json: Option<&str>) -> Exit {
    for line in untranslated_lines(err) {
        eprintln!("{line}");
    }
    if let Some(verb) = json {
        println!("{}", untranslated_document(err, verb));
    }
    Exit::CouldNotTell
}

/// The could-not-tell document for an error no verb turned into a refusal.
fn untranslated_document(err: &anyhow::Error, verb: &str) -> String {
    crate::envelope::refusal(verb, Exit::CouldNotTell, &reason(err))
}

/// The error's reason, as the document's `why` carries it: the text of every
/// line [`untranslated_lines`] writes, each without its `fleet: ` or
/// `caused by: ` prefix, joined by `: ` (shared.output.R13).
fn reason(err: &anyhow::Error) -> String {
    err.chain()
        .skip(usize::from(verb_only(err)))
        .map(|cause| cause.to_string())
        .collect::<Vec<_>>()
        .join(": ")
}

#[cfg(test)]
mod tests {
    use super::Exit;

    /// A context that names only the verb is not said again as a `fleet:`
    /// line: the verb said its own line above, and the causes follow it.
    #[test]
    fn untranslated_lines_leave_out_a_line_that_names_only_the_verb() {
        let err = anyhow::anyhow!("the seat list could not be read, so no poll ran")
            .context(super::Stopped::verb("observe"));
        assert_eq!(
            super::untranslated_lines(&err),
            vec![String::from(
                "  caused by: the seat list could not be read, so no poll ran"
            )]
        );
    }

    /// The control: an error that says more than the verb keeps its `fleet:`
    /// line, with or without causes, and a verb context under another one does
    /// not hide the outer line.
    #[test]
    fn untranslated_lines_keep_a_line_that_says_more_than_the_verb() {
        let plain = anyhow::anyhow!("the declared guard classes: no pack");
        assert_eq!(
            super::untranslated_lines(&plain),
            vec![String::from("fleet: the declared guard classes: no pack")]
        );
        let wrapped = anyhow::anyhow!("inner")
            .context(super::Stopped::verb("observe"))
            .context("outer");
        assert_eq!(
            super::untranslated_lines(&wrapped),
            vec![
                String::from("fleet: outer"),
                String::from("  caused by: fleet observe"),
                String::from("  caused by: inner"),
            ]
        );
    }

    /// Both shapes answer could-not-tell.
    #[test]
    fn an_untranslated_error_is_could_not_tell() {
        let err = anyhow::anyhow!("x").context(super::Stopped::verb("observe"));
        assert_eq!(super::untranslated(&err, None), Exit::CouldNotTell);
    }

    /// Under `--json`, an error no verb turned into a refusal is one
    /// could-not-tell document, its keys in the envelope's order and no `data`.
    #[test]
    fn an_untranslated_error_under_json_is_one_could_not_tell_document() {
        let err =
            anyhow::anyhow!("the store did not answer").context("the ready set could not be read");
        let document = super::untranslated_document(&err, "item list");
        assert!(
            document.starts_with(
                r#"{"ok":false,"verb":"item list","refusal":{"code":"could_not_tell","why":"#
            ),
            "{document}"
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&document).expect("the document parses");
        assert_eq!(
            parsed["refusal"]["why"], "the ready set could not be read: the store did not answer",
            "{document}"
        );
        assert!(parsed.get("data").is_none(), "{document}");
    }

    /// The reason never stops at the verb's name: a context that names only
    /// the verb is passed over, as its `fleet:` line is.
    #[test]
    fn the_reason_is_never_only_the_verbs_name() {
        let err = anyhow::anyhow!("the seat list could not be read, so no poll ran")
            .context(super::Stopped::verb("observe"));
        assert_eq!(
            super::reason(&err),
            "the seat list could not be read, so no poll ran"
        );
    }

    /// The matches a call parses to, through the binary's own command tree.
    fn matched(argv: &[&str]) -> clap::ArgMatches {
        use clap::CommandFactory;
        crate::Cli::command()
            .try_get_matches_from(argv)
            .unwrap_or_else(|e| panic!("{argv:?} parses: {e}"))
    }

    /// The verb `main` names in the could-not-tell document is the subcommand
    /// path as typed, where that verb took `--json` and was given it.
    #[test]
    fn the_verb_asked_for_json_is_the_subcommand_path_as_typed() {
        for (argv, verb) in [
            (
                &["fleet", "seat", "add", "--agent", "--json"][..],
                "seat add",
            ),
            (&["fleet", "event", "show", "an-id", "--json"], "event show"),
            (&["fleet", "item", "list", "--ready", "--json"], "item list"),
            (&["fleet", "doctor", "--json"], "doctor"),
            (&["fleet", "dispatch", "t-1", "--json"], "dispatch"),
            (&["fleet", "status", "--json"], "status"),
        ] {
            assert_eq!(
                super::asked(&matched(argv)).as_deref(),
                Some(verb),
                "{argv:?}"
            );
        }
    }

    /// The control: no flag, a verb that takes none, and `fleet event tail`,
    /// which shared.output.R13 leaves out, ask for no document.
    #[test]
    fn no_document_is_asked_for_without_the_flag_or_by_event_tail() {
        for argv in [
            &["fleet", "doctor"][..],
            &["fleet", "observe", "--once"],
            &["fleet", "--version"],
            &["fleet", "event", "tail", "--json"],
        ] {
            assert_eq!(super::asked(&matched(argv)), None, "{argv:?}");
        }
    }

    /// One arm per row, both ways, so a renumbering has to be deliberate.
    #[test]
    fn every_row_maps_to_the_tables_number_and_back() {
        for (exit, code) in [
            (Exit::Done, 0),
            (Exit::Refused, 1),
            (Exit::Usage, 2),
            (Exit::CouldNotTell, 3),
            (Exit::NoSession, 4),
            (Exit::NoCollector, 5),
            (Exit::WrongSeatKind, 6),
        ] {
            assert_eq!(exit.code(), code, "{exit:?} is {code}");
            assert_eq!(
                Exit::from_status(code).expect("the status is a row"),
                exit,
                "status {code} reads back as {exit:?}"
            );
        }
    }

    /// The names a `--json` refusal carries, as the spec's exit table spells
    /// them. A script branches on these words, so each is pinned whole.
    #[test]
    fn every_row_is_named_as_the_table_spells_it() {
        for (exit, name) in [
            (Exit::Done, "done"),
            (Exit::Refused, "refused"),
            (Exit::Usage, "usage"),
            (Exit::CouldNotTell, "could_not_tell"),
            (Exit::NoSession, "no_session"),
            (Exit::NoCollector, "no_collector"),
            (Exit::WrongSeatKind, "wrong_seat_kind"),
        ] {
            assert_eq!(exit.class(), name, "{exit:?}");
        }
    }

    /// The control the arm above needs: a status the table does not hold is
    /// refused rather than rounded into the nearest row.
    #[test]
    fn a_status_outside_the_table_is_not_a_row() {
        for status in [7u8, 42, 255] {
            let read = Exit::from_status(status);
            assert!(read.is_err(), "status {status} is not a row: {read:?}");
        }
    }

    /// A library's status read into the table: a row reads as itself, and a
    /// number outside the table reads as could-not-tell rather than an error.
    #[test]
    fn a_librarys_status_reads_as_its_row_or_could_not_tell() {
        for status in 0u8..=6 {
            assert_eq!(
                Exit::of(status),
                Exit::from_status(status).expect("the status is a row"),
                "status {status} reads as its own row"
            );
        }
        for status in [7u8, 42, 255] {
            assert_eq!(Exit::of(status), Exit::CouldNotTell, "status {status}");
        }
    }

    /// An `Err` is could-not-tell, so no error path can end in a success
    /// status. `main` maps every `Err` to this one number.
    #[test]
    fn an_error_never_carries_a_zero_status() {
        assert_eq!(Exit::CouldNotTell.code(), 3);
        assert_ne!(Exit::CouldNotTell.code(), Exit::Done.code());
    }
}
