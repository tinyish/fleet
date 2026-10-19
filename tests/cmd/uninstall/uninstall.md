# Uninstall takes tinyfleet off and leaves projects' files

`--non-interactive` answers yes: the supervisor stops and its unit goes, so
nothing comes back after a reboot; its state and the binary go; and
`fleet.toml` in the project stays.

```console
$ repo code/app

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ units
unit [..]
service [..]

$ fleet uninstall --non-interactive
tinyfleet is uninstalled: the supervisor, its state and [CWD]/.fleet/bin/fleet are gone

$ ls .fleet .fleet/bin
.fleet:
bin
home

.fleet/bin:

$ units

$ ls code/app
README.md
fleet.toml

```
