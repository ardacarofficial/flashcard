//! Service tests of the `automation` module (v0.9), each against the in-memory store and SQLite
//! (in memory and on a temp file).
//!
//! v0.9 acceptance — "an automation rule does not loop (depth limit test)":
//! [`automation_rule_does_not_loop`] (a rule whose action re-triggers itself runs once; a chain
//! of distinct rules stops at `automation.max_depth`; two rules re-triggering each other end by
//! cycle detection even with the deepest limit; `automation.max_runs_per_event` caps one
//! change). The rest of the scope: [`classified_note_rule_is_its_own_undo_step`] (condition with
//! a classification → move to deck + tag; one undo step, actor `automation`),
//! [`flag_and_answer_triggers`] (sets a classification value), [`preview_is_a_dry_run`],
//! [`module_switch_stops_rules`], [`finished_import_triggers_rules`],
//! [`scheduled_rules_run_when_due`].
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{action_reachable, ACTIONS, AUTOMATION_PERMISSIONS, MAX_DEPTH, MAX_RUNS};
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::attributes::*;
use fc_api::v1::automation::*;
use fc_api::v1::cardtypes::SetCardFlag;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::*;
use fc_api::v1::io::{ImportSource, StartImport};
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::*;
use fc_api::v1::oplog::{Actor, ListOplog, OplogEntryDto, OplogKind};
use fc_api::v1::search::QueryText;
use fc_api::v1::settings::SetSetting;
use fc_api::v1::study::*;
use fc_api::{ApiMethod, ErrorCode, MethodKind, Visibility};
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::automation::RuleTrigger;
use fc_core::card::Rating;
use fc_core::notetype::builtin;
use fc_core::paging::PageRequest;
use fc_core::settings::SettingScope;
use fc_core::{CardId, DeckId, NoteId, Timestamp};
use fc_storage::{SqliteStore, Store};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

type Factory = fn() -> Arc<dyn Store>;

macro_rules! suite {
    ($($name:ident),* $(,)?) => {
        mod mem {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::MemStore::new())); } )*
        }
        mod sqlite {
            $( #[test] fn $name() {
                super::$name(|| std::sync::Arc::new(super::SqliteStore::open_in_memory().unwrap()));
            } )*
        }
        mod sqlite_file {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::TempSqlite::new())); } )*
        }
    };
}

suite!(
    rules_crud_validates_and_undoes,
    classified_note_rule_is_its_own_undo_step,
    flag_and_answer_triggers,
    automation_rule_does_not_loop,
    preview_is_a_dry_run,
    module_switch_stops_rules,
    finished_import_triggers_rules,
    scheduled_rules_run_when_due,
    manual_runs_and_failures_are_logged,
    actions_are_undo_safe_and_all_the_automation_actor_reaches,
    plugins_and_mcp_cannot_manage_rules,
);

#[test]
fn internal_methods_are_pinned() {
    let t = TestApp::mem();
    assert_eq!(
        crate::tests::internal_methods_of(&t, &[super::ID]),
        ["automation.apply", "automation.rules.restore"]
    );
    let reg = t.app.registry();
    assert!(reg.descriptor("automation.rules.restore").unwrap().replayable);
    // A rule run is never stored as an inverse: undo/redo cannot replay it.
    assert!(!reg.descriptor("automation.apply").unwrap().replayable);
}

// ------------------------------------------------------------------ fixtures

struct W {
    t: TestApp,
    /// `İngilizce` (defines "Seviye" A1–C2).
    english: DeckId,
    /// `İngilizce::Fiiller`.
    verbs: DeckId,
    /// `Tekrar`.
    review: DeckId,
    seviye: AttributeDefDto,
}

fn deck(t: &TestApp, path: &str) -> DeckId {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
    .id
}

