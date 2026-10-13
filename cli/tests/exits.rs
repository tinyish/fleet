//! The exit table, through the shipped binary.
//!
//! One arm per row that a built verb can reach today, each rc read from the
//! process's own status and never from what it printed. The rows the built
//! verbs cannot reach yet — 4, and 6 — are the controller's and are measured in
//! `drive_seats.rs` beside the seat they refuse.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

mod common;
use common::hermetic::Hermetic;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A machine directory the test owns, removed when the test ends. Empty is the
/// point: a fleet directory with no `config.json` and no `projection.json` is
/// how an instrument goes unreadable without breaking one on the box.
struct Machine {
    root: PathBuf,
}

impl Machine {
    fn new(label: &str) -> Machine {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "fleet-cli-exits-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the temp root is created");
        Machine { root }
    }

    /// A seat list naming one seat, Orla, so a seat writer's argument resolves
    /// and the verb reaches the stream. The policy it names is never read by
    /// the writers that need no projection.
    fn list_orla(&self) {
        std::fs::write(
            self.root.join("config.json"),
            format!(
                "{{\"fleet_toml\": {:?}, \"children\": [{{\"id\": \
                 \"01a0d1f1-0aec-765f-9abe-d4f993b9739a\", \"name\": \"Orla\", \
                 \"worktrees\": {{\"demo\": \"/wt/orla\"}}}}]}}",
                self.root.join("fleet.toml").to_string_lossy()
            ),
        )
        .expect("the seat list is written");
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fleet"))
            .args(args)
            .hermetic(&self.root.join("home"), &self.root)
            .output()
            .expect("the built binary runs")
    }

    /// The same run from a directory under the machine's root that no project
    /// walk can resolve: nothing above it names a `fleet.toml` or a
    /// `.fleet/project.toml`, so a verb that needs the project refuses.
    fn run_nowhere(&self, args: &[&str]) -> Output {
        let cwd = self.root.join("no-project");
        std::fs::create_dir_all(&cwd).expect("the project-less directory is created");
        Command::new(env!("CARGO_BIN_EXE_fleet"))
            .args(args)
            .current_dir(&cwd)
            .hermetic(&self.root.join("home"), &self.root)
            .output()
            .expect("the built binary runs")
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The exit table's row names, by number: the `code` a refusal envelope
/// carries for the status the process exited with.
fn class_of(status: i32) -> &'static str {
    match status {
        0 => "done",
        1 => "refused",
        2 => "usage",
        3 => "could_not_tell",
        4 => "no_session",
        5 => "no_collector",
        6 => "wrong_seat_kind",
        other => panic!("exit {other} is not a row of the exit table"),
    }
}

/// Stdout under `--json` as the one refusal document it must be: a single
/// line, `ok` false, naming `verb`, its refusal's `code` the class of the
/// status the process exited with. Answers the parsed document.
fn one_refusal(out: &Output, verb: &str) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "stdout is one document: {stdout}");
    let document: serde_json::Value = serde_json::from_str(lines[0])
        .unwrap_or_else(|e| panic!("the document parses: {e}: {stdout}"));
    assert_eq!(document["ok"], false, "{stdout}");
    assert_eq!(document["verb"], verb, "{stdout}");
    let status = out.status.code().expect("the process exited with a status");
    assert_eq!(
        document["refusal"]["code"],
        class_of(status),
        "the envelope's code is the exit's row: {stdout}"
    );
    document
}

