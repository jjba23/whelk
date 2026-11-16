// whelk -- a Wayland shell written in Rust, GTK4 and GNU Guile Scheme
//
// SPDX-License-Identifier: MIT
// Copyright © Josep Bigorra <jjbigorra@gmail.com>

//! The lock screen's throttling policy, ported from `util/lockpolicy.c`.
//!
//! The policy is deliberately outside the UI: a mistyped password has to cost
//! the same wait whether the attempt came from the button, the entry's
//! `activate` signal or a restarted shell.

/// The longest a wrong password can make the owner wait.
pub const MAX_DELAY_MS: u64 = 30_000;

/// The wait before another attempt after `failures` consecutive refusals.
///
/// One second after the first, doubling to a 30-second cap.  `failures == 0`
/// is the first attempt and costs nothing.  The exponent is clamped before the
/// shift so the arithmetic cannot overflow.
pub fn retry_delay_ms(failures: u32) -> u64 {
    if failures == 0 {
        return 0;
    }

    let exponent = (failures - 1).min(5);

    (1_000u64 << exponent).min(MAX_DELAY_MS)
}

/// Whether a password may be submitted now.
///
/// The unlock button can deliver an activation while the entry is disabled, so
/// the authoritative guard lives here instead of in the widget.
pub fn submission_allowed(authenticating: bool, retrying: bool, entry_sensitive: bool) -> bool {
    !authenticating && !retrying && entry_sensitive
}

#[cfg(test)]
mod tests {
    use super::{MAX_DELAY_MS, retry_delay_ms, submission_allowed};

    #[test]
    fn the_first_attempt_is_free() {
        assert_eq!(retry_delay_ms(0), 0);
    }

    #[test]
    fn each_failure_doubles_up_to_the_cap() {
        assert_eq!(retry_delay_ms(1), 1_000);
        assert_eq!(retry_delay_ms(2), 2_000);
        assert_eq!(retry_delay_ms(3), 4_000);
        assert_eq!(retry_delay_ms(4), 8_000);
        assert_eq!(retry_delay_ms(5), 16_000);
        assert_eq!(retry_delay_ms(6), MAX_DELAY_MS);
        assert_eq!(retry_delay_ms(u32::MAX), MAX_DELAY_MS);
    }

    #[test]
    fn a_submission_needs_an_idle_sensitive_entry() {
        assert!(submission_allowed(false, false, true));
        assert!(!submission_allowed(true, false, true));
        assert!(!submission_allowed(false, true, true));
        assert!(!submission_allowed(false, false, false));
    }
}
