//! Query language backend: the SQL compiler must agree with the reference evaluator
//! (`fc_core::query::eval`) on random queries over a random collection; plus FTS folding,
//! paging/sorting, saved queries, maintenance reports, migration 0300 and the 100k benchmark.

use super::*;
use crate::repo::{CardSearch, CardSort};
use fc_core::attribute::{
    AttributeDef, AttributeKind, AttributeOption, AttributeTarget, AttributeValue, EntityRef,
};
use fc_core::card::CardState;
use fc_core::query::eval::{matches, AttrFact, CardFacts};
use fc_core::query::{compile_query, Catalog, Filter, SavedQuery};
use fc_core::rng::{Rng, SplitMix64};
use fc_core::tag::Tag;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{AttributeDefId, AttributeOptionId, SavedQueryId};
use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};
use std::time::Instant;

const NOW: Timestamp = Timestamp(1_800_000_000_000);
const WORDS: &[&str] = &[
    "kılıç",
    "kalkan",
    "apple",
    "elma",
    "Işık",
    "şeker",
    "öğrenci",
    "çay",
    "run",
    "ran",
    "İstanbul",
    "fiil",
];
const LEVEL: u128 = 0x9000;
const EXAM: u128 = 0x9001;
const SCORE: u128 = 0x9002;
const HARD: u128 = 0x9003;

fn aid(n: u128) -> AttributeDefId {
    AttributeDefId(u(n))
}

fn oid(n: u128) -> AttributeOptionId {
    AttributeOptionId(u(n))
}

fn attr(n: u128, key: &str, name: &str, kind: AttributeKind, opts: &[&str]) -> AttributeDef {
    AttributeDef {
        id: aid(n),
        scope_deck_id: None,
        key: key.into(),
        name: name.into(),
        kind,
        options: opts
            .iter()
            .enumerate()
            .map(|(i, l)| AttributeOption {
                id: oid(n * 16 + i as u128),
                label: (*l).into(),
                color: None,
                order: i as i32,
            })
            .collect(),
        target: AttributeTarget::Note,
        required: false,
        default_value: None,
        position: 0,
        template_id: None,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        deleted_at: None,
    }
}

/// One card of the fixture with everything the reference evaluator needs.
struct Row {
    card: Card,
    note: Note,
    tags: Vec<TagId>,
    values: Vec<(AttributeDefId, EntityRef, AttributeValue)>,
    text: String,
}

struct World {
    fx: Fx,
    decks: Vec<Deck>,
    tags: Vec<Tag>,
    notetypes: Vec<NoteType>,
    attrs: Vec<AttributeDef>,
    rows: Vec<Row>,
}

fn pick<'a, T>(rng: &mut SplitMix64, items: &'a [T]) -> &'a T {
    &items[(rng.next_u64() % items.len() as u64) as usize]
}

