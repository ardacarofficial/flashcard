//! Daily study reminders (docs/02 §19): "you have 40 cards today" at a chosen time on chosen
//! weekdays, never when today's study is already done. Pure: the app passes `now`, the local
//! UTC offset, what it knows about today and the last day it reminded.

use crate::time::MILLIS_PER_DAY;

/// Minutes in a day.
pub const MINUTES_PER_DAY: u32 = 24 * 60;

/// Every weekday (Monday = bit 0 … Sunday = bit 6).
pub const ALL_DAYS: u32 = 0b111_1111;

/// Default reminder time, minutes after local midnight (19:00).
pub const DEFAULT_MINUTE_OF_DAY: u32 = 19 * 60;

/// How late a missed reminder may still be shown (the app was closed or asleep at the
/// reminder time), minutes. Later than that the day is skipped.
pub const CATCH_UP_MINUTES: u32 = 3 * 60;

/// When to remind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReminderSchedule {
    /// Reminders are on.
    pub enabled: bool,
    /// Local time, minutes after midnight (0–1439).
    pub minute_of_day: u32,
    /// Weekday bit mask (Monday = bit 0 … Sunday = bit 6).
    pub weekdays: u32,
}

/// What the reminder check decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderDecision {
    /// Show the notification now and remember `local_day` as reminded.
    Notify {
        /// Local day index (days since 1970-01-01 in local time).
        local_day: i64,
    },
    /// Nothing to do now.
    Wait,
    /// Today needs no reminder (already studied, nothing due, already reminded, not a chosen
    /// weekday, or reminders are off).
    SkipToday,
}

/// Local day index of `now_ms` in a zone `tz_offset_mins` ahead of UTC.
pub fn local_day(now_ms: i64, tz_offset_mins: i32) -> i64 {
    local_ms(now_ms, tz_offset_mins).div_euclid(MILLIS_PER_DAY)
}

fn local_ms(now_ms: i64, tz_offset_mins: i32) -> i64 {
    now_ms.saturating_add(i64::from(tz_offset_mins) * 60_000)
}

/// Weekday of a local day index: Monday = 0 … Sunday = 6 (1970-01-01 was a Thursday).
pub fn weekday(local_day: i64) -> u32 {
    // rem_euclid(7) is 0..=6.
    u32::try_from((local_day + 3).rem_euclid(7)).unwrap_or(0)
}

/// Decides whether to remind at `now_ms`.
///
/// * `last_reminded_day`: the local day of the last notification (at most one a day).
/// * `studied_today`: the user already reviewed something today → no reminder.
/// * `cards_today`: what is waiting today (due + learning + new); `0` → no reminder.
pub fn decide(
    schedule: ReminderSchedule,
    now_ms: i64,
    tz_offset_mins: i32,
    last_reminded_day: Option<i64>,
    studied_today: bool,
    cards_today: u32,
) -> ReminderDecision {
    let day = local_day(now_ms, tz_offset_mins);
    if !schedule.enabled
        || schedule.weekdays & (1 << weekday(day)) == 0
        || last_reminded_day == Some(day)
    {
        return ReminderDecision::SkipToday;
    }
    let minute =
        u32::try_from(local_ms(now_ms, tz_offset_mins).rem_euclid(MILLIS_PER_DAY) / 60_000)
            .unwrap_or(0);
    let at = schedule.minute_of_day.min(MINUTES_PER_DAY - 1);
    if minute < at {
        return ReminderDecision::Wait;
    }
    if minute > at + CATCH_UP_MINUTES || studied_today || cards_today == 0 {
        return ReminderDecision::SkipToday;
    }
    ReminderDecision::Notify { local_day: day }
}

