//! Study reminders and the tray (v1.0, docs/02 §19): an OS notification ("you have 40 cards
//! today") at a chosen time on chosen weekdays, none on days already studied. The settings are
//! ordinary settings (`reminders.enabled`, `reminders.minute_of_day`, `reminders.weekdays`,
//! `reminders.keep_in_tray`); the desktop app checks once a minute and publishes
//! `reminder_due`, which the window turns into a notification.

use fc_core::Timestamp;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `reminders.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ReminderStatusRequest {
    /// Local UTC offset in minutes (default: the app's).
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// Response of `reminders.status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ReminderStatus {
    /// Reminders are on.
    pub enabled: bool,
    /// Local time, minutes after midnight.
    pub minute_of_day: u32,
    /// Weekday bit mask (Monday = bit 0 … Sunday = bit 6).
    pub weekdays: u32,
    /// Closing the window keeps the app running in the tray.
    pub keep_in_tray: bool,
    /// What waits today.
    pub counts: ReminderCounts,
    /// Something was reviewed today (no reminder today).
    pub studied_today: bool,
    /// When the next reminder can come (`null` = off / no day chosen).
    pub next_at: Option<Timestamp>,
}

/// Cards waiting today, over the whole collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct ReminderCounts {
    /// Reviews and (re)learning cards due today.
    pub due: u32,
    /// New cards available today.
    pub new: u32,
}
