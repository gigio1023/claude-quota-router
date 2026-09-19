//! Claude Code statusline integration.
//!
//! Claude Code calls this binary repeatedly with JSON on stdin. Rendering must
//! therefore stay lightweight, tolerate malformed input, and avoid noisy output
//! when quota data is absent.

use crate::context::AppContext;
use crate::domain::{AccountName, Config, LimitWindow, RateLimitSnapshot, RoutingMode, State};
use crate::keychain::Keychain;
use crate::notification;
use crate::routing;
use crate::storage;
use crate::switcher::{self, SwitchOptions};
use crate::time::{now_epoch, now_epoch_i64};
use crate::util::humanize;
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};

const APP_TITLE: &str = "claude-quota-router";

/// How long quota readings are ignored after a switch.
///
/// The statusline payload carries no account identity, and a running Claude Code
/// session keeps reporting the quota of the account it started with. Filing that
/// reading under the new account would both corrupt the cache and bounce the
/// router straight back out of the account it just entered.
const SWITCH_GRACE: i64 = 90;

pub(crate) fn handle(ctx: &AppContext, keychain: &Keychain) -> Result<()> {
    let input = read_stdin_to_string()?;
    if input.trim().is_empty() {
        return Ok(());
    }

    let parsed: Value = match serde_json::from_str(&input) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    let inner_output = run_inner_statusline(ctx, &input);
    let state = storage::load_state(ctx)?;
    let config = storage::load_config(ctx)?;
    // A broken state file or a failed Keychain read must not blank the whole
    // statusline, which would take the user's own command down with it.
    let router_output = render_router_output(ctx, keychain, &state, &config, &parsed).unwrap_or(None);

    match (router_output.as_deref(), inner_output.as_deref()) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => println!("{a} | {b}"),
        (Some(a), _) if !a.is_empty() => println!("{a}"),
        (_, Some(b)) if !b.is_empty() => println!("{b}"),
        _ => {}
    }

    Ok(())
}

fn render_router_output(
    ctx: &AppContext,
    keychain: &Keychain,
    state: &State,
    config: &Config,
    input: &Value,
) -> Result<Option<String>> {
    let Some(current) = state.current_account.as_deref() else {
        return Ok(None);
    };
    if !state.accounts.contains_key(current) {
        return Ok(None);
    }

    let now = now_epoch_i64();
    let settling = is_settling(state, now);
    let mut cache = storage::load_rate_limits(ctx)?;

    if !settling
        && let Some(snapshot) = parse_rate_limits(input)
    {
        ctx.ensure_app_dir()?;
        cache.accounts.insert(current.to_string(), snapshot);
        storage::save_rate_limits(ctx, &cache)?;
    }

    let mut routing_failed = false;
    if !settling && config.mode == RoutingMode::Auto {
        match auto_route(ctx, keychain, state, config, &cache, current, now) {
            Ok(Some(message)) => return Ok(Some(message)),
            Ok(None) => {}
            // A switch can fail on a missing or locked Keychain entry. Say so in
            // the statusline and keep rendering rather than dropping the line.
            Err(_) => routing_failed = true,
        }
    }

    Ok(Some(render_segments(
        ctx,
        state,
        config,
        &cache,
        current,
        now,
        routing_failed,
    )))
}

/// Move off an account that is at its limit, or back to one that has reset.
fn auto_route(
    ctx: &AppContext,
    keychain: &Keychain,
    state: &State,
    config: &Config,
    cache: &crate::domain::RateLimitCache,
    current: &str,
    now: i64,
) -> Result<Option<String>> {
    if let Some(target) = routing::return_target(state, config, cache, current, now) {
        return switch_and_report(ctx, keychain, &target);
    }
    if is_blocked(cache, current, now)
        && let Some(target) = routing::next_target(state, config, cache, current, now)
    {
        return switch_and_report(ctx, keychain, &target);
    }
    Ok(None)
}

fn switch_and_report(
    ctx: &AppContext,
    keychain: &Keychain,
    target: &str,
) -> Result<Option<String>> {
    let name = AccountName::parse(target)?;
    switcher::switch_to(
        ctx,
        keychain,
        &name,
        SwitchOptions {
            yes: true,
            emit: false,
        },
    )?;
    Ok(Some(format!("routed to {target}; restart Claude Code")))
}

