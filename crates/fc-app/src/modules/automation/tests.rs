//! Service tests of the `automation` module (v0.9), each against the in-memory store and SQLite
//! (in memory and on a temp file).
//!
//! v0.9 acceptance — "an automation rule does not loop (depth limit test)":
//! [`automation_rule_does_not_loop`] (a rule whose action re-triggers itself runs once; a chain
//! of distinct rules stops at `automation.max_depth`; two rules re-triggering each other end by
//! cycle detection even with the deepest limit; `automation.max_runs_per_event` caps one
//! change). The rest of the scope: [`classified_note_rule_undoes_with_the_note`] (condition
//! with a classification → move to deck + tag; one undo step with the note, actor
//! `automation`), [`answer_and_its_rules_are_one_undo_step`],
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
    classified_note_rule_undoes_with_the_note,
    flag_and_answer_triggers,
    automation_rule_does_not_loop,
    preview_is_a_dry_run,
    module_switch_stops_rules,
    finished_import_triggers_rules,
    scheduled_rules_run_when_due,
    manual_runs_and_failures_are_logged,
    actions_are_undo_safe,
    automation_actor_reaches_only_its_actions,
    plugins_and_mcp_cannot_manage_rules,
    only_the_users_clients_manage_rules,
    answer_and_its_rules_are_one_undo_step,
    follow_continues_only_the_newest_batch,
    every_due_schedule_runs_with_its_own_budget,
    schedule_survives_the_clock_going_back,
    event_rules_search_only_the_touched_cards,
    mode_answers_trigger_answer_rules,
    inbox_rules_accept_classified_proposals,
    inbox_actions_fit_inbox_triggers_only,
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

fn classified_note_rule_undoes_with_the_note(f: Factory) {
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

    // The rule run continues the note's batch, its entries recorded with the automation actor.
    let top = w.newest(3);
    let actor = Actor::Automation(rule.id.to_string());
    assert_eq!(top[0].actor, actor);
    assert_eq!(top[1].actor, actor);
    assert_eq!(top[0].batch_id, top[1].batch_id);
    assert_eq!(top[2].actor, Actor::User);
    assert_eq!(top[2].command, "v1.notes.create");
    assert_eq!(top[2].batch_id, top[0].batch_id, "one undo step");
    let runs = w.runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        (runs[0].status.as_str(), runs[0].trigger.as_str(), runs[0].matched, runs[0].depth),
        ("applied", "note_added", 1, 0)
    );
    assert_eq!(runs[0].batch_id, Some(top[0].batch_id));

    // One undo takes back the note and what its rule did; undoing triggers no rule.
    w.undo();
    assert_eq!(snapshot(&w.t), before);
    assert_eq!(w.runs().len(), 1);
    // One redo replays both exactly (stored inverses are replayable) and runs no rule.
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
        modes: None,
        scope: None,
    });
    let first = s.current.unwrap().card_id;
    let s = w.t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: first,
        rating: Rating::Good,
        duration_ms: 1000,
        reveal_ms: None,
        typed_answer: None,
    });
    assert!(w.ids("is:suspended").is_empty(), "good is not again");
    let second = s.current.unwrap().card_id;
    w.t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: second,
        rating: Rating::Again,
        duration_ms: 1000,
        reveal_ms: None,
        typed_answer: None,
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
    // The import and its rule are one undo step.
    w.undo();
    assert!(w.ids("tag:imported-a1").is_empty());
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

fn actions_are_undo_safe(f: Factory) {
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
}

/// The automation actor's part of the actor matrix (`phase3_plugins_tests` holds the others).
fn automation_actor_reaches_only_its_actions(f: Factory) {
    let w = world(f);
    let reg = w.t.app.registry();
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

// ------------------------------------------------------------------ review fixes

fn start_session(w: &W) -> SessionDto {
    w.t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Normal,
        mode: Some(StudyMode::Scheduled),
        deck_id: Some(w.english),
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
        scope: None,
    })
}

fn answer(w: &W, s: &SessionDto, rating: Rating) -> (CardId, SessionDto) {
    let card = s.current.as_ref().unwrap().card_id;
    let next = w.t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: card,
        rating,
        duration_ms: 1000,
        reveal_ms: None,
        typed_answer: None,
    });
    (card, next)
}

