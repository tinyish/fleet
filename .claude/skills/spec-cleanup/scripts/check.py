"""The mechanical half of /spec-cleanup: every check a script can do exactly.

    python3 .claude/skills/spec-cleanup/scripts/check.py [SPECS_DIR] [AREA ...]

SPECS_DIR defaults to brain/specs. Naming AREAs limits which feature specs are
checked; citations still resolve against every spec under SPECS_DIR. Each
problem prints as `path:line: CODE message`, and then a JSON line headed
`TODAY` that lists every Today: line, for the work list. Exit 0 when clean, 1
when anything was found, 2 when bd could not be read.

Reads only: it writes nothing but a board export into a temporary directory.
"""
import json
import os
import re
import subprocess
import sys
import tempfile

ID = r'[a-z][a-z0-9-]*\.[a-z][a-z0-9-]*\.[RN]\d+'
DEFINED = re.compile(r'^- (~~)?\*\*(' + ID + r')\*\*(~~)?')
RETIRED_OK = re.compile(r'^- ~~\*\*' + ID + r'\*\*~~ retired \d{4}-\d{2}-\d{2}: \S')
CITED = re.compile(r'\b(' + ID + r')\b')
BEAD = re.compile(r'\bfleet-[a-z0-9]+(?:\.\d+)*\b')
PAREN = re.compile(r'\(([^()]*)\)')
TOKEN = re.compile(r'(' + ID + r')|\b([RN]\d+)\b')
REQUIRED = ['## Interfaces', '## Open questions', '## Definition of Done']


def board():
    """Every bead's status, by id, from one export of the board."""
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, 'issues.jsonl')
        r = subprocess.run(['bd', 'export', '-o', out], capture_output=True, text=True)
        if r.returncode != 0:
            print(f'bd export failed: {r.stderr.strip()}', file=sys.stderr)
            sys.exit(2)
        return {d['id']: d.get('status') for d in map(json.loads, open(out)) if d}


def specs(root, areas):
    """Every feature spec under root: the area folders' .md files but mod.md."""
    found = []
    for area in sorted(os.listdir(root)):
        folder = os.path.join(root, area)
        if not os.path.isdir(folder):
            continue
        for name in sorted(os.listdir(folder)):
            if name.endswith('.md') and name != 'mod.md':
                found.append((area, os.path.join(folder, name)))
    return [(a, p) for a, p in found if not areas or a in areas]


def today_lines(lines):
    """Each Today: line under a clause, with its continuation, as (line no, text)."""
    out = []
    for i, line in enumerate(lines):
        if not line.startswith('  Today:'):
            continue
        text = line.strip()
        j = i + 1
        while j < len(lines) and lines[j].startswith('  ') and not lines[j].startswith('  Today:'):
            text += ' ' + lines[j].strip()
            j += 1
        out.append((i + 1, text))
    return out


def section(lines, heading):
    """The lines under a heading, up to the next heading of its level or above."""
    level = heading.split()[0]
    for i, line in enumerate(lines):
        if line.strip() == heading:
            body = []
            for line2 in lines[i + 1:]:
                if line2.startswith('#') and len(line2.split()[0]) <= len(level):
                    break
                body.append(line2)
            return i + 1, body
    return None, []