fn render_segments(
    ctx: &AppContext,
    state: &State,
    config: &Config,
    cache: &crate::domain::RateLimitCache,
    current: &str,
    now: i64,
    routing_failed: bool,
) -> String {
    let mut segments = vec![current.to_string()];
    let mut actionable = routing_failed;

    if let Some(snapshot) = cache.accounts.get(current)
        && let Some((label, pct)) = snapshot.peak_usage()
        && pct >= config.alert_at
    {
        if pct >= 100 {
            segments.push(format!("LIMIT({label})"));
            notify(
                ctx,
                &format!("limit:{current}"),
                &format!("{current} quota is full."),
            );
            actionable = true;
        } else {
            segments.push(format!("{pct}%({label})"));
            notify(
                ctx,
                &format!("alert:{current}"),
                &format!("{current} usage is {pct}% ({label}). Prepare to switch."),
            );
        }
    }

    if let Some(name) = routing::return_target(state, config, cache, current, now) {
        segments.push(format!("{name} reset done"));
        notify(
            ctx,
            &format!("reset:{name}"),
            &format!("{name} quota reset is complete."),
        );
        actionable = true;
    } else if let Some((name, label, reset)) =
        routing::pending_recovery(state, config, cache, current, now)
    {
        let remaining = reset - now;
        segments.push(format!("{name} reset in {}({label})", humanize(remaining)));
        notify_reset_soon(ctx, &name, remaining);
    }

    if routing_failed {
        segments.push("route failed".to_string());
    }
    if actionable {
        segments.push("-> claude-quota-router list".to_string());
    }
    segments.join(" ")
}

fn notify(ctx: &AppContext, key: &str, message: &str) {
    notification::notify_once(ctx, key, APP_TITLE, message).ok();
}

fn notify_reset_soon(ctx: &AppContext, account: &str, remaining: i64) {
    if remaining <= 60 {
        notify(
            ctx,
            &format!("reset-1min:{account}"),
            &format!("{account} quota resets within 1 minute."),
        );
    } else if remaining <= 300 {
        let minutes = (remaining as u64).div_ceil(60);
        notify(
            ctx,
            &format!("reset-5min:{account}"),
            &format!("{account} quota resets in {minutes} minutes."),
        );
    }
}

fn is_settling(state: &State, now: i64) -> bool {
    state
        .switched_at
        .is_some_and(|at| now.saturating_sub(at as i64) < SWITCH_GRACE)
}

fn is_blocked(cache: &crate::domain::RateLimitCache, account: &str, now: i64) -> bool {
    cache
        .accounts
        .get(account)
        .is_some_and(|snapshot| snapshot.is_blocked(now))
}

fn parse_rate_limits(input: &Value) -> Option<RateLimitSnapshot> {
    let rate_limits = input.get("rate_limits")?;
    Some(RateLimitSnapshot {
        detected_at: now_epoch(),
        five_hour: window(rate_limits, "five_hour"),
        seven_day: window(rate_limits, "seven_day"),
        spend_limit: window(rate_limits, "spend_limit"),
    })
}

fn window(rate_limits: &Value, name: &str) -> LimitWindow {
    let Some(value) = rate_limits.get(name) else {
        return LimitWindow::default();
    };
    LimitWindow {
        used_percentage: value_to_u8(value.get("used_percentage")),
        resets_at: value_to_i64(value.get("resets_at")),
    }
}

fn value_to_u8(value: Option<&Value>) -> Option<u8> {
    match value? {
        Value::Number(number) => number.as_u64().and_then(|value| u8::try_from(value).ok()),
        Value::String(text) => text.parse::<u8>().ok(),
        _ => None,
    }
}

fn value_to_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse::<i64>().ok(),
        _ => None,
    }
}

fn read_stdin_to_string() -> Result<String> {
    io::read_to_string(io::stdin()).context("failed to read stdin")
}

fn run_inner_statusline(ctx: &AppContext, input: &str) -> Option<String> {
    let command = fs::read_to_string(ctx.inner_statusline_path()).ok()?;
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    // Existing statusline commands are intentionally executed as the shell
    // command Claude Code already accepted in settings.json. The wrapper only
    // preserves compatibility; it does not reinterpret the user's command.
    let mut child = Command::new("/bin/sh")
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_every_rate_limit_window() {
        let input = json!({
            "rate_limits": {
                "five_hour": { "used_percentage": 91, "resets_at": 1000 },
                "seven_day": { "used_percentage": "12", "resets_at": "2000" },
                "spend_limit": { "used_percentage": 40, "resets_at": 3000 }
            }
        });
        let parsed = parse_rate_limits(&input).unwrap();
        assert_eq!(parsed.five_hour.used_percentage, Some(91));
        assert_eq!(parsed.seven_day.used_percentage, Some(12));
        assert_eq!(parsed.seven_day.resets_at, Some(2000));
        assert_eq!(parsed.spend_limit.used_percentage, Some(40));
    }

    #[test]
    fn ignores_readings_until_the_new_credential_settles() {
        let mut state = State::default();
        assert!(!is_settling(&state, 1000));
        state.switched_at = Some(1000);
        assert!(is_settling(&state, 1000 + SWITCH_GRACE - 1));
        assert!(!is_settling(&state, 1000 + SWITCH_GRACE));
    }
}
