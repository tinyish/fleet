# Start forgets a fleet whose directory is gone

Once, and then never again.

```console
$ repo code/app code/blog

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/blog fleet create --non-interactive
created a fleet in [CWD]/code/blog -- welcome aboard

$ gone code/blog

$ fleet start
the supervisor is already running
fleet: the fleet in [CWD]/code/blog was forgotten: its directory no longer exists

$ fleet start
the supervisor is already running

$ fleet stop
the supervisor is stopped

```
