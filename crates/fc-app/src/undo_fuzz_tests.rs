//! v1.4.1 (H3): the undo invariant across the API's commands. A seeded random sequence of
//! undoable commands runs against a real SQLite file; undoing everything gives back the
//! user data it started from, redoing everything gives back the end state, and the app is
//! restarted (store closed and reopened) in the middle of each phase.
//!
//! The list of commands is **derived from the registry**: every public, undoable, replayable
//! command must either have a generator here or be listed in [`NOT_GENERATED`] with the
//! reason — a new command cannot be forgotten silently.
//!
//! Deterministic: `FC_FUZZ_SEED` overrides the seed, `FC_FUZZ_CASES` the number of runs
//! (soak); failures print the seed and the step list.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{TestApp, TickClock};
use fc_api::v1::oplog::Actor;
use fc_api::{ErrorCode, MethodKind, Visibility};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_storage::repo::{CollectionDump, SqlValue};
use fc_storage::{SqliteStore, Store};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

const SEED: u64 = 0x4833_0d0e_2026_1123;

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// SplitMix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        (!v.is_empty()).then(|| &v[self.below(v.len())])
    }
    fn some<T: Clone>(&mut self, v: &[T], max: usize) -> Vec<T> {
        let n = 1 + self.below(max);
        (0..n).filter_map(|_| self.pick(v).cloned()).collect()
    }
    fn bool(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

/// Live ids the generators draw from (read from the store before each step).
#[derive(Default)]
struct Pool {
    decks: Vec<String>,
    deleted_decks: Vec<String>,
    notes: Vec<String>,
    deleted_notes: Vec<String>,
    cards: Vec<String>,
    tags: Vec<String>,
    saved_queries: Vec<String>,
    presets: Vec<String>,
}

fn ids(t: &TestApp, sql_table: &str, live: bool) -> Vec<String> {
    let dump = t.store.read().unwrap().dump_collection().unwrap();
    let Some(tab) = dump.tables.iter().find(|x| x.name == sql_table) else {
        return Vec::new();
    };
    let id = tab.columns.iter().position(|c| c == "id");
    let del = tab.columns.iter().position(|c| c == "deleted_at");
    tab.rows
        .iter()
        .filter(|r| del.is_none_or(|d| (r[d] == SqlValue::Null) == live))
        .filter_map(|r| match id.map(|i| &r[i]) {
            Some(SqlValue::Text(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

impl Pool {
    fn read(t: &TestApp) -> Self {
        let tags = t
            .app
            .invoke("v1.tags.list", json!({}), &Actor::User)
            .ok()
            .and_then(|v| v.get("tags").cloned())
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_owned))
            .collect();
        Self {
            decks: ids(t, "decks", true),
            deleted_decks: ids(t, "decks", false),
            notes: ids(t, "notes", true),
            deleted_notes: ids(t, "notes", false),
            cards: ids(t, "cards", true),
            tags,
            saved_queries: ids(t, "saved_queries", true),
            presets: ids(t, "presets", true),
        }
    }
}

const WORDS: &[&str] = &[
    "run",
    "ran",
    "go",
    "çay",
    "kılıç",
    "İstanbul",
    "house",
    "a b",
];
const TAGS: &[&str] = &["fiil", "isim", "fiil::düzensiz", "film", "zor"];

fn word(r: &mut Rng) -> String {
    format!("{}{}", r.pick(WORDS).unwrap(), r.below(5))
}

type Gen = fn(&Pool, &mut Rng) -> Option<Value>;

/// One generator per command: a request built from the live ids (`None` = not possible now).
fn generators() -> BTreeMap<&'static str, Gen> {
    let mut g: BTreeMap<&'static str, Gen> = BTreeMap::new();
    g.insert("decks.create", |p, r| {
        let parent = if r.bool() {
            p.decks.first().cloned()
        } else {
            None
        };
        Some(
            json!({"path": format!("D{}", r.below(6)), "parent_id": parent,
                    "description": null, "icon": null, "color": null}),
        )
    });
    g.insert("decks.rename", |p, r| {
        Some(json!({"id": r.pick(&p.decks)?, "name": format!("R{}", r.below(6))}))
    });
    g.insert("decks.move", |p, r| {
        let parent = if r.bool() {
            r.pick(&p.decks).cloned()
        } else {
            None
        };
        Some(json!({"id": r.pick(&p.decks)?, "parent_id": parent, "position": null}))
    });
    g.insert("decks.update", |p, r| {
        Some(
            json!({"id": r.pick(&p.decks)?, "description": word(r), "icon": null,
                    "color": if r.bool() { json!("#aa5500") } else { json!(null) }}),
        )
    });
    g.insert("decks.delete", |p, r| {
        (p.decks.len() > 1).then(|| json!({"id": r.pick(&p.decks[1..]).unwrap()}))
    });
    g.insert("decks.restore", |p, r| {
        Some(json!({"id": r.pick(&p.deleted_decks)?}))
    });
    g.insert("notes.create", |p, r| {
        let tags = r.some(TAGS, 2);
        Some(
            json!({"notetype_id": CLASSIC_ID, "deck_id": r.pick(&p.decks)?,
                    "fields": {"front": word(r), "back": word(r)}, "tags": tags,
                    "attributes": [], "allow_duplicate": true}),
        )
    });
    g.insert("notes.create_many", |p, r| {
        let d = r.pick(&p.decks)?.clone();
        let notes: Vec<Value> = (0..1 + r.below(3))
            .map(|_| {
                json!({"notetype_id": CLASSIC_ID, "deck_id": d,
                       "fields": {"front": word(r), "back": word(r)}, "tags": [],
                       "attributes": [], "allow_duplicate": true})
            })
            .collect();
        Some(json!({"notes": notes}))
    });
    g.insert("notes.update", |p, r| {
        Some(
            json!({"id": r.pick(&p.notes)?, "fields": {"front": word(r), "back": word(r)},
                    "tags": r.some(TAGS, 2), "attributes": []}),
        )
    });
    g.insert("notes.delete", |p, r| {
        Some(json!({"ids": [r.pick(&p.notes)?]}))
    });
    g.insert("notes.restore", |p, r| {
        Some(json!({"ids": [r.pick(&p.deleted_notes)?]}))
    });
    g.insert("tags.rename", |p, r| {
        Some(json!({"from": r.pick(&p.tags)?, "to": r.pick(TAGS).unwrap().to_string() + "2"}))
    });
    g.insert("tags.merge", |p, r| {
        Some(json!({"sources": [r.pick(&p.tags)?], "into": r.pick(TAGS).unwrap()}))
    });
    g.insert("tags.set_color", |p, r| {
        Some(json!({"name": r.pick(&p.tags)?,
                    "color": if r.bool() { json!("#123456") } else { json!(null) }}))
    });
    g.insert("bulk.add_tags", |p, r| {
        Some(json!({"note_ids": r.some(&p.notes, 3), "tags": r.some(TAGS, 2)}).take_if(&p.notes))
    });
    g.insert("bulk.remove_tags", |p, r| {
        Some(json!({"note_ids": r.some(&p.notes, 3), "tags": r.some(TAGS, 2)}).take_if(&p.notes))
    });
    g.insert("bulk.set_flag", |p, r| {
        Some(
            json!({"card_ids": r.some(&p.cards, 3), "flag": 1 + r.below(3), "on": r.bool()})
                .take_if(&p.cards),
        )
    });
    g.insert("bulk.set_suspended", |p, r| {
        Some(json!({"card_ids": r.some(&p.cards, 3), "suspended": r.bool()}).take_if(&p.cards))
    });
    g.insert("bulk.set_buried", |p, r| {
        Some(json!({"card_ids": r.some(&p.cards, 3), "buried": r.bool()}).take_if(&p.cards))
    });
    g.insert("bulk.move_cards", |p, r| {
        Some(
            json!({"card_ids": r.some(&p.cards, 3), "deck_id": r.pick(&p.decks)?})
                .take_if(&p.cards),
        )
    });
    g.insert("bulk.reset_progress", |p, r| {
        Some(json!({"card_ids": r.some(&p.cards, 3)}).take_if(&p.cards))
    });
    g.insert("bulk.find_replace", |p, r| {
        Some(
            json!({"note_ids": r.some(&p.notes, 3), "find": r.pick(WORDS).unwrap(),
                    "replace": word(r), "field": null, "regex": false, "match_case": r.bool()})
            .take_if(&p.notes),
        )
    });
    g.insert("cards.set_flag", |p, r| {
        Some(
            json!({"ids": r.some(&p.cards, 2), "flag": 1 + r.below(3), "on": r.bool()})
                .take_if(&p.cards),
        )
    });
    g.insert("cards.set_flags", |p, r| {
        let id = r.pick(&p.cards)?.clone();
        Some(json!({"items": [{"id": id, "flags": r.below(8)}]}))
    });
    g.insert("cards.set_user_note", |p, r| {
        Some(json!({"id": r.pick(&p.cards)?,
                    "text": if r.bool() { json!(word(r)) } else { json!(null) }}))
    });
    g.insert("study.suspend", |p, r| {
        Some(json!({"card_ids": r.some(&p.cards, 2), "suspended": r.bool()}).take_if(&p.cards))
    });
    g.insert("study.bury", |p, r| {
        Some(json!({"card_ids": r.some(&p.cards, 2), "buried": r.bool()}).take_if(&p.cards))
    });
    g.insert("study.postpone", |p, r| {
        Some(
            json!({"deck_id": r.pick(&p.decks)?, "include_subdecks": r.bool(),
                    "days": 1 + r.below(5)}),
        )
    });
    g.insert("words.mark", |_, r| {
        let mark = [json!("known"), json!("ignored"), json!(null)][r.below(3)].clone();
        Some(json!({"language": "en", "lemmas": [r.pick(WORDS).unwrap()], "mark": mark}))
    });
    g.insert("settings.set", |p, r| {
        let deck = r.pick(&p.decks)?.clone();
        let lang = ["en", "de"][r.below(2)];
        let days = 7 + r.below(30);
        let keep = 2 + r.below(5);
        Some(match r.below(3) {
            0 => json!({"scope": {"kind": "deck", "id": deck}, "key": "words.language",
                        "value": lang}),
            1 => json!({"scope": {"kind": "profile"}, "key": "words.known_interval_days",
                        "value": days}),
            _ => json!({"scope": {"kind": "profile"}, "key": "io.auto_backup_keep",
                        "value": keep}),
        })
    });
    g.insert("saved_queries.create", |_, r| {
        Some(
            json!({"name": format!("Q{}", r.below(100)), "query": "tag:fiil", "icon": null,
                    "position": null}),
        )
    });
    g.insert("saved_queries.delete", |p, r| {
        Some(json!({"id": r.pick(&p.saved_queries)?}))
    });
    g.insert("accepted_answers.add", |p, r| {
        Some(json!({"note_id": r.pick(&p.notes)?, "field": null, "answers": [word(r)]}))
    });
    g.insert("accepted_answers.remove", |p, r| {
        Some(json!({"note_id": r.pick(&p.notes)?, "field": null, "answers": [word(r)]}))
    });
    g.insert("presets.create", |_, r| {
        Some(json!({"name": format!("P{}", r.below(100)), "config": null}))
    });
    g.insert("presets.assign", |p, r| {
        Some(json!({"deck_ids": [r.pick(&p.decks)?],
                    "preset_id": if r.bool() { r.pick(&p.presets).cloned() } else { None }}))
    });
    g
}

trait TakeIf {
    fn take_if(self, pool: &[String]) -> Value;
}

impl TakeIf for Value {
    /// The request, or `null` (skipped) when the pool it draws from is empty.
    fn take_if(self, pool: &[String]) -> Value {
        if pool.is_empty() {
            Value::Null
        } else {
            self
        }
    }
}

/// Public undoable commands without a generator yet, with the reason. Each is covered by its
/// own module's undo test; the random sequence does not draw them.
const NOT_GENERATED: &[(&str, &str)] = &[
    ("ai.", "AI drafts/providers: own undo tests (mock provider)"),
    (
        "attributes.",
        "classifications: own undo tests (attributes, bulk.set_attribute)",
    ),
    ("automation.", "automation rules: own undo tests"),
    ("browse.repair_database", "maintenance, not user data"),
    ("bulk.set_attribute", "classifications: own undo tests"),
    ("bulk.change_notetype", "note type change: own undo test"),
    ("deep_links.open", "opens a link (inbox row): own test"),
    ("flags.update", "flag names: own undo test"),
    ("goals.", "goals: own undo tests"),
    ("inbox.", "inbox: own undo tests"),
    ("mcp.", "MCP servers: own undo tests"),
    (
        "modes.",
        "study modes need a running session: own undo tests",
    ),
    (
        "modules.set_enabled",
        "switching modules: module_switch_tests",
    ),
    ("notetypes.", "note types: own undo tests"),
    ("planning.", "exams and pace: own undo tests"),
    ("plugins.", "plugins need a package: own undo tests"),
    ("presets.update", "preset config: own undo tests"),
    ("presets.clone", "own undo test"),
    ("presets.delete", "own undo test"),
    ("reading.", "reading items: own undo tests"),
    ("saved_queries.update", "own undo test"),
    ("settings.reset", "own undo test"),
    ("shortcuts.", "keyboard shortcuts: own undo tests"),
    ("stats.leech_repairs.apply", "needs leeches: own undo test"),
    (
        "study.answer",
        "needs a study session: study module undo tests",
    ),
    ("subtitle.", "Subtitle captures: subtitle module undo tests"),
    ("themes.set", "own undo test"),
];

fn not_generated(name: &str) -> bool {
    NOT_GENERATED
        .iter()
        .any(|(p, _)| name == *p || (p.ends_with('.') && name.starts_with(p)))
}

/// Every public undoable replayable command is generated or listed with a reason.
#[test]
fn every_undoable_command_is_generated_or_listed() {
    let t = TestApp::mem();
    let gens = generators();
    let mut missing = Vec::new();
    let mut stale = Vec::new();
    let registered: BTreeSet<&str> = t
        .app
        .registry()
        .methods()
        .filter(|d| {
            d.kind == MethodKind::Command
                && d.undoable
                && d.replayable
                && d.visibility == Visibility::Public
        })
        .map(|d| d.name)
        .collect();
    for name in &registered {
        if !gens.contains_key(name) && !not_generated(name) {
            missing.push(*name);
        }
    }
    for name in gens.keys() {
        if !registered.contains(name) {
            stale.push(*name);
        }
    }
    assert!(
        missing.is_empty(),
        "undoable commands with no generator and no reason: {missing:?}"
    );
    assert!(
        stale.is_empty(),
        "generators for unknown/non-undoable commands: {stale:?}"
    );
    assert!(gens.len() >= 30, "{}", gens.len());
}

// ------------------------------------------------------------------ snapshot

/// Tables that are not the user's data: history, caches, derived indexes, bookkeeping.
const NOT_USER_DATA: &[&str] = &[
    "oplog",
    "jobs",
    "schema_migrations",
    "notes_fts",
    "word_index",
    "word_index_notes",
    "word_index_dirty",
    "word_index_dirty_keys",
    "word_note_language",
    "stats_cache_meta",
    "stats_day_cache",
    "session_queue",
    "unfinished_batches",
    "unfinished_batch_parts",
    "review_log_generation",
    "ux_state",
];

fn key(v: &SqlValue) -> String {
    format!("{v:?}")
}

/// Ids (`id` column) of the live rows of `table` in `dump`.
fn live_ids(dump: &CollectionDump, table: &str) -> BTreeSet<String> {
    let Some(t) = dump.tables.iter().find(|t| t.name == table) else {
        return BTreeSet::new();
    };
    let id = t.columns.iter().position(|c| c == "id");
    let del = t.columns.iter().position(|c| c == "deleted_at");
    t.rows
        .iter()
        .filter(|r| del.is_none_or(|d| r[d] == SqlValue::Null))
        .filter_map(|r| id.map(|i| key(&r[i])))
        .collect()
}

/// What the user would see: every user-data table, soft-deleted rows and the rows that hang
/// off them (a trashed note's tags and keys) left out, tags no live note uses and nobody
/// coloured left out (undoing the last note with a tag keeps the tag row, unseen), media
/// rows no note references left out (the media check removes them), edit
/// times blanked (an undone edit is the same data at a later time), rows sorted.
fn user_data(store: &dyn Store) -> BTreeMap<String, Vec<Vec<SqlValue>>> {
    let dump: CollectionDump = store.read().unwrap().dump_collection().unwrap();
    let notes = live_ids(&dump, "notes");
    let cards = live_ids(&dump, "cards");
    let used_tags: BTreeSet<String> = dump
        .tables
        .iter()
        .filter(|t| t.name == "note_tags")
        .flat_map(|t| {
            let n = t.columns.iter().position(|c| c == "note_id").unwrap();
            let g = t.columns.iter().position(|c| c == "tag_id").unwrap();
            t.rows
                .iter()
                .filter(|r| notes.contains(&key(&r[n])))
                .map(|r| key(&r[g]))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut out = BTreeMap::new();
    for t in &dump.tables {
        if NOT_USER_DATA.contains(&t.name.as_str()) {
            continue;
        }
        let col = |name: &str| t.columns.iter().position(|c| c == name);
        let (del, note, card) = (col("deleted_at"), col("note_id"), col("card_id"));
        let (id, color, refs) = (col("id"), col("color"), col("ref_count"));
        let blank: Vec<usize> = t
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| matches!(c.as_str(), "updated_at" | "modified_at" | "mtime"))
            .map(|(i, _)| i)
            .collect();
        let mut rows: Vec<Vec<SqlValue>> = t
            .rows
            .iter()
            .filter(|r| del.is_none_or(|d| r[d] == SqlValue::Null))
            .filter(|r| t.name == "notes" || note.is_none_or(|n| notes.contains(&key(&r[n]))))
            .filter(|r| t.name == "cards" || card.is_none_or(|c| cards.contains(&key(&r[c]))))
            .filter(|r| {
                t.name != "tags"
                    || id.is_some_and(|i| used_tags.contains(&key(&r[i])))
                    || color.is_some_and(|c| r[c] != SqlValue::Null)
            })
            // Media no note uses (an undone import's files) wait for the media check.
            .filter(|r| t.name != "media" || refs.is_none_or(|c| r[c] != SqlValue::Integer(0)))
            .map(|r| {
                let mut r = r.clone();
                for &i in &blank {
                    r[i] = SqlValue::Null;
                }
                r
            })
            .collect();
        rows.sort_by_key(|a| format!("{a:?}"));
        let cols = t.columns.join(",");
        out.insert(format!("{}({cols})", t.name), rows);
    }
    out
}

fn diff(
    a: &BTreeMap<String, Vec<Vec<SqlValue>>>,
    b: &BTreeMap<String, Vec<Vec<SqlValue>>>,
) -> String {
    let mut out = String::new();
    for (k, rows) in a {
        let other = b.get(k).cloned().unwrap_or_default();
        if *rows != other {
            let only_a: Vec<_> = rows.iter().filter(|r| !other.contains(r)).take(3).collect();
            let only_b: Vec<_> = other.iter().filter(|r| !rows.contains(r)).take(3).collect();
            out.push_str(&format!(
                "\n{k}:\n  expected only: {only_a:?}\n  actual only:   {only_b:?}"
            ));
        }
    }
    out
}

// ------------------------------------------------------------------ the run

fn open(path: &Path, clock_ms: i64) -> TestApp {
    TestApp::with_clock(
        Arc::new(SqliteStore::open(path).unwrap()),
        Arc::new(TickClock::new(clock_ms)),
    )
}

fn run_once(seed: u64, steps: usize) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let mut clock = 1_767_225_600_000i64;
    let mut t = open(&path, clock);
    let gens = generators();
    let names: Vec<&str> = gens.keys().copied().collect();
    // The fresh collection; the setup below is undone too.
    let start = user_data(&*t.store);
    let mut done = 0usize;
    // A small world to start from.
    let base = t
        .app.invoke("v1.decks.create",
            json!({"path": "Base", "parent_id": null, "description": null, "icon": null, "color": null}),
            &Actor::User,
        )
        .unwrap();
    let base_id = base["id"].clone();
    for i in 0..4 {
        t.app
            .invoke(
                "v1.notes.create",
                json!({"notetype_id": CLASSIC_ID, "deck_id": base_id,
                       "fields": {"front": format!("start{i}"), "back": "x"},
                       "tags": ["fiil"], "attributes": [], "allow_duplicate": true}),
                &Actor::User,
            )
            .unwrap();
        done += 1;
    }
    let mut rng = Rng(seed);
    let mut log: Vec<String> = Vec::new();
    let restart_do = steps / 2;
    let steps = steps + done;
    let mut attempts = 0usize;
    while done < steps && attempts < steps * 20 {
        attempts += 1;
        let name = *rng.pick(&names).unwrap();
        let pool = Pool::read(&t);
        let Some(req) = gens[name](&pool, &mut rng) else {
            continue;
        };
        if req.is_null() {
            continue;
        }
        let before = user_data(&*t.store);
        match t
            .app
            .invoke(&format!("v1.{name}"), req.clone(), &Actor::User)
        {
            Ok(_) => {
                done += 1;
                log.push(format!("{name} {req}"));
            }
            Err(e) => {
                // A refused command writes nothing.
                let after = user_data(&*t.store);
                assert!(
                    after == before,
                    "seed {seed}: refused {name} {req} ({e:?}) changed data:{}\nsteps: {log:#?}",
                    diff(&before, &after)
                );
            }
        }
        if done == restart_do && done > 0 {
            clock += 3_600_000;
            drop(t);
            t = open(&path, clock);
            log.push("-- restart --".into());
        }
    }
    let end = user_data(&*t.store);
    // Undo everything (restart half way).
    let mut undone = 0usize;
    loop {
        match t.app.invoke("v1.oplog.undo", json!({}), &Actor::User) {
            Ok(_) => undone += 1,
            Err(e) if e.code == ErrorCode::ConflictNothingToUndo => break,
            Err(e) => panic!("seed {seed}: undo #{undone} failed: {e:?}\nsteps: {log:#?}"),
        }
        if undone == done / 2 {
            clock += 3_600_000;
            drop(t);
            t = open(&path, clock);
        }
        assert!(undone <= done + 10, "seed {seed}: undo does not end");
    }
    let back = user_data(&*t.store);
    assert!(
        back == start,
        "seed {seed}: after undoing everything the data differs from the start:{}\nsteps: {log:#?}",
        diff(&start, &back)
    );
    // Redo everything (restart half way).
    let mut redone = 0usize;
    loop {
        match t.app.invoke("v1.oplog.redo", json!({}), &Actor::User) {
            Ok(_) => redone += 1,
            Err(e) if e.code == ErrorCode::ConflictNothingToRedo => break,
            Err(e) => panic!("seed {seed}: redo #{redone} failed: {e:?}\nsteps: {log:#?}"),
        }
        if redone == undone / 2 {
            clock += 3_600_000;
            drop(t);
            t = open(&path, clock);
        }
        assert!(redone <= undone, "seed {seed}: redo does not end");
    }
    assert_eq!(redone, undone, "seed {seed}");
    let again = user_data(&*t.store);
    assert!(
        again == end,
        "seed {seed}: after redoing everything the data differs from the end:{}\nsteps: {log:#?}",
        diff(&end, &again)
    );
}

#[test]
fn random_command_sequences_undo_and_redo_exactly_across_restarts() {
    let seed = env_u64("FC_FUZZ_SEED").unwrap_or(SEED);
    let runs = env_u64("FC_FUZZ_CASES").unwrap_or(3);
    for i in 0..runs {
        run_once(seed.wrapping_add(i), 40);
    }
}

fn call(t: &TestApp, name: &str, req: Value) -> Value {
    t.app
        .invoke(&format!("v1.{name}"), req.clone(), &Actor::User)
        .unwrap_or_else(|e| panic!("{name} {req}: {e:?}"))
}

/// Regression, minimal case of a sequence the random run found (seed 5202516348863123749):
/// renaming a coloured tag made a coloured row for the new name that undo did not remove.
#[test]
fn undoing_a_merge_of_a_renamed_coloured_tag_restores_the_start() {
    let t = TestApp::new(Arc::new(crate::testing::TempSqlite::new()));
    let start = user_data(&*t.store);
    let d = call(
        &t,
        "decks.create",
        json!({"path": "B", "parent_id": null,
        "description": null, "icon": null, "color": null}),
    )["id"]
        .clone();
    let note = |tag: &str| {
        call(
            &t,
            "notes.create",
            json!({"notetype_id": CLASSIC_ID, "deck_id": d,
            "fields": {"front": tag, "back": "x"}, "tags": [tag], "attributes": [],
            "allow_duplicate": true}),
        );
    };
    note("fiil");
    call(
        &t,
        "tags.set_color",
        json!({"name": "fiil", "color": "#123456"}),
    );
    call(&t, "tags.rename", json!({"from": "fiil", "to": "isim2"}));
    note("isim");
    call(
        &t,
        "tags.merge",
        json!({"sources": ["isim2"], "into": "isim"}),
    );
    let end = user_data(&*t.store);
    while t
        .app
        .invoke("v1.oplog.undo", json!({}), &Actor::User)
        .is_ok()
    {}
    let back = user_data(&*t.store);
    assert!(back == start, "{}", diff(&start, &back));
    while t
        .app
        .invoke("v1.oplog.redo", json!({}), &Actor::User)
        .is_ok()
    {}
    let again = user_data(&*t.store);
    assert!(again == end, "{}", diff(&end, &again));
}

// ------------------------------------------------------------------ imports (brief item 1)

/// Runs a file import job to its end.
fn import_file(t: &TestApp, path: &Path, format: Option<&str>) -> fc_api::v1::jobs::JobDto {
    let job = call(
        t,
        "io.import.start",
        json!({"source": {"kind": "file", "path": path.to_string_lossy()},
               "format": format, "deck_id": null, "notetype_id": null, "columns": null,
               "text_options": null, "skip_first_row": null, "duplicates": null,
               "create_missing_options": null, "include_history": null}),
    );
    t.app.run_pending_jobs().unwrap();
    serde_json::from_value(call(t, "jobs.get", json!({"id": job["id"]}))).unwrap()
}

/// Hostile Anki packages and broken text through the real import job: it either succeeds
/// (one undo step that gives back the collection) or fails with a code the app translates
/// (`errors.validation.import_format` …), and then nothing of it is left.
#[test]
fn hostile_imports_finish_cleanly_or_leave_nothing() {
    use fc_api::v1::jobs::JobStatus;
    let t = TestApp::new(Arc::new(crate::testing::TempSqlite::new()));
    let dir = tempfile::tempdir().unwrap();
    let mut files: Vec<(String, Vec<u8>, Option<&str>)> = fc_interop::anki::hostile::cases()
        .into_iter()
        .map(|c| (c.name.to_owned(), c.bytes, Some("anki")))
        .collect();
    files.push((
        "csv: quotes, BOMs, mixed line ends".into(),
        "\u{feff}a,\"b\r\nc\"\r\n\"unclosed,x\rq,\"\"\"\n\u{feff}k,v\n".into(),
        Some("text"),
    ));
    files.push((
        "csv: a million columns".into(),
        "a\t".repeat(1_000_000).into_bytes(),
        Some("text"),
    ));
    files.push((
        "csv: invalid UTF-8".into(),
        b"\xff\xfe\x00a,b\n\xc3(,x\n".to_vec(),
        Some("text"),
    ));
    files.push((
        "json: wrong shape".into(),
        b"{\"notes\": 5}".to_vec(),
        Some("json"),
    ));
    for (i, (name, bytes, format)) in files.iter().enumerate() {
        let path = dir.path().join(format!("in{i}.bin"));
        std::fs::write(&path, bytes).unwrap();
        let before = user_data(&*t.store);
        let started = std::time::Instant::now();
        let job = import_file(&t, &path, *format);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "{name}: slow"
        );
        match job.status {
            JobStatus::Succeeded => {
                // An import that added nothing (all duplicates) is no undo step.
                if let Err(e) = t.app.invoke("v1.oplog.undo", json!({}), &Actor::User) {
                    assert_eq!(e.code, ErrorCode::ConflictNothingToUndo, "{name}");
                }
                let back = user_data(&*t.store);
                assert!(back == before, "{name}: undo left:{}", diff(&before, &back));
            }
            JobStatus::Failed => {
                let e = job.error.unwrap();
                assert!(
                    matches!(
                        e.code,
                        ErrorCode::ValidationImportFormat
                            | ErrorCode::ConflictUnsupportedVersion
                            | ErrorCode::ValidationTextOptions
                    ),
                    "{name}: {e:?}"
                );
                assert!(!e.message.trim().is_empty(), "{name}");
                let after = user_data(&*t.store);
                assert!(
                    after == before,
                    "{name}: a failed import left:{}",
                    diff(&before, &after)
                );
            }
            other => panic!("{name}: {other:?}"),
        }
    }
}
