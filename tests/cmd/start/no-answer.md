# A supervisor that never comes up: start says it could not tell

The supervisor starts and exits at once (its state file is not a
database), so it never answers. `fleet start` stops waiting, says it could
not tell, and points at the log. This case takes about ten seconds.

```console
$ write .fleet/st/state.sqlite3 'not a database'

$ fleet start
? 4
fleet: the supervisor did not answer within 10s
hint: check its log: [..]

```
