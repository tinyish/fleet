# Guards

Guards refuse dangerous shell commands before a seat's agent runs them. This
area covers `fleet guard`, which an agent's pre-tool hook calls, and the four
guard classes: shell-trap, record, release-ref and production-write.

## Modules

- `cli/src/guard.rs` — Runs `fleet guard`, which judges one pre-tool payload, or
  reports each class's target with `--check`.
  Depends on: `cli/src/exit.rs`, `cli/src/ui.rs`, `controller/src/config.rs`,
  `controller/src/platform/mod.rs`, `controller/src/project/mod.rs`,
  `core/src/defaults.rs`, `core/src/guard/hook.rs`, `core/src/guard/mod.rs`,
  `core/src/item/brief.rs`, `core/src/pack.rs`, `core/src/store/mod.rs`.
- `core/src/guard/hook.rs` — Reads an agent's pre-tool hook payload and refusal
  template through the mapping its adapter declares.
  Depends on: nothing in fleet.
- `core/src/guard/lex.rs` — Splits a shell command into words and operators,
  keeping each word's quoting, for the classes to judge.
  Depends on: nothing in fleet.
- `core/src/guard/mod.rs` — Judges a command against the guard classes in force,
  and returns a verdict.
  Depends on: `core/src/pack.rs`, `core/src/policy.rs`, `core/src/resolve.rs`,
  `core/src/store/mod.rs`.
- `core/src/guard/production_write.rs` — Refuses writes to the buckets, cloud
  projects and applications a project lists as production.
  Depends on: `core/src/guard/lex.rs`, `core/src/guard/mod.rs`,
  `core/src/guard/release_ref.rs`.
- `core/src/guard/record.rs` — Refuses commands that destroy, bypass or forge an
  item's record, or name an item by a bare id.
  Depends on: `core/src/entry.rs`, `core/src/guard/lex.rs`,
  `core/src/guard/mod.rs`.
- `core/src/guard/release_ref.rs` — Refuses a push whose destination matches the
  project's release-ref glob.
  Depends on: `core/src/guard/lex.rs`, `core/src/guard/mod.rs`.
- `core/src/guard/shell_trap.rs` — Refuses shell commands that give a plausible
  wrong answer, and prints the rewrite.
  Depends on: `core/src/guard/lex.rs`, `core/src/guard/mod.rs`.

## Flow

1. An agent's pre-tool hook runs `fleet guard` with the payload on stdin, and
   `cli/src/main.rs` hands it to `cli/src/guard.rs`.
2. `cli/src/guard.rs` finds the agent adapter's hook mapping, and
   `core/src/guard/hook.rs` reads the tool and the command out of the payload.
3. `core/src/guard/mod.rs` reads which classes are in force from the packs and
   the policy, and judges the command.
4. `core/src/guard/lex.rs` splits the command into words, and each class in
   force checks them: `core/src/guard/shell_trap.rs`,
   `core/src/guard/record.rs`, `core/src/guard/release_ref.rs` and
   `core/src/guard/production_write.rs`.
5. `core/src/guard/hook.rs` puts a refusal into the adapter's template, and
   `cli/src/guard.rs` prints it for the agent.

## Depends on other areas

- packs — the classes a pack turns on, the agent adapter's hook table, and the
  defaults the layers end on.
- items — the pack layers the brief reads through, and the entry format the
  record class protects.
- seats — the seat list `fleet guard` reads.
- store — the store's default command name, which the record class watches.
- shared — the exit table, the terminal, the platform layer and the policy.