fn world(f: Factory) -> W {
    let t = TestApp::new(f());
    let english = deck(&t, "İngilizce");
    let verbs = deck(&t, "İngilizce::Fiiller");
    let review = deck(&t, "Tekrar");
    let seviye = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(english),
        key: Some("seviye".into()),
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: ["A1", "A2", "B1", "B2", "C1", "C2"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    W {
        t,
        english,
        verbs,
        review,
        seviye,
    }
}

impl W {
    fn level(&self, label: &str) -> AttributeValue {
        AttributeValue::Choice(
            self.seviye
                .options
                .iter()
                .find(|o| o.label == label)
                .unwrap()
                .id,
        )
    }

    fn note(&self, front: &str, level: Option<&str>, tags: &[&str]) -> NoteDto {
        self.t.ok::<NotesCreate>(CreateNote {
            notetype_id: builtin::CLASSIC_ID,
            deck_id: self.verbs,
            fields: BTreeMap::from([
                ("front".to_owned(), front.to_owned()),
                ("back".to_owned(), format!("{front}?")),
            ]),
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
            attributes: level
                .map(|l| {
                    vec![AttributeAssignment {
                        attr_id: self.seviye.id,
                        value: Some(self.level(l)),
                    }]
                })
                .unwrap_or_default(),
            allow_duplicate: None,
        })
    }

    fn rule(&self, name: &str, trigger: RuleTrigger, condition: &str, actions: &[(&str, Value)]) -> RuleDto {
        self.t.ok::<AutomationRulesCreate>(CreateRule {
            name: name.into(),
            enabled: None,
            trigger,
            condition: condition.into(),
            actions: actions
                .iter()
                .map(|(m, p)| RuleAction {
                    method: (*m).into(),
                    params: p.clone(),
                })
                .collect(),
        })
    }

    fn tags(&self, note: NoteId) -> Vec<String> {
        self.t
            .ok::<NotesGet>(NoteRef { id: note })
            .tags
            .into_iter()
            .map(|t| t.as_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn edit(&self, note: NoteId, add_tag: &str) {
        let n = self.t.ok::<NotesGet>(NoteRef { id: note });
        let mut tags: Vec<String> = n.tags.iter().map(|t| t.as_str().to_owned()).collect();
        tags.push(add_tag.into());
        self.t.ok::<NotesUpdate>(UpdateNote {
            id: note,
            deck_id: None,
            fields: n.fields,
            tags,
            attributes: vec![],
        });
    }

    fn card(&self, note: &NoteDto) -> CardId {
        note.cards[0].id
    }

    fn card_deck(&self, card: CardId) -> DeckId {
        let tx = self.t.store.read().unwrap();
        tx.get_card(card).unwrap().unwrap().deck_id
    }

    fn ids(&self, query: &str) -> Vec<CardId> {
        self.t
            .ok::<SearchIdsMethod>(QueryText {
                query: query.into(),
            })
            .card_ids
    }

    fn runs(&self) -> Vec<RunDto> {
        self.t
            .ok::<AutomationRunsList>(ListRuns {
                rule_id: None,
                limit: Some(500),
            })
            .runs
    }

    fn statuses(&self) -> Vec<(String, String)> {
        let names: BTreeMap<_, _> = self
            .t
            .ok::<AutomationRulesList>(Empty {})
            .rules
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect();
        let mut v: Vec<_> = self
            .runs()
            .into_iter()
            .map(|r| (names.get(&r.rule_id).cloned().unwrap_or_default(), r.status))
            .collect();
        v.reverse();
        v
    }

    fn set(&self, key: &str, value: Value) {
        self.t.ok::<SettingsSet>(SetSetting {
            scope: SettingScope::App,
            key: key.into(),
            value,
        });
    }

    fn newest(&self, n: u32) -> Vec<OplogEntryDto> {
        self.t
            .ok::<OplogList>(ListOplog {
                page: PageRequest {
                    cursor: None,
                    limit: Some(n),
                },
            })
            .items
    }

    fn undo(&self) {
        self.t.ok::<OplogUndo>(Empty {});
    }

    fn redo(&self) {
        self.t.ok::<OplogRedo>(Empty {});
    }
}

fn add_tag(tag: &str) -> (&'static str, Value) {
    ("bulk.add_tags", json!({ "tags": [tag] }))
}

/// Everything observable (`suite::snapshot`) + every live card row + every rule row (trashed
/// too).
/// The run log is left out (a ledger).
fn snapshot(t: &TestApp) -> Value {
    let mut v = crate::testing::suite::snapshot(t);
    let tx = t.store.read().unwrap();
    let cards: Vec<String> = tx
        .list_cards(&fc_storage::repo::CardFilter::default())
        .unwrap()
        .into_iter()
        .map(|c| format!("{:?}", (c.id, c.deck_id, c.flags, c.suspended, c.deleted_at)))
        .collect();
    let rules: Vec<String> = tx
        .list_automation_rules(true)
        .unwrap()
        .into_iter()
        .map(|r| {
            format!(
                "{:?}",
                (r.id, r.name, r.enabled, r.position, r.trigger, r.condition, r.actions, r.deleted_at)
            )
        })
        .collect();
    v["cards"] = json!(cards);
    v["rules"] = json!(rules);
    v
}

fn code(e: &fc_api::ApiError) -> (ErrorCode, Option<String>) {
    (
        e.code,
        e.details
            .get("field")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
    )
}

// ------------------------------------------------------------------ rules

fn rules_crud_validates_and_undoes(f: Factory) {
    let w = world(f);
    let create = |name: &str, trigger: RuleTrigger, condition: &str, actions: Vec<RuleAction>| {
        w.t.err::<AutomationRulesCreate>(CreateRule {
            name: name.into(),
            enabled: None,
            trigger,
            condition: condition.into(),
            actions,
        })
    };
    let tag = RuleAction {
        method: "bulk.add_tags".into(),
        params: json!({ "tags": ["x"] }),
    };
    // Name, trigger parameters, action allow-list and params, condition query.
    let e = create(" ", RuleTrigger::NoteAdded, "", vec![tag.clone()]);
    assert_eq!(code(&e), (ErrorCode::ValidationAutomationRule, Some("name".into())));
    let e = create("r", RuleTrigger::Schedule { every_mins: 1 }, "", vec![tag.clone()]);
    assert_eq!(code(&e), (ErrorCode::ValidationAutomationRule, Some("every_mins".into())));
    let e = create("r", RuleTrigger::CardFlagged { flags: vec![9] }, "", vec![tag.clone()]);
    assert_eq!(code(&e), (ErrorCode::ValidationAutomationRule, Some("flags".into())));
    let e = create("r", RuleTrigger::NoteAdded, "", vec![]);
    assert_eq!(code(&e), (ErrorCode::ValidationAutomationRule, Some("actions".into())));
    for (method, params, reason) in [
        ("notes.delete", json!({}), "not_allowed"),
        ("oplog.undo", json!({}), "not_allowed"),
        ("automation.rules.create", json!({}), "not_allowed"),
        ("bulk.set_flag", json!({ "flag": 1 }), "params"),
        ("bulk.add_tags", json!({ "tags": ["x"], "note_ids": [] }), "params"),
        ("bulk.add_tags", json!(["x"]), "params"),
    ] {
        let e = create(
            "r",
            RuleTrigger::NoteAdded,
            "",
            vec![RuleAction {
                method: method.into(),
                params,
            }],
        );
        assert_eq!(e.code, ErrorCode::ValidationAutomationRule, "{method}");
        assert_eq!(e.details["reason"], reason, "{method}");
    }
    let e = create("r", RuleTrigger::NoteAdded, "nosuchkey:1", vec![tag.clone()]);
    assert_eq!(e.code, ErrorCode::ValidationQuery);
    assert!(w.t.ok::<AutomationRulesList>(Empty {}).rules.is_empty());

    // Create: the condition is shown with current names, stored canonically.
    let a = w.rule("Leech", RuleTrigger::NoteAdded, "seviye:a1", &[add_tag("x")]);
    assert_eq!(a.condition, "seviye:A1");
    assert!(a.enabled);
    {
        let tx = w.t.store.read().unwrap();
        let stored = tx.get_automation_rule(a.id).unwrap().unwrap().condition;
        assert!(stored.starts_with("attr_"), "{stored}");
    }
    // Renaming the option keeps the rule working (and shown with the new name).
    let a1 = w.seviye.options[0].id;
    w.t.ok::<AttributesOptionRename>(RenameAttributeOption {
        attr_id: w.seviye.id,
        option_id: a1,
        label: "Başlangıç".into(),
    });
    let list = w.t.ok::<AutomationRulesList>(Empty {}).rules;
    assert_eq!(list[0].condition, "seviye:Başlangıç");
    w.undo();

    let b = w.rule("Second", RuleTrigger::Imported, "", &[add_tag("y")]);
    assert_eq!((a.position, b.position), (0, 1));
    let before = snapshot(&w.t);

    // Update, reorder, delete — each one undo step, undone exactly and redone.
    let u = w.t.ok::<AutomationRulesUpdate>(UpdateRule {
        id: a.id,
        name: Some("Renamed".into()),
        enabled: Some(false),
        trigger: Some(RuleTrigger::CardAnswered {
            ratings: vec![Rating::Again],
        }),
        condition: Some("lapses>=8".into()),
        actions: None,
    });
    assert_eq!((u.name.as_str(), u.enabled), ("Renamed", false));
    let order = w.t.ok::<AutomationRulesReorder>(ReorderRules {
        ids: vec![b.id, a.id],
    });
    assert_eq!(order.rules[0].id, b.id);
    let e = w.t.err::<AutomationRulesReorder>(ReorderRules { ids: vec![b.id] });
    assert_eq!(code(&e), (ErrorCode::ValidationAutomationRule, Some("ids".into())));
    w.t.ok::<AutomationRulesDelete>(RuleRef { id: a.id });
    assert_eq!(w.t.ok::<AutomationRulesList>(Empty {}).rules.len(), 1);
    let e = w.t.err::<AutomationRulesUpdate>(UpdateRule {
        id: a.id,
        name: Some("x".into()),
        enabled: None,
        trigger: None,
        condition: None,
        actions: None,
    });
    assert_eq!(e.code, ErrorCode::NotFoundAutomationRule);
    let after = snapshot(&w.t);
    for _ in 0..3 {
        w.undo();
    }
    assert_eq!(snapshot(&w.t), before);
    for _ in 0..3 {
        w.redo();
    }
    assert_eq!(snapshot(&w.t), after);
    // Undoing the creates removes the rows.
    for _ in 0..3 {
        w.undo();
    }
    w.undo();
    w.undo();
    assert!(w.t.ok::<AutomationRulesList>(Empty {}).rules.is_empty());
}

fn classified_note_rule_is_its_own_undo_step(f: Factory) {
    let w = world(f);
    // "seviye:A1 → move to Tekrar and tag".
    let rule = w.rule(
        "A1 review",
        RuleTrigger::NoteAdded,
        "seviye:A1",
        &[
            ("bulk.move_cards", json!({ "deck_id": w.review })),
            add_tag("a1"),
        ],
    );
    let b1 = w.note("to go", Some("B1"), &[]);
    assert_eq!(w.card_deck(w.card(&b1)), w.verbs);
    assert!(w.runs().is_empty(), "no match, no log");

    let before = snapshot(&w.t);
    let a1 = w.note("to be", Some("A1"), &["verbs"]);
    let card = w.card(&a1);
    assert_eq!(w.card_deck(card), w.review);
    assert_eq!(w.tags(a1.id), ["a1", "verbs"]);
    assert_eq!(w.card_deck(w.card(&b1)), w.verbs);

    // The rule run is its own batch, recorded with the automation actor, after the note.
    let top = w.newest(3);
    let actor = Actor::Automation(rule.id.to_string());
    assert_eq!(top[0].actor, actor);
    assert_eq!(top[1].actor, actor);
    assert_eq!(top[0].batch_id, top[1].batch_id);
    assert_eq!(top[2].actor, Actor::User);
    assert_eq!(top[2].command, "v1.notes.create");
    let runs = w.runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        (runs[0].status.as_str(), runs[0].trigger.as_str(), runs[0].matched, runs[0].depth),
        ("applied", "note_added", 1, 0)
    );
    assert_eq!(runs[0].batch_id, Some(top[0].batch_id));

    // Undo takes back the rule's step only; undoing it triggers no rule.
    w.undo();
    assert_eq!(w.card_deck(card), w.verbs);
    assert_eq!(w.tags(a1.id), ["verbs"]);
    assert_eq!(w.runs().len(), 1);
    w.undo();
    assert_eq!(snapshot(&w.t), before);
    // Redo replays both steps exactly (stored inverses are replayable).
    w.redo();
    assert_eq!(w.card_deck(card), w.verbs);
    w.redo();
    assert_eq!(w.card_deck(card), w.review);
    assert_eq!(w.tags(a1.id), ["a1", "verbs"]);
    assert_eq!(w.runs().len(), 1, "redo runs no rule");
}

fn flag_and_answer_triggers(f: Factory) {
    let w = world(f);
    // docs/02 §23: "a card flagged red → tag #düzelt" (+ set a classification value).
    w.rule(
        "Fix",
        RuleTrigger::CardFlagged { flags: vec![1] },
        "",
        &[
            add_tag("düzelt"),
            (
                "bulk.set_attribute",
                json!({ "attr_id": w.seviye.id, "value": w.level("C1") }),
            ),
        ],
    );
    // "forgotten → suspend and tag #leech".
    w.rule(
        "Leech",
        RuleTrigger::CardAnswered {
            ratings: vec![Rating::Again],
        },
        "reps>=1",
        &[("bulk.set_suspended", json!({ "suspended": true })), add_tag("leech")],
    );
    let n1 = w.note("one", Some("A1"), &[]);
    let n2 = w.note("two", Some("A1"), &[]);
    let set_flag = |card: CardId, flag: u8| {
        w.t.ok::<CardsSetFlag>(SetCardFlag {
            ids: vec![card],
            flag,
            on: true,
        });
    };
    set_flag(w.card(&n2), 2);
    assert!(w.tags(n2.id).is_empty(), "orange is not red");
    set_flag(w.card(&n1), 1);
    assert_eq!(w.tags(n1.id), ["düzelt"]);
    assert_eq!(w.ids("seviye:C1"), vec![w.card(&n1)]);
    assert_eq!(w.ids("seviye:A1"), vec![w.card(&n2)]);

    let s = w.t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Normal,
        mode: Some(StudyMode::Scheduled),
        deck_id: Some(w.english),
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(0),
    });
    let first = s.current.unwrap().card_id;
    let s = w.t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: first,
        rating: Rating::Good,
        duration_ms: 1000,
        reveal_ms: None,
    });
    assert!(w.ids("is:suspended").is_empty(), "good is not again");
    let second = s.current.unwrap().card_id;
    w.t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: second,
        rating: Rating::Again,
        duration_ms: 1000,
        reveal_ms: None,
    });
    assert_eq!(w.ids("is:suspended tag:leech"), vec![second]);
    let statuses = w.statuses();
    assert_eq!(
        statuses,
        [("Fix".to_owned(), "applied".to_owned()), ("Leech".into(), "applied".into())]
    );
}

