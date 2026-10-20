# A supervisor that no longer answers is replaced

When the service manager still has the supervisor but it does not answer,
`fleet start` takes the old one down and starts it fresh.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ gone .fleet/st/supervisor.sock

$ fleet start
the supervisor is running -- cleared for takeoff

$ fleet stop
the supervisor is stopped

```
