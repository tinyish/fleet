# Running create again makes no second fleet

A second `create`, by habit or by a script that always runs it, says what it
found and changes nothing.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/app fleet create --non-interactive
a fleet is already here, serving [CWD]/code/app; nothing changed

$ fleet stop
the supervisor is stopped

```
