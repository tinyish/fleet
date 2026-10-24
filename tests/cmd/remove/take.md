# Remove --project takes one project out, keeping the file as written

```console
$ repo code/api code/blog

$ write wing/fleet.toml '# my wingman\nprojects = [\n  "../code/api", # the api\n  "../code/blog",\n]\n'

$ in wing fleet create --non-interactive
started the supervisor; restored existing wingman fleet in [CWD]/wing serving [CWD]/code/api, [CWD]/code/blog

$ in wing fleet remove --project ../code/blog
[CWD]/code/blog is no longer served by the wingman fleet in [CWD]/wing

$ cat wing/fleet.toml
# my wingman
projects = [
  "../code/api", # the api
]

$ in wing fleet remove --project ../code/api
removed the wingman fleet in [CWD]/wing; fleet.toml was kept

$ fleet stop
the supervisor is stopped

```
