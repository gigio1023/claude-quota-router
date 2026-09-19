# Install

```bash
./install.sh
```

The script builds the release binary, copies it to `~/.local/bin`, and adds that directory to `PATH` in the file your login shell reads.

| Login shell | File it writes | Line it adds |
|---|---|---|
| zsh | `${ZDOTDIR:-$HOME}/.zprofile` | `export PATH=...` |
| bash | `~/.bash_profile`, else `~/.profile` | `export PATH=...` |
| fish | `~/.config/fish/config.fish` | `fish_add_path ...` |
| anything else | none | printed for you to add |

It leaves the profile alone when the directory is already on `PATH` or the file already mentions it, so rerunning it does not stack up lines.

| Option | Effect |
|---|---|
| `--bin-dir DIR` | Install into `DIR` instead of `~/.local/bin` |
| `--no-path` | Install the binary and touch no shell profile |

`CLAUDE_QUOTA_ROUTER_BIN_DIR` sets the same directory as `--bin-dir`.

Building needs Rust 1.88 or newer. The script falls back to `~/.cargo/bin/cargo` when `cargo` is not on `PATH`, which is where rustup puts it after an install that left the shell profile alone.

## By hand

```bash
cargo build --release
cp target/release/claude-quota-router ~/.local/bin/
```

`install.sh` is a POSIX shell script and covers macOS and Linux. On Windows, build with `cargo build --release`, copy `target\release\claude-quota-router.exe` to a directory on `PATH`, and add that directory with `setx PATH`.

## The statusline wrapper

```bash
claude-quota-router install
```

This writes `statusLine.command` in Claude Code's `settings.json` and saves whatever command was there. The router runs your old command on every render, feeds it the same JSON Claude Code sent, and prints its output after its own, so the statusline you already had keeps working.

Only `command` is replaced. Other keys on the `statusLine` object, such as `padding`, are left as they are, and the rest of `settings.json` keeps its key order.

Run the command from the copy on `PATH` rather than from `target/release`, because it records its own path in the setting. `claude-quota-router uninstall` puts the saved command back.

When `CLAUDE_CONFIG_DIR` is set, both commands follow it, because that is the directory Claude Code reads its settings from.