/// **v0.9 acceptance: an automation rule does not loop (depth limit test).**
fn automation_rule_does_not_loop(f: Factory) {
    let w = world(f);

    // 1. A rule whose own action re-triggers it (tagging a note is a note edit): runs once,
    //    the re-trigger is refused as a cycle.
    let echo = w.rule("Echo", RuleTrigger::NoteUpdated, "", &[add_tag("echo")]);
    let n = w.note("loop", None, &[]);
    w.edit(n.id, "start");
    assert_eq!(w.tags(n.id), ["echo", "start"]);
    assert_eq!(
        w.statuses(),
        [("Echo".to_owned(), "applied".to_owned()), ("Echo".into(), "skipped_cycle".into())]
    );
    w.t.ok::<AutomationRulesDelete>(RuleRef { id: echo.id });

    // 2. A chain of distinct rules (a → b → c → d) stops at the depth limit.
    w.set(MAX_DEPTH, json!(2));
    w.rule("R1", RuleTrigger::NoteUpdated, "tag:a -tag:b", &[add_tag("b")]);
    w.rule("R2", RuleTrigger::NoteUpdated, "tag:b -tag:c", &[add_tag("c")]);
    let r3 = w.rule("R3", RuleTrigger::NoteUpdated, "tag:c -tag:d", &[add_tag("d")]);
    // Last in the chain runs first: each rule only matches after the previous one ran, so the
    // chain really is R1 → R2 → R3 (rules later in order see earlier rules' changes).
    let mut order: Vec<_> = w
        .t
        .ok::<AutomationRulesList>(Empty {})
        .rules
        .into_iter()
        .map(|r| r.id)
        .collect();
    order.reverse();
    assert_eq!(order[0], r3.id);
    w.t.ok::<AutomationRulesReorder>(ReorderRules { ids: order });
    let before = w.runs().len();
    let m = w.note("chain", None, &[]);
    w.edit(m.id, "a");
    assert_eq!(w.tags(m.id), ["a", "b", "c"], "R3 (depth 2) did not run");
    let runs = w.runs();
    let new: Vec<(u32, &str)> = runs[..runs.len() - before]
        .iter()
        .rev()
        .map(|r| (r.depth, r.status.as_str()))
        .collect();
    assert_eq!(new, [(0, "applied"), (1, "applied"), (2, "skipped_depth")]);
    // With room for it, the same chain completes.
    w.set(MAX_DEPTH, json!(3));
    let m2 = w.note("chain 2", None, &[]);
    w.edit(m2.id, "a");
    assert_eq!(w.tags(m2.id), ["a", "b", "c", "d"]);

    // 3. Two rules that re-trigger each other forever (red → orange → red …) stop by cycle
    //    detection even with the deepest chain allowed.
    w.set(MAX_DEPTH, json!(10));
    w.rule(
        "Red→orange",
        RuleTrigger::CardFlagged { flags: vec![1] },
        "",
        &[("bulk.set_flag", json!({ "flag": 2, "on": true }))],
    );
    w.rule(
        "Orange→red",
        RuleTrigger::CardFlagged { flags: vec![2] },
        "",
        &[
            ("bulk.set_flag", json!({ "flag": 1, "on": false })),
            ("bulk.set_flag", json!({ "flag": 1, "on": true })),
        ],
    );
    let p = w.note("ping", None, &[]);
    let before = w.runs().len();
    w.t.ok::<CardsSetFlag>(SetCardFlag {
        ids: vec![w.card(&p)],
        flag: 1,
        on: true,
    });
    let runs = w.runs();
    let new: Vec<(u32, &str)> = runs[..runs.len() - before]
        .iter()
        .rev()
        .filter(|r| r.status != "skipped_cycle")
        .map(|r| (r.depth, r.status.as_str()))
        .collect();
    assert_eq!(new, [(0, "applied"), (1, "applied")]);
    assert!(runs.iter().any(|r| r.status == "skipped_cycle"));
    assert_eq!(w.ids("flag:1 flag:2"), vec![w.card(&p)]);

    // 4. The per-event budget caps how many runs one change starts.
    w.set(MAX_RUNS, json!(2));
    for i in 0..3 {
        w.rule(&format!("Added {i}"), RuleTrigger::NoteAdded, "", &[add_tag(&format!("t{i}"))]);
    }
    let q = w.note("budget", None, &[]);
    assert_eq!(w.tags(q.id), ["t0", "t1"]);
    assert_eq!(w.runs()[0].status, "skipped_budget");
}