fn world(n_notes: u128, seed: u64) -> World {
    let fx = Fx::seeded();
    let mut rng = SplitMix64::new(seed);
    let decks = vec![
        deck(1, None, "Dil"),
        deck(2, Some(1), "İngilizce"),
        deck(3, None, "YKS"),
    ];
    let mut hard = attr(HARD, "zor", "Zor", AttributeKind::Bool, &[]);
    hard.target = AttributeTarget::Card;
    let attrs = vec![
        attr(
            LEVEL,
            "level",
            "Seviye",
            AttributeKind::OrderedEnum,
            &["A1", "A2", "B1", "B2", "C1", "C2"],
        ),
        attr(EXAM, "sinav", "Sınav", AttributeKind::Enum, &["TYT", "AYT"]),
        attr(SCORE, "puan", "Puan", AttributeKind::Number, &[]),
        hard,
    ];
    let tag_names = ["fiil", "fiil::düzensiz", "isim"];
    let mut rows = Vec::new();
    fx.write(|tx| {
        for d in &decks {
            tx.insert_deck(d).unwrap();
        }
        for a in &attrs {
            tx.insert_attribute_def(a).unwrap();
        }
        for i in 0..n_notes {
            let front = format!("{} {}", pick(&mut rng, WORDS), pick(&mut rng, WORDS));
            let back = (*pick(&mut rng, WORDS)).to_owned();
            let tags: Vec<&str> = tag_names
                .iter()
                .copied()
                .filter(|_| rng.next_u64() % 3 == 0)
                .collect();
            let mut n = note(i, &front, &back, &tags, 100 + i as i64);
            let deck_n = [1u128, 2, 3][(rng.next_u64() % 3) as usize];
            n = homed(n, did(deck_n));
            ensure_note_tags(tx, &n);
            let text = search_text(&n);
            tx.insert_note(&n, &text).unwrap();
            let mut c = card(i, i, deck_n);
            c.created_at = Timestamp(NOW.0 - (rng.next_u64() % 20) as i64 * MILLIS_PER_DAY);
            match rng.next_u64() % 4 {
                0 => {}
                k => {
                    let s = &mut c.scheduling;
                    s.state = [
                        CardState::Learning,
                        CardState::Review,
                        CardState::Relearning,
                    ][(k - 1) as usize];
                    s.due = Some(Timestamp(
                        NOW.0 + (rng.next_u64() % 10) as i64 * MILLIS_PER_DAY - 5 * MILLIS_PER_DAY,
                    ));
                    s.stability = Some((rng.next_u64() % 400) as f64 / 10.0);
                    s.difficulty = Some((rng.next_u64() % 100) as f64 / 10.0);
                    s.reps = (rng.next_u64() % 6) as u32;
                    s.lapses = (rng.next_u64() % 6) as u32;
                    s.last_review = Some(Timestamp(
                        NOW.0 - (rng.next_u64() % 30) as i64 * MILLIS_PER_DAY,
                    ));
                }
            }
            c.suspended = rng.next_u64() % 7 == 0;
            if rng.next_u64() % 9 == 0 {
                c.buried_until = Some(Timestamp(NOW.0 + MILLIS_PER_DAY));
            }
            c.flags = (rng.next_u64() % 4) as u32;
            tx.insert_card(&c).unwrap();
            let mut values = Vec::new();
            if rng.next_u64() % 3 != 0 {
                values.push((
                    aid(LEVEL),
                    EntityRef::Note(n.id),
                    AttributeValue::Choice(oid(LEVEL * 16 + u128::from(rng.next_u64() % 6))),
                ));
            }
            if rng.next_u64() % 2 == 0 {
                values.push((
                    aid(EXAM),
                    EntityRef::Note(n.id),
                    AttributeValue::Choice(oid(EXAM * 16 + u128::from(rng.next_u64() % 2))),
                ));
            }
            if rng.next_u64() % 2 == 0 {
                values.push((
                    aid(SCORE),
                    EntityRef::Note(n.id),
                    AttributeValue::Number((rng.next_u64() % 10) as f64 / 2.0),
                ));
            }
            if rng.next_u64() % 2 == 0 {
                values.push((
                    aid(HARD),
                    EntityRef::Card(c.id),
                    AttributeValue::Bool(rng.next_u64() % 2 == 0),
                ));
            }
            for (a, e, v) in &values {
                tx.set_attribute_values(*e, &[(*a, Some(v.clone()))])
                    .unwrap();
            }
            let tag_ids = n.tags.iter().map(tid_of).collect();
            rows.push(Row {
                card: c,
                note: n,
                tags: tag_ids,
                values,
                text,
            });
        }
        // One trashed note: never matches.
        let n = note(0xFFF_FFFF, "kılıç trashed", "x", &[], 1);
        put_note(tx, &n, &search_text(&n)).unwrap();
        tx.insert_card(&card(0xFFF_FFFF, 0xFFF_FFFF, 1)).unwrap();
        tx.set_notes_deleted(&[n.id], Some(Timestamp(5))).unwrap();
    });
    let tags = fx.read(|r| r.list_tags().unwrap().into_iter().map(|(t, _)| t).collect());
    World {
        fx,
        decks,
        tags,
        notetypes: vec![classic()],
        attrs,
        rows,
    }
}

impl World {
    fn filter(&self, q: &str) -> Filter {
        let cat = Catalog {
            decks: &self.decks,
            tags: &self.tags,
            notetypes: &self.notetypes,
            attributes: &self.attrs,
            flag_names: &[],
            now: NOW,
        };
        compile_query(q, &cat).unwrap_or_else(|e| panic!("{q}: {e}"))
    }

