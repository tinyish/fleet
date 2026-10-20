# A stopped fleet stays stopped

After `fleet stop` the supervisor's unit is gone, so nothing comes back after
a logout or a reboot, until `fleet start` puts it back.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ units
unit [..]
service [..]

$ fleet stop
the supervisor is stopped

$ units

$ fleet start
the supervisor is running -- cleared for takeoff

$ units
unit [..]
service [..]

$ fleet stop
the supervisor is stopped

```
