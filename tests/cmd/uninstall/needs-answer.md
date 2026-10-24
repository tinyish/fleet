# Uninstall never runs without an answer

With no terminal and no `--non-interactive`, it asks first, so it removes
nothing and exits 3.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ fleet uninstall
? 3
fleet: uninstall asks first, and there is no terminal to ask
hint: add --non-interactive to uninstall without asking

$ ls .fleet/bin .fleet/st
.fleet/bin:
fleet

.fleet/st:
...
state.sqlite3
...

$ fleet stop
the supervisor is stopped

```