/// Row 0. The one verb that answers without reading anything.
#[test]
fn done_is_zero() {
    let machine = Machine::new("done");
    let out = machine.run(&["--version"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        env!("CARGO_PKG_VERSION")
    );
}

/// Row 1. A refusal the verb spells: the directory named holds no manifest, so
/// `pack check` lists what is wrong with it and stops. A refusal is a returned
/// exit and never an error, so nothing of the error chain appears.
#[test]
fn a_refusal_the_verb_spells_is_one() {
    let machine = Machine::new("refused");
    let empty = machine.root.join("not-a-pack");
    std::fs::create_dir_all(&empty).expect("the empty directory is created");

    let out = machine.run(&["pack", "check", &empty.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        !stderr(&out).contains("caused by:"),
        "a refusal carries no error chain: {}",
        stderr(&out)
    );
}

/// Row 2. Clap's own usage error, which is this row exactly.
#[test]
fn a_usage_error_is_two() {
    let machine = Machine::new("usage");
    for args in [
        vec![],
        vec!["pack"],
        vec!["event"],
        vec!["guard", "astrologer"],
        vec!["seat", "woke", "builder-1"],
    ] {
        let out = machine.run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
        assert!(
            out.stdout.is_empty(),
            "{args:?}: usage goes to stderr, never stdout"
        );
    }
}

/// Row 3, and the one arm that proves an error's whole path: the seat list is
/// the instrument `observe` needs, an empty machine directory holds none, and
/// what comes back is the chain — the verb named by the cli's own context, then
/// the cause under it — with the status the table gives an error and no other.
#[test]
fn an_unreadable_instrument_is_three_and_prints_the_chain() {
    let machine = Machine::new("could-not-tell");

    let out = machine.run(&["observe", "--once"]);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    let printed = stderr(&out);
    assert!(
        printed.contains("fleet: fleet observe"),
        "the chain's first line names the verb: {printed}"
    );
    assert!(
        printed.contains("caused by: the seat list could not be read"),
        "the chain carries its cause: {printed}"
    );

    // The control the arm needs: the same verb over a machine directory that
    // HAS a seat list reaches the loop, so the 3 above is the missing
    // instrument and not `observe` refusing everything it is handed. Its
    // fleet runs on the agent stub, which answers from no state at all.
    std::fs::write(
        machine.root.join("fleet.toml"),
        "[fleet]\npoll_seconds = 1\n",
    )
    .expect("the policy is written");
    common::stub_agent(&machine.root);
    std::fs::write(
        machine.root.join("config.json"),
        format!(
            "{{\"fleet_toml\": {:?}, \"children\": []}}",
            machine.root.join("fleet.toml").to_string_lossy()
        ),
    )
    .expect("the seat list is written");

    let out = machine.run(&["observe", "--once"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "the control polls and exits 0: {}",
        stderr(&out)
    );
}

/// Row 5. The stream's refusal, reached through the cli's mapping rather than
/// asserted about the controller: a rest with no projection beside it is a rest
/// nothing would collect. The seat IS on the list — a seat argument is resolved
/// before the stream is asked anything — so the 5 is the missing collector's.
#[test]
fn no_collector_is_five() {
    let machine = Machine::new("no-collector");
    machine.list_orla();
    let out = machine.run(&["event", "rest", "Orla", "--reason", "x"]);
    assert_eq!(out.status.code(), Some(5), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("no collector is consuming"),
        "{}",
        stderr(&out)
    );
}

/// Row 3, for a record that cannot be appended (R21): the stream's path is a
/// directory, so the open fails and nothing is written. `rest` and
/// `clear-halt` take the same append, after a projection check this machine
/// directory would refuse first, so the three writers that need no
/// projection stand for all five.
#[test]
fn a_seat_record_that_cannot_be_appended_is_three() {
    let machine = Machine::new("append");
    machine.list_orla();
    let stream = machine.root.join("events.jsonl");
    std::fs::create_dir_all(&stream).expect("the stream's path is a directory");
    for verb in ["woke", "handed-off", "exited"] {
        let out = machine.run(&["event", verb, "Orla"]);
        assert_eq!(out.status.code(), Some(3), "{verb}: {}", stderr(&out));
        assert!(
            stderr(&out).starts_with(&format!("fleet event {verb}: could not append to ")),
            "{verb}: {}",
            stderr(&out)
        );
    }

    // The control: with the path free, the same writer appends and exits 0.
    std::fs::remove_dir(&stream).expect("the directory is removed");
    let out = machine.run(&["event", "woke", "Orla"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

/// Row 3, for a record that does not read back (R18): the stream ends in
/// half a line, so the line a seat writer appends joins it and no line the
/// stream carries is the one written. The writer acted and cannot say it
/// landed, which is could-not-tell and not the call's fault.
#[test]
fn a_seat_record_that_does_not_read_back_is_three() {
    let machine = Machine::new("read-back");
    machine.list_orla();
    let stream = machine.root.join("events.jsonl");
    for verb in ["woke", "handed-off", "exited"] {
        std::fs::write(&stream, "{").expect("the half line is written");
        let out = machine.run(&["event", verb, "Orla"]);
        assert_eq!(out.status.code(), Some(3), "{verb}: {}", stderr(&out));
        assert!(
            stderr(&out).starts_with(&format!("fleet event {verb}: ")),
            "{verb}: {}",
            stderr(&out)
        );
        assert!(
            stderr(&out).contains("the record is not confirmed"),
            "{verb}: {}",
            stderr(&out)
        );
    }

    // The control: over a whole stream the same writer reads its line back.
    std::fs::write(&stream, "").expect("the stream is emptied");
    let out = machine.run(&["event", "woke", "Orla"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
}

// The refusal paths, one arm per verb family, pinned before their helpers
// were given one home in `exit.rs`: each arm holds the exact exit and the
// exact head of the stderr line, and under `--json` the one envelope.

/// The event verbs' refusal: a machine directory with no stream is row 5.
#[test]
fn an_event_read_with_no_stream_refuses_as_no_collector() {
    let machine = Machine::new("event-show");

    let out = machine.run(&["event", "show", "01X"]);
    assert_eq!(out.status.code(), Some(5), "{}", stderr(&out));
    assert!(
        stderr(&out).starts_with("fleet event show: no event stream at "),
        "{}",
        stderr(&out)
    );
    assert!(out.stdout.is_empty(), "no document was asked for");

    let out = machine.run(&["event", "show", "01X", "--json"]);
    assert_eq!(out.status.code(), Some(5), "{}", stderr(&out));
    assert!(
        stderr(&out).starts_with("fleet event show: no event stream at "),
        "{}",
        stderr(&out)
    );
    let document = one_refusal(&out, "event show");
    assert_eq!(document["refusal"]["code"], "no_collector");
}

/// The nudge's line, under its own verb.
/// No fleet is a refusal, row 1 (shared.exit-codes.R23).
#[test]
fn a_nudge_with_no_project_says_so_under_its_verb() {
    let machine = Machine::new("nudge");
    let out = machine.run_nowhere(&["seat", "nudge", "Orla", "--text", "x"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).starts_with("fleet seat nudge: "),
        "{}",
        stderr(&out)
    );
}

/// The attach's line, under its own verb.
/// No fleet is a refusal, row 1 (shared.exit-codes.R23).
#[test]
fn an_attach_with_no_project_says_so_under_its_verb() {
    let machine = Machine::new("attach");
    let out = machine.run_nowhere(&["seat", "attach", "Orla"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).starts_with("fleet seat attach: "),
        "{}",
        stderr(&out)
    );
}

/// An enveloped item verb's refusal: the line, then the one document.
/// No fleet is a refusal, row 1 (shared.exit-codes.R23).
#[test]
fn a_land_with_no_project_refuses_in_one_envelope() {
    let machine = Machine::new("land");
    let out = machine.run_nowhere(&["land", "fx-1", "abc1234", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(stderr(&out).starts_with("fleet land: "), "{}", stderr(&out));
    one_refusal(&out, "land");
}

/// A transient verb's refusal: the line, then the one document.
/// No fleet is a refusal, row 1 (shared.exit-codes.R23).
#[test]
fn a_spawn_with_no_project_refuses_in_one_envelope() {
    let machine = Machine::new("spawn");
    let out = machine.run_nowhere(&["seat", "spawn", "--first-turn", "/nonexistent", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(
        stderr(&out).starts_with("fleet seat spawn: "),
        "{}",
        stderr(&out)
    );
    one_refusal(&out, "seat spawn");
}

/// The control on the four arms above: a fleet IS found here, and what it
/// needs cannot be read (the machine directory holds no `defaults/`, so the
/// pack layers do not resolve). That is still could-not-tell, row 3
/// (shared.exit-codes.R4): only the walk's no-fleet answer moved to 1.
#[test]
fn a_fleet_whose_packs_do_not_resolve_is_still_three() {
    let machine = Machine::new("found-unresolved");
    let project = machine.root.join("a-project");
    std::fs::create_dir_all(&project).expect("the project is created");
    std::fs::write(project.join("fleet.toml"), "").expect("the policy is written");
    let out = Command::new(env!("CARGO_BIN_EXE_fleet"))
        .args(["doctor", "--json"])
        .current_dir(&project)
        .hermetic(&machine.root.join("home"), &machine.root)
        .output()
        .expect("the built binary runs");
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    one_refusal(&out, "doctor");
}
