//! The study day. A "day" ends at a local rollover hour (default 04:00), not at midnight, so a
//! late-night session still counts as the same day. The UTC offset is supplied by the caller
//! (the frontend knows the user's time zone; the core never reads it).

use fc_core::time::MILLIS_PER_DAY;
use fc_core::Timestamp;

/// Default local hour at which a new study day starts.
pub const DEFAULT_ROLLOVER_HOUR: u8 = 4;

/// Largest accepted UTC offset magnitude, minutes (UTC−14:00 … UTC+14:00).
pub const MAX_TZ_OFFSET_MINS: i32 = 14 * 60;

/// A user's study calendar: days start at local `rollover_hour:00` in a zone `tz_offset_mins`
/// minutes ahead of UTC (Istanbul: `180`). Out-of-range inputs are clamped (offset to ±14 h,
/// hour to 0–23). Day indexes are counted from the study day containing the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StudyDay {
    /// UTC offset, minutes.
    pub tz_offset_mins: i32,
    /// Local hour at which a new day starts.
    pub rollover_hour: u8,
}

impl Default for StudyDay {
    /// UTC, rollover at [`DEFAULT_ROLLOVER_HOUR`].
    fn default() -> Self {
        Self::new(0, DEFAULT_ROLLOVER_HOUR)
    }
}

impl StudyDay {
    /// A calendar for `tz_offset_mins` / `rollover_hour`.
    pub const fn new(tz_offset_mins: i32, rollover_hour: u8) -> Self {
        Self {
            tz_offset_mins,
            rollover_hour,
        }
    }

    /// Milliseconds from UTC midnight to the local day start (offset − rollover, negated).
    fn shift_ms(self) -> i64 {
        let offset = i64::from(
            self.tz_offset_mins
                .clamp(-MAX_TZ_OFFSET_MINS, MAX_TZ_OFFSET_MINS),
        ) * 60_000;
        let roll = i64::from(self.rollover_hour.min(23)) * 3_600_000;
        offset - roll
    }

    /// Index of the study day containing `t`.
    pub fn index(self, t: Timestamp) -> i64 {
        t.as_millis()
            .saturating_add(self.shift_ms())
            .div_euclid(MILLIS_PER_DAY)
    }

    /// First instant of study day `index`.
    pub fn start(self, index: i64) -> Timestamp {
        Timestamp(index.saturating_mul(MILLIS_PER_DAY) - self.shift_ms())
    }

    /// End of the study day containing `now` (= start of the next one).
    pub fn end(self, now: Timestamp) -> Timestamp {
        self.start(self.index(now) + 1)
    }

    /// Whole study days from `from` to `to` (calendar days, not 24-hour spans; 0 when `to`
    /// is earlier).
    pub fn days_between(self, from: Timestamp, to: Timestamp) -> u32 {
        let d = (self.index(to) - self.index(from)).max(0);
        u32::try_from(d).unwrap_or(u32::MAX)
    }
}

/// End of the study day containing `now`: the first local `rollover_hour:00` strictly after
/// `now`, for a zone `tz_offset_mins` minutes ahead of UTC. See [`StudyDay`].
pub fn day_end(now: Timestamp, tz_offset_mins: i32, rollover_hour: u8) -> Timestamp {
    StudyDay::new(tz_offset_mins, rollover_hour).end(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: i64 = 3_600_000;

    #[test]
    fn utc_rollover_at_four() {
        // 2026-01-01T10:00Z → day ends 2026-01-02T04:00Z.
        let jan1 = 1_767_225_600_000;
        assert_eq!(
            day_end(Timestamp(jan1 + 10 * H), 0, 4),
            Timestamp(jan1 + 28 * H)
        );
        // 03:59 still belongs to the previous day.
        assert_eq!(
            day_end(Timestamp(jan1 + 3 * H), 0, 4),
            Timestamp(jan1 + 4 * H)
        );
        // Exactly at the rollover a new day starts.
        assert_eq!(
            day_end(Timestamp(jan1 + 4 * H), 0, 4),
            Timestamp(jan1 + 28 * H)
        );
    }

    #[test]
    fn offset_shifts_the_boundary() {
        let jan1 = 1_767_225_600_000;
        // Istanbul (UTC+3): local 04:00 = 01:00Z.
        assert_eq!(
            day_end(Timestamp(jan1 + 2 * H), 180, 4),
            Timestamp(jan1 + 25 * H)
        );
        assert_eq!(day_end(Timestamp(jan1), 180, 4), Timestamp(jan1 + H));
        // Clamping keeps absurd input sane.
        assert_eq!(
            day_end(Timestamp(jan1), 100_000, 99),
            day_end(Timestamp(jan1), 840, 23)
        );
    }

    #[test]
    fn days_between_counts_calendar_days() {
        let jan1 = 1_767_225_600_000;
        let d = StudyDay::new(180, 4); // Istanbul, new day at 01:00Z
                                       // 15:00 local → next day 09:00 local: 18 hours, but one study day.
        let a = Timestamp(jan1 + 12 * H);
        assert_eq!(d.days_between(a, Timestamp(jan1 + 30 * H)), 1);
        // 23:00 → 03:00 local is still the same study day; 05:00 is the next.
        let late = Timestamp(jan1 + 20 * H);
        assert_eq!(d.days_between(late, Timestamp(jan1 + 24 * H)), 0);
        assert_eq!(d.days_between(late, Timestamp(jan1 + 26 * H)), 1);
        assert_eq!(d.days_between(late, Timestamp(jan1)), 0);
        // start/index round-trip.
        let i = d.index(a);
        assert!(d.start(i) <= a && a < d.start(i + 1));
        assert_eq!(d.index(d.start(i)), i);
        assert_eq!(d.end(a), d.start(i + 1));
    }
}
