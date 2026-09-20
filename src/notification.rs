//! Desktop notifications and their per quota event marker.
//!
//! Statusline commands run frequently, so notifications must be idempotent per
//! quota event. The marker file stores only event keys, never account data.

use crate::context::AppContext;
use crate::storage;
use anyhow::{Context, Result};
use std::fs;

pub(crate) fn notify_once(ctx: &AppContext, key: &str, title: &str, message: &str) -> Result<()> {
    ctx.ensure_app_dir()?;
    let mut notified = storage::load_notified(ctx)?;
    if notified.contains(key) {
        return Ok(());
    }

    show(title, message);
    notified.insert(key.to_string());
    storage::save_notified(ctx, &notified)
}

pub(crate) fn clear_notified(ctx: &AppContext) -> Result<()> {
    if ctx.notified_path().exists() {
        fs::remove_file(ctx.notified_path()).context("failed to clear notification state")?;
    }
    Ok(())
}

/// Raise a desktop notification, best effort.
///
/// The statusline already carries the same message, so a missing or failing
/// notifier is never an error.
#[cfg(target_os = "macos")]
fn show(title: &str, message: &str) {
    use std::process::{Command, Stdio};
    let script = format!(
        "display notification \"{}\" with title \"{}\"",
        escape_applescript(message),
        escape_applescript(title)
    );
    Command::new("osascript")
        .args(["-e", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

#[cfg(target_os = "macos")]
fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `notify-send` ships with libnotify and is present on the common desktops.
#[cfg(target_os = "linux")]
fn show(title: &str, message: &str) {
    use std::process::{Command, Stdio};
    Command::new("notify-send")
        .args([title, message])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
}

/// Windows has no notifier that is present by default and safe to shell out to,
/// so the statusline text is the only channel there.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn show(_title: &str, _message: &str) {}
