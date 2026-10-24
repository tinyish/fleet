# A script with no terminal never waits

`fleet create` needs the supervisor. With no terminal to ask and no
`--non-interactive`, it does not start it (no state appears) and does not wait: it ends at once,
exit 3, and writes nothing.

```console
$ repo code/app

$ in code/app fleet create
? 3
fleet: the supervisor is not running
hint: run fleet start to start it

$ ls code/app
README.md

$ ls .fleet
bin
home

```
