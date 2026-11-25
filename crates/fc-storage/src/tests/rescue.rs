//! Salvage of a damaged collection (`rescue::salvage_into`), v1.4.2 review (J1).

use super::{note, put_note, Fx};
use crate::rescue::salvage_into;
use rusqlite::Connection;

/// Closes `fx`'s store (its WAL is checkpointed into the file) and returns the paths.
fn closed(fx: Fx) -> (tempfile::TempDir, std::path::PathBuf) {
    let path = fx.db_path();
    let Fx { store, dir } = fx;
    drop(store);
    (dir, path)
}

/// Rows of a lost parent are dropped, also in WITHOUT ROWID tables (`note_tags`, `links`,
/// `attribute_values`, …): `PRAGMA foreign_key_check` reports those with a NULL rowid, which
/// made the whole "copy what is readable" rescue fail as soon as one note that had tags was
/// lost.
#[test]
fn orphans_in_without_rowid_tables_do_not_stop_the_salvage() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        put_note(tx, &note(1, "lost", "x", &["film", "verb"], 10), "lost x").unwrap();
        put_note(tx, &note(2, "kept", "y", &["film"], 11), "kept y").unwrap();
    });
    let lost = super::nid(1).to_string();
    {
        // What a damaged page does: the note row is gone, its tag rows are not.
        let raw = fx.raw();
        raw.pragma_update(None, "foreign_keys", "OFF").unwrap();
        raw.execute("DELETE FROM notes WHERE id = ?1", [&lost])
            .unwrap();
    }
    let (dir, damaged) = closed(fx);
    let fresh = dir.path().join("fresh.db");
    let report = salvage_into(&damaged, &fresh).expect("the salvage completes");
    let conn = Connection::open(&fresh).unwrap();
    let tags_of = |id: &str| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM note_tags WHERE note_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(tags_of(&lost), 0, "the lost note's tag rows are dropped");
    assert_eq!(
        tags_of(&super::nid(2).to_string()),
        1,
        "the kept note keeps its tag"
    );
    let fk: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(fk, 0);
    assert_eq!(
        report.tables["note_tags"].1, 2,
        "{:?}",
        report.tables["note_tags"]
    );
}

