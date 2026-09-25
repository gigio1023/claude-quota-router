//! Running the user's existing statusline command.
//!
//! Claude Code accepts a `statusLine.command` as a shell command string, so the
//! wrapper has to hand it to the same kind of shell rather than reinterpreting
//! it. Which shell that is, and how a path is quoted back into the setting, are
//! the two things that differ per platform.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Run a statusline command, feeding it the JSON Claude Code sent.
///
/// Returns `None` when the command is missing, fails, or prints nothing, so a
/// broken inner command cannot take the router's own output down with it.
pub(crate) fn run_statusline(command: &str, input: &str) -> Option<String> {
    let mut child = shell_command(command)
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

/// Claude Code spawns `statusLine.command` through Node with `shell: true`,
/// which on this platform is `/bin/sh -c`, and hands it the environment the
/// session already has. Matching that exactly matters: a login shell would
/// re-read the profile files and could give the user's command a different
/// `PATH` than the one it runs under today.
#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("/bin/sh");
    shell.arg("-c").arg(command);
    shell
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("cmd");
    shell.arg("/C").arg(command);
    shell
}

/// Quote this binary's path for the `statusLine.command` string.
#[cfg(not(windows))]
pub(crate) fn quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Quote this binary's path for the `statusLine.command` string.
///
/// A Windows path cannot contain a double quote, so wrapping is sufficient and
/// no escaping is needed.
#[cfg(windows)]
pub(crate) fn quote(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy())
}