    fn expected(&self, f: &Filter) -> Vec<CardId> {
        let mut out: Vec<(Timestamp, CardId)> = Vec::new();
        for r in &self.rows {
            let stored: Vec<(AttributeDefId, EntityRef, fc_core::attribute::StoredValue)> = r
                .values
                .iter()
                .flat_map(|(a, e, v)| v.to_stored().into_iter().map(move |s| (*a, *e, s)))
                .collect();
            let facts: Vec<AttrFact<'_>> = stored
                .iter()
                .map(|(a, e, s)| AttrFact {
                    attr_id: *a,
                    target: e.target(),
                    value: &s.value,
                    value_num: s.value_num,
                })
                .collect();
            let cf = CardFacts {
                card: &r.card,
                note: &r.note,
                tag_ids: &r.tags,
                attrs: &facts,
                search_text: &r.text,
            };
            if matches(f, &cf, NOW) {
                out.push((r.card.created_at, r.card.id));
            }
        }
        out.sort();
        out.into_iter().map(|(_, id)| id).collect()
    }

    fn actual(&self, f: &Filter) -> Vec<CardId> {
        self.fx.read(|r| r.search_card_ids(f, NOW).unwrap())
    }
}

/// Query atoms covering every predicate kind.
const ATOMS: &[&str] = &[
    "kilic",
    "KILIÇ",
    "ışık",
    "isik",
    "app",
    "apple",
    "\"kılıç kalkan\"",
    "\"elma çay\"",
    "s*ker",
    "istanbul",
    "deck:Dil",
    "deck:İngilizce",
    "deck:\"dil::ingilizce\"",
    "deste:YKS",
    "deck:yok",
    "tag:fiil",
    "tag:fiil::düzensiz",
    "tag:isim",
    "tag:none",
    "etiket:fi*",
    "is:new",
    "is:learn",
    "is:review",
    "is:relearn",
    "is:due",
    "is:suspended",
    "is:buried",
    "is:flagged",
    "flag:1",
    "flag:2",
    "flag:0",
    "flag:red",
    "stability>10",
    "stability<=5.5",
    "stability!=0",
    "difficulty>=5",
    "lapses>2",
    "lapses=0",
    "reps<3",
    "retrievability<0.9",
    "retrievability>=0.95",
    "added:3",
    "added:10",
    "reviewed:7",
    "edited:100000",
    "due:0",
    "due:2",
    "due:-2",
    "seviye:B1",
    "level:b1",
    "seviye>=B1",
    "seviye<A2",
    "seviye!=C2",
    "seviye:*",
    "sınav:TYT",
    "sinav:ayt",
    "puan>2",
    "puan<=1.5",
    "puan=0",
    "zor:evet",
    "zor:no",
    "front:kılıç*",
    "front:*ç",
    "front:*ç *",
    "front:\"kılıç kalkan\"",
    "back:el*",
    "front:ran*",
    "front:*an*",
    "front:i*l*",
    "back:çay",
    "back:elma",
    "front:*an",
    "re:^ç",
    "back:re:^(run|ran)$",
    "notetype:classic",
    "type:classic",
    "card:forward",
];

fn arb_query() -> impl Strategy<Value = String> {
    let atom = prop::sample::select(ATOMS).prop_map(str::to_owned);
    atom.prop_recursive(3, 12, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 2..4).prop_map(|v| format!("({})", v.join(" "))),
            prop::collection::vec(inner.clone(), 2..4)
                .prop_map(|v| format!("({})", v.join(" OR "))),
            inner.prop_map(|q| format!("-{q}")),
        ]
    })
}

#[test]
fn every_atom_agrees_with_the_reference_evaluator() {
    let w = world(150, 7);
    let mut non_trivial = 0;
    for q in ATOMS {
        let f = w.filter(q);
        let (exp, act) = (w.expected(&f), w.actual(&f));
        assert_eq!(act, exp, "query {q}: {f:?}");
        if !exp.is_empty() && exp.len() < w.rows.len() {
            non_trivial += 1;
        }
    }
    // The fixture must exercise the predicates (not everything empty/everything).
    assert!(
        non_trivial > ATOMS.len() * 3 / 4,
        "only {non_trivial} atoms discriminate"
    );
}

