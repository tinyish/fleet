# The evals

Each `<verb>/<case>.md` is one eval: a transcript of commands and what a
script sees, checked by `trycmd` on every `cargo test`
(`crates/fleet/tests/cmd.rs`). CONTRIBUTING § Evals says what earns a case.

A case is a Markdown file with one or more ```` ```console ```` blocks:

```console
$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ fleet uninstall
? 3
fleet: uninstall asks first, and there is no terminal to ask
hint: add --non-interactive to uninstall without asking
```

- `$ …` runs a command; the lines after it are its stdout and stderr, as a
  script sees them (piped: no colour, `fleet:` and `hint:` prefixes, whole
  paths).
- `? N` is the exit code; without it, the command must succeed.
- `[CWD]` is the case's temp directory. `[..]` matches anything on a line,
  `...` on its own line matches any lines. Use them only where a change would
  not matter to a person or a script.
- Every case needs an empty `<case>.out/` beside it (holding `.keep`), so
  its commands run in a temp directory of its own.

Commands are not run by a shell: no `>`, `|`, `cd` or `&&`. The helpers in
`bin/` cover what a case needs:

| Command | Does |
| --- | --- |
| `fleet …` | the built binary, hermetic: a copy of it, run with an empty environment except `HOME` and the state directory inside the case, `LC_ALL=C`, and a `PATH` whose service manager is the fake. On macOS it also runs under `sandbox-exec`, which denies `/bin/launchctl` and any write in your real home. It exits 99 rather than run outside a case's temp directory |
| `in DIR CMD…` | runs CMD in DIR (made if missing), under the case's directory |
| `repo DIR…` | makes each DIR a git repository with one commit |
| `write FILE TEXT` | writes TEXT, with `\n` escapes, to FILE |
| `gone PATH…` | deletes each PATH, which must be relative and inside the case |
| `units` | lists the unit files and loaded services the fake service manager holds; empty when nothing would come back after a reboot |
| `git …` | git without your config, hooks or `GIT_*` environment |
| `cat`, `ls` | the system's own |

A case may run only these: the runner refuses any other command, since
trycmd would skip it silently. It also refuses a case without its `.out/`,
and refuses to run at all if the service driver ever names a tool by its path
instead of by name, because the fakes are found by name.

`bin/launchctl` and `bin/systemctl` are fakes of the service managers: they
answer only the calls the drivers make, the way the real tools do, and run
`fleet supervise` as a plain background process started from `/` with only
the unit's environment. `write .fleet/home/.fake-down ''` makes the service
manager look unreachable. A supervisor a case leaves running is killed when
the run ends.

To write a new case, put its commands down with no output, run
`TRYCMD=overwrite cargo test -p fleet --test cmd`, then read every captured
line against the promise the case pins before keeping it.