fn preview_is_a_dry_run(f: Factory) {
    let w = world(f);
    let a = w.note("a", Some("A1"), &["leech"]);
    let b = w.note("b", Some("A1"), &[]);
    w.note("c", Some("B2"), &[]);
    let rule = w.rule(
        "Tag A1",
        RuleTrigger::Schedule { every_mins: 60 },
        "seviye:A1",
        &[add_tag("leech"), ("bulk.move_cards", json!({ "deck_id": w.review }))],
    );
    let before = snapshot(&w.t);
    w.t.take_events();
    let p = w.t.ok::<AutomationPreview>(PreviewRule {
        rule_id: Some(rule.id),
        draft: None,
    });
    assert_eq!((p.matched, p.changed, p.truncated), (2, 2, false));
    assert_eq!(p.actions[0].response["changed"], 1, "a is tagged already");
    assert_eq!(p.actions[1].response["changed"], 2);
    let cards: Vec<(CardId, bool)> = p.cards.iter().map(|c| (c.card_id, c.changes)).collect();
    assert_eq!(cards, [(w.card(&a), true), (w.card(&b), true)]);
    assert_eq!(p.cards[0].deck, "İngilizce::Fiiller");
    assert_eq!(p.cards[0].text, "a");
    // Nothing written, no undo step, no event, no log.
    assert_eq!(snapshot(&w.t), before);
    assert!(w.t.take_events().is_empty());
    assert!(w.runs().is_empty());

    // An unsaved draft: only the tag (a already has it → only b changes).
    let p = w.t.ok::<AutomationPreview>(PreviewRule {
        rule_id: None,
        draft: Some(RuleDraft {
            trigger: RuleTrigger::NoteAdded,
            condition: "seviye:A1".into(),
            actions: vec![RuleAction {
                method: "bulk.add_tags".into(),
                params: json!({ "tags": ["leech"] }),
            }],
        }),
    });
    assert_eq!((p.matched, p.changed), (2, 1));
    assert_eq!(p.cards[0].card_id, w.card(&b));
    assert!(p.cards[0].changes && !p.cards[1].changes);
    assert_eq!(snapshot(&w.t), before);
    // Invalid drafts are refused like rules.
    let e = w.t.err::<AutomationPreview>(PreviewRule {
        rule_id: None,
        draft: Some(RuleDraft {
            trigger: RuleTrigger::NoteAdded,
            condition: String::new(),
            actions: vec![RuleAction {
                method: "notes.delete".into(),
                params: json!({}),
            }],
        }),
    });
    assert_eq!(e.code, ErrorCode::ValidationAutomationRule);
    let e = w.t.err::<AutomationPreview>(PreviewRule {
        rule_id: None,
        draft: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

fn module_switch_stops_rules(f: Factory) {
    let w = world(f);
    w.rule("Tag", RuleTrigger::NoteAdded, "", &[add_tag("auto")]);
    w.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: super::ID.into(),
        enabled: false,
    });
    let n = w.note("off", None, &[]);
    assert!(w.tags(n.id).is_empty());
    let e = w.t.err::<AutomationRulesList>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    assert!(super::run_due(&w.t.app).unwrap().is_empty());
    // Back on: rules run again (nothing was queued meanwhile).
    w.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: super::ID.into(),
        enabled: true,
    });
    assert!(w.runs().is_empty());
    let n = w.note("on", None, &[]);
    assert_eq!(w.tags(n.id), ["auto"]);
    // A rule's action whose module is off fails the run (logged), not the user's command.
    w.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "browse".into(),
        enabled: false,
    });
    let n = w.note("browse off", None, &[]);
    assert!(w.tags(n.id).is_empty());
    let last = &w.runs()[0];
    assert_eq!(last.status, "failed");
    assert_eq!(
        last.error.as_ref().map(|e| e.code),
        Some(ErrorCode::ConflictModuleDisabled)
    );
}