/// `search_card_ids_among` = the full search restricted to the given ids (every predicate
/// kind, correlated set probes included; unknown ids and duplicates are harmless).
#[test]
fn search_among_ids_agrees_with_the_full_search() {
    let w = world(150, 7);
    let all: Vec<CardId> = w.actual(&Filter::All);
    let mut among: Vec<CardId> = all.iter().copied().step_by(3).collect();
    among.push(among[0]);
    among.push(CardId(uuid::Uuid::from_u128(0xdead)));
    let wanted: std::collections::HashSet<CardId> = among.iter().copied().collect();
    for q in ATOMS {
        let f = w.filter(q);
        let exp: Vec<CardId> = w
            .actual(&f)
            .into_iter()
            .filter(|c| wanted.contains(c))
            .collect();
        let act =
            w.fx.read(|r| r.search_card_ids_among(&f, NOW, &among).unwrap());
        assert_eq!(act, exp, "query {q}");
    }
    let none =
        w.fx.read(|r| r.search_card_ids_among(&Filter::All, NOW, &[]).unwrap());
    assert!(none.is_empty());
}

#[test]
fn random_queries_agree_with_the_reference_evaluator() {
    let w = world(120, 11);
    let mut runner = TestRunner::new(Config {
        cases: 300,
        ..Config::default()
    });
    runner
        .run(&arb_query(), |q| {
            let f = w.filter(&q);
            prop_assert_eq!(w.actual(&f), w.expected(&f), "query {}", q);
            Ok(())
        })
        .unwrap();
}

#[test]
fn turkish_folding_reaches_the_index() {
    let w = world(0, 1);
    w.fx.write(|tx| {
        let n = homed(note(1, "KILIÇ", "Işıl", &[], 1), did(1));
        tx.insert_note(&n, &search_text(&n)).unwrap();
        tx.insert_card(&card(1, 1, 1)).unwrap();
    });
    for q in ["kilic", "kılıç", "KILIC", "isil", "IŞIL", "\"kilic\""] {
        assert_eq!(w.actual(&w.filter(q)).len(), 1, "{q}");
    }
    let content: String =
        w.fx.raw()
            .query_row(
                "SELECT content FROM notes_fts WHERE note_id = ?1",
                [nid(1).to_string()],
                |r| r.get(0),
            )
            .unwrap();
    assert_eq!(content, "kilic\nisil");
}

#[test]
fn paging_sorting_and_notes_mode() {
    let w = world(40, 3);
    let all = w.filter("");
    let page = |sort, descending, offset, limit, notes_only| {
        w.fx.read(|r| {
            r.search_cards(&CardSearch {
                filter: all.clone(),
                now: NOW,
                sort,
                descending,
                notes_only,
                offset,
                limit,
            })
            .unwrap()
        })
    };
    let p = page(CardSort::Created, false, 0, 15, false);
    assert_eq!(p.total, 40);
    assert_eq!(p.hits.len(), 15);
    let ids: Vec<CardId> = (0..3)
        .flat_map(|i| page(CardSort::Created, false, i * 15, 15, false).hits)
        .map(|h| h.card.id)
        .collect();
    assert_eq!(ids.len(), 40);
    assert_eq!(ids, w.expected(&all));
    let desc = page(CardSort::Lapses, true, 0, 40, false);
    assert!(desc
        .hits
        .windows(2)
        .all(|h| h[0].card.scheduling.lapses >= h[1].card.scheduling.lapses));
    let by_text = page(CardSort::SortField, false, 0, 40, false);
    // `COLLATE NOCASE`: ASCII case folding, then code point order.
    let texts: Vec<String> = by_text
        .hits
        .iter()
        .map(|h| h.sort_field.to_ascii_lowercase())
        .collect();
    assert!(texts.windows(2).all(|t| t[0] <= t[1]), "{texts:?}");
    // Notes mode: add a second card to note 0.
    w.fx.write(|tx| {
        let mut c = card(5000, 0, 1);
        c.template_key = "reverse".into();
        tx.insert_card(&c).unwrap();
    });
    assert_eq!(page(CardSort::Created, false, 0, 100, false).total, 41);
    let notes = page(CardSort::Created, false, 0, 100, true);
    assert_eq!((notes.total, notes.hits.len()), (40, 40));
    let hit = &page(CardSort::Created, false, 0, 100, false).hits[0];
    assert_eq!(hit.notetype_id, classic().id);
}

