//! Domain types and small invariants.
//!
//! Types in this module describe the durable state model. The module keeps
//! validation close to the values it protects, which makes command handlers
//! smaller and prevents raw strings from crossing security-sensitive code paths.

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// How long a fully used window with no reset timestamp keeps an account out of
/// the rotation. Claude Code always sends `resets_at` next to `used_percentage`,
/// so this only bounds the damage when a window arrives without one.
const UNKNOWN_RESET_GRACE: i64 = 3600;

/// Device names Windows refuses to use as a file name.
const WINDOWS_DEVICE_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct AccountName(String);

impl AccountName {
    /// Validate and normalize a user supplied account name.
    ///
    /// The default name is the account's email address, which is what
    /// distinguishes one login from another at a glance. The character set is
    /// therefore wide enough for an address and still narrow enough to keep the
    /// derived Keychain account names free of shell-looking or path-looking
    /// characters. Names are compared after lowercasing, as addresses are.
    pub(crate) fn parse(input: &str) -> Result<Self> {
        let value = input.trim().to_ascii_lowercase();
        if value.is_empty() || value.len() > 128 {
            bail!("account name must be 1-128 characters");
        }
        if value.starts_with('-') || value.ends_with('-') {
            bail!("account name cannot start or end with '-'");
        }
        if !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '+' | '@'))
        {
            bail!("account name must use letters, digits, and - . _ + @ only");
        }
        if !value.chars().any(|ch| ch.is_ascii_alphanumeric()) {
            bail!("account name must contain a letter or a digit");
        }
        // A saved account becomes a file wherever there is no Keychain, and
        // Windows reserves these stems even with an extension appended.
        let stem = value.split('.').next().unwrap_or(value.as_str());
        if WINDOWS_DEVICE_NAMES.contains(&stem) {
            bail!("account name cannot be the reserved device name {stem}");
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which Claude plan an account is on.
///
/// The kind is metadata plus a tie breaker. Routing order comes from the
/// configured priority list, and the kind only decides the default order for
/// accounts the user has not ranked explicitly.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AccountKind {
    Personal,
    Team,
    Enterprise,
    Other,
}