/// Undo in the study screen (`oplog.undo`) takes back the answer **and** what its rules did,
/// in one step; redo replays both.
fn answer_and_its_rules_are_one_undo_step(f: Factory) {
    let w = world(f);
    let leech = w.rule(
        "Leech",
        RuleTrigger::CardAnswered {
            ratings: vec![Rating::Again],
        },
        "",
        &[("bulk.set_suspended", json!({ "suspended": true })), add_tag("leech")],
    );
    w.note("one", None, &[]);
    w.note("two", None, &[]);
    let s = start_session(&w);
    let before = snapshot(&w.t);
    let (card, _) = answer(&w, &s, Rating::Again);
    assert_eq!(w.ids("is:suspended tag:leech"), vec![card]);
    // One batch: the answer (user) then the rule's actions (automation).
    let top = w.newest(3);
    assert!(top.iter().all(|e| e.batch_id == top[0].batch_id));
    assert_eq!(top[2].command, "v1.study.answer");
    assert_eq!(top[0].actor, Actor::Automation(leech.id.to_string()));
    assert_eq!(w.runs()[0].batch_id, Some(top[0].batch_id));
    let status = w.t.ok::<OplogStatus>(Empty {});
    assert_eq!(
        status.undo_summary.map(|s| s.key),
        Some("oplog.summary.study.answer".to_owned())
    );

    w.undo();
    assert_eq!(snapshot(&w.t), before, "answer and rule undone together");
    w.redo();
    assert_eq!(w.ids("is:suspended tag:leech"), vec![card]);
    assert_eq!(w.runs().len(), 1, "undo/redo run no rule");
    w.undo();
    assert_eq!(snapshot(&w.t), before);
}

