# Claude Quota Router

Claude Quota Router keeps Claude Code on an account that still has quota. It reads the quota Claude Code hands to the statusline, remembers what each saved account had left when you were last on it, and moves the active credential down an order you set. Personal, team, and enterprise plans are all routed the same way.

![Claude Code runs the router as its statusLine.command and hands it rate_limits. The router files the reading under the current account in rate-limits.json, picks the first account the order in config.json still allows, and replaces the active credential with the one saved for that account, which Claude Code picks up at the next session.](docs/figures/switch-loop.svg)

## Install

Building needs Rust 1.88 or newer. On macOS and Linux:

```bash
./install.sh
```

The script builds the binary, copies it to `~/.local/bin`, and adds that directory to `PATH` in the file your login shell reads. It leaves the profile alone when the directory is already there. Open a new shell, then confirm:

```bash
claude-quota-router --version
```

[docs/install.md](docs/install.md) covers the options, a build by hand, and Windows.

## Set up your accounts

Run `setup` once per account, each time while Claude Code is signed in to that account. The name defaults to the email, because that is what tells two logins apart.

1. Save the account you are signed in to now.

   ```bash
   claude auth status          # check which account this is
   claude-quota-router setup
   ```

2. Sign in as the next account and save that one too. Repeat for every account you want in the rotation.

   ```bash
   claude auth login
   claude-quota-router setup
   ```

   Do not run `claude auth logout` for this. Logging out empties the credential Claude Code is holding without removing the record, which leaves Claude Code signed out and its statusline with no quota to report. `setup` refuses to save a credential in that state, and `claude-quota-router switch <name>` puts a working one back.

3. Put the accounts in the order you want them used.

   ```bash
   claude-quota-router config --priority me@work.com,me@gmail.com
   ```

   An account left out of the list falls in behind it by plan kind, in the order personal, team, enterprise, other.

4. Hook up the statusline, then restart Claude Code.

   ```bash
   claude-quota-router install
   ```

   This wraps the statusline command you already have rather than replacing it. Your command keeps running on every render and its output is printed after the router's own. Only `command` on the `statusLine` object changes, so keys such as `padding` survive, and `settings.json` is copied to `settings.json.bak-<timestamp>` first.

   That puts the router's segment at the front of the line, which is one opinion about a setting that is yours. If you want it somewhere else, or your statusline is built in a way the wrapper does not suit, skip `install` and paste this into Claude Code instead:

   ```text
   I installed a CLI called claude-quota-router and want its segment in my Claude Code statusline.

   The command is `claude-quota-router statusline`. It reads the same JSON payload on stdin that
   Claude Code gives any statusLine command, prints one short segment on stdout, prints nothing
   when it has nothing to report, and always exits 0.

   Please wire it into statusLine.command in ~/.claude/settings.json. If I already have a
   statusLine command, keep it and show both parts, and put the router's segment wherever suits
   my line best. Watch out for stdin: it can only be read once, so buffer the payload and give
   the same text to each command rather than piping stdin straight through to both.

   Back the file up before you edit it, and show me the command you ended up with.
   ```

   Wired this way the router prints only its own segment, because the command it would otherwise re-run is the one `install` records and you never ran it.

5. Move to the account you want to start on, then check the result.

   ```bash
   claude-quota-router switch me@work.com
   claude-quota-router list
   ```

   ```
   * 1. me@work.com     team        42%(5h)
     2. me@gmail.com    personal    -
   ```

   The star marks the account in use, the number is its place in the order, and the last column is the most recent quota reading for that account. A dash means nothing has been read yet, which is normal until you have spent a session on it.

## Everyday use

| Command | What it does |
|---|---|
| `claude-quota-router list` | Saved accounts in routing order, with their last quota reading |
| `claude-quota-router status` | Current account, settings, and the whole quota cache |
| `claude-quota-router current` | The current account name only |
| `claude-quota-router switch <name>` | Move to a saved account, asking first |
| `claude-quota-router toggle` | Move back to the account you came from |
| `claude-quota-router setup <name>` | Save again under a name you choose instead of the email |
| `claude-quota-router remove <name>` | Forget a saved account and delete its stored credential |

