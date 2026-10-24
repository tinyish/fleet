# fleet supervise takes no arguments

The service manager runs `fleet supervise`. Given anything after it, it
refuses with a usage error and serves nothing, so `--help` can never start a
supervisor on the real state (fleet-lc94.10).

```console
$ fleet supervise --help
? 2
fleet supervise: takes no arguments, not --help; the service manager runs it, and `fleet start` installs it

$ fleet supervise now
? 2
fleet supervise: takes no arguments, not now; the service manager runs it, and `fleet start` installs it

$ ls .fleet
bin
home

```
