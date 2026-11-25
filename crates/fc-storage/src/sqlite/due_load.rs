//! The load balancer's "cards due per day", kept on the writer connection (v1.4.2 K2).
//!
//! The load balancer asks [`StatsRead::due_counts_by_day`](crate::repo::StatsRead) for the
//! days around every interval it plans: once for the answer and once per button of the next
//! card. Counting from the `cards_due_load` index costs time in proportion to the cards due on
//! those days. Inside a **write transaction** the counts of the days asked so far are kept in
//! a `TEMP` table of the writer connection instead, and `TEMP` triggers on `cards` and `notes`
//! keep them equal to what the counting query would return:
//!
//! * a card inserted, updated or deleted moves its day's count by the same membership test the
//!   query uses (scheduled state, not suspended, not trashed, note not trashed);
//! * a note trashed, restored, inserted trashed or deleted forgets every count (its cards'
//!   membership changes without their rows changing; rare);
//! * the `TEMP` table is part of the transaction, so a rollback takes the counts back with the
//!   rows;
//! * a commit by **another connection** (a second process on the same file) changes
//!   `PRAGMA data_version` of this one: every count is forgotten;
//! * a calendar with another shift (time zone, day start) starts over.
//!
//! Only day-aligned ranges of at most [`MAX_DAYS`] days are served; anything else, a read
//! snapshot, a failure to set the tables up, or the cache switched off
//! ([`set_due_load_cache`]) runs the counting query as before. `due_load_cache_equals_the_query`
//! (storage tests) checks the two against each other over random writes.

use super::{all, exec, one};
use crate::error::StorageResult;
use fc_core::Timestamp;
use rusqlite::{params, Connection};
use std::sync::atomic::{AtomicBool, Ordering};

const T: &str = "cards";
const DAY: i64 = fc_core::time::MILLIS_PER_DAY;
/// Longest range served from the kept counts (a range is a few percent of an interval).
const MAX_DAYS: i64 = 1024;
/// Triggers [`SETUP`] creates.
const TRIGGERS: i64 = 6;

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Switches the kept counts off or on for this process (on by default). Off, every
/// `due_counts_by_day` runs the counting query: same results, for measurements and for tests
/// that compare the two.
pub fn set_due_load_cache(on: bool) {
    ENABLED.store(on, Ordering::SeqCst);
}

#[cfg(test)]
thread_local! {
    /// Counting queries run to fill the kept counts by this thread (tests).
    pub(crate) static LOADS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The counting query: `(day, cards)` of `[from, to)`, days without cards left out.
pub(super) fn count(
    conn: &Connection,
    from: Timestamp,
    to: Timestamp,
    shift_ms: i64,
) -> StorageResult<Vec<(i64, u32)>> {
    // Floor division (`div_euclid`) in SQL: SQLite's `/` truncates toward zero.
    all(
        conn,
        T,
        "SELECT d, count(*) FROM (
             SELECT ((c.due + ?3) - (((c.due + ?3) % 86400000) + 86400000) % 86400000)
                    / 86400000 AS d
             FROM cards c INDEXED BY cards_due_load
             WHERE c.due >= ?1 AND c.due < ?2
               AND c.state IN ('learning', 'review', 'relearning')
               AND c.suspended = 0 AND c.deleted_at IS NULL
               AND c.note_id NOT IN
                   (SELECT id FROM notes INDEXED BY notes_trashed WHERE deleted_at IS NOT NULL)
         ) GROUP BY d ORDER BY d",
        params![from.0, to.0, shift_ms],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, u32>(1)?)),
    )
}

/// `UPDATE` of the kept count of the day of row `p` (`OLD`/`NEW`) by `step`, when the row is
/// one the counting query counts.
fn step(p: &str, step: &str) -> String {
    format!(
        "UPDATE fc_due_load SET n = n {step}
          WHERE day = (SELECT (({p}.due + s.shift)
                               - ((({p}.due + s.shift) % 86400000) + 86400000) % 86400000)
                              / 86400000 FROM fc_due_load_cfg s)
            AND {p}.due IS NOT NULL
            AND {p}.state IN ('learning', 'review', 'relearning')
            AND {p}.suspended = 0 AND {p}.deleted_at IS NULL
            AND NOT EXISTS (SELECT 1 FROM notes x
                             WHERE x.id = {p}.note_id AND x.deleted_at IS NOT NULL);"
    )
}

fn setup_sql() -> String {
    format!(
        "CREATE TEMP TABLE IF NOT EXISTS fc_due_load (day INTEGER PRIMARY KEY, n INTEGER NOT NULL);
         CREATE TEMP TABLE IF NOT EXISTS fc_due_load_cfg (
             id INTEGER PRIMARY KEY CHECK (id = 0), shift INTEGER NOT NULL,
             version INTEGER NOT NULL);
         INSERT OR IGNORE INTO fc_due_load_cfg (id, shift, version) VALUES (0, 0, -1);
         DELETE FROM fc_due_load;
         CREATE TEMP TRIGGER IF NOT EXISTS fc_due_load_card_ins AFTER INSERT ON main.cards
         BEGIN {ins} END;
         CREATE TEMP TRIGGER IF NOT EXISTS fc_due_load_card_del AFTER DELETE ON main.cards
         BEGIN {del} END;
         CREATE TEMP TRIGGER IF NOT EXISTS fc_due_load_card_upd AFTER UPDATE ON main.cards
         BEGIN {del} {ins} END;
         CREATE TEMP TRIGGER IF NOT EXISTS fc_due_load_note_ins AFTER INSERT ON main.notes
         WHEN NEW.deleted_at IS NOT NULL
         BEGIN DELETE FROM fc_due_load; END;
         CREATE TEMP TRIGGER IF NOT EXISTS fc_due_load_note_del BEFORE DELETE ON main.notes
         BEGIN DELETE FROM fc_due_load; END;
         CREATE TEMP TRIGGER IF NOT EXISTS fc_due_load_note_upd AFTER UPDATE ON main.notes
         WHEN (OLD.deleted_at IS NULL) <> (NEW.deleted_at IS NULL) OR OLD.id <> NEW.id
         BEGIN DELETE FROM fc_due_load; END;",
        ins = step("NEW", "+ 1"),
        del = step("OLD", "- 1"),
    )
}