impl AccountKind {
    pub(crate) fn from_subscription(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "max" | "pro" | "free" => Self::Personal,
            "team" => Self::Team,
            "enterprise" => Self::Enterprise,
            _ => Self::Other,
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Team => "team",
            Self::Enterprise => "enterprise",
            Self::Other => "other",
        }
    }

    /// Default routing rank used when the priority list does not name the account.
    pub(crate) const fn rank(self) -> u8 {
        match self {
            Self::Personal => 0,
            Self::Team => 1,
            Self::Enterprise => 2,
            Self::Other => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RoutingMode {
    Manual,
    Auto,
}

impl RoutingMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) alert_at: u8,
    pub(crate) mode: RoutingMode,
    /// Account names in the order they should be used. Names that are not saved
    /// are ignored, and saved accounts missing from the list are appended in
    /// kind order.
    #[serde(default)]
    pub(crate) priority: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            alert_at: 95,
            mode: RoutingMode::Manual,
            priority: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AccountEntry {
    pub(crate) kind: AccountKind,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct State {
    pub(crate) current_account: Option<String>,
    pub(crate) previous_account: Option<String>,
    pub(crate) accounts: BTreeMap<String, AccountEntry>,
    /// When the active credential was last replaced.
    ///
    /// A running Claude Code session keeps reporting the quota of the account it
    /// started with, so quota readings taken right after a switch would be filed
    /// under the wrong account without this timestamp.
    #[serde(default)]
    pub(crate) switched_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct LimitWindow {
    pub(crate) used_percentage: Option<u8>,
    pub(crate) resets_at: Option<i64>,
}

impl LimitWindow {
    fn is_full(&self) -> bool {
        self.used_percentage.is_some_and(|pct| pct >= 100)
    }

    /// The window still blocks work: it is full and has not reset yet.
    fn blocks(&self, now: i64, detected_at: i64) -> bool {
        if !self.is_full() {
            return false;
        }
        match self.resets_at {
            Some(reset) => reset > now,
            None => now.saturating_sub(detected_at) < UNKNOWN_RESET_GRACE,
        }
    }

    /// The window was full when it was observed and its reset time has passed.
    fn recovered(&self, now: i64) -> bool {
        self.is_full() && self.resets_at.is_some_and(|reset| reset <= now)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct RateLimitSnapshot {
    pub(crate) detected_at: i64,
    #[serde(default)]
    pub(crate) five_hour: LimitWindow,
    #[serde(default)]
    pub(crate) seven_day: LimitWindow,
    /// Spend based window reported for accounts billed through the gateway.
    #[serde(default)]
    pub(crate) spend_limit: LimitWindow,
}

impl RateLimitSnapshot {
    pub(crate) fn windows(&self) -> [(&'static str, &LimitWindow); 3] {
        [
            ("5h", &self.five_hour),
            ("7d", &self.seven_day),
            ("spend", &self.spend_limit),
        ]
    }

    /// The window closest to its limit, which is the one worth showing.
    pub(crate) fn peak_usage(&self) -> Option<(&'static str, u8)> {
        self.windows()
            .into_iter()
            .filter_map(|(label, window)| window.used_percentage.map(|pct| (label, pct)))
            .max_by_key(|(_, pct)| *pct)
    }

    /// The earliest reset among the windows that are currently blocking.
    pub(crate) fn blocking_reset(&self, now: i64) -> Option<(&'static str, i64)> {
        self.windows()
            .into_iter()
            .filter(|(_, window)| window.blocks(now, self.detected_at))
            .filter_map(|(label, window)| window.resets_at.map(|reset| (label, reset)))
            .min_by_key(|(_, reset)| *reset)
    }

    pub(crate) fn is_blocked(&self, now: i64) -> bool {
        self.windows()
            .into_iter()
            .any(|(_, window)| window.blocks(now, self.detected_at))
    }

    pub(crate) fn is_recovered(&self, now: i64) -> bool {
        self.windows()
            .into_iter()
            .any(|(_, window)| window.recovered(now))
    }
}

/// Quota readings kept per account.
///
/// Only the account Claude Code is logged into reports its own quota, so the
/// last reading for every other account is what the router has to reason with.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct RateLimitCache {
    #[serde(default)]
    pub(crate) accounts: BTreeMap<String, RateLimitSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(pct: Option<u8>, reset: Option<i64>) -> LimitWindow {
        LimitWindow {
            used_percentage: pct,
            resets_at: reset,
        }
    }

    fn snapshot(five: LimitWindow, seven: LimitWindow) -> RateLimitSnapshot {
        RateLimitSnapshot {
            detected_at: 1000,
            five_hour: five,
            seven_day: seven,
            spend_limit: LimitWindow::default(),
        }
    }

    #[test]
    fn accepts_email_addresses_and_lowercases_them() {
        assert_eq!(
            AccountName::parse("Relilau00+work@Gmail.com").unwrap().as_str(),
            "relilau00+work@gmail.com"
        );
        assert!(AccountName::parse("team-main").is_ok());
        assert!(AccountName::parse("-team").is_err());
        assert!(AccountName::parse("team main").is_err());
        assert!(AccountName::parse("...").is_err());
        assert!(AccountName::parse("nul").is_err());
    }

    #[test]
    fn maps_personal_subscriptions_to_personal_kind() {
        assert_eq!(AccountKind::from_subscription("max"), AccountKind::Personal);
        assert_eq!(AccountKind::from_subscription("Pro"), AccountKind::Personal);
        assert_eq!(AccountKind::from_subscription("team"), AccountKind::Team);
        assert_eq!(
            AccountKind::from_subscription("enterprise"),
            AccountKind::Enterprise
        );
        assert_eq!(AccountKind::from_subscription("unknown"), AccountKind::Other);
    }

    #[test]
    fn blocks_until_the_full_window_resets() {
        let full = snapshot(window(Some(100), Some(2000)), window(Some(40), Some(9000)));
        assert!(full.is_blocked(1500));
        assert!(!full.is_blocked(2500));
        assert!(!full.is_recovered(1500));
        assert!(full.is_recovered(2500));
        assert_eq!(full.blocking_reset(1500), Some(("5h", 2000)));
        assert_eq!(full.peak_usage(), Some(("5h", 100)));
    }

    #[test]
    fn treats_a_full_window_without_reset_as_blocked_for_one_hour() {
        let unknown = snapshot(window(Some(100), None), LimitWindow::default());
        assert!(unknown.is_blocked(2000));
        assert!(!unknown.is_blocked(1000 + UNKNOWN_RESET_GRACE));
        assert_eq!(unknown.blocking_reset(2000), None);
    }
}
