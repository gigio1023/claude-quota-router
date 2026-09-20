//! Claude Code account inspection.
//!
//! `claude auth status` is the CLI's own view of the account that the active
//! credential belongs to, so it is the source for both the account's email and
//! its plan. The credential JSON is a fallback for tests and offline setup flows
//! where the command is unavailable.

use crate::domain::AccountKind;
use serde_json::Value;
use std::process::Command;

#[derive(Clone, Debug, Default)]
pub(crate) struct AuthStatus {
    pub(crate) email: Option<String>,
    pub(crate) kind: Option<AccountKind>,
}

/// Ask Claude Code who is logged in.
///
/// One invocation answers both questions `setup` asks, which keeps the command
/// from being spawned twice for a single save.
pub(crate) fn auth_status() -> Option<AuthStatus> {
    let output = Command::new("claude")
        .args(["auth", "status"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_status(&output.stdout)
}

fn parse_status(payload: &[u8]) -> Option<AuthStatus> {
    let value: Value = serde_json::from_slice(payload).ok()?;
    Some(AuthStatus {
        email: value
            .get("email")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|email| !email.is_empty()),
        kind: value
            .get("subscriptionType")
            .and_then(Value::as_str)
            .map(AccountKind::from_subscription),
    })
}

/// Whether a credential actually carries an OAuth token.
///
/// `claude auth logout` leaves the credential record in place with its metadata
/// and empty token strings, and Claude Code reports that state as logged out.
/// Treating it as a credential would let the router back it up over a saved
/// account, or activate it and log the user out.
pub(crate) fn has_oauth_token(credential: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(credential) else {
        return false;
    };
    value
        .pointer("/claudeAiOauth/accessToken")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty())
}

pub(crate) fn detect_account_kind_from_credential(credential: &str) -> Option<AccountKind> {
    let value: Value = serde_json::from_str(credential).ok()?;
    value
        .pointer("/claudeAiOauth/subscriptionType")
        .and_then(Value::as_str)
        .map(AccountKind::from_subscription)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_credential_whose_tokens_were_cleared() {
        assert!(has_oauth_token(r#"{"claudeAiOauth":{"accessToken":"abc"}}"#));
        // What `claude auth logout` leaves behind.
        assert!(!has_oauth_token(
            r#"{"claudeAiOauth":{"accessToken":"","subscriptionType":"max"}}"#
        ));
        assert!(!has_oauth_token("{}"));
        assert!(!has_oauth_token("not json"));
    }

    #[test]
    fn reads_email_and_plan_from_auth_status() {
        let payload = br#"{
            "loggedIn": true,
            "authMethod": "claude.ai",
            "email": "someone@example.com",
            "orgName": "Example",
            "subscriptionType": "max"
        }"#;
        let status = parse_status(payload).unwrap();
        assert_eq!(status.email.as_deref(), Some("someone@example.com"));
        assert_eq!(status.kind, Some(AccountKind::Personal));
    }
}
