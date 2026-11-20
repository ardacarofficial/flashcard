//! What to offer at start after the previous run did not end cleanly.

use serde::{Deserialize, Serialize};

/// Why the recovery offer is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RecoveryReason {
    /// The app crashed (a crash report was written during the previous run).
    Crash,
    /// The app stopped without closing (killed, power loss, OS shutdown) — no crash report.
    UncleanShutdown,
}

/// The reason to offer recovery, if any: only after a run that did not end cleanly.
/// `crashes_in_previous_run` = crash reports written since that run started.
pub fn reason(
    previous_run_unclean: bool,
    crashes_in_previous_run: usize,
) -> Option<RecoveryReason> {
    match (previous_run_unclean, crashes_in_previous_run) {
        (false, _) => None,
        (true, 0) => Some(RecoveryReason::UncleanShutdown),
        (true, _) => Some(RecoveryReason::Crash),
    }
}

/// Verdict of SQLite's `PRAGMA integrity_check` / `quick_check` output rows plus the number
/// of `PRAGMA foreign_key_check` violations: healthy only when the check said exactly `ok` and
/// no foreign key is broken. Returns the problems (bounded to `max`).
pub fn integrity_problems(check_rows: &[String], fk_violations: u64, max: usize) -> Vec<String> {
    let mut problems: Vec<String> = check_rows
        .iter()
        .filter(|r| r.trim() != "ok")
        .take(max)
        .cloned()
        .collect();
    if check_rows.is_empty() {
        problems.push("integrity check returned nothing".to_owned());
    }
    if fk_violations > 0 && problems.len() < max.max(1) {
        problems.push(format!("{fk_violations} foreign key violation(s)"));
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_only_after_an_unclean_run() {
        assert_eq!(reason(false, 0), None);
        assert_eq!(
            reason(false, 3),
            None,
            "a crash report alone (old run) is not news"
        );
        assert_eq!(reason(true, 0), Some(RecoveryReason::UncleanShutdown));
        assert_eq!(reason(true, 1), Some(RecoveryReason::Crash));
    }

    #[test]
    fn integrity_verdicts() {
        assert!(integrity_problems(&["ok".into()], 0, 10).is_empty());
        assert_eq!(
            integrity_problems(&["ok".into()], 2, 10),
            vec!["2 foreign key violation(s)"]
        );
        let rows: Vec<String> = (0..50)
            .map(|i| format!("row {i} missing from index"))
            .collect();
        assert_eq!(integrity_problems(&rows, 0, 10).len(), 10);
        assert_eq!(integrity_problems(&[], 0, 10).len(), 1);
    }
}
