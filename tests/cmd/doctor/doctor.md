# Doctor says what is fine, and changes nothing

Before anything, with the supervisor not started (a warning, exit 0); then
in a fleet with it running.

```console
$ fleet doctor
fine            service manager  [..] is up and running
fine            git              git version [..]
fine            curl             curl [..]
warning         supervisor       not running

hint: run fleet start to start it

$ ls .fleet
bin
home

$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/app fleet doctor
fine            service manager  [..] is up and running
fine            git              git version [..]
fine            curl             curl [..]
fine            supervisor       running, version [..]
fine            state            [CWD]/.fleet/st: open at schema 1
fine            project          [CWD]/code/app: 1 project, all present

$ fleet stop
the supervisor is stopped

```
