# Development

## Checks

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy --target x86_64-unknown-linux-gnu --all-targets -- -D warnings
cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo build --release
```

The cross target runs type check the platform branches the host cannot execute; add the targets once with `rustup target add`. `Cargo.toml` turns on `clippy::pedantic` and forbids unsafe, so `-D warnings` gates both.

## Module layout

| Module | Holds |
|---|---|
| `domain` | Account names, plan kinds, config, state, and what a quota reading means |
| `routing` | The account order and the choice of the next account |
| `statusline` | The statusline command: parse, record, decide, render |
| `switcher` | The switch transaction, including the backup of the outgoing credential |
| `credentials` | The credential store, one backend per platform |
| `storage` | JSON persistence for everything that is not a secret |
| `context` | Every path the tool reads or writes |
| `shell` | Running the inner statusline command and quoting a path into the setting |
| `notification` | Desktop notifications and their per event markers |

Platform branches live in `credentials`, `shell`, `notification`, and `context`, and nowhere else. `app` and `cli` hold command wiring only.

## The README figure

`docs/figures/switch-loop.svg` is rendered from `switch-loop.d2` beside it, which carries the exact command in a comment at the top. Keep the source and the render together, and check after a change that no label falls below 12px at a 720px delivery width.

## Adding a platform backend

The credential store is the part most likely to need one. `credentials/mod.rs` selects a backend by `cfg`, and each backend provides the same six functions: `describe`, `read_active`, `write_active`, `read_saved`, `write_saved`, and `delete_saved`. A new backend is a module beside `keychain.rs` and `file.rs` plus a `cfg` arm, with no change to any caller.

Keep a new backend compiled on every platform and give it unit tests that run anywhere, the way `file.rs` does. A backend that only compiles on the machine that cannot run it is not covered by anything.

## Tests

Tests cover the parts that decide behavior: name validation, what a reading means, the account order, the settling window, and the file backend's round trip, permissions, and symlink refusal. There is no test per function and no test of CLI output text.
