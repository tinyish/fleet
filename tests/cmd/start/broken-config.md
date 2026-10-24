# A fleet whose config does not read stays off, and the rest come on

A script can tell a partial start (exit 1, naming the fleet and why) from a
full one. Fixing the file and starting again brings it on.

```console
$ repo code/app code/api

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in wing fleet create --non-interactive --project ../code/api
created a wingman fleet in [CWD]/wing serving [CWD]/code/api

$ write wing/fleet.toml 'projects = ['

$ fleet start
? 1
the supervisor is already running; [CWD]/code/app is enabled
fleet: the fleet in [CWD]/wing stayed off: [CWD]/wing/fleet.toml does not read: line 1, column 13: unclosed array, expected `]`
hint: fix each fleet.toml named above, then run fleet start

$ write wing/fleet.toml 'projects = ["../code/api"]\n'

$ fleet start
the supervisor is already running

$ fleet stop
the supervisor is stopped

```
