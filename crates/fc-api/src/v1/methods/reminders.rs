//! Study reminders (v1.0).

use crate::v1::common::Empty;
use crate::v1::reminders::*;

methods! {
    /// Reminder settings, what waits today and when the next reminder comes.
    RemindersStatus = "reminders.status", Query, "reminders", "reminders:read", Public, undo: false, ReminderStatusRequest => ReminderStatus;
    /// Publishes `reminder_due` now with today's counts (tries the notification).
    RemindersTest = "reminders.test", Command, "reminders", "reminders:write", Public, undo: false, Empty => ReminderCounts;
}