/// [`OplogMode::Follow`] continues a batch only while it is the newest; otherwise the run is
/// its own undo step (another thread committed in between).
fn follow_continues_only_the_newest_batch(f: Factory) {
    use crate::app::OplogMode;
    let w = world(f);
    let create = |path: &str| fc_api::InvokeRequest {
        method: format!("v1.{}", DecksCreate::NAME),
        params: serde_json::to_value(CreateDeck {
            path: path.into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
        .unwrap(),
    };
    let before = snapshot(&w.t);
    deck(&w.t, "A");
    let a = w.newest(1)[0].batch_id;
    w.t.app
        .invoke_with(&create("B"), &Actor::User, OplogMode::Follow(a))
        .unwrap();
    let top = w.newest(2);
    assert_eq!((top[0].batch_id, top[1].batch_id), (a, a));
    let after_ab = snapshot(&w.t);
    deck(&w.t, "C");
    w.t.app
        .invoke_with(&create("D"), &Actor::User, OplogMode::Follow(a))
        .unwrap();
    let d = w.newest(1)[0].batch_id;
    assert_ne!(d, a, "C came in between: D is its own batch");
    w.undo(); // D
    w.undo(); // C
    assert_eq!(snapshot(&w.t), after_ab);
    w.undo(); // A + B, one step
    assert_eq!(snapshot(&w.t), before);
}

/// Review fix: the loop-guard budget is per source job — however many schedules are due, each
/// runs — and a schedule is marked in its run's own transaction.
fn every_due_schedule_runs_with_its_own_budget(f: Factory) {
    let w = world(f);
    w.set(MAX_RUNS, json!(1));
    let n = w.note("x", None, &[]);
    let rules: Vec<_> = (0..5)
        .map(|i| {
            w.rule(
                &format!("S{i}"),
                RuleTrigger::Schedule { every_mins: 60 },
                "",
                &[add_tag(&format!("s{i}"))],
            )
            .id
        })
        .collect();
    let runs = super::run_due(&w.t.app).unwrap();
    let got: Vec<_> = runs.iter().map(|r| (r.rule_id, r.status.as_str())).collect();
    let want: Vec<_> = rules.iter().map(|id| (*id, "applied")).collect();
    assert_eq!(got, want);
    assert_eq!(w.tags(n.id), ["s0", "s1", "s2", "s3", "s4"]);
    // Each rule's last run is the time its run was logged (same transaction).
    let logged: BTreeMap<_, _> = w.runs().into_iter().map(|r| (r.rule_id, r.at)).collect();
    for r in w.t.ok::<AutomationRulesList>(Empty {}).rules {
        assert_eq!(r.last_run_at, logged.get(&r.id).copied(), "{}", r.name);
    }
    assert!(super::run_due(&w.t.app).unwrap().is_empty());
    // Scheduled runs are their own undo steps (nothing to continue).
    w.undo();
    assert_eq!(w.tags(n.id), ["s0", "s1", "s2", "s3"]);
}

/// Review fix: a last run in the future (the clock went back) resets to now instead of
/// stopping the schedule until the clock catches up.
fn schedule_survives_the_clock_going_back(f: Factory) {
    let w = world(f);
    let n = w.note("x", None, &[]);
    let rule = w.rule(
        "Hourly",
        RuleTrigger::Schedule { every_mins: 60 },
        "",
        &[add_tag("h")],
    );
    let set_last = |t: Timestamp| {
        let mut tx = w.t.store.write().unwrap();
        tx.set_automation_rule_last_run(rule.id, t).unwrap();
        tx.commit().unwrap();
    };
    let last = || {
        w.t.ok::<AutomationRulesList>(Empty {}).rules[0]
            .last_run_at
            .unwrap()
    };
    let future = Timestamp(w.t.app.now().0 + 86_400_000);
    set_last(future);
    assert!(super::run_due(&w.t.app).unwrap().is_empty(), "not run on a clock change");
    let reset = last();
    assert!(reset.0 < future.0 && reset.0 <= w.t.app.now().0, "reset to now");
    assert!(super::run_due(&w.t.app).unwrap().is_empty(), "waits one interval");
    assert!(w.tags(n.id).is_empty());
    // An interval after the reset it runs again.
    set_last(Timestamp(reset.0 - 3_600_000));
    assert_eq!(super::run_due(&w.t.app).unwrap().len(), 1);
    assert_eq!(w.tags(n.id), ["h"]);
}

/// Review fix: rules are managed by the user's own clients only, whatever permissions another
/// actor holds — checked in the handlers, past the registry's permission check.
fn only_the_users_clients_manage_rules(f: Factory) {
    let w = world(f);
    let rule = w.rule("R", RuleTrigger::NoteAdded, "", &[add_tag("x")]);
    let draft = json!({
        "name": "evil",
        "trigger": { "kind": "note_added" },
        "actions": [{ "method": "bulk.add_tags", "params": { "tags": ["x"] } }]
    });
    let calls = [
        (AutomationRulesCreate::NAME, draft),
        (AutomationRulesUpdate::NAME, json!({ "id": rule.id, "name": "y" })),
        (AutomationRulesDelete::NAME, json!({ "id": rule.id })),
        (AutomationRulesReorder::NAME, json!({ "ids": [rule.id] })),
        (AutomationRulesRun::NAME, json!({ "id": rule.id })),
        (AutomationPreview::NAME, json!({ "rule_id": rule.id })),
    ];
    let before = snapshot(&w.t);
    for actor in [
        Actor::LocalApi("l".into()),
        Actor::Plugin("p".into()),
        Actor::Mcp("m".into()),
        Actor::Automation("a".into()),
    ] {
        for (m, p) in &calls {
            // Trusted in-process call: no permission check, the handler still refuses.
            let e = w
                .t
                .app
                .invoke_trusted(
                    &fc_api::InvokeRequest {
                        method: format!("v1.{m}"),
                        params: p.clone(),
                    },
                    &actor,
                )
                .unwrap_err();
            assert_eq!(e.code, ErrorCode::PermissionDenied, "{actor:?} {m}");
            assert_eq!(e.details.get("reason"), Some(&json!("actor")), "{actor:?} {m}");
        }
    }
    assert_eq!(snapshot(&w.t), before);
    assert!(w.runs().is_empty());
    for actor in [Actor::User, Actor::Cli, Actor::System] {
        let r = w.t.app.invoke(
            &format!("v1.{}", AutomationPreview::NAME),
            json!({ "rule_id": rule.id }),
            &actor,
        );
        assert!(r.is_ok(), "{actor:?}: {r:?}");
    }
    // The preview is a write-permission command (it takes the writer), still a dry run.
    let d = w.t.app.registry().descriptor(AutomationPreview::NAME).unwrap();
    assert_eq!((d.kind, d.permission), (MethodKind::Command, "automation:write"));
    assert!(!d.undoable);
    assert_eq!(snapshot(&w.t), before);
}

/// Review fix: an event rule looks only at the cards the change touched; rules sharing a
/// condition share one search until one of them writes.
fn event_rules_search_only_the_touched_cards(f: Factory) {
    let w = world(f);
    for i in 0..20 {
        w.note(&format!("n{i}"), None, &[]);
    }
    w.rule("A", RuleTrigger::NoteUpdated, "", &[add_tag("a")]);
    w.rule("B", RuleTrigger::NoteUpdated, "", &[add_tag("b")]);
    w.rule("C", RuleTrigger::NoteUpdated, "tag:a", &[add_tag("c")]);
    let n = w.note("target", None, &[]);
    w.edit(n.id, "x");
    // B shares A's condition, C sees what A wrote in the same dispatch.
    assert_eq!(w.tags(n.id), ["a", "b", "c", "x"]);
    let top: Vec<(u32, u32, String)> = w
        .runs()
        .into_iter()
        .rev()
        .filter(|r| r.depth == 0)
        .map(|r| (r.depth, r.matched, r.status))
        .collect();
    assert_eq!(top, vec![(0, 1, "applied".to_owned()); 3], "only the edited note's card");
    assert_eq!(w.ids("tag:a").len(), 1);
}

/// Per-answer overhead of automation (review fix #1): 100 rules on "card answered", over a
/// collection built directly through the storage layer, none matching the answered cards (the
/// usual case: the cost is evaluating them). Overhead = median `study.answer` with the module
/// on − with it off. Full size: `cargo test -p fc-app --release automation_bench_100k --
/// --ignored --nocapture`; the CI variant runs with every `cargo test`.
mod bench {
    use super::*;
    use fc_core::card::CardState;
    use fc_core::note::searchable_text;
    use std::time::Instant;

    fn build(notes: usize) -> W {
        let w = world(|| Arc::new(TempSqlite::new()));
        let decks: Vec<DeckId> = (0..10).map(|i| deck(&w.t, &format!("İngilizce::D{i}"))).collect();
        let first = w.note("seed", None, &[]);
        let mut tx = w.t.store.write().unwrap();
        let card0 = tx.get_card(w.card(&first)).unwrap().unwrap();
        let note0 = tx.get_note(first.id).unwrap().unwrap();
        let nt = tx.get_notetype(note0.notetype_id).unwrap().unwrap();
        let t0 = w.t.app.now().0;
        for i in 1..notes as u128 {
            let mut note = note0.clone();
            note.id = NoteId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0002_0000_0000 | i));
            note.fields.insert(
                fc_core::notetype::FieldKey::new(builtin::FRONT),
                format!("kelime {i}"),
            );
            note.checksum = format!("{i:016x}");
            let mut card = card0.clone();
            card.id = CardId(note.id.0);
            card.note_id = note.id;
            card.deck_id = decks[i as usize % decks.len()];
            note.home_deck_id = card.deck_id;
            let s = &mut card.scheduling;
            s.state = CardState::Review;
            s.stability = Some(20.0);
            s.difficulty = Some(5.0);
            s.last_review = Some(Timestamp(t0 - 30 * 86_400_000));
            s.due = Some(Timestamp(t0 - 86_400_000 - i as i64));
            s.reps = 5;
            tx.insert_note(&note, &searchable_text(&nt, &note.fields)).unwrap();
            tx.insert_card(&card).unwrap();
        }
        tx.commit().unwrap();
        // 100 rules, 50 distinct conditions (pairs share one), none matching (lapses 0).
        for i in 0..100 {
            let condition = match (i / 2) % 5 {
                0 => format!("deck:\"İngilizce::D{}\" lapses>={}", i % 10, 3 + i % 7),
                1 => format!("lapses>{} stability<{}", 4 + i % 5, 100 + i),
                2 => format!("deck:\"İngilizce::D{}\" seviye:C2", i % 10),
                3 => format!("tag:missing{} or lapses>={}", i / 2, 9 + i % 3),
                _ => format!("\"kelime {}\" lapses>=2", 500_000 + i / 2),
            };
            w.rule(
                &format!("R{i}"),
                RuleTrigger::CardAnswered { ratings: vec![] },
                &condition,
                &[add_tag("matched")],
            );
        }
        w
    }

    fn median(mut v: Vec<f64>) -> f64 {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    }

    /// Milliseconds per answer, median of `n`.
    fn answers(w: &W, n: usize) -> f64 {
        let mut s = start_session(w);
        let mut times = Vec::with_capacity(n);
        for _ in 0..n {
            let t = Instant::now();
            let (_, next) = answer(w, &s, Rating::Good);
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            s = next;
        }
        median(times)
    }

    fn set_module(w: &W, on: bool) {
        w.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: super::super::ID.into(),
            enabled: on,
        });
    }

    /// `(with rules, without, overhead)` in ms.
    fn run(notes: usize, n: usize) -> (f64, f64, f64) {
        let setup = Instant::now();
        let w = build(notes);
        let setup = setup.elapsed().as_secs_f64();
        answers(&w, 3); // warm up
        let on = answers(&w, n);
        set_module(&w, false);
        let off = answers(&w, n);
        set_module(&w, true);
        assert!(w.ids("tag:matched").is_empty(), "no rule matched");
        assert!(w.runs().is_empty());
        let overhead = (on - off).max(0.0);
        println!(
            "automation bench: 100 rules × {notes} notes (setup {setup:.1} s): \
             answer {on:.2} ms with rules, {off:.2} ms without → overhead {overhead:.2} ms"
        );
        (on, off, overhead)
    }

    /// Release build, desktop-class machine: < 20 ms overhead per answer.
    #[test]
    #[ignore = "benchmark: builds a 100k-note collection"]
    fn automation_bench_100k() {
        let (_, _, overhead) = run(100_000, 40);
        assert!(overhead < 20.0, "{overhead:.2} ms");
    }

    /// CI regression check (debug build, shared runners: generous budget).
    #[test]
    fn automation_bench_5k_scaled() {
        let (_, _, overhead) = run(5_000, 15);
        assert!(overhead < 150.0, "{overhead:.2} ms");
    }
}

