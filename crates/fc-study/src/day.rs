//! The study day. A "day" ends at a local rollover hour (default 04:00), not at midnight, so a
//! late-night session still counts as the same day. The UTC offset is supplied by the caller
//! (the frontend knows the user's time zone; the core never reads it).

use fc_core::time::MILLIS_PER_DAY;
use fc_core::Timestamp;

/// Default local hour at which a new study day starts.
pub const DEFAULT_ROLLOVER_HOUR: u8 = 4;

/// Largest accepted UTC offset magnitude, minutes (UTC−14:00 … UTC+14:00).
pub const MAX_TZ_OFFSET_MINS: i32 = 14 * 60;

/// End of the study day containing `now`: the first local `rollover_hour:00` strictly after
/// `now`, for a zone `tz_offset_mins` minutes ahead of UTC (Istanbul: `180`). Out-of-range
/// inputs are clamped (offset to ±14 h, hour to 0–23).
pub fn day_end(now: Timestamp, tz_offset_mins: i32, rollover_hour: u8) -> Timestamp {
    let offset = i64::from(tz_offset_mins.clamp(-MAX_TZ_OFFSET_MINS, MAX_TZ_OFFSET_MINS)) * 60_000;
    let roll = i64::from(rollover_hour.min(23)) * 3_600_000;
    let local = now.as_millis().saturating_add(offset);
    let start_local = (local - roll).div_euclid(MILLIS_PER_DAY) * MILLIS_PER_DAY + roll;
    Timestamp(start_local + MILLIS_PER_DAY - offset)
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
}
