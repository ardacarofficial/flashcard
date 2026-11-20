//! Interrupting long statements at a caller's deadline.
//!
//! A caller that must not wait past a point in time (the app servicing a plugin's host call:
//! its work counts against the plugin's deadline) sets one with [`deadline`] for as long as
//! the returned guard lives. Every SQLite connection checks it from a progress handler (every
//! [`CHECK_EVERY`] virtual machine steps); past the deadline the running statement stops with
//! [`crate::StorageError::Interrupted`] and its transaction is rolled back like any failed
//! one. The deadline is per thread (statements run on the caller's thread) and nests: an inner
//! deadline never extends an outer one.

use std::cell::Cell;
use std::time::Instant;

/// VM steps between two deadline checks (a check is one thread-local read).
pub const CHECK_EVERY: i32 = 1000;

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Restores the previous deadline when dropped.
#[derive(Debug)]
#[must_use = "the deadline holds only while the guard lives"]
pub struct DeadlineGuard {
    previous: Option<Instant>,
}

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        DEADLINE.with(|d| d.set(self.previous));
    }
}

/// Statements on this thread stop once `at` has passed (or an earlier enclosing deadline).
pub fn deadline(at: Instant) -> DeadlineGuard {
    DEADLINE.with(|d| {
        let previous = d.get();
        d.set(Some(previous.map_or(at, |p| p.min(at))));
        DeadlineGuard { previous }
    })
}

/// Whether this thread's deadline has passed.
pub fn expired() -> bool {
    DEADLINE.with(|d| d.get().is_some_and(|t| Instant::now() >= t))
}

/// Installs the deadline check on a connection.
pub(crate) fn install(conn: &mut rusqlite::Connection) -> crate::StorageResult<()> {
    conn.progress_handler(CHECK_EVERY, Some(expired))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn nests_and_restores() {
        assert!(!expired());
        let far = Instant::now() + Duration::from_secs(60);
        let outer = deadline(far);
        assert!(!expired());
        {
            let _past = deadline(Instant::now());
            assert!(expired());
            // An inner, later deadline never extends the outer one.
            let _later = deadline(far + Duration::from_secs(60));
            assert!(expired());
        }
        assert!(!expired());
        drop(outer);
        assert!(!expired());
    }

    #[test]
    fn long_statements_stop_at_the_deadline() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        install(&mut conn).unwrap();
        let endless = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) \
                       SELECT count(*) FROM c";
        let start = Instant::now();
        let r = {
            let _g = deadline(Instant::now() + Duration::from_millis(100));
            conn.query_row(endless, [], |r| r.get::<_, i64>(0))
        };
        let e = crate::StorageError::from(r.unwrap_err());
        assert!(matches!(e, crate::StorageError::Interrupted), "{e:?}");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
        // Without a deadline the connection works as before.
        let one: i64 = conn.query_row("SELECT 1", [], |r| r.get(0)).unwrap();
        assert_eq!(one, 1);
    }
}
