# A fleet.toml no fleet owns is taken up as it is

A config left by a removed fleet is restored by the next create. With
`--project`, the projects must be the ones the file lists.

```console
$ repo code/app code/api code/blog

$ in code/app fleet create --non-interactive
started the supervisor; created a fleet in [CWD]/code/app -- welcome aboard

$ in code/app fleet remove --non-interactive
removed the fleet in [CWD]/code/app; fleet.toml was kept

$ in code/app fleet create --non-interactive
restored existing fleet in [CWD]/code/app

$ in wing fleet create --non-interactive --project ../code/api --project ../code/blog
created a wingman fleet in [CWD]/wing serving [CWD]/code/api, [CWD]/code/blog

$ in wing fleet remove --non-interactive
removed the wingman fleet in [CWD]/wing; fleet.toml was kept

$ in wing fleet create --non-interactive --project ../code/api
? 1
fleet: [CWD]/wing/fleet.toml is already here, serving [CWD]/code/api, [CWD]/code/blog
hint: run fleet create without --project to take it up as it is

$ in wing fleet create --non-interactive --project ../code/blog --project ../code/api
restored existing wingman fleet in [CWD]/wing serving [CWD]/code/api, [CWD]/code/blog

$ fleet stop
the supervisor is stopped

```
