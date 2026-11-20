//! App lock rules (docs/02 §29). The lock is an **access** lock for the desktop window, not
//! database encryption (docs/08 K4). The secret is never stored: the app keeps only its
//! Argon2id hash (in the OS keychain where it persists). This module holds the pure rules:
//! what a PIN or password may look like, when an idle app locks itself, and how long to wait
//! after wrong attempts.

use serde::{Deserialize, Serialize};

/// What the user types to unlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum LockKind {
    /// Digits only, [`PIN_MIN`]–[`PIN_MAX`] long.
    Pin,
    /// Any text, [`PASSWORD_MIN`]–[`PASSWORD_MAX`] characters.
    Password,
}

/// Shortest PIN.
pub const PIN_MIN: usize = 4;
/// Longest PIN.
pub const PIN_MAX: usize = 12;
/// Shortest password, in characters.
pub const PASSWORD_MIN: usize = 6;
/// Longest password, in characters.
pub const PASSWORD_MAX: usize = 128;

/// Idle timeouts the settings offer, in minutes (`0` = never lock while open).
pub const IDLE_CHOICES: &[u32] = &[0, 1, 5, 15, 30, 60];

/// Default idle timeout, minutes.
pub const DEFAULT_IDLE_MINUTES: u32 = 15;

/// Wrong attempts allowed before waiting starts.
pub const FREE_ATTEMPTS: u32 = 5;

/// Longest wait between attempts, milliseconds.
pub const MAX_RETRY_DELAY_MS: i64 = 5 * 60 * 1000;

/// Why a new secret was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    /// Too short for its kind.
    TooShort,
    /// Too long for its kind.
    TooLong,
    /// A PIN with something other than digits 0–9.
    NotDigits,
    /// Only whitespace (password).
    Blank,
}

impl SecretError {
    /// Stable reason code (`details.reason` of the API error).
    pub fn code(self) -> &'static str {
        match self {
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
            Self::NotDigits => "not_digits",
            Self::Blank => "blank",
        }
    }
}

/// Checks the shape of a new PIN or password. (Unlocking compares hashes; any input is
/// accepted there.)
pub fn validate_secret(kind: LockKind, secret: &str) -> Result<(), SecretError> {
    let n = secret.chars().count();
    match kind {
        LockKind::Pin => {
            if !secret.chars().all(|c| c.is_ascii_digit()) {
                Err(SecretError::NotDigits)
            } else if n < PIN_MIN {
                Err(SecretError::TooShort)
            } else if n > PIN_MAX {
                Err(SecretError::TooLong)
            } else {
                Ok(())
            }
        }
        LockKind::Password => {
            if secret.trim().is_empty() {
                Err(SecretError::Blank)
            } else if n < PASSWORD_MIN {
                Err(SecretError::TooShort)
            } else if n > PASSWORD_MAX {
                Err(SecretError::TooLong)
            } else {
                Ok(())
            }
        }
    }
}

/// Whether `minutes` is one of [`IDLE_CHOICES`].
pub fn valid_idle_minutes(minutes: u32) -> bool {
    IDLE_CHOICES.contains(&minutes)
}

/// Whether an app last used at `last_activity_ms` has been idle long enough to lock at
/// `now_ms`. `idle_minutes == 0` never locks by itself.
pub fn idle_expired(last_activity_ms: i64, now_ms: i64, idle_minutes: u32) -> bool {
    idle_minutes > 0 && now_ms.saturating_sub(last_activity_ms) >= i64::from(idle_minutes) * 60_000
}

/// How long to wait before the next attempt after `failures` wrong ones in a row: nothing
/// for the first [`FREE_ATTEMPTS`], then 1 s, 2 s, 4 s … up to [`MAX_RETRY_DELAY_MS`].
pub fn retry_delay_ms(failures: u32) -> i64 {
    if failures < FREE_ATTEMPTS {
        return 0;
    }
    let exp = (failures - FREE_ATTEMPTS).min(20);
    (1000_i64 << exp).min(MAX_RETRY_DELAY_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_are_digits_of_the_right_length() {
        assert_eq!(validate_secret(LockKind::Pin, "1234"), Ok(()));
        assert_eq!(
            validate_secret(LockKind::Pin, "123"),
            Err(SecretError::TooShort)
        );
        assert_eq!(
            validate_secret(LockKind::Pin, "1234567890123"),
            Err(SecretError::TooLong)
        );
        assert_eq!(
            validate_secret(LockKind::Pin, "12a4"),
            Err(SecretError::NotDigits)
        );
        assert_eq!(
            validate_secret(LockKind::Pin, "١٢٣٤"),
            Err(SecretError::NotDigits)
        );
    }

    #[test]
    fn passwords_need_six_real_characters() {
        assert_eq!(validate_secret(LockKind::Password, "şifre1"), Ok(()));
        assert_eq!(
            validate_secret(LockKind::Password, "kısa"),
            Err(SecretError::TooShort)
        );
        assert_eq!(
            validate_secret(LockKind::Password, "       "),
            Err(SecretError::Blank)
        );
        assert_eq!(
            validate_secret(LockKind::Password, &"x".repeat(129)),
            Err(SecretError::TooLong)
        );
    }

    #[test]
    fn idle_lock_respects_the_timeout_and_never() {
        assert!(!idle_expired(0, 4 * 60_000, 5));
        assert!(idle_expired(0, 5 * 60_000, 5));
        assert!(!idle_expired(0, i64::MAX, 0));
        assert!(valid_idle_minutes(15) && !valid_idle_minutes(7));
    }

    #[test]
    fn retries_back_off_after_free_attempts() {
        assert_eq!(retry_delay_ms(0), 0);
        assert_eq!(retry_delay_ms(4), 0);
        assert_eq!(retry_delay_ms(5), 1000);
        assert_eq!(retry_delay_ms(6), 2000);
        assert_eq!(retry_delay_ms(40), MAX_RETRY_DELAY_MS);
        assert_eq!(retry_delay_ms(u32::MAX), MAX_RETRY_DELAY_MS);
    }
}
