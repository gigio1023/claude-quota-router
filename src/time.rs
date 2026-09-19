//! Time helpers.
//!
//! The current implementation stores Unix timestamps because Claude Code
//! statusline input uses epoch reset times. Keeping conversion here avoids
//! sprinkling clock access through rendering and persistence code.

use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Same instant as [`now_epoch`], typed for arithmetic against reset times.
pub(crate) fn now_epoch_i64() -> i64 {
    now_epoch() as i64
}