/// A damaged page in the middle of a table: the rows on both sides of it come back (the
/// rowid walk used to stop at the first unreadable page and drop everything after it,
/// uncounted).
#[test]
fn rows_after_a_damaged_page_are_salvaged_too() {
    let fx = Fx::seeded();
    let n = 3000i64;
    fill_reviews(&fx, n);
    let (dir, damaged) = closed(fx);
    let leaves = leaf_pages(&damaged);
    assert!(leaves.len() > 10, "{leaves:?}");
    let page = u64::try_from(leaves[leaves.len() / 2]).unwrap();
    // The whole page, header included: reading it fails.
    damage(&damaged, page, 0);
    let fresh = dir.path().join("fresh.db");
    let report = salvage_into(&damaged, &fresh).unwrap();
    let (copied, lost) = report.tables["review_log"];
    let conn = Connection::open(&fresh).unwrap();
    let last: Option<i64> = conn
        .query_row("SELECT max(created_at) FROM review_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(last, Some(n - 1), "rows after the damaged page come back");
    // One 4 KiB page holds a few dozen of these rows; everything else is copied.
    assert!(
        copied >= u64::try_from(n).unwrap() - 100,
        "copied {copied}, lost {lost}"
    );
    assert!(
        lost > 0,
        "the damaged page rows are counted as lost: copied {copied}, lost {lost}"
    );
}

/// Overwrites page `page` (1-based) from byte `from` with junk.
fn damage(db: &std::path::Path, page: u64, from: u64) {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().write(true).open(db).unwrap();
    f.seek(SeekFrom::Start((page - 1) * 4096 + from)).unwrap();
    f.write_all(&vec![0xA5; usize::try_from(4096 - from).unwrap()])
        .unwrap();
    f.sync_all().unwrap();
}

/// A page whose cells were overwritten but whose header still reads: SQLite returns junk
/// rows, which `INSERT OR IGNORE` dropped without counting them (the report said nothing was
/// lost).
#[test]
fn junk_rows_of_a_damaged_page_are_counted_as_lost() {
    let fx = Fx::seeded();
    let n = 3000i64;
    fill_reviews(&fx, n);
    let (dir, damaged) = closed(fx);
    let leaves = leaf_pages(&damaged);
    damage(
        &damaged,
        u64::try_from(leaves[leaves.len() / 2]).unwrap(),
        100,
    );
    let fresh = dir.path().join("fresh.db");
    let report = salvage_into(&damaged, &fresh).unwrap();
    let (copied, lost) = report.tables["review_log"];
    let conn = Connection::open(&fresh).unwrap();
    let last: Option<i64> = conn
        .query_row("SELECT max(created_at) FROM review_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(last, Some(n - 1), "rows after the damaged page come back");
    // One 4 KiB page holds a few dozen of these rows; everything else is copied.
    assert!(
        copied >= u64::try_from(n).unwrap() - 100,
        "copied {copied}, lost {lost}"
    );
    assert!(
        lost > 0,
        "the damaged page rows are counted as lost: copied {copied}, lost {lost}"
    );
}

fn fill_reviews(fx: &Fx, n: i64) {
    let raw = fx.raw();
    let tx = raw.unchecked_transaction().unwrap();
    for i in 0..n {
        tx.execute(
            "INSERT INTO review_log (id, card_id, rating, state_before, state_after,
                scheduled_days, elapsed_days, duration_ms, created_at)
             VALUES (?1, 'c', 'good', 'review', 'review', 1, 1, 1000, ?2)",
            rusqlite::params![format!("r{i:06}-{}", "x".repeat(80)), i],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}

/// Leaf pages of `review_log`, in file order.
fn leaf_pages(db: &std::path::Path) -> Vec<i64> {
    pages_of(db, "review_log")
}

/// Leaf pages of `table`, in file order.
fn pages_of(db: &std::path::Path, table: &str) -> Vec<i64> {
    let c = Connection::open(db).unwrap();
    let mut st = c
        .prepare("SELECT pageno FROM dbstat WHERE name = ?1 AND pagetype = 'leaf' ORDER BY pageno")
        .unwrap();
    let v: Vec<i64> = st
        .query_map([table], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(v.len() > 10, "{v:?}");
    v
}

/// Rows the migrations seed in every new collection (flag names and colours, prices, the
/// statistics generation counter) come from the damaged collection, not from the defaults:
/// `INSERT OR IGNORE` into the freshly migrated file kept the seeded row and silently dropped
/// the user's.
#[test]
fn rows_seeded_by_migrations_keep_the_collections_values() {
    let fx = Fx::seeded();
    {
        let raw = fx.raw();
        raw.execute(
            "UPDATE card_flag_defs SET name = 'zor', color = 'blue', updated_at = 5 WHERE flag = 1",
            [],
        )
        .unwrap();
        raw.execute("UPDATE review_log_generation SET n = 41 WHERE id = 1", [])
            .unwrap();
    }
    let (dir, damaged) = closed(fx);
    let fresh = dir.path().join("fresh.db");
    let report = salvage_into(&damaged, &fresh).unwrap();
    let conn = Connection::open(&fresh).unwrap();
    let flag: (Option<String>, String) = conn
        .query_row(
            "SELECT name, color FROM card_flag_defs WHERE flag = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(flag, (Some("zor".to_owned()), "blue".to_owned()));
    let generation: i64 = conn
        .query_row(
            "SELECT n FROM review_log_generation WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(generation, 41);
    assert_eq!(report.tables["card_flag_defs"], (7, 0));
    assert_eq!(report.lost(), 0, "{:?}", report.tables);
}

/// A WITHOUT ROWID table (`settings`) with a damaged page: the rows on both sides of it come
/// back and the loss is reported (such a table used to be given up whole, as "0 lost").
#[test]
fn a_without_rowid_table_with_a_damaged_page_keeps_its_other_rows() {
    let fx = Fx::seeded();
    let n = 2000i64;
    {
        let raw = fx.raw();
        let tx = raw.unchecked_transaction().unwrap();
        for i in 0..n {
            tx.execute(
                "INSERT INTO settings (scope, key, value, updated_at) VALUES ('global', ?1, ?2, 1)",
                rusqlite::params![format!("k{i:06}"), format!("\"{}\"", "v".repeat(60))],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let (dir, damaged) = closed(fx);
    let leaves = pages_of(&damaged, "settings");
    damage(
        &damaged,
        u64::try_from(leaves[leaves.len() / 2]).unwrap(),
        0,
    );
    let fresh = dir.path().join("fresh.db");
    let report = salvage_into(&damaged, &fresh).unwrap();
    let (copied, lost) = report.tables["settings"];
    let conn = Connection::open(&fresh).unwrap();
    let has = |key: String| -> bool {
        conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM settings WHERE key = ?1)",
            [key],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(
        has("k000000".into()) && has(format!("k{:06}", n - 1)),
        "both ends come back"
    );
    assert!(
        copied >= u64::try_from(n).unwrap() - 150,
        "copied {copied}, lost {lost}"
    );
    assert!(lost > 0, "copied {copied}, lost {lost}");
}