#[test]
fn saved_queries_duplicates_trash_and_integrity() {
    let w = world(5, 5);
    let q = SavedQuery {
        id: SavedQueryId(u(77)),
        name: "Zor fiillerim".into(),
        query: "tag:fiil seviye>=B1".into(),
        icon: None,
        position: 0,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    };
    w.fx.write(|tx| tx.upsert_saved_query(&q).unwrap());
    let mut q2 = q.clone();
    q2.name = "Renamed".into();
    w.fx.write(|tx| tx.upsert_saved_query(&q2).unwrap());
    assert_eq!(
        w.fx.read(|r| r.list_saved_queries().unwrap()),
        vec![q2.clone()]
    );
    assert_eq!(w.fx.read(|r| r.get_saved_query(q.id).unwrap()), Some(q2));
    assert!(w.fx.write(|tx| tx.delete_saved_query(q.id).unwrap()));
    assert!(!w.fx.write(|tx| tx.delete_saved_query(q.id).unwrap()));

    // Duplicates: same checksum twice.
    w.fx.write(|tx| {
        for i in [200, 201] {
            let n = homed(note(i, "same", "x", &[], 1), did(1));
            tx.insert_note(&n, &search_text(&n)).unwrap();
            tx.insert_card(&card(i, i, 1)).unwrap();
        }
    });
    let groups = w.fx.read(|r| r.duplicate_note_groups().unwrap());
    assert!(groups.contains(&vec![nid(200), nid(201)]), "{groups:?}");

    let trashed = w.fx.read(|r| r.list_trashed_notes(10).unwrap());
    assert_eq!(trashed.len(), 1);
    assert_eq!(trashed[0].id, nid(0xFFF_FFFF));

    // Integrity: a live card of the trashed note, a review card without due, a note
    // without cards.
    w.fx.write(|tx| {
        let mut c = tx.get_card(cid(200)).unwrap().unwrap();
        c.scheduling.state = CardState::Review;
        c.scheduling.due = None;
        tx.update_card(&c).unwrap();
        tx.set_cards_deleted(&[cid(201)], Some(Timestamp(9)))
            .unwrap();
    });
    let rep = w.fx.read(|r| r.integrity_report().unwrap());
    assert_eq!(rep.orphan_cards, vec![cid(0xFFF_FFFF)]);
    assert_eq!(rep.unscheduled_cards, vec![cid(200)]);
    assert_eq!(rep.cardless_notes, vec![nid(201)]);
    assert!(rep.missing_search_index.is_empty());
    assert!(rep.cards_in_trashed_decks.is_empty());
    assert_eq!(rep.dangling_values, 0);
}

#[test]
fn migration_0300_folds_existing_index_rows() {
    // A database at v1 (before 0300) with a dotless-ı row.
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("old.db");
    {
        let mut conn = Connection::open(&db).unwrap();
        crate::migrate::configure_connection(&conn).unwrap();
        let v1: Vec<_> = crate::migrate::MIGRATIONS
            .iter()
            .copied()
            .filter(|m| m.version < 300)
            .collect();
        crate::migrate::migrate_with(&mut conn, &v1).unwrap();
        conn.execute(
            "INSERT INTO notes_fts (note_id, sort_field, content) VALUES ('x', 'Işık', 'ılık Işık')",
            [],
        )
        .unwrap();
    }
    // Up to 0301 (0302 rebuilds the index from notes, and this row has none).
    let mut conn = Connection::open(&db).unwrap();
    crate::migrate::configure_connection(&conn).unwrap();
    let to_0301: Vec<_> = crate::migrate::MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version < 302)
        .collect();
    crate::migrate::migrate_with(&mut conn, &to_0301).unwrap();
    let (s, c): (String, String) = conn
        .query_row("SELECT sort_field, content FROM notes_fts", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((s.as_str(), c.as_str()), ("Işik", "ilik Işik"));
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'content : isik'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 1);
    assert!(crate::migrate::schema_object_exists(&conn, "cards_created").unwrap());
}