/// Phase 4: the v0.10 modes rate through the same engine, so a confidence "1" (mapped to
/// "Again") fires a `card_answered{again}` rule — one undo step with the answer.
fn mode_answers_trigger_answer_rules(f: Factory) {
    use fc_api::v1::modes::{ConfidenceAnswer, RatingScale, StartModes};
    let w = world(f);
    w.rule(
        "Leech",
        RuleTrigger::CardAnswered {
            ratings: vec![Rating::Again],
        },
        "",
        &[("bulk.set_suspended", json!({ "suspended": true }))],
    );
    w.note("one", None, &[]);
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
        modes: Some(StartModes {
            rating_scale: Some(RatingScale::Confidence),
            ..StartModes::default()
        }),
        scope: None,
    });
    let before = snapshot(&w.t);
    let card = s.current.as_ref().unwrap().card_id;
    w.t.ok::<ModesConfidenceAnswer>(ConfidenceAnswer {
        session_id: s.id,
        card_id: card,
        level: 1,
        duration_ms: 1000,
        reveal_ms: None,
    });
    assert_eq!(w.ids("is:suspended"), vec![card]);
    let top = w.newest(2);
    assert_eq!(top[0].batch_id, top[1].batch_id);
    assert_eq!(top[1].command, "v1.modes.confidence.answer");
    w.undo();
    assert_eq!(snapshot(&w.t), before, "answer and rule undone together");
}

