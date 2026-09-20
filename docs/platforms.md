# Platforms

| Platform | Credential store | Desktop notifications | Inner statusline shell | Verification |
|---|---|---|---|---|
| macOS | Keychain, through `security` | `osascript` | `/bin/sh -c` | built and run |
| Linux | owner-only files | `notify-send` | `/bin/sh -c` | compiled and unit tested, not run |
| Windows | files under the profile ACL | none | `cmd /C` | compiled and unit tested, not run |

The inner shell matches how Claude Code runs a statusline command itself, which is Node's `shell: true`. A login shell would re-read your profile files on every render and could hand your command a different `PATH` than the one it runs under today.

On Windows the statusline text is the only channel for a quota event, because no notifier is present by default that is safe to shell out to.

## Credential stores

On macOS both stores are Keychain services: `Claude Code-credentials` holds the credential Claude Code is using, and `claude-quota-router` holds the saved accounts. Every read and write goes through `/usr/bin/security`, because a Keychain item's access list is keyed on the calling binary: Apple's own tool keeps one grant across rebuilds of this one, and calling the Security framework in process would raise an authorization dialog while the statusline is rendering.

Two consequences of that route are worth knowing. A credential travels in the argv of the `security` child process, where any process running as your user can read it for as long as the call lasts; that is not new access, since the same processes can read the items outright with the same command. And `security` has stored a silently truncated value while still exiting 0, so every write is read back and compared, and a write that does not match is undone before the error is reported.

Everywhere else both are files. The active credential is Claude Code's own `.credentials.json` in its config directory, and each saved account is `accounts/<name>.json` in this tool's configuration directory. A file is replaced by creating a fresh owner-only file and renaming it over the old one, so an interrupted write cannot leave half a credential behind, and a symlink at either path is refused rather than followed. That is the treatment Claude Code gives its own file. On Unix the files are `0600` and the `accounts` directory is `0700`; Windows has no mode bits, so the files inherit the user profile's access control list.

The file backend is compiled and unit tested on every platform, including macOS, so its behavior is covered even where it is not the backend in use.

## Locations

The configuration directory is `%APPDATA%\claude-quota-router` on Windows, `$XDG_CONFIG_HOME/claude-quota-router` on Linux when that variable is set, and `~/.config/claude-quota-router` otherwise. `CLAUDE_QUOTA_ROUTER_HOME` overrides it.

| Data | Location |
|---|---|
| Account metadata | `state.json` in the configuration directory |
| Alert threshold, mode, and account order | `config.json` in the configuration directory |
| Quota cache, one reading per account | `rate-limits.json` in the configuration directory |
| Your previous statusline command | `inner-statusline.txt` in the configuration directory |
| Claude Code settings the wrapper edits | `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json` |

## What a switch does not change

A switch replaces the active credential and nothing else. The `oauthAccount` block in `~/.claude.json` still holds the previous account's email, organization, and seat tier until Claude Code refetches the profile, which it does at most once a day. Requests go to the new account because the OAuth token decides that, but anything Claude Code reads back out of that block lags behind, including the `email` that `claude auth status` reports.

`setup` names an account after that email, so running it right after a switch would file the new credential under the previous account's name. It refuses instead: if the active credential is already saved under another name, it says so and stops.
