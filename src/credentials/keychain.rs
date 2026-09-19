//! macOS Keychain backend.
//!
//! This is the only module that talks to the `security` command. Keeping the
//! boundary narrow makes it clear where secret material can enter process memory
//! and keeps persistence code from accidentally writing credentials to disk.

use crate::context::AppContext;
use crate::domain::AccountName;
use anyhow::{Context, Result, anyhow, bail};
use std::process::Command;
use std::sync::OnceLock;

const ACTIVE_SERVICE: &str = "Claude Code-credentials";
const ACCOUNT_SERVICE: &str = "claude-quota-router";

pub(crate) fn describe(_ctx: &AppContext) -> String {
    format!("macOS Keychain, services {ACTIVE_SERVICE} and {ACCOUNT_SERVICE}")
}

/// Read the credential Claude Code currently uses.
///
/// The returned string is secret material. Callers should pass it directly to
/// another credential operation or to a short-lived in-memory comparison.
pub(crate) fn read_active(_ctx: &AppContext) -> Result<String> {
    read(ACTIVE_SERVICE, active_account()?)
}

/// Replace the active Claude Code credential with a previously saved account.
pub(crate) fn write_active(_ctx: &AppContext, credential: &str) -> Result<()> {
    upsert(ACTIVE_SERVICE, active_account()?, credential)
}

pub(crate) fn read_saved(_ctx: &AppContext, name: &AccountName) -> Result<String> {
    read(ACCOUNT_SERVICE, &stored_account(name))
}

/// Store an account credential without writing it to the filesystem.
pub(crate) fn write_saved(_ctx: &AppContext, name: &AccountName, credential: &str) -> Result<()> {
    upsert(ACCOUNT_SERVICE, &stored_account(name), credential)
}

pub(crate) fn delete_saved(_ctx: &AppContext, name: &AccountName) -> Result<()> {
    delete(ACCOUNT_SERVICE, &stored_account(name))
}

/// The Keychain account name Claude Code files its credential under.
///
/// It does not change while the process runs, and resolving it costs a
/// `security` invocation, so it is resolved once.
fn active_account() -> Result<&'static str> {
    static ACCOUNT: OnceLock<Option<String>> = OnceLock::new();
    ACCOUNT
        .get_or_init(|| detect_active_account().ok())
        .as_deref()
        .ok_or_else(|| {
            anyhow!("Claude Code credential not found in Keychain; log in with claude first")
        })
}

fn detect_active_account() -> Result<String> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", ACTIVE_SERVICE])
        .output()
        .context("failed to run security to detect active Claude Code account")?;
    if !output.status.success() {
        bail!("Claude Code credential not found in Keychain; log in with claude first");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_keychain_account(&text).ok_or_else(|| anyhow!("failed to parse active Keychain account"))
}

fn read(service: &str, account: &str) -> Result<String> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .with_context(|| format!("failed to run security for service {service}"))?;
    if !output.status.success() {
        bail!("security find-generic-password failed for service {service} account {account}");
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end_matches('\n')
        .to_string())
}

fn upsert(service: &str, account: &str, password: &str) -> Result<()> {
    let output = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            service,
            "-a",
            account,
            "-w",
        ])
        .arg(password)
        .output()
        .with_context(|| format!("failed to run security for service {service}"))?;
    if !output.status.success() {
        bail!(
            "security add-generic-password failed for service {service} account {account}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn delete(service: &str, account: &str) -> Result<()> {
    let output = Command::new("security")
        .args(["delete-generic-password", "-s", service, "-a", account])
        .output()
        .with_context(|| format!("failed to run security for service {service}"))?;
    if !output.status.success() {
        bail!("security delete-generic-password failed");
    }
    Ok(())
}

fn stored_account(name: &AccountName) -> String {
    format!("account:{name}")
}

fn parse_keychain_account(output: &str) -> Option<String> {
    for line in output.lines() {
        if !line.contains("\"acct\"") {
            continue;
        }
        let (_, value) = line.split_once("=\"")?;
        return Some(value.trim_end_matches('"').to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keychain_account() {
        let text = r#"
keychain: "/Users/me/Library/Keychains/login.keychain-db"
class: "genp"
attributes:
    "acct"<blob>="me@example.com"
    "svce"<blob>="Claude Code-credentials"
"#;
        assert_eq!(
            parse_keychain_account(text).as_deref(),
            Some("me@example.com")
        );
    }
}