fn propose(w: &W, front: &str, deck: Option<DeckId>, level: &str) -> fc_api::v1::inbox::InboxItemDto {
    w.t.ok::<InboxAdd>(fc_api::v1::inbox::AddInboxItem {
        front: Some(front.into()),
        back: Some(format!("{front}?")),
        deck_id: deck,
        attributes: BTreeMap::from([("Seviye".to_owned(), json!(level))]),
        ..Default::default()
    })
}

fn pending(w: &W) -> Vec<String> {
    use fc_api::v1::inbox::ListInbox;
    w.t.ok::<InboxList>(ListInbox::default())
        .items
        .into_iter()
        .map(|i| i.item.front.unwrap_or_default())
        .collect()
}

/// Phase 4 (track J deferred it to track I's inbox): "when a B1 word lands in the inbox, add it
/// as a note" — the `inbox_added` trigger, a classification condition evaluated on the card
/// the proposal would become, the `inbox.accept` action; one undo step with the proposal;
/// the new note triggers `note_added` rules in turn.
fn inbox_rules_accept_classified_proposals(f: Factory) {
    let w = world(f);
    w.rule(
        "B1 words",
        RuleTrigger::InboxAdded,
        "seviye:B1",
        &[("inbox.accept", json!({ "deck_id": w.english }))],
    );
    w.rule("Tag new", RuleTrigger::NoteAdded, "", &[add_tag("new")]);
    let before = snapshot(&w.t);

    // A1 stays for the user to review; B1 becomes a note (classified, in the item's deck).
    propose(&w, "early", Some(w.english), "A1");
    assert_eq!(pending(&w), ["early"]);
    let b1 = propose(&w, "abandon", Some(w.english), "B1");
    assert_eq!(pending(&w), ["early"]);
    let cards = w.ids("seviye:B1 tag:new");
    assert_eq!(cards.len(), 1, "accepted as a classified note, then tagged by its rule");
    assert_eq!(w.card_deck(cards[0]), w.english);
    // The run is logged with its trigger; the chain continues the same batch.
    let runs = w.runs();
    assert!(runs
        .iter()
        .any(|r| r.trigger == "inbox_added" && r.status == "applied" && r.matched == 1));
    let top = w.newest(4);
    assert!(top.iter().all(|e| e.batch_id == top[0].batch_id));
    assert!(top.iter().any(|e| e.command == "v1.inbox.add"));
    assert!(top.iter().any(|e| e.command == "v1.inbox.accept"));
    assert!(top.iter().any(|e| matches!(&e.actor, Actor::Automation(_))));

    // One undo takes back the proposal and everything its rules did.
    w.undo();
    assert!(w.ids("seviye:B1").is_empty());
    assert_eq!(pending(&w), ["early"]);
    {
        let tx = w.t.store.read().unwrap();
        assert!(tx.get_inbox_item(b1.id).unwrap().is_none());
    }
    w.redo();
    assert_eq!(w.ids("seviye:B1 tag:new").len(), 1);
    w.undo();
    w.undo();
    assert_eq!(snapshot(&w.t), before);
    assert!(pending(&w).is_empty());

    // Without a deck the classification cannot resolve: no match, the item waits.
    propose(&w, "orphan", None, "B1");
    assert_eq!(pending(&w), ["orphan"]);
}