fn finished_import_triggers_rules(f: Factory) {
    let w = world(f);
    w.rule("Imported A1", RuleTrigger::Imported, "seviye:A1", &[add_tag("imported-a1")]);
    let added = w.rule("Added", RuleTrigger::NoteAdded, "", &[add_tag("added")]);
    let job = w.t.ok::<IoImportStart>(StartImport {
        source: ImportSource::Text {
            text: "front;back;level\nev;house;A1\nrüya;dream;B2".into(),
        },
        format: None,
        text_options: None,
        deck_id: Some(w.verbs),
        notetype_id: None,
        columns: Some(vec![
            fc_api::v1::io::ColumnTarget::Field {
                key: "front".into(),
            },
            fc_api::v1::io::ColumnTarget::Field { key: "back".into() },
            fc_api::v1::io::ColumnTarget::Attribute {
                attr_id: w.seviye.id,
            },
        ]),
        skip_first_row: Some(true),
        duplicates: None,
        create_missing_options: None,
        include_history: None,
    });
    w.t.app.run_pending_jobs().unwrap();
    let job = w.t.ok::<JobsGet>(JobRef { id: job.id });
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(w.ids("tag:imported-a1").len(), 1);
    assert_eq!(w.ids("tag:imported-a1 seviye:A1").len(), 1);
    // Imported notes are an import, not "note added" one by one.
    assert!(w.ids("tag:added").is_empty());
    let runs = w.runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].trigger, "imported");
    assert!(runs.iter().all(|r| r.rule_id != added.id));
    // The import and the rule are two undo steps (rule first).
    w.undo();
    assert!(w.ids("tag:imported-a1").is_empty());
    assert_eq!(w.ids("seviye:A1").len(), 1);
    w.undo();
    assert!(w.ids("seviye:A1").is_empty());
}

