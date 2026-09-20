//! Small helpers that are not tied to a domain model.

use anyhow::{Context, Result, bail};
use std::io::{self, IsTerminal, Write};

/// Require an explicit acknowledgement before an irreversible change.
///
/// Statusline auto-switch and scripted use pass `yes`; interactive shell use
/// gets a prompt so that a typo does not silently replace a credential or
/// delete the saved copy of one.
pub(crate) fn confirm(question: &str, yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        bail!("refusing to continue without confirmation on non-interactive input; pass --yes");
    }

    eprint!("{question} [y/N] ");
    io::stderr().flush().ok();
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("failed to read confirmation")?;
    match answer.trim() {
        "y" | "Y" | "yes" | "YES" => Ok(()),
        _ => bail!("cancelled"),
    }
}

pub(crate) fn display_pct(value: Option<u8>) -> String {
    value.map_or_else(|| "-".to_string(), |value| format!("{value}%"))
}

/// Render a duration in seconds as a short statusline friendly string.
pub(crate) fn humanize(seconds: i64) -> String {
    // A reset in the past reads as elapsed rather than as a negative duration.
    let Ok(seconds) = u64::try_from(seconds) else {
        return "0s".to_string();
    };
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds.div_ceil(60);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    format!("{}h{}m", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_remaining_time() {
        assert_eq!(humanize(-5), "0s");
        assert_eq!(humanize(45), "45s");
        assert_eq!(humanize(61), "2m");
        assert_eq!(humanize(7800), "2h10m");
    }
}