#[test]
fn migration_0302_rebuilds_the_index_as_folded_plain_text() {
    let fx = Fx::seeded();
    let cloze =
        "{{c1::Ankara::city}} <b>başkent</b> [[Türkiye|ülke]] media:".to_owned() + &"a".repeat(64);
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Deck")).unwrap();
        add_note(tx, &note(1, &cloze, "Işık", &[], 1), 1);
        add_note(tx, &note(2, "trashed", "x", &[], 2), 1);
        tx.set_notes_deleted(&[nid(2)], Some(Timestamp(3))).unwrap();
    });
    let row = |conn: &Connection, id: u128| -> (String, String) {
        conn.query_row(
            "SELECT f.sort_field, f.content FROM notes n JOIN notes_fts f ON f.rowid = n.fts_rowid
              WHERE n.id = ?1",
            [nid(id).to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    let written = row(&fx.raw(), 1);
    assert_eq!(
        written,
        (
            "ankara baskent ulke".into(),
            "ankara baskent ulke\nisik".into()
        )
    );
    // An index as older builds wrote it (raw markup, unfolded; one note missing its row).
    {
        let conn = fx.raw();
        conn.execute_batch(&format!(
            "UPDATE notes_fts SET sort_field = 'x', content = '{{{{c1::Ankara}}}} <b>Işık</b>';
             DELETE FROM notes_fts WHERE rowid = (SELECT fts_rowid FROM notes WHERE id = '{}');
             DELETE FROM schema_migrations WHERE version = 302;",
            nid(2)
        ))
        .unwrap();
    }
    drop(fx.store);
    let store = SqliteStore::open(&fx.dir.path().join("collection.db")).unwrap();
    drop(store);
    let conn = Connection::open(fx.dir.path().join("collection.db")).unwrap();
    assert_eq!(row(&conn, 1), written);
    assert_eq!(row(&conn, 2), ("trashed".into(), "trashed\nx".into()));
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'content : (ankara ulke) NOT c1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

/// Browser-like queries of the benchmarks (see docs/04 v0.4).
const BENCH_QUERIES: &[&str] = &[
    "",
    "kilic",
    "\"kılıç kalkan\"",
    "deck:İngilizce is:due",
    "tag:fiil -is:suspended",
    "seviye>=B1 sınav:TYT",
    "stability>20 lapses>=2",
    "retrievability<0.8",
    "front:kılıç*",
    "(seviye:B1 OR seviye:B2) app",
];

/// Times a full browser request (count + first page of 50 + row data) per query, best of
/// `runs`; prints and returns the slowest query's time in µs.
fn bench(w: &World, runs: usize) -> u128 {
    let mut worst = 0u128;
    for q in BENCH_QUERIES {
        let f = w.filter(q);
        let mut best = u128::MAX;
        let mut total = 0;
        for _ in 0..runs {
            let t = Instant::now();
            let p = w.fx.read(|r| {
                r.search_cards(&CardSearch {
                    filter: f.clone(),
                    now: NOW,
                    sort: CardSort::SortField,
                    descending: true,
                    notes_only: false,
                    offset: 0,
                    limit: 50,
                })
                .unwrap()
            });
            best = best.min(t.elapsed().as_micros());
            total = p.total;
        }
        worst = worst.max(best);
        println!("{:>8.2} ms  {total:>6} hits  {q}", best as f64 / 1000.0);
    }
    worst
}

/// docs/04 v0.4: "100k notta arama < 100 ms". Run with
/// `cargo test -p fc-storage --release -- --ignored --nocapture bench_search_100k`.
#[test]
#[ignore = "benchmark: builds a 100k-note database"]
fn bench_search_100k() {
    let w = world(100_000, 42);
    let worst = bench(&w, 5);
    assert!(worst < 100_000, "slowest query took {worst} µs");
}

/// The CI-sized benchmark (runs with every `cargo test`, not ignored): 20k notes, same
/// queries. The docs/04 budget (100 ms for 100k notes) scales linearly to 20 ms; CI runners
/// are shared and noisy, so the assertion allows 3× that. (Debug builds measure about the
/// same as release here: the time is spent in SQLite, built with `opt-level = 1`.)
#[test]
fn bench_search_20k_scaled() {
    let w = world(20_000, 42);
    let budget_us: u128 = 60_000;
    let worst = bench(&w, 3);
    assert!(
        worst < budget_us,
        "slowest query took {worst} µs (budget {budget_us} µs)"
    );
}

/// Pages read by walking a sort index (large result sets) equal sorting every match.
#[test]
fn ordered_scan_pages_match_a_full_sort() {
    let w = world(20_400, 5);
    let queries = [
        "",
        "tag:fiil -is:suspended",
        "seviye>=B1",
        "-zor:yes app",
        "kilic",
    ];
    let sorts = [
        (CardSort::SortField, "n.sort_field COLLATE NOCASE"),
        (CardSort::Created, "c.created_at"),
        (CardSort::Modified, "n.updated_at"),
        (CardSort::Lapses, "c.lapses"),
        // v1.0: walks `cards_due_order` (migration 1070).
        (CardSort::Due, "c.due"),
    ];
    for q in queries {
        let f = w.filter(q);
        let ids = w.actual(&f);
        for (sort, col) in sorts {
            for descending in [false, true] {
                let dir = if descending { "DESC" } else { "ASC" };
                let expected: Vec<CardId> =
                    w.fx.raw()
                        .prepare(&format!(
                            "SELECT c.id FROM cards c JOIN notes n ON n.id = c.note_id
                     WHERE c.id IN (SELECT value FROM json_each(?1))
                     ORDER BY {col} {dir}, c.id {dir}"
                        ))
                        .unwrap()
                        .query_map([crate::sqlite::json_list(&ids)], |r| {
                            Ok(r.get::<_, String>(0)?.parse::<CardId>().unwrap())
                        })
                        .unwrap()
                        .map(Result::unwrap)
                        .collect();
                // Past the middle the index is walked backwards (v1.0).
                for (offset, limit) in [(0u32, 50u32), (1_000, 30), (12_000, 40), (20_390, 50)] {
                    let page = w.fx.read(|r| {
                        r.search_cards(&CardSearch {
                            filter: f.clone(),
                            now: NOW,
                            sort,
                            descending,
                            notes_only: false,
                            offset,
                            limit,
                        })
                        .unwrap()
                    });
                    assert_eq!(page.total as usize, ids.len(), "{q}");
                    let got: Vec<CardId> = page.hits.iter().map(|h| h.card.id).collect();
                    let want: Vec<CardId> = expected
                        .iter()
                        .skip(offset as usize)
                        .take(limit as usize)
                        .copied()
                        .collect();
                    assert_eq!(got, want, "{q} {sort:?} desc={descending} offset={offset}");
                }
            }
        }
    }
}

#[test]
fn migration_0301_adds_covering_indexes() {
    let fx = Fx::new();
    let conn = fx.raw();
    for ix in ["notes_id_live", "cards_note_live", "notes_trashed"] {
        assert!(
            crate::migrate::schema_object_exists(&conn, ix).unwrap(),
            "{ix}"
        );
    }
    // The search join is answered from them.
    let plan: Vec<String> = conn
        .prepare(
            "EXPLAIN QUERY PLAN SELECT count(*) FROM cards c JOIN notes n ON n.id = c.note_id
             WHERE c.deleted_at IS NULL AND n.deleted_at IS NULL AND c.suspended = 0",
        )
        .unwrap()
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        plan.iter().all(|p| p.contains("COVERING INDEX")),
        "{plan:?}"
    );
}

/// v1.0: the due-date sort of the browser walks an index instead of sorting every match.
#[test]
fn migration_1070_indexes_the_due_sort() {
    let fx = Fx::new();
    let conn = fx.raw();
    for ix in [
        "cards_due_order",
        "notes_sort_live",
        "notes_updated_live",
        "cards_due_load",
    ] {
        assert!(
            crate::migrate::schema_object_exists(&conn, ix).unwrap(),
            "{ix}"
        );
    }
    let plan: Vec<String> = conn
        .prepare(
            "EXPLAIN QUERY PLAN SELECT c.id FROM cards c INDEXED BY cards_due_order
             ORDER BY c.due, c.id LIMIT 100",
        )
        .unwrap()
        .query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(plan.iter().all(|p| !p.contains("TEMP B-TREE")), "{plan:?}");
}
