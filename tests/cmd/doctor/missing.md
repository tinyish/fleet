# Doctor names what is missing, and exits 1

With the service manager not answering, doctor says so, in its own line,
and the exit code tells a script something is missing.

```console
$ write .fleet/home/.fake-down ''

$ fleet doctor
? 1
missing         service manager  [..] does not answer: [..]
fine            git              git version [..]
fine            curl             curl [..]
warning         supervisor       not running

hint: run fleet start to start it

```