/// `(shift, data version at the last use)` of the kept counts, `None` when the tables or a
/// trigger are missing (never set up on this connection, or set up in a transaction that was
/// rolled back).
fn config(conn: &Connection) -> Option<(i64, i64)> {
    one(
        conn,
        T,
        "SELECT shift, version,
                (SELECT count(*) FROM temp.sqlite_master
                  WHERE type = 'trigger' AND name GLOB 'fc_due_load_*')
           FROM temp.fc_due_load_cfg",
        [],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        },
    )
    .ok()
    .flatten()
    .filter(|(_, _, triggers)| *triggers == TRIGGERS)
    .map(|(shift, version, _)| (shift, version))
}

/// The kept counts of `[from, to)` on a writer connection inside its transaction, filled for
/// the days not asked before; `None` when this range is not served (see the module docs).
fn kept(
    conn: &Connection,
    from: Timestamp,
    to: Timestamp,
    shift_ms: i64,
) -> StorageResult<Option<Vec<(i64, u32)>>> {
    let (Some(a), Some(b)) = (from.0.checked_add(shift_ms), to.0.checked_add(shift_ms)) else {
        return Ok(None);
    };
    if a.rem_euclid(DAY) != 0 || b.rem_euclid(DAY) != 0 || b <= a {
        return Ok(None);
    }
    let (first, last) = (a.div_euclid(DAY), b.div_euclid(DAY) - 1);
    let span = last - first + 1;
    if span > MAX_DAYS {
        return Ok(None);
    }
    let (shift, version) = match config(conn) {
        Some(c) => c,
        None => {
            conn.execute_batch(&setup_sql())
                .map_err(|e| crate::error::in_table(e, T))?;
            match config(conn) {
                Some(c) => c,
                None => return Ok(None),
            }
        }
    };
    // Commits of other connections move this number; this connection's own do not.
    let now: i64 = conn
        .prepare_cached("PRAGMA data_version")
        .and_then(|mut s| s.query_row([], |r| r.get(0)))
        .map_err(|e| crate::error::in_table(e, T))?;
    if shift != shift_ms || version != now {
        exec(conn, T, "DELETE FROM temp.fc_due_load", [])?;
        exec(
            conn,
            T,
            "UPDATE temp.fc_due_load_cfg SET shift = ?1, version = ?2",
            params![shift_ms, now],
        )?;
    }
    let read = |conn: &Connection| {
        all(
            conn,
            T,
            "SELECT day, n FROM temp.fc_due_load WHERE day >= ?1 AND day <= ?2 ORDER BY day",
            params![first, last],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
    };
    let mut rows = read(conn)?;
    if rows.len() as i64 != span {
        // The days from the first to the last one not kept yet, counted in one query.
        let mut have = rows.iter().map(|(d, _)| *d).peekable();
        let mut missing: Option<(i64, i64)> = None;
        for day in first..=last {
            if have.peek() == Some(&day) {
                have.next();
            } else {
                missing = Some((missing.map_or(day, |(m, _)| m), day));
            }
        }
        let Some((m0, m1)) = missing else {
            return Ok(None);
        };
        let at = |day: i64| day.checked_mul(DAY).and_then(|t| t.checked_sub(shift_ms));
        let (Some(lo), Some(hi)) = (at(m0), at(m1 + 1)) else {
            return Ok(None);
        };
        #[cfg(test)]
        LOADS.with(|l| l.set(l.get() + 1));
        let counted = count(conn, Timestamp(lo), Timestamp(hi), shift_ms)?;
        let mut counted = counted.into_iter().peekable();
        for day in m0..=m1 {
            let n = counted.next_if(|(d, _)| *d == day).map_or(0, |(_, n)| n);
            exec(
                conn,
                T,
                "INSERT OR REPLACE INTO temp.fc_due_load (day, n) VALUES (?1, ?2)",
                params![day, n],
            )?;
        }
        rows = read(conn)?;
        if rows.len() as i64 != span {
            return Ok(None);
        }
    }
    let mut out = Vec::with_capacity(rows.len());
    for (day, n) in rows {
        // A count the triggers could not have produced: do not trust any of them.
        let Ok(n) = u32::try_from(n) else {
            exec(conn, T, "DELETE FROM temp.fc_due_load", [])?;
            return Ok(None);
        };
        if n > 0 {
            out.push((day, n));
        }
    }
    Ok(Some(out))
}

/// [`StatsRead::due_counts_by_day`](crate::repo::StatsRead): from the kept counts in a write
/// transaction (`write`), else by the counting query.
pub(super) fn due_counts_by_day(
    conn: &Connection,
    write: bool,
    from: Timestamp,
    to: Timestamp,
    shift_ms: i64,
) -> StorageResult<Vec<(i64, u32)>> {
    if write && ENABLED.load(Ordering::SeqCst) {
        // Any failure of the kept counts (no temp storage, a lost transaction) is not the
        // caller's: the query answers.
        if let Ok(Some(rows)) = kept(conn, from, to, shift_ms) {
            return Ok(rows);
        }
    }
    count(conn, from, to, shift_ms)
}
