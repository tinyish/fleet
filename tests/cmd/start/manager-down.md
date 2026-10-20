# A service manager that refuses: start fails at once and says why

It names the service manager's command and what it said, exit 1. The unit
file it wrote stays, but nothing is loaded.

```console
$ write .fleet/home/.fake-down ''

$ fleet start
? 1
fleet: the service manager did not start the supervisor: [..] failed: [..]

$ units
unit [..]

```
