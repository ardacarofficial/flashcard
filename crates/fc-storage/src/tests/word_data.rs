//! v1.4 migration 1440: word-data packs, their rows and families, joined with the word index.

use super::*;
use crate::migrate::MIGRATIONS;
use crate::repo::{
    WordDataPackRecord, WordDataQuery, WordDataRecord, WordDataSort, WordIndexQuery,
    WordIndexRecord, NO_BAND, NO_RANK,
};

fn pack(language: &str, version: &str) -> WordDataPackRecord {
    WordDataPackRecord {
        language: language.into(),
        version: version.into(),
        name: "Test".into(),
        licence: "CC0".into(),
        sources: serde_json::json!([{"name": "x", "licence": "CC0"}]),
        notes: String::new(),
        origin: "bundled".into(),
        words: 3,
        forms: 4,
        families: 1,
        checksum: "abc".into(),
        installed_at: Timestamp(5),
    }
}

fn row(form: &str, lemma: &str, rank: Option<u32>, band: Option<&str>) -> WordDataRecord {
    WordDataRecord {
        language: "en".into(),
        form: form.into(),
        lemma: lemma.into(),
        rank,
        band: band.map(Into::into),
        band_origin: band.map(|_| "rank".into()),
    }
}

fn word(key: &str, cards: u32) -> WordIndexRecord {
    WordIndexRecord {
        language: "en".into(),
        key: key.into(),
        mark: None,
        mark_at: None,
        notes: 1,
        cards,
        max_interval_days: None,
        last_review: None,
        first_seen: Some(Timestamp(1)),
    }
}

/// A collection at schema 1439 opened by this build gets the (empty) word-data tables;
/// the word index is untouched.
#[test]
fn migration_1440_adds_empty_word_data_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let upto = MIGRATIONS.iter().position(|m| m.version >= 1440).unwrap();
    {
        let old = SqliteStore::open_with_migrations(&path, &MIGRATIONS[..upto]).unwrap();
        let mut tx = old.write().unwrap();
        tx.put_word_index_entry(&word("go", 1)).unwrap();
        tx.commit().unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    let fx = Fx { store, dir };
    fx.read(|r| {
        assert!(r.word_data_packs().unwrap().is_empty());
        assert!(r.word_data_get("en", &["go".into()]).unwrap().is_empty());
        let page = r
            .word_index_page_with_data(
                &WordIndexQuery {
                    threshold_days: 21,
                    limit: 10,
                    ..Default::default()
                },
                &WordDataQuery::default(),
            )
            .unwrap();
        assert_eq!(page, vec![(word("go", 1), None)]);
    });
}

#[test]
fn packs_replace_and_join_the_index() {
    let fx = Fx::seeded();
    let rows = [
        row("go", "go", Some(1), Some("A1")),
        row("went", "go", Some(1), Some("A1")),
        row("act", "act", Some(9), Some("A2")),
        row("actor", "actor", Some(30), Some("B2")),
        row("according to", "according to", None, Some("A2")),
    ];
    let families = [("actor".to_owned(), "act".to_owned())];
    fx.write(|tx| {
        tx.replace_word_data(&pack("en", "1"), &rows, &families)
            .unwrap();
        tx.replace_word_data(&pack("de", "1"), &[], &[]).unwrap();
        for (k, c) in [("go", 1), ("actor", 0), ("act", 1), ("pick up", 1)] {
            tx.put_word_index_entry(&word(k, c)).unwrap();
        }
    });
    fx.read(|r| {
        let packs = r.word_data_packs().unwrap();
        assert_eq!(
            packs
                .iter()
                .map(|p| p.language.as_str())
                .collect::<Vec<_>>(),
            ["de", "en"]
        );
        assert_eq!(packs[1], pack("en", "1"));
        let got = r
            .word_data_get("en", &["went".into(), "nope".into()])
            .unwrap();
        assert_eq!(got, vec![rows[1].clone()]);
        assert_eq!(
            r.word_families_of("en", &["act".into()]).unwrap(),
            vec![("actor".to_owned(), "act".to_owned())]
        );
        let q = WordIndexQuery {
            threshold_days: 21,
            limit: 10,
            ..Default::default()
        };
        let page = |d: WordDataQuery| -> Vec<String> {
            r.word_index_page_with_data(&q, &d)
                .unwrap()
                .into_iter()
                .map(|(w, _)| w.key)
                .collect()
        };
        assert_eq!(
            page(WordDataQuery {
                sort: WordDataSort::Rank,
                ..Default::default()
            }),
            ["go", "act", "actor", "pick up"]
        );
        assert_eq!(
            page(WordDataQuery {
                sort: WordDataSort::Band,
                bands: vec!["A2".into(), "B2".into()],
                ..Default::default()
            }),
            ["act", "actor"]
        );
        assert_eq!(
            page(WordDataQuery {
                max_rank: Some(9),
                ..Default::default()
            }),
            ["act", "go"]
        );
        // The cursor: after `act` (rank 9) in rank order.
        let after = r
            .word_index_page_with_data(
                &WordIndexQuery {
                    after: Some(("en".into(), "act".into())),
                    ..q.clone()
                },
                &WordDataQuery {
                    sort: WordDataSort::Rank,
                    after_sort: Some((NO_BAND.into(), 9)),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(after.len(), 2);
        assert_eq!(after[1].1, None);
        let tail = r
            .word_index_page_with_data(
                &WordIndexQuery {
                    after: Some(("en".into(), "actor".into())),
                    ..q.clone()
                },
                &WordDataQuery {
                    sort: WordDataSort::Rank,
                    after_sort: Some((NO_BAND.into(), NO_RANK)),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(tail[0].0.key, "pick up");
        let counts = r.word_index_band_counts(21).unwrap();
        assert!(counts.contains(&("en".into(), Some("A1".into()), "learning".into(), 1)));
        assert!(counts.contains(&("en".into(), None, "learning".into(), 1)));
    });
    // Replacing a language's pack drops its old rows; removing drops everything.
    fx.write(|tx| {
        tx.replace_word_data(&pack("en", "2"), &rows[..1], &[])
            .unwrap();
    });
    fx.read(|r| {
        assert!(r.word_data_get("en", &["went".into()]).unwrap().is_empty());
        assert!(r
            .word_families_of("en", &["act".into()])
            .unwrap()
            .is_empty());
        assert_eq!(r.word_data_packs().unwrap()[1].version, "2");
    });
    fx.write(|tx| tx.remove_word_data("en").unwrap());
    fx.read(|r| {
        assert_eq!(r.word_data_packs().unwrap().len(), 1);
        assert!(r.word_data_get("en", &["go".into()]).unwrap().is_empty());
    });
}

/// 1441: the `word_frequency` view the language queries read is the word data, keyed by form.
#[test]
fn migration_1441_word_frequency_view_reads_the_word_data() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        let rows = [
            row("go", "go", Some(1), Some("A1")),
            row("went", "go", Some(1), Some("A1")),
            row("according to", "according to", None, Some("A2")),
        ];
        tx.replace_word_data(&pack("en", "1"), &rows, &[]).unwrap();
    });
    let conn = fx.raw();
    let got: Vec<(String, String, Option<i64>, Option<String>)> = conn
        .prepare("SELECT language, key, rank, band FROM word_frequency ORDER BY key")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        got,
        vec![
            ("en".into(), "according to".into(), None, Some("A2".into())),
            ("en".into(), "go".into(), Some(1), Some("A1".into())),
            ("en".into(), "went".into(), Some(1), Some("A1".into())),
        ]
    );
    // The view is not a table: a backup dump never carries it.
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'word_frequency' AND type = 'table'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
}