def main():
    args = sys.argv[1:]
    root = args.pop(0) if args and os.path.isdir(args[0]) else 'brain/specs'
    status = board()
    every = specs(root, [])
    defined, retired = {}, set()
    for _, path in every:
        for n, line in enumerate(open(path).read().split('\n'), 1):
            m = DEFINED.match(line)
            if m:
                cid = m.group(2)
                if cid in defined and defined[cid][0] == path:
                    print(f'{path}:{n}: DUP-ID {cid} is defined twice in this file')
                defined.setdefault(cid, (path, n))
                if m.group(1):
                    retired.add(cid)

    # A spec's Known gaps may point at a gap another spec's clause holds (a
    # shared clause, say), so a gap is an orphan only where no Today: line in
    # any spec names its item.
    anywhere = set()
    for _, path in every:
        for _, t in today_lines(open(path).read().split('\n')):
            anywhere.update(BEAD.findall(t))

    # Where each item's Today: lines sit, as (spec, clause), across every
    # spec: one item under several clauses is a lead for the judgement step.
    under = {}
    for _, path in every:
        lines = open(path).read().split('\n')
        for n, t in today_lines(lines):
            clause = next((m.group(2) for m in map(DEFINED.match, reversed(lines[:n])) if m), '?')
            for b in BEAD.findall(t):
                under.setdefault(b, []).append(clause)

    problems, work, scoped_beads = 0, [], set()

    def report(path, n, code, msg):
        nonlocal problems
        problems += 1
        print(f'{path}:{n}: {code} {msg}')

    for area, path in specs(root, args):
        lines = open(path).read().split('\n')
        text = '\n'.join(lines)
        for heading in REQUIRED:
            if heading not in lines:
                report(path, 1, 'NO-SECTION', f'no "{heading}" heading')
        for n, line in enumerate(lines, 1):
            if 'Changing:' in line:
                report(path, n, 'CHANGING', 'a Changing: line is left; the method writes Today:')
            if line.startswith('- ~~**') and not RETIRED_OK.match(line):
                report(path, n, 'RETIRED-FORM', 'a retired clause reads `- ~~**id**~~ retired YYYY-MM-DD: <why>`')
            if line.strip().startswith('### ') and 'falls short' in line:
                report(path, n, 'GAPS-HEADING', 'the gap list is headed `### Known gaps`')
            for cid in CITED.findall(line):
                if cid not in defined:
                    report(path, n, 'CITE-MISSING', f'{cid} is defined in no spec')
                elif cid in retired and not DEFINED.match(line):
                    report(path, n, 'CITE-RETIRED', f'{cid} is retired')
        own = '.'.join(os.path.relpath(path, root)[:-3].split(os.sep))
        for m in PAREN.finditer(text):
            n = text.count('\n', 0, m.start()) + 1
            spec = None
            for t in TOKEN.finditer(m.group(1)):
                if t.group(1):
                    spec = t.group(1).rsplit('.', 1)[0]
                elif spec and spec != own:
                    report(path, n, 'BARE-CITE', f'{t.group(2)} follows a {spec} id: spell it {spec}.{t.group(2)}')
        todays = today_lines(lines)
        today_beads = set()
        for n, t in todays:
            beads = BEAD.findall(t)
            if not beads:
                report(path, n, 'TODAY-NO-BEAD', f'"{t[:70]}" names no item')
            for b in beads:
                today_beads.add(b)
                if b not in status:
                    report(path, n, 'TODAY-UNKNOWN', f'{b} is not on the board')
                elif status[b] == 'closed':
                    report(path, n, 'TODAY-CLOSED', f'{b} is closed')
            work.append({'spec': path, 'line': n, 'beads': beads, 'text': t})
            scoped_beads.update(beads)
        start, gaps = section(lines, '### Known gaps')
        gap_beads = set(BEAD.findall('\n'.join(gaps)))
        if todays and start is None:
            report(path, 1, 'NO-GAPS', 'has Today: lines but no `### Known gaps` heading')
        for b in sorted(gap_beads - anywhere):
            report(path, start, 'GAP-ORPHAN', f'Known gaps names {b}, which no Today: line carries')
        for b in sorted(today_beads - gap_beads):
            report(path, start or 1, 'GAP-MISSING', f'a Today: line names {b}, which Known gaps leaves out')
        qstart, qbody = section(lines, '## Open questions')
        items = [l for l in qbody if l.startswith('- ')]
        prose = [l for l in qbody if l.strip() and not l.startswith(('- ', '  '))]
        if qstart and not items and prose != ['None.']:
            report(path, qstart, 'QUESTIONS-FORM', 'Open questions holds neither items nor `None.`')
        for l in items:
            if not l.startswith('- [NEEDS CLARIFICATION:'):
                report(path, qstart, 'QUESTIONS-FORM', f'"{l[:60]}" is not `[NEEDS CLARIFICATION: …]`')

    # Leads, not findings: one gap may truly break clauses in several specs.
    for b in sorted(scoped_beads):
        places = sorted(set(under.get(b, [])))
        if len(places) > 1:
            print(f'LEAD TODAY-REPEAT {b} is on the Today: lines of {", ".join(places)}')
    print('TODAY ' + json.dumps(work))
    sys.exit(1 if problems else 0)


if __name__ == '__main__':
    main()
