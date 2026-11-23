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
    /// `(id, config)` of every preset (the config `presets.update` edits).
    preset_configs: Vec<(String, Value)>,
    /// Live attribute definitions (`AttributeDefDto` JSON).
    attrs: Vec<Value>,
    deleted_attrs: Vec<String>,
    goals: Vec<String>,
    exams: Vec<String>,
    reading_items: Vec<String>,
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
            preset_configs: ids(t, "presets", true)
                .into_iter()
                .filter_map(|id| {
                    let p = t
                        .app
                        .invoke("v1.presets.get", json!({ "id": id }), &Actor::User)
                        .ok()?;
                    Some((id, p.get("config")?.clone()))
                })
                .collect(),
            presets: ids(t, "presets", true),
            attrs: t
                .app
                .invoke(
                    "v1.attributes.list",
                    json!({"scope_deck_id": null, "include_deleted": false}),
                    &Actor::User,
                )
                .ok()
                .and_then(|v| v.get("defs").and_then(Value::as_array).cloned())
                .unwrap_or_default(),
            deleted_attrs: ids(t, "attribute_defs", false),
            goals: ids(t, "goals", true),
            exams: ids(t, "exam_dates", true),
            reading_items: ids(t, "reading_items", true),
        }
    }

    /// A live attribute definition whose kind is one of `kinds`.
    fn attr<'a>(&'a self, r: &mut Rng, kinds: &[&str]) -> Option<&'a Value> {
        let fit: Vec<&Value> = self
            .attrs
            .iter()
            .filter(|a| kinds.contains(&a["kind"].as_str().unwrap_or("")))
            .collect();
        r.pick(&fit).copied()
    }
}

