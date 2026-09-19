# Claude Quota Router

Claude Quota Router switches saved Claude Code accounts on macOS and shows quota timing in the statusline. Personal, team, and enterprise plans are all routed the same way.

## Scenario

Use it when the account you prefer is near its limit and another saved account should carry the work until the first one resets.

```text
me@work.com account
  -> quota alert in statusline
  -> switch to me@gmail.com
  -> me@work.com reset countdown
  -> switch back to me@work.com
```

## Install

Build the binary and place it on `PATH`.

```bash
cargo build --release
cp target/release/claude-quota-router ~/.local/bin/
```

## Setup

Save each Claude Code login once. The account name defaults to the email address that `claude auth status` reports, which is what tells two logins apart. The plan kind comes from the same command: `max`, `pro`, and `free` are saved as `personal`, and `team` and `enterprise` keep their own names.

1. Log in with the first account and save it.

```bash
claude-quota-router setup
```

2. Log out, log in with the next account, then save it. Repeat for every account.

```bash
claude-quota-router setup
```

Pass a name when a shorter one reads better, and `--kind` when the plan should be recorded as something other than what `claude auth status` reports.

```bash
claude-quota-router setup team-side --kind team
```

3. Install the statusline wrapper. An existing `statusLine` command is preserved and still runs; other keys on that setting, such as `padding`, are left alone.

```bash
claude-quota-router install
```

4. Set the alert threshold and the account order.

```bash
claude-quota-router config --alert-at 95 --mode manual
claude-quota-router config --priority me@work.com,me@gmail.com
```

Accounts missing from `--priority` fall in behind it by plan kind, in the order personal, team, enterprise, other. Pass `--priority ''` to clear the list.

## Daily Use

Switch accounts from a shell or from Claude Code with `!`.

```bash
claude-quota-router switch me@gmail.com --yes
claude-quota-router toggle --yes
claude-quota-router list
claude-quota-router status
```

`list` prints accounts in routing order with the last quota reading for each one.

```text
  1. me@work.com	team	100%(5h) reset in 2h10m(5h)
* 2. me@gmail.com	personal	42%(5h)
  3. me@enterprise.com	enterprise	-
```

## Auto Mode

Auto mode follows the account order, not the plan kind.

```bash
claude-quota-router config --mode auto
```

| State | Action |
|---|---|
| current account reaches 100% | switch to the first account in the order that is not at its own limit |
| a higher ranked account passes its reset time | switch back to that account |
| every account is at its limit | stay put and report it in the statusline |

Two rules keep auto mode from acting on stale readings. Only the account Claude Code is logged into reports its own quota, so every other account is judged by the reading taken when the router last left it. After a switch, quota readings are ignored for 90 seconds, because a running Claude Code session keeps reporting the quota of the account it started with until it picks up the new credential.

## Storage

| Data | Location |
|---|---|
| Saved account credentials | macOS Keychain service `claude-quota-router` |
| Active Claude Code credential | macOS Keychain service `Claude Code-credentials` |
| Account metadata | `~/.config/claude-quota-router/state.json` |
| Alert, mode, and account order | `~/.config/claude-quota-router/config.json` |
| Quota cache, one entry per account | `~/.config/claude-quota-router/rate-limits.json` |

Switching replaces the Keychain credential only. The `oauthAccount` block in `~/.claude.json` still describes the previous account until Claude Code refetches the profile.

## Verify

Run the checks before publishing a change.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```
