---
name: spec-cleanup
description: Use when Alberto types /spec-cleanup [area ...], or asks whether the feature specs under brain/specs contradict or repeat themselves, whether every Today: gap has an item, or what work the specs' Known gaps add up to. Run it with him present; it is a sitting.
---

# spec-cleanup

The feature specs are the contract planners write implementation plans from
(CONTRIBUTING.md § Specs). A contract that contradicts itself, says one rule
twice, or names a gap no item will close fails a planner silently. This skill
checks the specs, puts each defect to Alberto, files what he rules, and ends
with every open gap as a work list.

**A sitting edits no spec**, with one exception: step 4 adds a new item's id
to the `Today:` line that lacked one. Every other change is a `spec` item.
`docs/` is never an input. Never open fleet-dtu.

**Checking the code** means reading it first. Where you must run fleet, run
only `fleet create`, `fleet doctor` or `fleet prime`, with `FLEET_DIR` set to
a fresh `mktemp -d`. Never run `fleet start` or `fleet stop`: they load and
unload the real per-user controller. Item bodies are written to that same
kind of temporary directory.

## 1. Scope and the sitting bead

The scope is the area folders named after `/spec-cleanup`, or every area under
`brain/specs/`. Citations always resolve against every spec.

```sh
bd create "spec-cleanup <YYYY-MM-DD>: <areas or 'all specs'>" -t task -p 2 --body-file <file>
bd label add <sitting> spec-cleanup:sitting
bd label add <sitting> fleet
bd update <sitting> --claim
```

The body names the scope and HEAD's sha. Write bodies to a file and pass
`--body-file`. Findings, rulings and dismissals go on it as they happen, as
comments: `bd comments add <sitting> -f <file>`.

## 2. The mechanical checks

```sh
python3 .claude/skills/spec-cleanup/scripts/check.py brain/specs <area ...>
```

Each `path:line: CODE message` line is a finding. Exit 0 is clean, and 1 means findings.

| Code | Defect |
| --- | --- |
| TODAY-NO-BEAD | a `Today:` line names no item; this is step 4's work, not a finding to walk |
| TODAY-CLOSED / TODAY-UNKNOWN | a `Today:` line names a closed or missing item |
| GAP-ORPHAN / GAP-MISSING | `### Known gaps` and the `Today:` lines disagree |
| CITE-MISSING / CITE-RETIRED | a citation resolves to nothing, or to a retired clause |
| RETIRED-FORM, CHANGING, GAPS-HEADING, NO-SECTION, NO-GAPS, QUESTIONS-FORM, DUP-ID | the file's form breaks § Specs |

The `TODAY` JSON line at the end lists every `Today:` line, for step 6.

## 3. The judgement checks

Read CONTRIBUTING.md § Specs and § Change kinds. Read every spec in scope in
full, and every spec it cites. Then look for:

| Check | A defect | Not a defect |
| --- | --- | --- |
| Contradiction | two clauses, or a clause and its story, give different values for one case (an exit, a bound, a path, a stream, a first word), in one spec or across specs | two clauses that cover different cases |
| Duplication | two clauses state one rule; an area clause restates a shared clause instead of citing its id | the story repeating its own clauses; Interfaces listing what other areas rely on |
| Coverage | a story behaviour with no clause, or a clause the story never mentions | |
| `Today:` placement | the line sits under a clause the gap does not break, or no longer describes the code (one focused check) | |
| Open questions | a question that a ruling or another spec already answers | |
| Form | a wait with no bound; a whole message quoted; a platform difference with no `WHERE` | |

**Before a defect stands, look for a ruling and for an item that already holds it.**

```sh
bd list --all --desc-contains <clause id> --json
bd search "<words>"
```

- Alberto's ruling, in an item's description, notes or comments, makes a case deliberate: note it on the sitting as *ruled*, with the item.
- An open item that already covers the defect: note it as *held by <id>*.

Neither one is walked.

## 4. `Today:` lines with no item

For each TODAY-NO-BEAD:

1. Check that the line still describes the code.
2. If it does, file the item, citing the clause:
   `bd create -t bug -p 2 -l fix --deps discovered-from:<sitting> --title "<the gap>" --body-file <file>`
3. Add its id to the `Today:` line and to `### Known gaps`.

If the line is no longer true, the line itself is a finding for step 5. Commit
the id edits as one commit: `<sitting id>: Today: lines name their items`.

## 5. Walk the findings, one at a time

Put each finding to Alberto with AskUserQuestion, one question per finding, in
file order. Script lines that one edit fixes, such as a closed item on a
`Today:` line and the Known gaps mismatch it causes, are one finding.

- The question gives the file:line, the clauses, and the defect in one or two sentences.
- The options are the fixes, the one you recommend first, then **"Not a defect"**.

After his answer:

- **A fix:** file it.
  `bd create -t task -p 2 -l spec --deps discovered-from:<sitting> --title "<spec>: <the fix>" --body-file <file>`
  The body gives the defect, the clauses, and "Ruled by Alberto <date>: <his answer>".
- **Not a defect:** note his reason on the sitting, and file nothing.

## 6. The report

The report has exactly these parts, in order.

1. **Checked:** the scope, the sha, and the script's count by code.
2. **Filed:** one row per finding: item | spec | clauses | the fix he ruled.
3. **Not filed:** each finding ruled, held, or dismissed, with its item or his reason.
4. **Work to do:** every `Today:` item and every `### Known gaps` entry, grouped by spec: item | kind | clause | the gap in a few words. An item named by several specs is listed once, under the spec that holds its clause, and its other places are given beside it.

Add the report to the sitting with `bd comments add <sitting> -f <file>`,
check that it landed, then close the sitting:
`bd close <sitting> --reason "<n> filed, <n> not filed"`.
