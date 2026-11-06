# Uninstall after an agent ran ends its tmux server and takes the socket

An agent's tmux server outlives the supervisor's stop: uninstall ends it,
on tinyfleet's own socket, so no agent keeps running once tinyfleet is gone.
The socket written here stands in for the server's, which the fake tmux
ends. Then the socket goes with the rest, so the directory goes and nothing
is said to be the person's.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ write .fleet/st/tmux.sock ''

$ fleet uninstall --non-interactive
tinyfleet is uninstalled: the supervisor, its state and [CWD]/.fleet/bin/fleet are gone

$ cat .fleet/home/.fake-tmux
kill-server [CWD]/.fleet/st/tmux.sock

$ ls .fleet
bin
home

```
