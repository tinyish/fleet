# Remove takes a fleet off, keeping its file when nobody is asked

Piped, the fleet goes and `fleet.toml` stays, and starting afterwards says
nothing of it. Nothing to remove says so and exits 0. Inside a fleet's repository, remove names the fleet's directory.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/app fleet remove
removed the fleet in [CWD]/code/app; fleet.toml was kept

$ ls code/app
README.md
fleet.toml

$ in code/app fleet remove
there is no fleet in [CWD]/code/app; nothing was removed

$ fleet start
the supervisor is already running; there are no fleets yet
hint: run fleet create in a repository to make one

$ in code/app fleet create --non-interactive
restored existing fleet in [CWD]/code/app

$ in code/app/src fleet remove
? 1
fleet: [CWD]/code/app/src is inside the fleet in [CWD]/code/app
hint: run fleet remove in [CWD]/code/app

$ fleet stop
the supervisor is stopped

```
