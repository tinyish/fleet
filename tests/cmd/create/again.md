# Running create again makes no second fleet

A second `create`, by habit or by a script that always runs it, says what it
found and changes nothing, for a wingman fleet too, whatever order its
projects come in.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/app fleet create --non-interactive
a fleet is already here, serving [CWD]/code/app; nothing changed

$ repo code/api code/blog

$ in wing fleet create --non-interactive --project ../code/api --project ../code/blog
created a wingman fleet in [CWD]/wing serving [CWD]/code/api, [CWD]/code/blog

$ in wing fleet create --non-interactive --project ../code/blog --project ../code/api
a fleet is already here, serving [CWD]/code/api, [CWD]/code/blog; nothing changed

$ fleet stop
the supervisor is stopped

```
