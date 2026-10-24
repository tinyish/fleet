# A wingman fleet leaves the repositories it serves untouched

The fleet's config lives in the wingman's own directory, so the repositories
have nothing to commit, ignore or explain.

```console
$ repo code/api code/blog

$ in wing fleet create --non-interactive --project ../code/api --project ../code/blog
started the supervisor; created a wingman fleet in [CWD]/wing serving [CWD]/code/api, [CWD]/code/blog

$ in code/api git status --porcelain

$ in code/blog git status --porcelain

$ fleet stop
the supervisor is stopped

```
