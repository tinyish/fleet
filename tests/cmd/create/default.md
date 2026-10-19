# A repository becomes a fleet, and nothing is committed

First time, from a script: `--non-interactive` starts the supervisor without
asking. The fleet's config is written into the repository and left
uncommitted: the repository keeps its one commit.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/app git status --porcelain
?? fleet.toml

$ in code/app git rev-list --count HEAD
1

$ fleet stop
the supervisor is stopped

```
