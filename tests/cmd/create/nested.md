# A fleet is never made inside or around another

Inside a fleet's repository, inside a fleet's directory as a wingman, or in
a directory that holds one, create is refused and writes nothing.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/app/src fleet create --non-interactive
? 1
fleet: [CWD]/code/app/src is inside the repository at [CWD]/code/app
hint: run fleet create in [CWD]/code/app

$ in code fleet create --non-interactive --project app
? 1
fleet: [CWD]/code contains the fleet in [CWD]/code/app

$ repo code/api

$ in code/app/notes fleet create --non-interactive --project ../../api
? 1
fleet: [CWD]/code/app/notes is inside the fleet in [CWD]/code/app

$ ls code code/app/src code/app/notes
code:
api
app

code/app/notes:

code/app/src:

$ fleet stop
the supervisor is stopped

```