Add `--yes` to `switch` and `toggle` to skip the prompt. Restart Claude Code after a switch: a running session keeps using the credential it started with.

## What the statusline shows

The router prints its own part first and your old command's output after it, separated by `|`.

| Segment | Meaning |
|---|---|
| `me@work.com` | The account in use |
| `97%(5h)` | Usage has passed the alert threshold, in the five hour window |
| `LIMIT(7d)` | That window is full |
| `me@gmail.com reset in 2h10m(5h)` | A higher ranked account is waiting on a reset |
| `me@gmail.com reset done` | That account is usable again |
| `route failed` | An automatic switch could not complete |
| `-> claude-quota-router list` | Something needs your attention |

`claude-quota-router config --alert-at 90` moves the threshold, which is 95 by default.

## Automatic switching

```bash
claude-quota-router config --mode auto
```

In `auto` the statusline switches on its own: off an account that has hit its limit, onto the next usable one in the order, and back up to a higher ranked account once its reset has passed. It never pulls you back to an account below the one you moved to by hand. The default is `manual`, which only prints.

Readings are ignored for 90 seconds after any switch. The statusline payload carries no account identity, so a reading arriving right after a switch still belongs to the account you left.

## Removing it

```bash
./uninstall.sh --purge
```

This restores your old statusline command, deletes the saved credentials and this tool's directory, removes the binary, and takes out the `PATH` line the installer added. That line is matched by its marker comment, so a `PATH` line you wrote by hand survives.

| Command | Removes |
|---|---|
| `./uninstall.sh --purge` | Everything, including the binary and the `PATH` line |
| `./uninstall.sh` | The same, but keeps the saved accounts for later |
| `claude-quota-router uninstall --purge` | Saved credentials and state, keeping the binary on `PATH` |
| `claude-quota-router uninstall` | The statusline wrapper only |

A purge asks before it deletes; `--yes` answers for it. None of these touch the account Claude Code is signed in to, so removing the router logs you out of nothing.

## Good to know

- **Restart Claude Code after a switch.** A running session holds the credential it started with. The statusline says so when it switches for you.
- **The account Claude Code displays lags behind.** A switch replaces the active credential and nothing else. The `oauthAccount` block in `~/.claude.json` keeps the previous email, organization, and seat tier until Claude Code refetches the profile, which it does at most once a day. Requests go to the new account regardless, because the OAuth token decides that.
- **That lag reaches `claude auth status`,** which is where `setup` gets its default name. Running `setup` right after a switch would file the new credential under the previous account's name, so it refuses when the active credential is already saved under another name. Sign in properly with `claude auth login`, or pass the name yourself.
- **Saved credentials are real credentials.** On macOS they are Keychain items under the service `claude-quota-router`; elsewhere they are owner-only files. They carry a live refresh token, so treat them the way you treat the login itself.
- **A broken state file costs you the router, not your statusline.** If this tool's own files cannot be read, it prints nothing and your original command still renders.
- **No quota in the statusline usually means Claude Code is signed out.** Claude Code leaves `rate_limits` out of the statusline payload entirely when it has no live quota window, and neither this tool nor your own command can show a number that is not in the payload. `claude-quota-router status` prints a `signed in:` line, and `claude auth status` reports `loggedIn`.

## Documentation

| Document | Contents |
|---|---|
| [docs/routing.md](docs/routing.md) | How the order and the cached readings pick an account, and what the statusline prints |
| [docs/platforms.md](docs/platforms.md) | Credential stores, notifications, and file locations per platform |
| [docs/install.md](docs/install.md) | Install options, a build by hand, the statusline wrapper, and removal |
| [docs/development.md](docs/development.md) | Module layout, checks, and adding a platform backend |

## License

MIT. See [LICENSE](LICENSE).