/// Inbox actions only for inbox triggers and vice versa; a manual run of an inbox rule
/// works through the pending items; preview counts them without writing.
fn inbox_actions_fit_inbox_triggers_only(f: Factory) {
    let w = world(f);
    let actions = |m: &str, p: Value| {
        vec![RuleAction {
            method: m.into(),
            params: p,
        }]
    };
    let bad = |trigger: RuleTrigger, a: Vec<RuleAction>| {
        let e = w.t.err::<AutomationRulesCreate>(CreateRule {
            name: "x".into(),
            enabled: None,
            trigger,
            condition: String::new(),
            actions: a,
        });
        assert_eq!(code(&e).0, ErrorCode::ValidationAutomationRule);
        assert_eq!(e.details.get("reason").and_then(Value::as_str), Some("trigger"));
    };
    bad(RuleTrigger::NoteAdded, actions("inbox.accept", json!({})));
    bad(RuleTrigger::InboxAdded, actions("bulk.add_tags", json!({ "tags": ["x"] })));
    bad(
        RuleTrigger::Schedule { every_mins: 60 },
        actions("inbox.accept", json!({})),
    );
    // A disabled inbox rule, run by hand over what is already pending.
    propose(&w, "one", Some(w.english), "B2");
    propose(&w, "two", Some(w.english), "A2");
    let rule = w.t.ok::<AutomationRulesCreate>(CreateRule {
        name: "B2".into(),
        enabled: Some(false),
        trigger: RuleTrigger::InboxAdded,
        condition: "seviye:B2".into(),
        actions: actions("inbox.accept", json!({})),
    });
    // Switching it to a card trigger would leave an inbox action behind.
    let e = w.t.err::<AutomationRulesUpdate>(UpdateRule {
        id: rule.id,
        name: None,
        enabled: None,
        trigger: Some(RuleTrigger::NoteAdded),
        condition: None,
        actions: None,
    });
    assert_eq!(e.details.get("reason").and_then(Value::as_str), Some("trigger"));
    let p = w.t.ok::<AutomationPreview>(PreviewRule {
        rule_id: Some(rule.id),
        draft: None,
    });
    assert_eq!((p.matched, p.changed), (1, 1));
    assert_eq!(pending(&w).len(), 2, "preview writes nothing");
    let r = w.t.ok::<AutomationRulesRun>(RuleRef { id: rule.id });
    assert_eq!(r.runs[0].status, "applied");
    assert_eq!(pending(&w), ["two"]);
    assert_eq!(w.ids("seviye:B2").len(), 1);
}
