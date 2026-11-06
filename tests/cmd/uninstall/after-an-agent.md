# Uninstall after an agent ran takes the tmux socket too

An agent's tmux server leaves its socket in the state directory, as the one
written here stands in for: uninstall deletes it with the rest, so the
directory goes and nothing is said to be the person's.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ write .fleet/st/tmux.sock ''

$ fleet uninstall --non-interactive
tinyfleet is uninstalled: the supervisor, its state and [CWD]/.fleet/bin/fleet are gone

$ ls .fleet
bin
home

```
