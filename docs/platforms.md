# Platforms

| Platform | Credential store | Desktop notifications | Inner statusline shell | Verification |
|---|---|---|---|---|
| macOS | Keychain, through `security` | `osascript` | `/bin/sh -lc` | built and run |
| Linux | owner-only files | `notify-send` | `/bin/sh -lc` | compiled and unit tested, not run |
| Windows | files under the profile ACL | none | `cmd /C` | compiled and unit tested, not run |

On Windows the statusline text is the only channel for a quota event, because no notifier is present by default that is safe to shell out to.

## Credential stores

On macOS both stores are Keychain services: `Claude Code-credentials` holds the credential Claude Code is using, and `claude-quota-router` holds the saved accounts.

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

A switch replaces the active credential and nothing else. The `oauthAccount` block in `~/.claude.json` still holds the previous account's email, organization, and seat tier until Claude Code refetches the profile. Requests go to the new account because the OAuth token decides that, but anything Claude Code displays from that block lags behind.
