//! Shared exponential-backoff constants/logic for retrying a failed location
//! resolution attempt. Both [`crate::portal_location`] (portal/GeoClue automatic
//! location) and [`crate::ip_geolocation`] (IP-based location) independently needed
//! the exact same shape — never a tight retry loop, self-recovering without the user
//! needing to manually toggle the location mode off and on — so it lives here once
//! instead of being copy-pasted between them.

use std::time::Duration;

/// The initial backoff delay after the first failed resolution attempt.
pub const INITIAL_BACKOFF: Duration = Duration::from_secs(30);
/// The backoff ceiling — never waited longer than this between retries.
pub const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// The next backoff delay after a failed attempt — doubles, capped at [`MAX_BACKOFF`].
/// The call site resets to [`INITIAL_BACKOFF`] after every successful resolution.
pub fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_BACKOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_backoff_doubles_and_caps_at_five_minutes() {
        let mut backoff = INITIAL_BACKOFF;
        assert_eq!(backoff, Duration::from_secs(30));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(60));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(120));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, Duration::from_secs(240));

        backoff = next_backoff(backoff);
        assert_eq!(backoff, MAX_BACKOFF); // 480s would exceed the cap.

        backoff = next_backoff(backoff);
        assert_eq!(backoff, MAX_BACKOFF);
    }
}
