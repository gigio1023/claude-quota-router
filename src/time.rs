//! Time helpers.
//!
//! Timestamps are Unix seconds as `i64`, matching the reset times Claude Code
//! reports. Keeping one signed type across state, cache, and arithmetic removes
//! the casts that a mixed signed and unsigned model needs.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch.
///
/// A clock set before 1970, or past year 292277026596, saturates rather than
/// wrapping. Neither is a state this tool can act on.
pub(crate) fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX))
}