fn scheduled_rules_run_when_due(f: Factory) {
    let w = world(f);
    let n = w.note("old", Some("B1"), &[]);
    let rule = w.rule(
        "Nightly",
        RuleTrigger::Schedule { every_mins: 60 },
        "seviye:B1",
        &[add_tag("b1")],
    );
    let empty = w.rule(
        "Nothing",
        RuleTrigger::Schedule { every_mins: 60 },
        "seviye:C2",
        &[add_tag("c2")],
    );
    // Event triggers never start a schedule.
    w.edit(n.id, "x");
    assert_eq!(w.tags(n.id), ["x"]);
    // A new rule is due at once.
    let runs = super::run_due(&w.t.app).unwrap();
    let got: Vec<_> = runs.iter().map(|r| (r.rule_id, r.status.as_str())).collect();
    assert_eq!(got, [(rule.id, "applied"), (empty.id, "no_match")]);
    assert_eq!(w.tags(n.id), ["b1", "x"]);
    assert_eq!(runs[0].trigger, "schedule");
    // Not again within the interval.
    assert!(super::run_due(&w.t.app).unwrap().is_empty());
    // An hour later it is due again (back-date its last run).
    {
        let mut tx = w.t.store.write().unwrap();
        tx.set_automation_rule_last_run(rule.id, Timestamp(0)).unwrap();
        tx.commit().unwrap();
    }
    let runs = super::run_due(&w.t.app).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, "applied");
    assert_eq!(runs[0].batch_id, None, "tag present: nothing changed, no undo step");
    // Disabled rules do not run.
    w.t.ok::<AutomationRulesUpdate>(UpdateRule {
        id: rule.id,
        name: None,
        enabled: Some(false),
        trigger: None,
        condition: None,
        actions: None,
    });
    {
        let mut tx = w.t.store.write().unwrap();
        tx.set_automation_rule_last_run(rule.id, Timestamp(0)).unwrap();
        tx.commit().unwrap();
    }
    assert!(super::run_due(&w.t.app).unwrap().is_empty());
    let listed = w.t.ok::<AutomationRulesList>(Empty {}).rules;
    assert!(listed.iter().all(|r| r.last_run_at.is_some()));
}

