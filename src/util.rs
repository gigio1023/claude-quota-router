//! Formatting helpers that are not tied to a domain model.

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
