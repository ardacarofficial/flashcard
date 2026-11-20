//! Rotated log file names. The current file is `<base>` (`flashcard.log`); older ones are
//! `<base>.1` (newest) … `<base>.<keep>` (oldest). Rotating drops the oldest and shifts the
//! rest by one; the writer then starts a fresh `<base>`.

/// File name of generation `n` (`0` = the current file).
pub fn file_name(base: &str, n: usize) -> String {
    if n == 0 {
        base.to_owned()
    } else {
        format!("{base}.{n}")
    }
}

/// Generation of `name` for `base` (`flashcard.log` → 0, `flashcard.log.3` → 3); `None` for
/// any other file.
pub fn generation(base: &str, name: &str) -> Option<usize> {
    if name == base {
        return Some(0);
    }
    let rest = name.strip_prefix(base)?.strip_prefix('.')?;
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) || rest.starts_with('0') {
        return None;
    }
    rest.parse().ok()
}

/// One rotation step with `keep` older files: first delete [`Plan::delete`], then apply the
/// renames in order (each target is free when its turn comes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// File to delete first (the oldest kept generation, about to fall off).
    pub delete: String,
    /// `(from, to)` renames, oldest first.
    pub renames: Vec<(String, String)>,
}

/// The rotation of `base` keeping `keep` (≥ 1) older files.
pub fn plan(base: &str, keep: usize) -> Plan {
    let keep = keep.max(1);
    let renames = (0..keep)
        .rev()
        .map(|n| (file_name(base, n), file_name(base, n + 1)))
        .collect();
    Plan {
        delete: file_name(base, keep),
        renames,
    }
}

/// Whether a write of `incoming` bytes to a file of `current` bytes must rotate first. A
/// single record larger than the limit still goes into a fresh file (never an endless loop).
pub fn must_rotate(current: u64, incoming: u64, max_bytes: u64) -> bool {
    current > 0 && current.saturating_add(incoming) > max_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_generations_agree() {
        for n in 0..12 {
            assert_eq!(
                generation("flashcard.log", &file_name("flashcard.log", n)),
                Some(n)
            );
        }
        for other in [
            "flashcard.log.",
            "flashcard.log.01",
            "flashcard.logx",
            "x.log.1",
            "flashcard.log.a",
        ] {
            assert_eq!(generation("flashcard.log", other), None, "{other}");
        }
    }

    #[test]
    fn plan_shifts_oldest_first() {
        let p = plan("f.log", 3);
        assert_eq!(p.delete, "f.log.3");
        assert_eq!(
            p.renames,
            vec![
                ("f.log.2".to_owned(), "f.log.3".to_owned()),
                ("f.log.1".to_owned(), "f.log.2".to_owned()),
                ("f.log".to_owned(), "f.log.1".to_owned()),
            ]
        );
        assert_eq!(plan("f.log", 0).renames.len(), 1);
    }

    #[test]
    fn rotation_threshold() {
        assert!(
            !must_rotate(0, 5_000, 1_000),
            "a fresh file takes any record"
        );
        assert!(!must_rotate(500, 500, 1_000));
        assert!(must_rotate(501, 500, 1_000));
    }
}
