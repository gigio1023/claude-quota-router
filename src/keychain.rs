//! macOS Keychain adapter.
//!
//! This is the only module that talks to the `security` command. Keeping the
//! boundary narrow makes it clear where secret material can enter process memory
//! and keeps persistence code from accidentally writing credentials to disk. A
//! port to another platform replaces this module's body rather than its callers.

use crate::domain::AccountName;
use anyhow::{Context, Result, anyhow, bail};
use std::process::Command;

const ACTIVE_SERVICE: &str = "Claude Code-credentials";
const ACCOUNT_SERVICE: &str = "claude-quota-router";

/// Read the credential Claude Code currently uses.
///
/// The returned string is secret material. Callers should pass it directly to
/// another Keychain operation or to a short-lived in-memory comparison.
pub(crate) fn read_active(account: &str) -> Result<String> {
    read(ACTIVE_SERVICE, account)
}

/// Replace the active Claude Code credential with a previously saved account.
pub(crate) fn upsert_active(account: &str, credential: &str) -> Result<()> {
    upsert(ACTIVE_SERVICE, account, credential)
}

/// Read a saved account credential from the app-owned Keychain service.
pub(crate) fn read_account(name: &AccountName) -> Result<String> {
    read(ACCOUNT_SERVICE, &stored_account(name))
}

/// Store an account credential without writing it to the filesystem.
pub(crate) fn upsert_account(name: &AccountName, credential: &str) -> Result<()> {
    upsert(ACCOUNT_SERVICE, &stored_account(name), credential)
}

pub(crate) fn delete_account(name: &AccountName) -> Result<()> {
    delete(ACCOUNT_SERVICE, &stored_account(name))
}

/// The Keychain account name under which Claude Code stores its credential.
pub(crate) fn detect_active_account() -> Result<String> {
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
