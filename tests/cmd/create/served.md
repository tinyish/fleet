# A repository a wingman serves gets no second fleet

A plain `create` in a repository a wingman fleet already serves is refused,
and the repository stays clean, as the wingman promised it would.

```console
$ repo code/api

$ in wing fleet create --non-interactive --project ../code/api
started the supervisor; created a wingman fleet in [CWD]/wing serving [CWD]/code/api

$ in code/api fleet create --non-interactive
? 1
fleet: [CWD]/code/api is already served by the fleet in [CWD]/wing

$ in code/api git status --porcelain

$ fleet stop
the supervisor is stopped

```
