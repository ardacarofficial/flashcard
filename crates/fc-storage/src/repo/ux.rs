//! First-run and reminder bookkeeping (v1.0, migration `1000_ux_state`): key → JSON value,
//! written straight (no oplog: it is not user content and is never undone).

use super::*;

/// Key: the first-run guide was finished or skipped (`true`).
pub const UX_ONBOARDING_DONE: &str = "onboarding.done";
/// Key: local day index of the last study reminder shown.
pub const UX_REMINDER_LAST_DAY: &str = "reminders.last_day";

/// `ux_state` reads.
pub trait UxStateRead {
    /// The value of `key`, if set.
    fn ux_state(&self, key: &str) -> StorageResult<Option<Value>>;
}

/// `ux_state` writes.
pub trait UxStateWrite: UxStateRead {
    /// Sets `key` (insert or replace).
    fn set_ux_state(&mut self, key: &str, value: &Value, now: Timestamp) -> StorageResult<()>;
}