/// The next instant (UTC ms) at or after `now_ms` when a reminder could fire, if any: today's
/// time if it has not passed and today was not handled, else the next chosen weekday.
pub fn next_at(
    schedule: ReminderSchedule,
    now_ms: i64,
    tz_offset_mins: i32,
    last_reminded_day: Option<i64>,
) -> Option<i64> {
    if !schedule.enabled || schedule.weekdays & ALL_DAYS == 0 {
        return None;
    }
    let today = local_day(now_ms, tz_offset_mins);
    let at = i64::from(schedule.minute_of_day.min(MINUTES_PER_DAY - 1)) * 60_000;
    let offset = i64::from(tz_offset_mins) * 60_000;
    (0..=7).find_map(|d| {
        let day = today + d;
        let fire = day * MILLIS_PER_DAY + at - offset;
        let chosen = schedule.weekdays & (1 << weekday(day)) != 0;
        (chosen && fire >= now_ms && last_reminded_day != Some(day)).then_some(fire)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const IST: i32 = 180;
    /// 2026-11-20, a Friday (local day index).
    const FRIDAY: i64 = 20_777;

    /// UTC ms of local `h:m` (Istanbul), `day_offset` days after the Friday.
    fn at(day_offset: i64, h: i64, m: i64) -> i64 {
        (FRIDAY + day_offset) * MILLIS_PER_DAY + (h * 60 + m) * 60_000 - i64::from(IST) * 60_000
    }

    fn every_day() -> ReminderSchedule {
        ReminderSchedule {
            enabled: true,
            minute_of_day: 19 * 60,
            weekdays: ALL_DAYS,
        }
    }

    #[test]
    fn weekdays_count_from_monday() {
        assert_eq!(weekday(0), 3); // 1970-01-01, Thursday
        assert_eq!(weekday(local_day(at(0, 12, 0), IST)), 4); // Friday
        assert_eq!(weekday(local_day(at(3, 12, 0), IST)), 0); // Monday
    }

    #[test]
    fn fires_once_at_the_chosen_time() {
        let s = every_day();
        assert_eq!(
            decide(s, at(0, 18, 59), IST, None, false, 40),
            ReminderDecision::Wait
        );
        let ReminderDecision::Notify { local_day: d } =
            decide(s, at(0, 19, 0), IST, None, false, 40)
        else {
            panic!("should notify");
        };
        assert_eq!(
            decide(s, at(0, 19, 5), IST, Some(d), false, 40),
            ReminderDecision::SkipToday
        );
        // Next day: again.
        assert!(matches!(
            decide(s, at(1, 19, 1), IST, Some(d), false, 40),
            ReminderDecision::Notify { .. }
        ));
    }

    #[test]
    fn studied_today_or_nothing_due_means_no_reminder() {
        let s = every_day();
        assert_eq!(
            decide(s, at(0, 19, 0), IST, None, true, 40),
            ReminderDecision::SkipToday
        );
        assert_eq!(
            decide(s, at(0, 19, 0), IST, None, false, 0),
            ReminderDecision::SkipToday
        );
    }

    #[test]
    fn respects_weekdays_off_switch_and_catch_up_window() {
        let weekdays_only = ReminderSchedule {
            weekdays: 0b001_1111,
            ..every_day()
        };
        assert_eq!(
            decide(weekdays_only, at(1, 19, 0), IST, None, false, 5), // Saturday
            ReminderDecision::SkipToday
        );
        let off = ReminderSchedule {
            enabled: false,
            ..every_day()
        };
        assert_eq!(
            decide(off, at(0, 19, 0), IST, None, false, 5),
            ReminderDecision::SkipToday
        );
        // Opened at 21:30: still reminded; at 22:30 (3.5 h late) the day is skipped.
        assert!(matches!(
            decide(every_day(), at(0, 21, 30), IST, None, false, 5),
            ReminderDecision::Notify { .. }
        ));
        assert_eq!(
            decide(every_day(), at(0, 22, 30), IST, None, false, 5),
            ReminderDecision::SkipToday
        );
    }

    #[test]
    fn next_reminder_time() {
        let s = every_day();
        assert_eq!(next_at(s, at(0, 10, 0), IST, None), Some(at(0, 19, 0)));
        let today = local_day(at(0, 10, 0), IST);
        assert_eq!(
            next_at(s, at(0, 10, 0), IST, Some(today)),
            Some(at(1, 19, 0))
        );
        let mondays = ReminderSchedule { weekdays: 1, ..s };
        assert_eq!(
            next_at(mondays, at(0, 20, 0), IST, None),
            Some(at(3, 19, 0))
        );
        assert_eq!(
            next_at(
                ReminderSchedule { weekdays: 0, ..s },
                at(0, 1, 0),
                IST,
                None
            ),
            None
        );
    }
}
