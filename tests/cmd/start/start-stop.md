# Start and stop say what happened, every time

Turning the fleet off for a while and back on: each step confirms what it
did, and stopping twice is not an error.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ fleet stop
the supervisor is stopped

$ fleet stop
the supervisor was not running

$ fleet start
the supervisor is running -- cleared for takeoff

$ fleet stop
the supervisor is stopped

```
