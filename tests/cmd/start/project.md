# One project on or off; the rest keep theirs

```console
$ repo code/api code/blog

$ in wing fleet create --non-interactive --project ../code/api --project ../code/blog
started the supervisor; created a wingman fleet in [CWD]/wing serving [CWD]/code/api, [CWD]/code/blog

$ fleet stop --project code/api
[CWD]/code/api is off; [CWD]/code/blog keeps its own on or off

$ fleet start --project code/api
[CWD]/code/api is on

$ fleet stop --project code/api/src
? 1
fleet: [CWD]/code/api/src is inside the project [CWD]/code/api
hint: name its root, [CWD]/code/api

$ fleet stop
the supervisor is stopped

```