fn manual_runs_and_failures_are_logged(f: Factory) {
    let w = world(f);
    let target = deck(&w.t, "Hedef");
    let a = w.note("a", Some("A2"), &[]);
    let rule = w.t.ok::<AutomationRulesCreate>(CreateRule {
        name: "Move A2".into(),
        enabled: Some(false),
        trigger: RuleTrigger::NoteAdded,
        condition: "seviye:A2".into(),
        actions: vec![RuleAction {
            method: "bulk.move_cards".into(),
            params: json!({ "deck_id": target }),
        }],
    });
    // Disabled: events do not start it, a manual run does (one undo step).
    w.note("b", Some("A2"), &[]);
    assert!(w.runs().is_empty());
    let r = w.t.ok::<AutomationRulesRun>(RuleRef { id: rule.id });
    assert_eq!(r.runs.len(), 1);
    assert_eq!((r.runs[0].status.as_str(), r.runs[0].matched), ("applied", 2));
    assert_eq!(r.runs[0].trigger, "manual");
    assert_eq!(w.card_deck(w.card(&a)), target);
    let top = w.newest(1);
    assert_eq!(top[0].kind, OplogKind::Do);
    assert_eq!(Some(top[0].batch_id), r.runs[0].batch_id);
    w.undo();
    assert_eq!(w.card_deck(w.card(&a)), w.verbs);

    // The target deck goes to the trash: the run fails, is logged, and changes nothing.
    w.t.ok::<DecksDelete>(DeckRef { id: target });
    let before = snapshot(&w.t);
    let r = w.t.ok::<AutomationRulesRun>(RuleRef { id: rule.id });
    assert_eq!(r.runs[0].status, "failed");
    assert!(r.runs[0].error.is_some());
    // Only the log grew.
    assert_eq!(snapshot(&w.t), before);
    let e = w.t.err::<AutomationRulesRun>(RuleRef {
        id: fc_core::AutomationRuleId(uuid::Uuid::nil()),
    });
    assert_eq!(e.code, ErrorCode::NotFoundAutomationRule);
    // Runs of one rule.
    let own = w.t.ok::<AutomationRunsList>(ListRuns {
        rule_id: Some(rule.id),
        limit: Some(1),
    });
    assert_eq!(own.runs.len(), 1);
    assert_eq!(own.runs[0].status, "failed");
}