/// Option ids of an attribute definition (`AttributeDefDto` JSON).
fn options(attr: &Value) -> Vec<Value> {
    attr["options"]
        .as_array()
        .map(|o| o.iter().map(|o| o["id"].clone()).collect())
        .unwrap_or_default()
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
            .map(|_| json!({"fields": {"front": word(r), "back": word(r)}}))
            .collect();
        Some(
            json!({"notetype_id": CLASSIC_ID, "target": {"kind": "deck", "id": d},
                    "notes": notes, "tags": r.some(TAGS, 2), "attributes": [],
                    "allow_duplicate": true}),
        )
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
            1 => json!({"scope": {"kind": "app"}, "key": "words.known_interval_days",
                        "value": days}),
            _ => json!({"scope": {"kind": "app"}, "key": "io.auto_backup_keep",
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
    // v1.4.1 (I1): the rest of the commands with a cheap generator.
    g.insert("presets.update", |p, r| {
        let (id, config) = r.pick(&p.preset_configs)?.clone();
        let mut config = config;
        config["new_per_day"] = json!(5 + r.below(30));
        config["reviews_per_day"] = json!(50 + 50 * r.below(4));
        Some(json!({"id": id, "name": format!("P{}", r.below(100)), "config": config}))
    });
    g.insert("presets.clone", |p, r| {
        Some(json!({"id": r.pick(&p.presets)?, "name": format!("C{}", r.below(100))}))
    });
    g.insert("presets.delete", |p, r| {
        Some(json!({"id": r.pick(&p.presets)?}))
    });
    g.insert("saved_queries.update", |p, r| {
        let query = ["tag:isim", "is:new", "deck:Base"][r.below(3)];
        Some(
            json!({"id": r.pick(&p.saved_queries)?, "name": format!("U{}", r.below(100)),
                    "query": query, "icon": null,
                    "position": if r.bool() { json!(r.below(3)) } else { json!(null) }}),
        )
    });
    g.insert("settings.reset", |p, r| {
        let deck = r.pick(&p.decks)?.clone();
        Some(match r.below(3) {
            0 => json!({"scope": {"kind": "deck", "id": deck}, "key": "words.language"}),
            1 => json!({"scope": {"kind": "app"}, "key": "words.known_interval_days"}),
            _ => json!({"scope": {"kind": "app"}, "key": "io.auto_backup_keep"}),
        })
    });
    g.insert("flags.update", |_, r| {
        let color =
            [json!(null), json!("red"), json!("blue"), json!("#123456")][r.below(4)].clone();
        Some(json!({"flag": 1 + r.below(7),
                    "name": if r.bool() { json!(word(r)) } else { json!(null) },
                    "color": color}))
    });
    g.insert("shortcuts.set", |_, r| {
        let action = ["ui.palette", "browse.info", "browse.delete"][r.below(3)];
        let combo = [
            json!("Ctrl+Shift+Y"),
            json!("Ctrl+P"),
            json!("Alt+J"),
            json!(null),
        ][r.below(4)]
        .clone();
        Some(json!({"action": action, "combo": combo}))
    });
    g.insert("shortcuts.reset", |_, r| {
        let action = [json!(null), json!("ui.palette"), json!("browse.info")][r.below(3)].clone();
        Some(json!({ "action": action }))
    });
    g.insert("themes.set", |_, r| {
        let mode = ["system", "light", "dark"][r.below(3)];
        Some(json!({"mode": mode, "preset": null,
                    "accent": if r.bool() { json!("#aa5500") } else { json!(null) }}))
    });
    g.insert("goals.create", |_, r| {
        let kind = ["cards_learned", "study_minutes", "daily_cards"][r.below(3)];
        Some(json!({"kind": kind, "target": 5 + r.below(50),
                    "deadline": if r.bool() { json!("2026-12-31") } else { json!(null) },
                    "query": if r.bool() { json!("tag:fiil") } else { json!(null) }}))
    });
    g.insert("goals.update", |p, r| {
        Some(
            json!({"id": r.pick(&p.goals)?, "target": 5 + r.below(50), "deadline": null,
                    "query": null}),
        )
    });
    g.insert("goals.delete", |p, r| {
        Some(json!({"id": r.pick(&p.goals)?}))
    });
    g.insert("planning.exams.create", |p, r| {
        Some(
            json!({"name": format!("E{}", r.below(10)), "date": "2027-06-20",
                    "deck_id": if r.bool() { r.pick(&p.decks).cloned() } else { None },
                    "query": if r.bool() { json!("tag:fiil") } else { json!(null) }}),
        )
    });
    g.insert("planning.exams.update", |p, r| {
        let date = ["2027-01-10", "2027-06-21"][r.below(2)];
        Some(
            json!({"id": r.pick(&p.exams)?, "name": format!("F{}", r.below(10)),
                    "date": date, "deck_id": null,
                    "query": null}),
        )
    });
    g.insert("planning.exams.delete", |p, r| {
        Some(json!({"id": r.pick(&p.exams)?}))
    });
    g.insert("attributes.define", |p, r| {
        let kind = ["enum", "ordered_enum", "multi_enum", "number", "text"][r.below(5)];
        let options: Vec<Value> = if kind.ends_with("enum") {
            (0..1 + r.below(3))
                .map(|i| json!({"label": format!("o{i}"), "color": null}))
                .collect()
        } else {
            Vec::new()
        };
        Some(
            json!({"scope_deck_id": if r.bool() { r.pick(&p.decks).cloned() } else { None },
                    "key": null, "name": format!("A{}", r.below(20)), "kind": kind,
                    "target": null, "required": null, "options": options}),
        )
    });
    g.insert("attributes.define_from_template", |p, r| {
        let template = ["cefr", "yks"][r.below(2)];
        Some(json!({"template_id": template,
                    "scope_deck_id": if r.bool() { r.pick(&p.decks).cloned() } else { None },
                    "locale": null}))
    });
    g.insert("attributes.update", |p, r| {
        let a = r.pick(&p.attrs)?;
        Some(
            json!({"id": a["id"], "name": format!("N{}", r.below(20)), "required": false,
                    "default_value": null, "position": r.below(3)}),
        )
    });
    g.insert("attributes.delete", |p, r| {
        Some(json!({"id": r.pick(&p.attrs)?["id"]}))
    });
    g.insert("attributes.restore", |p, r| {
        Some(json!({"id": r.pick(&p.deleted_attrs)?}))
    });
    g.insert("attributes.option.add", |p, r| {
        let a = p.attr(r, &["enum", "ordered_enum", "multi_enum"])?;
        let after = r.pick(&options(a)).cloned();
        Some(json!({"attr_id": a["id"], "label": word(r), "color": null, "after": after}))
    });
    g.insert("attributes.option.set_color", |p, r| {
        let a = p.attr(r, &["enum", "ordered_enum", "multi_enum"])?;
        let o = r.pick(&options(a))?.clone();
        Some(json!({"attr_id": a["id"], "option_id": o,
                    "color": if r.bool() { json!("#00ff00") } else { json!(null) }}))
    });
    g.insert("attributes.option.rename", |p, r| {
        let a = p.attr(r, &["enum", "ordered_enum", "multi_enum"])?;
        let o = r.pick(&options(a))?.clone();
        Some(json!({"attr_id": a["id"], "option_id": o, "label": word(r)}))
    });
    g.insert("bulk.set_attribute", |p, r| {
        let a = p.attr(r, &["enum", "ordered_enum", "multi_enum", "number", "text"])?;
        let opts = options(a);
        let value = match (a["kind"].as_str().unwrap_or(""), r.below(4)) {
            (_, 0) => json!(null),
            ("number", _) => json!({"type": "number", "value": r.below(20)}),
            ("text", _) => json!({"type": "text", "value": word(r)}),
            ("multi_enum", _) => json!({"type": "choices", "value": [r.pick(&opts)?]}),
            _ => json!({"type": "choice", "value": r.pick(&opts)?}),
        };
        Some(
            json!({"card_ids": r.some(&p.cards, 3), "attr_id": a["id"], "value": value})
                .take_if(&p.cards),
        )
    });
    g.insert("reading.items.create", |_, r| {
        Some(json!({"title": word(r), "language": "en",
                    "body": "Ankara is the capital. It has many parks.", "queue": r.bool()}))
    });
    g.insert("reading.items.update", |p, r| {
        let lang = ["en", "en-GB"][r.below(2)];
        Some(json!({"id": r.pick(&p.reading_items)?, "title": word(r),
                    "language": lang}))
    });
    g.insert("reading.items.delete", |p, r| {
        Some(json!({"id": r.pick(&p.reading_items)?}))
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

/// Public undoable commands without a generator: each names the test that proves undo (and,
/// where it says so, redo) restores the state for it, as `(command, "file::fn", token)` with
/// `file` under `crates/fc-app/src`. [`named_undo_tests_exist`] checks that the function exists
/// and that its body runs the command (`token`: its method type, or the helper the test calls
/// it through) and undoes.
const NOT_GENERATED: &[(&str, &str, &str)] = &[
    ("ai.providers.create", "modules/ai/tests.rs::provider_commands_are_exactly_undoable_and_keep_one_default", "AiProvidersCreate"),
    ("ai.providers.update", "modules/ai/tests_v2.rs::changing_the_address_removes_the_key_unless_confirmed", "AiProvidersUpdate"),
    ("ai.providers.delete", "modules/ai/tests.rs::provider_commands_are_exactly_undoable_and_keep_one_default", "AiProvidersDelete"),
    ("ai.routes.set", "modules/ai/tests.rs::routes_pick_provider_and_model_per_feature", "AiRoutesSet"),
    ("ai.prices.set", "modules/ai/tests.rs::prices_budget_and_usage_summary", "AiPricesSet"),
    ("ai.prices.delete", "modules/ai/tests.rs::prices_budget_and_usage_summary", "AiPricesDelete"),
    ("ai.notes.add", "modules/ai/tests.rs::generate_cards_end_to_end_is_level_aware", "AiNotesAdd"),
    ("ai.notes.update", "modules/ai/tests_phase3.rs::ai_note_batches_are_replayable_undo_steps", "AiNotesUpdate"),
    ("ai.meaning.apply", "modules/ai/tests_meaning.rs::the_meaning_in_this_sentence_is_asked_previewed_and_applied_on_request", "AiMeaningApply"),
    ("attributes.option.reorder", "testing/suite.rs::option_restore_is_exact", "AttributesOptionReorder"),
    ("attributes.option.merge", "testing/suite.rs::attribute_option_merge_and_delete_undo", "AttributesOptionMerge"),
    ("attributes.option.delete", "testing/suite.rs::attribute_option_merge_and_delete_undo", "AttributesOptionDelete"),
    ("attributes.values.set", "testing/suite.rs::attribute_values_required_and_applicability", "AttributesValuesSet"),
    ("automation.rules.create", "modules/automation/tests.rs::rules_crud_validates_and_undoes", "AutomationRulesCreate"),
    ("automation.rules.update", "modules/automation/tests.rs::rules_crud_validates_and_undoes", "AutomationRulesUpdate"),
    ("automation.rules.delete", "modules/automation/tests.rs::rules_crud_validates_and_undoes", "AutomationRulesDelete"),
    ("automation.rules.reorder", "modules/automation/tests.rs::rules_crud_validates_and_undoes", "AutomationRulesReorder"),
    ("browse.repair_database", "modules/browse/tests.rs::database_check_and_repair_undo", "BrowseRepairDatabase"),
    ("bulk.change_notetype", "phase2_manage_tests.rs::change_notetype_keeps_history_and_undoes", "BulkChangeNotetype"),
    ("notetypes.create", "cards_tests.rs::notetype_create_delete_restore", "NotetypesCreate"),
    ("notetypes.update", "cards_tests.rs::notetype_field_add_remove_keeps_cards_and_history", "NotetypesUpdate"),
    ("notetypes.delete", "cards_tests.rs::notetype_create_delete_restore", "NotetypesDelete"),
    ("notetypes.restore", "cards_tests.rs::notetype_create_delete_restore", "NotetypesRestore"),
    ("modules.set_enabled", "testing/suite.rs::modules_toggle_and_undo", "ModulesSetEnabled"),
    ("inbox.add", "modules/inbox/tests.rs::reject_and_undo", "InboxAdd"),
    ("inbox.accept", "modules/inbox/tests.rs::classified_items_are_accepted_as_one_undo_step", "InboxAccept"),
    ("inbox.reject", "modules/inbox/tests.rs::reject_and_undo", "InboxReject"),
    ("deep_links.open", "modules/inbox/tests.rs::deep_links_navigate_or_propose_never_write_notes", "DeepLinksOpen"),
    ("mcp.servers.add", "modules/mcp/tests.rs::servers_crud_validate_and_undo", "McpServersAdd"),
    ("mcp.servers.update", "modules/mcp/tests.rs::servers_crud_validate_and_undo", "McpServersUpdate"),
    ("mcp.servers.remove", "modules/mcp/tests.rs::servers_crud_validate_and_undo", "McpServersRemove"),
    ("modes.learn.answer", "modules/modes/tests.rs::learn_mode_learns_cards_at_every_stage", "learn_step("),
    ("modes.quiz.answer", "modules/modes/quiz_tests.rs::quiz_feedback_result_and_missed_restart", "ModesQuizAnswer"),
    ("modes.confidence.answer", "modules/automation/tests.rs::mode_answers_trigger_answer_rules", "ModesConfidenceAnswer"),
    ("modes.timed.expire", "modules/modes/tests.rs::timed_mode_is_enforced_on_the_backend_clock", "ModesTimedExpire"),
    ("modes.test.submit", "modules/modes/tests.rs::test_generator_mixes_types_scores_and_never_schedules", "ModesTestSubmit"),
    ("modes.match.finish", "modules/modes/tests.rs::match_rounds_are_timed_practice_and_undoable", "ModesMatchFinish"),
    ("modes.inline.create", "modules/modes/tests.rs::inline_cards_during_study_are_one_undo_step", "ModesInlineCreate"),
    ("planning.apply_pace", "modules/stats/tests.rs::forecast_and_apply_pace", "PlanningApplyPace"),
    ("plugins.install", "modules/plugins/tests.rs::install_update_uninstall_are_undoable", "PluginsInstall"),
    ("plugins.uninstall", "modules/plugins/tests.rs::install_update_uninstall_are_undoable", "PluginsUninstall"),
    ("plugins.set_enabled", "modules/plugins/tests.rs::install_update_uninstall_are_undoable", "PluginsSetEnabled"),
    ("plugins.set_permissions", "modules/plugins/tests.rs::install_update_uninstall_are_undoable", "PluginsSetPermissions"),
    ("plugins.settings.set", "modules/plugins/tests.rs::example_hook_fills_a_field", "PluginsSettingsSet"),
    ("plugins.word_decks.set", "modules/plugins/tests.rs::words_need_their_permission_and_stay_in_the_plugins_decks", "PluginsWordDecksSet"),
    ("reading.items.set_position", "modules/reading/tests.rs::items_queue_and_extracts", "ReadingItemsSetPosition"),
    ("reading.queue.set", "modules/reading/tests.rs::items_queue_and_extracts", "ReadingQueueSet"),
    ("reading.queue.read", "modules/reading/tests.rs::items_queue_and_extracts", "ReadingQueueRead"),
    ("reading.queue.postpone", "modules/reading/tests.rs::items_queue_and_extracts", "ReadingQueuePostpone"),
    ("reading.extracts.create", "modules/reading/tests.rs::items_queue_and_extracts", "ReadingExtractsCreate"),
    ("reading.extracts.cloze", "modules/reading/tests.rs::items_queue_and_extracts", "ReadingExtractsCloze"),
    ("reading.words.mark", "modules/reading/tests.rs::turkish_words_and_marks_undo", "ReadingWordsMark"),
    ("reading.cards.make", "modules/reading/tests.rs::word_and_sentence_cards", "ReadingCardsMake"),
    ("stats.leech_repairs.apply", "modules/subtitle/examples_tests.rs::a_leech_gets_evidence_repairs_and_an_honest_report", "StatsLeechesRepair"),
    ("study.answer", "modules/study/tests.rs::answer_undo_redo_is_exact", "StudyAnswer"),
    ("subtitle.default_deck.set", "modules/subtitle/tests.rs::captures_go_to_their_deck_or_the_default_deck", "SubtitleDefaultDeckSet"),
    ("subtitle.encounters.set_primary", "modules/subtitle/encounters_tests.rs::switching_the_primary_example_leaves_the_schedule_identical", "SubtitleEncountersSetPrimary"),
    ("subtitle.encounters.move", "modules/subtitle/encounters_tests.rs::moving_and_merging_keep_the_note_with_more_history", "SubtitleEncountersMove"),
    ("subtitle.encounters.merge_notes", "modules/subtitle/encounters_tests.rs::moving_and_merging_keep_the_note_with_more_history", "SubtitleEncountersMergeNotes"),
    ("subtitle.encounters.resolve", "modules/subtitle/encounters_tests.rs::a_different_meaning_stays_separate_until_the_user_decides", "SubtitleEncountersResolve"),
];

fn not_generated(name: &str) -> bool {
    NOT_GENERATED.iter().any(|(n, _, _)| *n == name)
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
    let listed_stale: Vec<&str> = NOT_GENERATED
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| !registered.contains(n) || gens.contains_key(n))
        .collect();
    assert!(
        listed_stale.is_empty(),
        "listed as not generated but unknown, not undoable or generated: {listed_stale:?}"
    );
    assert!(gens.len() >= 60, "{}", gens.len());
}

/// The body of `fn name` in `src` (rustfmt layout: from the signature to the closing brace at
/// the signature's indentation).
fn fn_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let mut start = None;
    let mut indent = "";
    let mut offset = 0usize;
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_start();
        match start {
            None => {
                let sig = trimmed
                    .strip_prefix("pub(crate) ")
                    .or_else(|| trimmed.strip_prefix("pub(super) "))
                    .or_else(|| trimmed.strip_prefix("pub "))
                    .unwrap_or(trimmed);
                if let Some(rest) = sig.strip_prefix("fn ").and_then(|r| r.strip_prefix(name)) {
                    if rest.starts_with('(') || rest.starts_with('<') {
                        start = Some(offset);
                        indent = &line[..line.len() - trimmed.len()];
                    }
                }
            }
            Some(s) => {
                if line.trim_end() == format!("{indent}}}") {
                    return Some(&src[s..offset + line.len()]);
                }
            }
        }
        offset += line.len();
    }
    None
}

/// Every test [`NOT_GENERATED`] names exists, runs its command and undoes.
#[test]
fn named_undo_tests_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    for (name, test, token) in NOT_GENERATED {
        let (file, func) = test.split_once("::").expect("file::fn");
        let src = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("{name}: {file}: {e}"));
        match fn_body(&src, func) {
            None => bad.push(format!("{name}: no fn {func} in {file}")),
            Some(body) => {
                if !body.contains(token) {
                    bad.push(format!("{name}: {test} does not call {token}"));
                }
                if !body.to_lowercase().contains("undo") {
                    bad.push(format!("{name}: {test} does not undo"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
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

/// Per command: (accepted, refused) attempts of one run.
type Tally = BTreeMap<&'static str, (u32, u32)>;

fn run_once(seed: u64, steps: usize) -> Tally {
    let mut tally = Tally::new();
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
                tally.entry(name).or_default().0 += 1;
                log.push(format!("{name} {req}"));
            }
            Err(e) => {
                tally.entry(name).or_default().1 += 1;
                if std::env::var_os("FC_FUZZ_STATS").is_some() {
                    eprintln!("refused {name} {req}: {:?} {}", e.code, e.message);
                }
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
    tally
}

#[test]
fn random_command_sequences_undo_and_redo_exactly_across_restarts() {
    let seed = env_u64("FC_FUZZ_SEED").unwrap_or(SEED);
    let runs = env_u64("FC_FUZZ_CASES").unwrap_or(3);
    let mut total = Tally::new();
    for i in 0..runs {
        for (name, (ok, refused)) in run_once(seed.wrapping_add(i), 40) {
            let t = total.entry(name).or_default();
            t.0 += ok;
            t.1 += refused;
        }
    }
    // `FC_FUZZ_STATS=1`: how often each command ran and was refused (a generator that is
    // always refused tests nothing).
    if std::env::var_os("FC_FUZZ_STATS").is_some() {
        for (name, (ok, refused)) in &total {
            eprintln!("{name:40} ok {ok:4} refused {refused:4}");
        }
        let never: Vec<&str> = generators()
            .keys()
            .copied()
            .filter(|n| total.get(n).is_none_or(|t| t.0 == 0))
            .collect();
        eprintln!("never accepted: {never:?}");
    }
}

/// Every generator writes something somewhere: a generator whose requests are always refused
/// (a wrong field name, a scope the setting does not allow) would test nothing. A long
/// sequence without the undo checks.
#[test]
fn every_generator_is_accepted_at_least_once() {
    let t = TestApp::new(Arc::new(crate::testing::TempSqlite::new()));
    let gens = generators();
    let names: Vec<&str> = gens.keys().copied().collect();
    let mut rng = Rng(env_u64("FC_FUZZ_SEED").unwrap_or(SEED));
    let mut accepted: BTreeSet<&str> = BTreeSet::new();
    call(
        &t,
        "decks.create",
        json!({"path": "Base", "parent_id": null, "description": null, "icon": null,
               "color": null}),
    );
    for _ in 0..3000 {
        if accepted.len() == names.len() {
            break;
        }
        let name = *rng.pick(&names).unwrap();
        let pool = Pool::read(&t);
        let Some(req) = gens[name](&pool, &mut rng) else {
            continue;
        };
        if !req.is_null()
            && t.app
                .invoke(&format!("v1.{name}"), req, &Actor::User)
                .is_ok()
        {
            accepted.insert(name);
        }
    }
    let never: Vec<&str> = names
        .into_iter()
        .filter(|n| !accepted.contains(n))
        .collect();
    assert!(never.is_empty(), "generators never accepted: {never:?}");
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
