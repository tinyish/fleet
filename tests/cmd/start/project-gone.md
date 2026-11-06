# A project whose repository is gone is turned off by the path given

```console
$ repo code/api code/blog

$ in wing fleet create --non-interactive --project ../code/api --project ../code/blog
started the supervisor; created a wingman fleet in [CWD]/wing serving [CWD]/code/api, [CWD]/code/blog

$ gone code/blog

$ in code/api fleet stop --project ../blog
[CWD]/code/blog is off; [CWD]/code/api keeps its own on or off

$ fleet stop
the supervisor is stopped

```