fn actions_are_undo_safe_and_all_the_automation_actor_reaches(f: Factory) {
    let w = world(f);
    let reg = w.t.app.registry();
    for (method, _) in ACTIONS {
        let d = reg.descriptor(method).unwrap();
        assert_eq!(d.kind, MethodKind::Command, "{method}");
        assert_eq!(d.visibility, Visibility::Public, "{method}");
        assert!(d.undoable && d.replayable, "{method} must be undo-safe");
        assert!(AUTOMATION_PERMISSIONS.contains(&d.permission), "{method}");
    }
    let listed = w.t.ok::<AutomationActions>(Empty {}).actions;
    assert_eq!(listed.len(), ACTIONS.len());
    // The automation actor holds exactly its permissions…
    for d in catalog() {
        let actor = Actor::Automation("r".into());
        assert_eq!(
            reg.permits(&actor, d.permission),
            AUTOMATION_PERMISSIONS.contains(&d.permission),
            "{}",
            d.name
        );
    }
    // …and reaches nothing but the actions, whatever the call (reads included); calling one
    // with empty params gets past the access check (fails on params instead).
    let before = snapshot(&w.t);
    let actor = Actor::Automation("r".into());
    for d in catalog().iter().filter(|d| d.visibility == Visibility::Public) {
        let r = w
            .t
            .app
            .invoke(&d.qualified_name(), json!({}), &actor)
            .map(|_| ());
        let denied = matches!(&r, Err(e) if e.code == ErrorCode::PermissionDenied);
        assert_eq!(denied, !action_reachable(d.name), "{}: {r:?}", d.name);
    }
    assert_eq!(snapshot(&w.t), before);
}

fn plugins_and_mcp_cannot_manage_rules(f: Factory) {
    let w = world(f);
    let grants = ["automation:read", "automation:write", "notes:write", "cards:write"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    w.t.app.registry().set_plugin_grants("p", Some(grants));
    let create = json!({
        "name": "evil",
        "trigger": { "kind": "note_added" },
        "actions": [{ "method": "bulk.add_tags", "params": { "tags": ["x"] } }]
    });
    for actor in [
        Actor::Plugin("p".into()),
        Actor::Mcp("claude".into()),
        Actor::LocalApi("l".into()),
        Actor::Automation("a".into()),
    ] {
        for (m, p) in [
            (AutomationRulesCreate::NAME, create.clone()),
            (AutomationRulesList::NAME, json!({})),
        ] {
            let e = w
                .t
                .app
                .invoke(&format!("v1.{m}"), p, &actor)
                .unwrap_err();
            assert_eq!(e.code, ErrorCode::PermissionDenied, "{actor:?} {m}");
        }
    }
    assert!(!fc_plugins::permission::api_grantable("automation:write"));
    assert!(fc_plugins::permission::hook_target_allowed(
        "automation.rules.create",
        "notes:write"
    )
    .is_err());
}
