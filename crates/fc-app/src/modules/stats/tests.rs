//! Service tests of the v0.6 modules (`stats`, `planning`, `goals`, `optimizer`). Scenarios run
//! on the in-memory store, SQLite in memory and SQLite on a temp file (`v06_suite!` below).
//!
//! v0.6 acceptance criteria (docs/04):
//! * statistics are reproducible from `review_log` and the cache gives the same result →
//!   [`cache_equals_recomputation`] (answers, undo, imported and trial reviews, purge, other
//!   calendars; incremental folding on SQLite);
//! * optimization runs as a cancellable background job → [`optimizer_job_saves_weights`],
//!   [`optimizer_job_cancelled_before_it_runs`], [`optimizer_job_cancelled_while_training`]
//!   (+ `fc_study::optimize` tests: the abort flag stops training);
//! * load balancing lowers daily-load variance in a simulation → `fc_study::planning` tests;
//!   here [`load_balancing_spreads_equal_cards`] shows the study engine applies it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::events::InMemoryEventBus;
use crate::{App, AppDeps};
use fc_api::v1::attributes::{AttributeAssignment, DefineAttribute, NewAttributeOption};
use fc_api::v1::browse::PurgeTrash;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto, DeckRef};
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteIds};
use fc_api::v1::oplog::Actor;
use fc_api::v1::planning::*;
use fc_api::v1::presets::{AssignPreset, CreatePreset, PresetRef};
use fc_api::v1::settings::SetSetting;
use fc_api::v1::stats::*;
use fc_api::v1::study::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::card::{Card, CardState, Rating};
use fc_core::goal::GoalKind;
use fc_core::notetype::builtin;
use fc_core::rng::SplitMix64;
use fc_core::settings::SettingScope;
use fc_core::time::{FixedClock, MILLIS_PER_DAY};
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, ReviewLogId, Timestamp, V7IdGen};
use fc_storage::repo::ReviewLogRecord;
use fc_storage::Store;
use std::collections::BTreeMap;
use std::sync::Arc;

/// 2026-01-05T10:00Z (a Monday).
const T0: i64 = 1_767_225_600_000 + 4 * MILLIS_PER_DAY + 10 * 3_600_000;
const DAY: i64 = MILLIS_PER_DAY;

type Factory = dyn Fn() -> Arc<dyn Store>;

struct T {
    app: Arc<App>,
    store: Arc<dyn Store>,
    clock: Arc<FixedClock>,
    /// The store folds appended reviews into its cache (SQLite) instead of rebuilding.
    incremental: bool,
}

impl T {
    fn new(f: &Factory) -> Self {
        let store = f();
        let clock = Arc::new(FixedClock::new(Timestamp(T0)));
        let app = App::new(AppDeps {
            store: store.clone(),
            ids: Box::new(V7IdGen::new(clock.clone(), SplitMix64::new(6))),
            clock: clock.clone(),
            events: Arc::new(InMemoryEventBus::new()),
        })
        .unwrap();
        app.load_state().unwrap();
        // MemStore's generation is a hash of every row, so it never folds incrementally.
        let incremental = store.read().unwrap().review_log_mark().unwrap().generation == 0;
        Self {
            app: Arc::new(app),
            store,
            clock,
            incremental,
        }
    }

    fn ok<M: ApiMethod>(&self, req: M::Request) -> M::Response {
        match self.app.call::<M>(&req, &Actor::User) {
            Ok(r) => r,
            Err(e) => panic!("{} failed: {e:?}", M::NAME),
        }
    }

    fn err<M: ApiMethod>(&self, req: M::Request) -> ApiError {
        match self.app.call::<M>(&req, &Actor::User) {
            Ok(_) => panic!("{} unexpectedly succeeded", M::NAME),
            Err(e) => e,
        }
    }

    fn day(&self, d: i64) {
        self.clock.set(Timestamp(T0 + d * DAY));
    }

    fn deck(&self, path: &str) -> DeckDto {
        self.ok::<DecksCreate>(CreateDeck {
            path: path.into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
    }

    fn note(&self, deck: DeckId, front: &str, attr: Option<(AttributeDefId, AttributeOptionId)>) -> CardId {
        self.ok::<NotesCreate>(CreateNote {
            notetype_id: builtin::CLASSIC_ID,
            deck_id: deck,
            fields: BTreeMap::from([
                ("front".to_owned(), front.to_owned()),
                ("back".to_owned(), "x".to_owned()),
            ]),
            tags: vec![],
            attributes: attr
                .map(|(a, o)| {
                    vec![AttributeAssignment {
                        attr_id: a,
                        value: Some(AttributeValue::Choice(o)),
                    }]
                })
                .unwrap_or_default(),
            allow_duplicate: None,
        })
        .cards[0]
            .id
    }

    fn start(&self, mode: StudyMode, deck: Option<DeckId>) -> Option<SessionDto> {
        let req = StartSession {
            kind: StudyKind::Normal,
            mode: Some(mode),
            deck_id: deck,
            include_subdecks: None,
            filter: None,
            from_session: None,
            new_limit: Some(100),
            order: None,
            tz_offset_mins: Some(0),
            modes: None,
            scope: None,
        };
        // v1.3: nothing to study starts no session.
        match self.app.call::<StudyStart>(&req, &Actor::User) {
            Ok(s) => Some(s),
            Err(e) if e.code == fc_api::ErrorCode::ConflictNothingToStudy => None,
            Err(e) => panic!("study.start failed: {e:?}"),
        }
    }

    /// Answers every card of a session until it is empty (every `again_every`-th card is
    /// forgotten the first time it is shown), returns how many answers.
    fn study(&self, mode: StudyMode, deck: Option<DeckId>, again_every: usize) -> usize {
        let Some(mut s) = self.start(mode, deck) else {
            return 0;
        };
        let mut n = 0;
        let mut seen = std::collections::HashSet::new();
        while let Some(c) = s.current.clone() {
            n += 1;
            let first = seen.insert(c.card_id);
            let rating = if first && again_every > 0 && seen.len() % again_every == 0 {
                Rating::Again
            } else {
                Rating::Good
            };
            let shown = c.intervals.iter().find(|p| p.rating == rating).unwrap().scheduled_days;
            self.clock.advance(5_000);
            s = self.ok::<StudyAnswer>(AnswerCard {
                session_id: s.id,
                card_id: c.card_id,
                rating,
                duration_ms: 3_000 + 1_000 * u32::try_from(n % 7).unwrap(),
                reveal_ms: None,
            });
            if mode == StudyMode::Scheduled {
                // The button showed what the answer got (planning included).
                let rows = self.store.read().unwrap().stats_reviews(0, Some(&[c.card_id])).unwrap();
                assert_eq!(rows.last().unwrap().1.scheduled_days, shown, "preview = applied");
            }
            assert!(n < 1000);
        }
        self.ok::<StudyEnd>(SessionRef { session_id: s.id });
        n
    }

    fn card(&self, id: CardId) -> Card {
        self.store.read().unwrap().get_card(id).unwrap().unwrap()
    }

    fn set_card(&self, id: CardId, f: impl FnOnce(&mut Card)) {
        let mut tx = self.store.write().unwrap();
        let mut c = tx.get_card(id).unwrap().unwrap();
        f(&mut c);
        tx.update_card(&c).unwrap();
        tx.commit().unwrap();
    }

    fn add_review(&self, n: u128, card: CardId, at: i64, rating: Rating, before: CardState, mode: &str) {
        let mut tx = self.store.write().unwrap();
        tx.insert_review_log(&ReviewLogRecord {
            id: ReviewLogId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0001_0000 | n)),
            card_id: card,
            session_id: None,
            rating,
            state_before: before,
            state_after: CardState::Review,
            scheduled_days: 3,
            elapsed_days: if before == CardState::New { 0 } else { 3 },
            duration_ms: 7_000,
            answer_duration_ms: None,
            hints_used: 0,
            mode: mode.into(),
            stability_after: Some(3.0),
            difficulty_after: Some(5.0),
            created_at: Timestamp(at),
        })
        .unwrap();
        tx.commit().unwrap();
    }

    fn overview(&self, scope: StatsScope, tz: i32, fresh: bool) -> StatsOverview {
        self.ok::<StatsGetOverview>(StatsRequest {
            scope,
            tz_offset_mins: Some(tz),
            days: None,
            forecast_days: Some(14),
            fresh: Some(fresh),
        })
    }

    fn refresh(&self, tz: i32) -> StatsCacheInfo {
        self.ok::<StatsRefreshCache>(RefreshStatsCache {
            tz_offset_mins: Some(tz),
        })
    }

    fn undo(&self) {
        self.ok::<OplogUndo>(Empty {});
    }

    fn redo(&self) {
        self.ok::<OplogRedo>(Empty {});
    }

    fn total_reviews(&self) -> usize {
        self.store
            .read()
            .unwrap()
            .stats_reviews(0, None)
            .unwrap()
            .len()
    }
}

/// The cached and the recomputed overview agree on everything (`from_cache` aside); returns
/// whether the cached one used the cache.
fn agree(t: &T, tz: i32) -> bool {
    let cached = t.overview(StatsScope::default(), tz, false);
    let fresh = t.overview(StatsScope::default(), tz, true);
    assert!(!fresh.from_cache);
    assert_eq!(
        StatsOverview {
            from_cache: false,
            ..cached.clone()
        },
        fresh,
        "cache + tail must equal a recomputation from review_log"
    );
    assert_eq!(
        cached.reviews.totals.reviews as usize,
        t.total_reviews(),
        "every non-trial review row counted"
    );
    cached.from_cache
}

// ------------------------------------------------------------------ statistics

/// docs/04 v0.6: statistics are reproducible from `review_log` and the cache gives the same
/// result, through every kind of review-log change.
pub fn cache_equals_recomputation(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("Kelimeler");
    let cards: Vec<CardId> = (0..8).map(|i| t.note(d.id, &format!("w{i}"), None)).collect();
    // Nothing cached yet.
    assert!(!agree(&t, 0));
    assert!(t.study(StudyMode::Scheduled, None, 3) > 8, "learning steps answered too");
    let info = t.refresh(0);
    assert!(info.rebuilt && info.days == 1);
    assert!(agree(&t, 0), "valid cache is used");
    // Up to date: nothing to do.
    assert_eq!(t.refresh(0).folded, 0);

    // Next days: appended reviews are folded on top of the cache (SQLite).
    for day in 1..5 {
        t.day(day);
        t.study(StudyMode::Scheduled, None, 4);
        assert_eq!(agree(&t, 0), t.incremental, "day {day}");
    }
    let info = t.refresh(0);
    assert_eq!(info.rebuilt, !t.incremental);

    // Undo of an answer deletes its review row: the cache is not used until rebuilt.
    t.day(9);
    let s = t.start(StudyMode::Scheduled, None).unwrap();
    let c = s.current.clone().expect("cards due by day 9");
    t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: c.card_id,
        rating: Rating::Hard,
        duration_ms: 9_000,
        reveal_ms: None,
    });
    t.refresh(0);
    let before_undo = t.total_reviews();
    t.undo();
    assert_eq!(t.total_reviews(), before_undo - 1);
    assert!(!agree(&t, 0), "a removed row invalidates the cache");
    t.redo();
    agree(&t, 0);
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    // SQLite saw a delete (generation bumped); MemStore's content hash is back to the cached
    // one after the redo, so its cache is valid again.
    assert_eq!(t.refresh(0).rebuilt, t.incremental);
    assert!(agree(&t, 0));

    // Imported history counts in statistics (and is marked).
    let imported_before = t.overview(StatsScope::default(), 0, true).reviews.totals.imported;
    t.add_review(1, cards[0], T0 - 30 * DAY, Rating::Good, CardState::New, "imported");
    t.add_review(2, cards[0], T0 - 27 * DAY, Rating::Again, CardState::Review, "imported");
    assert_eq!(agree(&t, 0), t.incremental);
    let o = t.overview(StatsScope::default(), 0, false);
    assert_eq!(o.reviews.totals.imported, imported_before + 2);
    assert_eq!(o.reviews.heatmap.first().map(|h| h.day), Some(o.today - 9 - 30));

    // Trial answers never reach statistics.
    t.refresh(0);
    let n = t.total_reviews();
    t.day(40);
    assert!(t.study(StudyMode::Trial, None, 2) > 0);
    assert_eq!(t.total_reviews(), n);
    // (MemStore's hash covers trial rows too, so its cache is rebuilt; SQLite folds nothing.)
    assert_eq!(agree(&t, 0), t.incremental);

    // Purging a trashed note keeps its review history: totals unchanged, cache still valid.
    t.refresh(0);
    let totals = t.overview(StatsScope::default(), 0, false).reviews;
    let note = t.card(cards[1]).note_id;
    t.ok::<NotesDelete>(NoteIds { ids: vec![note] });
    t.ok::<BrowsePurgeTrash>(PurgeTrash {
        older_than_days: Some(0),
    });
    assert_eq!(t.total_reviews(), n);
    assert!(agree(&t, 0));
    assert!(!t.refresh(0).rebuilt, "a purge does not touch review_log");
    let after = t.overview(StatsScope::default(), 0, false).reviews;
    assert_eq!(after.totals, totals.totals);
    assert_eq!(after.heatmap, totals.heatmap);
    assert_eq!(after.streak, totals.streak);

    // Another calendar has its own cache.
    assert!(!agree(&t, 180));
    t.refresh(180);
    assert!(agree(&t, 180));
    assert!(agree(&t, 0));

    // A deck scope is always recomputed and only sees its cards.
    let other = t.deck("Başka");
    t.note(other.id, "z", None);
    t.study(StudyMode::Scheduled, Some(other.id), 0);
    let scoped = t.overview(
        StatsScope {
            deck_id: Some(other.id),
            ..StatsScope::default()
        },
        0,
        false,
    );
    assert!(!scoped.from_cache);
    assert_eq!(scoped.reviews.totals.reviews, 2, "two learning steps");
    assert_eq!(scoped.cards.states.total, 1);
    // Everything about the report is filled in.
    let o = t.overview(StatsScope::default(), 0, false);
    assert!(o.reviews.streak.longest >= 2);
    assert!(o.reviews.true_retention.is_some());
    assert_eq!(o.reviews.hours.len(), 24);
    assert_eq!(o.cards.forecast.len(), 14);
    assert!(o.cards.states.total >= 8);
    assert!(!o.reviews.weekly.is_empty() && !o.reviews.monthly.is_empty());
    assert!((o.target_retention - 0.9).abs() < 1e-9);
}

/// Every statistic can be split by a classification (retention per level, workload per exam).
pub fn breakdown_by_classification(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("İngilizce");
    let def = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: None,
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: ["A1", "B1", "C1"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    let opt = |l: &str| def.options.iter().find(|o| o.label == l).unwrap().id;
    for i in 0..3 {
        t.note(d.id, &format!("a{i}"), Some((def.id, opt("A1"))));
    }
    t.note(d.id, "b", Some((def.id, opt("B1"))));
    t.note(d.id, "none", None);
    t.study(StudyMode::Scheduled, None, 2);
    let b = t.ok::<StatsGetBreakdown>(StatsBreakdownRequest {
        scope: StatsScope::default(),
        attr_id: def.id,
        tz_offset_mins: Some(0),
        days: None,
    });
    assert_eq!(b.name, "Seviye");
    let labels: Vec<_> = b.rows.iter().map(|r| r.label.clone()).collect();
    assert_eq!(
        labels,
        vec![Some("A1".into()), Some("B1".into()), Some("C1".into()), None]
    );
    assert_eq!(b.rows[0].states.total, 3);
    assert_eq!(b.rows[1].states.total, 1);
    assert_eq!(b.rows[2].states.total, 0);
    assert_eq!(b.rows[3].states.total, 1);
    assert!(b.rows[0].reviews >= 3);
    let sum: u32 = b.rows.iter().map(|r| r.reviews).sum();
    assert_eq!(sum as usize, t.total_reviews());
    // Workload per value: cards come due tomorrow or later.
    t.day(30);
    let b = t.ok::<StatsGetBreakdown>(StatsBreakdownRequest {
        scope: StatsScope::default(),
        attr_id: def.id,
        tz_offset_mins: Some(0),
        days: Some(7),
    });
    assert_eq!(b.rows[0].due_today, 3);
    assert_eq!(b.rows[0].reviews, 0, "range: last 7 days");
    // Scoped by query too.
    let q = t.overview(
        StatsScope {
            query: Some("seviye:A1".into()),
            ..StatsScope::default()
        },
        0,
        true,
    );
    assert_eq!(q.cards.states.total, 3);
    // Only choice classifications can be broken down.
    let text = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: None,
        name: "Kaynak".into(),
        kind: AttributeKind::Text,
        target: None,
        required: None,
        options: vec![],
    });
    let e = t.err::<StatsGetBreakdown>(StatsBreakdownRequest {
        scope: StatsScope::default(),
        attr_id: text.id,
        tz_offset_mins: None,
        days: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationBreakdownAttribute);
}

/// The cached and the recomputed breakdown agree (all time and the last 7 days); returns
/// whether the cache was used.
fn breakdown_agrees(t: &T, req: &StatsBreakdownRequest) -> bool {
    let tx = t.store.read().unwrap();
    let ctx = crate::handler::QueryCtx {
        tx: &*tx,
        now: fc_core::time::Clock::now(&*t.clock),
        locale: fc_core::i18n::Locale::default(),
        registry: t.app.registry(),
    };
    for days in [None, Some(7)] {
        let req = StatsBreakdownRequest { days, ..req.clone() };
        let cached = super::breakdown::breakdown_with(&req, &ctx, true).unwrap();
        let fresh = super::breakdown::breakdown_with(&req, &ctx, false).unwrap();
        assert_eq!(cached, fresh, "cache + tail must equal a recomputation");
    }
    super::breakdown::uses_cache(req, &ctx).unwrap()
}

/// v1.0: the breakdown's day counts are cached per classification and equal a
/// recomputation; classification edits, trashed cards, removed reviews and other scopes
/// never use a stale cache.
pub fn breakdown_cache_equals_recomputation(f: &Factory) {
    use fc_api::v1::attributes::SetAttributeValues;
    use fc_core::attribute::EntityRef;
    let t = T::new(f);
    let d = t.deck("Kelimeler");
    let def = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: None,
        name: "Konu".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: None,
        options: ["isim", "fiil"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    let (noun, verb) = (def.options[0].id, def.options[1].id);
    let cards: Vec<CardId> = (0..6)
        .map(|i| t.note(d.id, &format!("w{i}"), (i < 4).then_some((def.id, if i % 2 == 0 { noun } else { verb }))))
        .collect();
    let req = StatsBreakdownRequest {
        scope: StatsScope::default(),
        attr_id: def.id,
        tz_offset_mins: Some(0),
        days: None,
    };
    t.study(StudyMode::Scheduled, None, 3);
    assert!(!breakdown_agrees(&t, &req), "nothing cached yet");
    t.refresh(0);
    assert!(breakdown_agrees(&t, &req), "valid cache is used");
    // Appended reviews are folded on top of the cache (SQLite).
    for day in 1..4 {
        t.day(day);
        t.study(StudyMode::Scheduled, None, 2);
        assert_eq!(breakdown_agrees(&t, &req), t.incremental, "day {day}");
    }
    // A classification edit (a card changes value) changes the card sets.
    t.refresh(0);
    assert!(breakdown_agrees(&t, &req));
    t.ok::<AttributesValuesSet>(SetAttributeValues {
        entity: EntityRef::Note(t.card(cards[0]).note_id),
        values: vec![AttributeAssignment {
            attr_id: def.id,
            value: Some(AttributeValue::Choice(verb)),
        }],
    });
    assert!(!breakdown_agrees(&t, &req), "edited values: not cached");
    t.refresh(0);
    assert!(breakdown_agrees(&t, &req));
    // A trashed note leaves the breakdown.
    t.ok::<NotesDelete>(NoteIds {
        ids: vec![t.card(cards[1]).note_id],
    });
    assert!(!breakdown_agrees(&t, &req), "trashed card: not cached");
    t.refresh(0);
    assert!(breakdown_agrees(&t, &req));
    // A removed review (undo of an answer) invalidates the cache.
    t.day(9);
    let s = t.start(StudyMode::Scheduled, None).unwrap();
    let c = s.current.clone().expect("cards due by day 9");
    t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: c.card_id,
        rating: Rating::Good,
        duration_ms: 3_000,
        reveal_ms: None,
    });
    t.refresh(0);
    t.undo();
    assert!(!breakdown_agrees(&t, &req), "a removed row invalidates the cache");
    // Another scope or calendar is recomputed.
    t.refresh(0);
    assert!(breakdown_agrees(&t, &req));
    let scoped = StatsBreakdownRequest {
        scope: StatsScope {
            query: Some("w2".into()),
            ..StatsScope::default()
        },
        ..req.clone()
    };
    assert!(!breakdown_agrees(&t, &scoped));
    let other = StatsBreakdownRequest {
        tz_offset_mins: Some(180),
        ..req.clone()
    };
    assert!(!breakdown_agrees(&t, &other));
}

/// Leeches and deck report cards.
pub fn leeches_and_report_cards(f: &Factory) {
    let t = T::new(f);
    let parent = t.deck("Dil");
    let child = t.deck("Dil::Almanca");
    let a = t.note(child.id, "Haus", None);
    let b = t.note(parent.id, "Baum", None);
    t.set_card(a, |c| {
        c.scheduling.lapses = 9;
        c.scheduling.state = CardState::Review;
        c.scheduling.stability = Some(40.0);
        c.scheduling.last_review = Some(Timestamp(T0 - 30 * DAY));
        c.scheduling.due = Some(Timestamp(T0 + 20 * DAY));
    });
    t.set_card(b, |c| c.scheduling.lapses = 3);
    let l = t.ok::<StatsLeeches>(LeechRequest::default());
    assert_eq!((l.threshold, l.total), (8, 1));
    assert_eq!(l.items[0].card_id, a);
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: super::LEECH_THRESHOLD.into(),
        value: serde_json::json!(2),
    });
    let l = t.ok::<StatsLeeches>(LeechRequest::default());
    assert_eq!(l.total, 2);
    assert_eq!(l.items[0].card_id, a, "most lapses first");
    let r = t.ok::<StatsDeckReports>(DeckReportsRequest::default());
    let of = |id: DeckId| r.items.iter().find(|x| x.deck_id == id).unwrap();
    assert_eq!(r.items[0].deck_id, parent.id, "tree order");
    assert_eq!(of(parent.id).report.cards, 2, "sub-decks included");
    assert_eq!(of(child.id).report.cards, 1);
    assert_eq!(of(child.id).report.mature, 1);
    assert!((of(child.id).report.settled_pct - 100.0).abs() < 1e-9);
    assert!((of(parent.id).report.settled_pct - 50.0).abs() < 1e-9);
    assert!(of(child.id).report.mean_recall.unwrap() > 0.5);
}

/// `stats.apply_to_leeches` acts on every leech of the scope (not the listed page), as one
/// undo step; the tag action tags each note once.
pub fn apply_to_leeches_covers_every_leech(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("Zor");
    let other = t.deck("Kolay");
    let leeches: Vec<CardId> = (0..7).map(|i| t.note(d.id, &format!("l{i}"), None)).collect();
    let calm = t.note(d.id, "calm", None);
    let elsewhere = t.note(other.id, "e", None);
    for c in leeches.iter().chain([&elsewhere]) {
        t.set_card(*c, |c| c.scheduling.lapses = 9);
    }
    t.set_card(leeches[0], |c| c.suspended = true);
    let scope = StatsScope {
        deck_id: Some(d.id),
        ..StatsScope::default()
    };
    let page = t.ok::<StatsLeeches>(LeechRequest {
        scope: scope.clone(),
        threshold: None,
        limit: Some(2),
    });
    assert_eq!((page.total, page.items.len()), (7, 2));
    let r = t.ok::<StatsApplyToLeeches>(ApplyToLeeches {
        scope: scope.clone(),
        threshold: None,
        action: LeechAction::Suspend,
        tag: None,
    });
    assert_eq!((r.threshold, r.cards, r.changed, r.skipped), (8, 7, 6, 1));
    assert!(leeches.iter().all(|c| t.card(*c).suspended));
    assert!(!t.card(calm).suspended && !t.card(elsewhere).suspended, "scope and rule");
    t.undo();
    assert!(leeches[1..].iter().all(|c| !t.card(*c).suspended), "one undo step");
    assert!(t.card(leeches[0]).suspended);

    let r = t.ok::<StatsApplyToLeeches>(ApplyToLeeches {
        scope,
        threshold: Some(9),
        action: LeechAction::Tag,
        tag: None,
    });
    assert_eq!((r.cards, r.changed), (7, 7));
    let tagged = |t: &T, c: CardId| {
        let note = t.card(c).note_id;
        t.store
            .read()
            .unwrap()
            .get_note(note)
            .unwrap()
            .unwrap()
            .tags
            .iter()
            .any(|g| g.as_str() == "leech")
    };
    assert!(leeches.iter().all(|c| tagged(&t, *c)));
    assert!(!tagged(&t, calm) && !tagged(&t, elsewhere));
    t.undo();
    assert!(!leeches.iter().any(|c| tagged(&t, *c)));
    let bad = t.err::<StatsApplyToLeeches>(ApplyToLeeches {
        scope: StatsScope::default(),
        threshold: Some(0),
        action: LeechAction::Suspend,
        tag: None,
    });
    assert_eq!(bad.code, ErrorCode::ValidationInvalidParams);
}

/// The target retention comes from the user's presets, not the built-in default: a deck
/// scope uses its deck's; the whole collection its cards' common value or card-weighted mean.
pub fn target_retention_follows_presets(f: &Factory) {
    let t = T::new(f);
    let builtin = fc_core::preset::PresetConfig::default().desired_retention;
    let all = |t: &T| t.overview(StatsScope::default(), 0, false).target_retention;
    assert!((all(&t) - builtin).abs() < 1e-12, "empty collection: built-in");
    let a = t.deck("A");
    let b = t.deck("B");
    for i in 0..3 {
        t.note(a.id, &format!("a{i}"), None);
    }
    t.note(b.id, "b", None);
    let preset = |name: &str, r: f64, decks: Vec<DeckId>| {
        let p = t.ok::<PresetsCreate>(CreatePreset {
            name: name.into(),
            config: Some(fc_core::preset::PresetConfig {
                desired_retention: r,
                ..fc_core::preset::PresetConfig::default()
            }),
        });
        t.ok::<PresetsAssign>(AssignPreset {
            deck_ids: decks,
            preset_id: Some(p.id),
        });
    };
    preset("Ortak", 0.85, vec![a.id, b.id]);
    assert!((all(&t) - 0.85).abs() < 1e-12, "every deck shares one preset");
    preset("Sıkı", 0.95, vec![b.id]);
    // 3 cards at 0.85, 1 at 0.95.
    assert!((all(&t) - (3.0 * 0.85 + 0.95) / 4.0).abs() < 1e-12);
    let of_b = t.overview(
        StatsScope {
            deck_id: Some(b.id),
            ..StatsScope::default()
        },
        0,
        false,
    );
    assert!((of_b.target_retention - 0.95).abs() < 1e-12);
}

// ------------------------------------------------------------------ planning

/// Puts `n` review cards in the exact same memory state, all due today.
fn twins(t: &T, deck: DeckId, n: usize) -> Vec<CardId> {
    (0..n)
        .map(|i| {
            let id = t.note(deck, &format!("t{i}"), None);
            t.set_card(id, |c| {
                c.scheduling.state = CardState::Review;
                c.scheduling.stability = Some(30.0);
                c.scheduling.difficulty = Some(5.0);
                c.scheduling.reps = 4;
                c.scheduling.last_review = Some(Timestamp(T0 - 30 * DAY));
                c.scheduling.due = Some(Timestamp(T0 - DAY));
            });
            id
        })
        .collect()
}

fn due_days(t: &T, ids: &[CardId]) -> Vec<i64> {
    ids.iter()
        .map(|c| t.card(*c).scheduling.due.unwrap().as_millis().div_euclid(DAY))
        .collect()
}

/// The study engine applies load balancing: equal cards answered equally spread over their
/// fuzz range instead of landing on one day; with the module off they all land together.
pub fn load_balancing_spreads_equal_cards(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("D");
    let ids = twins(&t, d.id, 12);
    t.study(StudyMode::Scheduled, None, 0);
    let mut days = due_days(&t, &ids);
    days.sort();
    days.dedup();
    assert!(days.len() >= 3, "spread: {days:?}");

    let t = T::new(f);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: crate::modules::planning::ID.into(),
        enabled: false,
    });
    let d = t.deck("D");
    let ids = twins(&t, d.id, 12);
    t.study(StudyMode::Scheduled, None, 0);
    let mut days = due_days(&t, &ids);
    days.dedup();
    assert_eq!(days.len(), 1, "plain scheduling: one day");

    // Easy days: with Saturday and Sunday at "minimum", nothing lands on a weekend.
    let t = T::new(f);
    for k in &crate::modules::planning::EASY_DAYS[5..] {
        t.ok::<SettingsSet>(SetSetting {
            scope: SettingScope::App,
            key: (*k).into(),
            value: serde_json::json!("minimum"),
        });
    }
    let d = t.deck("D");
    let ids = twins(&t, d.id, 12);
    t.study(StudyMode::Scheduled, None, 0);
    for day in due_days(&t, &ids) {
        assert!(fc_core::stats::weekday(day) < 5, "weekend day {day}");
    }
}

/// Exam-date mode: a card of the exam's scope whose next review would fall after the exam is
/// pulled into the days just before it; other cards are not. Exam commands undo exactly.
pub fn exam_mode_and_exam_commands(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("YKS");
    let def = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: None,
        name: "Sınav".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: None,
        options: ["TYT", "AYT"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    let tyt = def.options.iter().find(|o| o.label == "TYT").unwrap().id;
    let ayt = def.options.iter().find(|o| o.label == "AYT").unwrap().id;
    let in_exam = t.note(d.id, "türev", Some((def.id, tyt)));
    let other = t.note(d.id, "integral", Some((def.id, ayt)));
    for c in [in_exam, other] {
        t.set_card(c, |c| {
            c.scheduling.state = CardState::Review;
            c.scheduling.stability = Some(120.0);
            c.scheduling.difficulty = Some(4.0);
            c.scheduling.reps = 6;
            c.scheduling.last_review = Some(Timestamp(T0 - 120 * DAY));
            c.scheduling.due = Some(Timestamp(T0 - DAY));
        });
    }
    let today = T0.div_euclid(DAY);
    let exam_date = fc_core::stats::iso_date(today + 20);
    let e = t.ok::<PlanningExamsCreate>(CreateExam {
        name: " YKS  2027 ".into(),
        date: exam_date.clone(),
        deck_id: None,
        query: Some("sınav:TYT".into()),
    });
    assert_eq!(e.name, "YKS 2027");
    // Create is undoable; redo brings the same exam back.
    t.undo();
    assert!(t.ok::<PlanningExamsList>(ListExams::default()).items.is_empty());
    t.redo();
    assert_eq!(t.ok::<PlanningExamsList>(ListExams::default()).items[0].exam, e);
    // Shown with the classification's current key and option label.
    assert_eq!(e.query.as_deref(), Some("sinav:TYT"));
    let list = t.ok::<PlanningExamsList>(ListExams {
        tz_offset_mins: Some(0),
    });
    assert_eq!(list.items[0].days_left, 20);
    assert_eq!(list.items[0].cards, 1);
    // The buttons already show the exam-bounded interval.
    let s = t.start(StudyMode::Scheduled, None).unwrap();
    let shown = s.current.as_ref().unwrap();
    let good = shown.intervals.iter().find(|p| p.rating == Rating::Good).unwrap();
    if shown.card_id == in_exam {
        assert!((17..=19).contains(&good.scheduled_days), "{}", good.scheduled_days);
    } else {
        assert!(good.scheduled_days > 60);
    }
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    t.study(StudyMode::Scheduled, None, 0);
    let due = |c: CardId| t.card(c).scheduling.due.unwrap().as_millis().div_euclid(DAY) - today;
    assert!((17..=19).contains(&due(in_exam)), "pulled before the exam: {}", due(in_exam));
    assert!(due(other) > 60, "not in the exam: {}", due(other));

    // Renaming the option keeps the exam's (canonical) query working.
    t.ok::<AttributesOptionRename>(fc_api::v1::attributes::RenameAttributeOption {
        attr_id: def.id,
        option_id: tyt,
        label: "TYT-2027".into(),
    });
    let list = t.ok::<PlanningExamsList>(ListExams::default());
    assert_eq!(list.items[0].exam.query.as_deref(), Some("sinav:TYT-2027"));
    assert_eq!(list.items[0].cards, 1);

    // Update / delete / undo / redo are exact.
    let snapshot = |t: &T| t.ok::<PlanningExamsList>(ListExams::default());
    let before = snapshot(&t);
    t.ok::<PlanningExamsUpdate>(UpdateExam {
        id: e.id,
        name: "YKS".into(),
        date: "2027-06-20".into(),
        deck_id: Some(d.id),
        query: None,
    });
    let after = snapshot(&t);
    assert_eq!(after.items[0].exam.date, "2027-06-20");
    t.undo();
    assert_eq!(snapshot(&t), before);
    t.redo();
    assert_eq!(snapshot(&t), after);
    t.ok::<PlanningExamsDelete>(ExamRef { id: e.id });
    assert!(snapshot(&t).items.is_empty());
    t.undo();
    assert_eq!(snapshot(&t), after);
    t.undo(); // the update
    assert_eq!(snapshot(&t), before);
    // Validation.
    let bad = |date: &str, name: &str, query: Option<&str>| {
        t.err::<PlanningExamsCreate>(CreateExam {
            name: name.into(),
            date: date.into(),
            deck_id: None,
            query: query.map(Into::into),
        })
        .code
    };
    assert_eq!(bad("2027-02-30", "x", None), ErrorCode::ValidationDate);
    assert_eq!(bad("2027-02-03", " ", None), ErrorCode::ValidationExamName);
    assert_eq!(bad("2027-02-03", "x", Some("nope:1")), ErrorCode::ValidationQuery);
    assert_eq!(
        t.err::<PlanningExamsDelete>(ExamRef {
            id: fc_core::ExamId(uuid::Uuid::nil())
        })
        .code,
        ErrorCode::NotFoundExam
    );
}

// ------------------------------------------------------------------ goals

pub fn goals_progress_and_undo(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("D");
    for i in 0..6 {
        t.note(d.id, &format!("n{i}"), None);
    }
    let today = T0.div_euclid(DAY);
    let g = t.ok::<GoalsCreate>(CreateGoal {
        kind: GoalKind::CardsLearned,
        target: 20.0,
        deadline: Some(fc_core::stats::iso_date(today + 4)),
        query: None,
    });
    let daily = t.ok::<GoalsCreate>(CreateGoal {
        kind: GoalKind::StudyMinutes,
        target: 30.0,
        deadline: None,
        query: Some("deck:D".into()),
    });
    // Day 0: 3 cards learned.
    let mut s = t.start(StudyMode::Scheduled, None).unwrap();
    for _ in 0..3 {
        let c = s.current.clone().unwrap();
        t.clock.advance(60_000);
        s = t.ok::<StudyAnswer>(AnswerCard {
            session_id: s.id,
            card_id: c.card_id,
            rating: Rating::Easy,
            duration_ms: 60_000,
            reveal_ms: None,
        });
    }
    let list = t.ok::<GoalsList>(ListGoals {
        tz_offset_mins: Some(0),
    });
    let p = &list.items[0].progress;
    assert_eq!(p.current, 3.0);
    // 3 a day → 17 more in 6 days → day 6, two days after the deadline (day 4).
    assert_eq!(p.days_late, Some(2));
    assert!(!p.done);
    let m = &list.items[1].progress;
    assert!(m.daily);
    assert!((m.current - 3.0).abs() < 1e-9, "3 × 60 s");
    assert_eq!(list.items[1].goal.query.as_deref(), Some("deck:D"));
    // Edit, delete, undo, redo.
    let before = t.ok::<GoalsList>(ListGoals::default());
    t.ok::<GoalsUpdate>(UpdateGoal {
        id: g.id,
        target: 3.0,
        deadline: None,
        query: None,
    });
    assert!(t.ok::<GoalsList>(ListGoals::default()).items[0].progress.done);
    t.ok::<GoalsDelete>(GoalRef { id: daily.id });
    assert_eq!(t.ok::<GoalsList>(ListGoals::default()).items.len(), 1);
    t.undo();
    t.undo();
    assert_eq!(t.ok::<GoalsList>(ListGoals::default()), before);
    t.redo();
    assert!(t.ok::<GoalsList>(ListGoals::default()).items[0].progress.done);
    let e = t.err::<GoalsCreate>(CreateGoal {
        kind: GoalKind::DeckCompletion,
        target: 120.0,
        deadline: None,
        query: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationGoalTarget);
    let e = t.err::<GoalsCreate>(CreateGoal {
        kind: GoalKind::Streak,
        target: 5.0,
        deadline: Some("tomorrow".into()),
        query: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationDate);
}

// ------------------------------------------------------------------ optimizer

/// A preset on a deck with `cards` cards whose history follows a memory (recall drops with the
/// gap), written straight into the review log.
fn optimizer_world(t: &T, cards: usize, reviews: usize) -> (fc_core::PresetId, DeckId) {
    let d = t.deck("Opt");
    let p = t.ok::<PresetsCreate>(CreatePreset {
        name: "Mine".into(),
        config: None,
    });
    t.ok::<PresetsAssign>(AssignPreset {
        deck_ids: vec![d.id],
        preset_id: Some(p.id),
    });
    let ids: Vec<CardId> = (0..cards).map(|i| t.note(d.id, &format!("o{i}"), None)).collect();
    let mut rng = SplitMix64::new(3);
    let mut tx = t.store.write().unwrap();
    let mut n: u128 = 0;
    for c in &ids {
        let mut day = -400i64;
        let mut gap = 1i64;
        for k in 0..reviews {
            let p_recall = 0.97f64.powf(gap as f64 / 4.0);
            let roll = (fc_core::Rng::next_u64(&mut rng) >> 11) as f64 / (1u64 << 53) as f64;
            let (rating, before) = if k == 0 {
                (Rating::Good, CardState::New)
            } else if roll < p_recall {
                (Rating::Good, CardState::Review)
            } else {
                (Rating::Again, CardState::Review)
            };
            n += 1;
            tx.insert_review_log(&ReviewLogRecord {
                id: ReviewLogId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0002_0000_0000 | n)),
                card_id: *c,
                session_id: None,
                rating,
                state_before: before,
                state_after: CardState::Review,
                scheduled_days: 1,
                elapsed_days: 1,
                duration_ms: 4_000,
                answer_duration_ms: None,
                hints_used: 0,
                mode: if k % 2 == 0 { "normal" } else { "imported" }.into(),
                stability_after: None,
                difficulty_after: None,
                created_at: Timestamp(T0 + day * DAY),
            })
            .unwrap();
            gap = if rating == Rating::Again { 1 } else { (gap * 5 / 2).max(gap + 1) };
            day += gap;
        }
    }
    tx.commit().unwrap();
    (p.id, d.id)
}

fn weights(t: &T, p: fc_core::PresetId) -> Vec<f64> {
    t.ok::<PresetsGet>(PresetRef { id: p }).config.fsrs_weights
}

fn job(t: &T, j: &JobDto) -> JobDto {
    t.ok::<JobsGet>(JobRef { id: j.id })
}

/// The optimizer job learns weights from the history (imported reviews included) and saves
/// them into the preset as one undoable step; too little data fails without touching it.
pub fn optimizer_job_saves_weights(f: &Factory) {
    let t = T::new(f);
    let (p, deck) = optimizer_world(&t, 40, 8);
    // Workload estimate before (default weights, no review cards in state yet).
    let w = t.ok::<OptimizerWorkload>(WorkloadRequest {
        deck_id: Some(deck),
        retentions: None,
    });
    assert!((w.current_retention - 0.9).abs() < 1e-9);
    assert!(w.points.iter().any(|x| (x.relative - 1.0).abs() < 1e-9));
    let started = t.ok::<OptimizerRun>(RunOptimizer {
        preset_id: p,
        tz_offset_mins: Some(0),
    });
    assert_eq!(started.job.status, JobStatus::Queued);
    assert!(weights(&t, p).is_empty());
    t.app.run_pending_jobs().unwrap();
    let done = job(&t, &started.job);
    assert_eq!(done.status, JobStatus::Succeeded, "{:?}", done.error);
    let result: OptimizerResult = serde_json::from_value(done.result.unwrap()).unwrap();
    assert_eq!(result.cards, 40);
    assert!(result.items >= 64);
    let learned = weights(&t, p);
    assert_eq!(learned.len(), 21);
    assert_eq!(learned, result.weights);
    // Saved through presets.update: one undo step.
    t.undo();
    assert!(weights(&t, p).is_empty());
    t.redo();
    assert_eq!(weights(&t, p), learned);
    // The scheduler uses them.
    let eff = t.ok::<PresetsForDeck>(fc_api::v1::presets::PresetForDeck { deck_id: deck });
    assert_eq!(eff.config.fsrs_weights, learned);

    // Too little data: the job fails with a clear code and leaves the preset alone.
    let t = T::new(f);
    let (p, _) = optimizer_world(&t, 3, 3);
    let started = t.ok::<OptimizerRun>(RunOptimizer {
        preset_id: p,
        tz_offset_mins: None,
    });
    t.app.run_pending_jobs().unwrap();
    let done = job(&t, &started.job);
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().code, ErrorCode::ConflictNotEnoughReviews);
    assert!(weights(&t, p).is_empty());
    assert_eq!(
        t.err::<OptimizerRun>(RunOptimizer {
            preset_id: fc_core::PresetId(uuid::Uuid::nil()),
            tz_offset_mins: None,
        })
        .code,
        ErrorCode::NotFoundPreset
    );
    // Module switch (rule 7).
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: crate::modules::optimizer::ID.into(),
        enabled: false,
    });
    assert_eq!(
        t.err::<OptimizerWorkload>(WorkloadRequest::default()).code,
        ErrorCode::ConflictModuleDisabled
    );
}

/// Cancelled while queued: never runs, preset untouched.
pub fn optimizer_job_cancelled_before_it_runs(f: &Factory) {
    let t = T::new(f);
    let (p, _) = optimizer_world(&t, 40, 8);
    let started = t.ok::<OptimizerRun>(RunOptimizer {
        preset_id: p,
        tz_offset_mins: None,
    });
    t.ok::<JobsCancel>(JobRef { id: started.job.id });
    t.app.run_pending_jobs().unwrap();
    assert_eq!(job(&t, &started.job).status, JobStatus::Cancelled);
    assert!(weights(&t, p).is_empty());
}

/// Cancelled while the background worker trains: training stops, the job ends `cancelled`,
/// the preset is untouched.
pub fn optimizer_job_cancelled_while_training(f: &Factory) {
    let t = T::new(f);
    // Enough history that training takes a while.
    let (p, _) = optimizer_world(&t, 1500, 14);
    let worker = crate::jobs::spawn_worker(t.app.clone()).unwrap();
    let started = t.ok::<OptimizerRun>(RunOptimizer {
        preset_id: p,
        tz_offset_mins: None,
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while job(&t, &started.job).status == JobStatus::Queued {
        assert!(std::time::Instant::now() < deadline, "job never started");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let cancel_at = std::time::Instant::now();
    t.ok::<JobsCancel>(JobRef { id: started.job.id });
    let status = loop {
        let s = job(&t, &started.job).status;
        if s != JobStatus::Running && s != JobStatus::Queued {
            break s;
        }
        assert!(std::time::Instant::now() < deadline, "job never stopped");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    drop(worker);
    assert_eq!(status, JobStatus::Cancelled);
    assert!(
        cancel_at.elapsed() < std::time::Duration::from_secs(30),
        "stopped promptly"
    );
    assert!(weights(&t, p).is_empty());
}

/// The modules are switchable; a disabled `stats` module rejects its methods.
pub fn modules_can_be_switched_off(f: &Factory) {
    let t = T::new(f);
    for (id, check) in [
        ("stats", Box::new(|t: &T| t.err::<StatsGetOverview>(StatsRequest::default()).code) as Box<dyn Fn(&T) -> ErrorCode>),
        ("goals", Box::new(|t: &T| t.err::<GoalsList>(ListGoals::default()).code)),
        ("planning", Box::new(|t: &T| t.err::<PlanningExamsList>(ListExams::default()).code)),
    ] {
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: id.into(),
            enabled: false,
        });
        assert_eq!(check(&t), ErrorCode::ConflictModuleDisabled, "{id}");
    }
    let _ = t.ok::<DecksGet>(DeckRef {
        id: t.deck("x").id,
    });
}

// ------------------------------------------------------------------ v1.3

fn forecast(t: &T, deck: Option<DeckId>, pace: Option<u32>, exam: Option<String>) -> WorkloadForecast {
    t.ok::<PlanningForecast>(ForecastRequest {
        deck_id: deck,
        new_per_day: pace,
        exam_date: exam,
        days: Some(14),
        tz_offset_mins: Some(0),
    })
}

fn pace_of(t: &T, deck: DeckId) -> u32 {
    t.ok::<PresetsForDeck>(fc_api::v1::presets::PresetForDeck { deck_id: deck })
        .config
        .new_per_day
}

/// `planning.forecast` reads the deck's cards and preset; `planning.apply_pace` writes the pace
/// into the preset it names (creating one when the deck has none), adds the exam, and is one
/// undo step.
pub fn forecast_and_apply_pace(f: &Factory) {
    let t = T::new(f);
    let today = T0.div_euclid(DAY);
    let d = t.deck("Eng");
    for i in 0..30 {
        t.note(d.id, &format!("w{i}"), None);
    }
    let fc = forecast(&t, Some(d.id), None, None);
    assert_eq!((fc.current_new_per_day, fc.new_per_day), (20, 20));
    assert_eq!((fc.unseen, fc.started, fc.new_done_today), (30, 0, 0));
    assert_eq!(fc.days.len(), 14);
    assert_eq!(fc.days[0].date, fc_core::stats::iso_date(today));
    assert_eq!((fc.days[0].new_cards, fc.days[1].new_cards), (20.0, 10.0));
    // 3 answers per new card on its first day (2 learning steps + 1).
    assert!((fc.days[0].cards - 60.0).abs() < 1e-9);
    assert_eq!(fc.new_finished_on, Some(fc_core::stats::iso_date(today + 1)));
    assert!(!fc.measured_time && (fc.secs_per_answer - 8.0).abs() < 1e-9);
    assert_eq!(fc.peak.offset, 0);
    assert_eq!(fc.weeks.len(), 2);
    assert!((fc.weeks[0].new_cards - 30.0).abs() < 1e-9);
    assert_eq!(fc.weeks[1].start_date, fc_core::stats::iso_date(today + 7));
    let target = fc.target.clone().unwrap();
    assert_eq!(target.preset_id, None);
    assert_eq!(target.decks.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(), ["Eng"]);
    // A slower pace never asks for more up to any day; the whole collection works too.
    let slow = forecast(&t, Some(d.id), Some(5), None);
    let sum = |f: &WorkloadForecast, n: usize| f.days[..n].iter().map(|d| d.cards).sum::<f64>();
    for n in 1..=14 {
        assert!(sum(&slow, n) <= sum(&fc, n) + 1e-9);
    }
    assert_eq!(forecast(&t, None, None, None).unseen, 30);
    assert!(forecast(&t, None, None, None).target.is_none());
    // An exam in 10 days: 3 new cards a day start all 30 the day before.
    let exam_date = fc_core::stats::iso_date(today + 10);
    let ex = forecast(&t, Some(d.id), Some(2), Some(exam_date.clone()));
    let e = ex.exam.clone().unwrap();
    assert_eq!((e.days_left, e.new_per_day_needed), (10, Some(3)));
    assert_eq!(e.new_left, 10); // 2 a day for 10 days
    assert!(ex.days.len() >= 11);
    assert_eq!(
        t.err::<PlanningForecast>(ForecastRequest {
            deck_id: Some(d.id),
            new_per_day: Some(10_000),
            ..ForecastRequest::default()
        })
        .code,
        ErrorCode::ValidationInvalidParams
    );

    // Apply: the deck has no preset → one is created and assigned; the exam is added.
    let applied = t.ok::<PlanningApplyPace>(ApplyPace {
        deck_id: d.id,
        new_per_day: 7,
        preset_name: None,
        exam_date: Some(exam_date.clone()),
        exam_name: Some("TOEFL".into()),
    });
    assert!(applied.created_preset);
    assert_eq!(applied.previous_new_per_day, 20);
    assert_eq!(applied.target.current_new_per_day, 7);
    assert_eq!(applied.target.preset_name.as_deref(), Some("Eng"));
    assert_eq!(applied.exam.as_ref().unwrap().name, "TOEFL");
    assert_eq!(pace_of(&t, d.id), 7);
    assert_eq!(forecast(&t, Some(d.id), None, None).current_new_per_day, 7);
    let exams = || t.ok::<PlanningExamsList>(ListExams::default()).items.len();
    assert_eq!(exams(), 1);
    // One undo step takes everything back; redo brings it again.
    t.undo();
    assert_eq!(pace_of(&t, d.id), 20);
    assert_eq!(exams(), 0);
    assert!(t.ok::<PresetsList>(Empty {}).items.is_empty());
    t.redo();
    assert_eq!(pace_of(&t, d.id), 7);
    assert_eq!(exams(), 1);
    // With a preset shared by two decks: the forecast names both, apply updates the preset.
    let other = t.deck("Fr");
    let p = t.ok::<PresetsForDeck>(fc_api::v1::presets::PresetForDeck { deck_id: d.id }).preset_id;
    t.ok::<PresetsAssign>(AssignPreset {
        deck_ids: vec![other.id],
        preset_id: p,
    });
    let target = forecast(&t, Some(d.id), Some(12), None).target.unwrap();
    assert_eq!(target.decks.iter().map(|d| d.path.as_str()).collect::<Vec<_>>(), ["Eng", "Fr"]);
    let again = t.ok::<PlanningApplyPace>(ApplyPace {
        deck_id: d.id,
        new_per_day: 12,
        preset_name: None,
        exam_date: Some(exam_date),
        exam_name: None,
    });
    assert!(!again.created_preset && again.exam.is_none(), "same exam not added twice");
    assert_eq!((again.previous_new_per_day, pace_of(&t, other.id)), (7, 12));
    assert_eq!(t.ok::<PresetsList>(Empty {}).items.len(), 1);
    t.undo();
    assert_eq!(pace_of(&t, other.id), 7);

    // Studying today (the helper's session takes up to 100 new cards): started cards leave the
    // pool, today's new cards are counted and the started ones come back as reviews.
    let n = t.study(StudyMode::Scheduled, Some(d.id), 0);
    assert!(n >= 30);
    let after = forecast(&t, Some(d.id), None, None);
    assert_eq!(after.new_done_today, 30);
    assert_eq!((after.unseen, after.started), (0, 30));
    assert_eq!(after.days[0].new_cards, 0.0);
    assert!(after.days.iter().skip(1).map(|d| d.reviews).sum::<f64>() > 0.0);
    assert!(after.new_finished_on.is_none());
    assert!(after.measured_time);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: crate::modules::planning::ID.into(),
        enabled: false,
    });
    assert_eq!(
        t.err::<PlanningForecast>(ForecastRequest::default()).code,
        ErrorCode::ConflictModuleDisabled
    );
}

/// `stats.today` compares today with yesterday; `stats.struggles` lists the cards answered
/// wrong in the window (any mode), most misses first, with a query that studies exactly them.
pub fn today_and_struggles(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("D");
    let other = t.deck("Other");
    let a = t.note(d.id, "<b>go</b>", None);
    let b = t.note(d.id, "see", None);
    let c = t.note(d.id, "eat", None);
    let old = t.note(d.id, "run", None);
    let elsewhere = t.note(other.id, "fly", None);
    let today_req = || TodayRequest {
        scope: StatsScope::default(),
        tz_offset_mins: Some(0),
    };
    let r = t.ok::<StatsToday>(today_req());
    assert_eq!((r.pace, r.today.answered), (TodayPace::NotStarted, 0));
    // Yesterday: 4 answers; today: a missed (normal), b right; earlier: b missed in a test,
    // `a` missed in practice, `old` missed 10 days ago, `elsewhere` missed today.
    let mut n = 0;
    let mut add = |card, at: i64, rating, mode: &str| {
        n += 1;
        t.add_review(n, card, at, rating, CardState::Review, mode);
    };
    for card in [a, b, c, c] {
        add(card, T0 - DAY, Rating::Good, "normal");
    }
    add(a, T0 - DAY + 60_000, Rating::Again, "trial");
    add(b, T0 - 3 * DAY, Rating::Again, "test");
    add(old, T0 - 10 * DAY, Rating::Again, "normal");
    add(a, T0 - 60_000, Rating::Again, "normal");
    add(b, T0 - 30_000, Rating::Good, "normal");
    add(elsewhere, T0 - 20_000, Rating::Again, "normal");
    let r = t.ok::<StatsToday>(today_req());
    assert_eq!((r.today.answered, r.today.correct), (3, 1));
    assert_eq!(r.yesterday.answered, 4);
    assert!(r.reference_is_yesterday);
    assert_eq!(r.pace, TodayPace::Behind, "3 so far vs 4 yesterday");
    add(c, T0 - 10_000, Rating::Good, "normal");
    let r = t.ok::<StatsToday>(today_req());
    assert_eq!((r.pace, r.today.answered, r.streak.current), (TodayPace::Same, 4, 2));
    let list = |days: Option<u32>, deck: Option<DeckId>| {
        t.ok::<StatsStruggles>(StrugglesRequest {
            scope: StatsScope {
                deck_id: deck,
                ..StatsScope::default()
            },
            days,
            limit: None,
            tz_offset_mins: Some(0),
        })
    };
    let s = list(None, Some(d.id));
    assert_eq!((s.days, s.total, s.today), (7, 2, 1));
    let ids: Vec<CardId> = s.items.iter().map(|i| i.card_id).collect();
    assert_eq!(ids, [a, b], "a: 2 misses (practice counts), b: 1");
    assert_eq!((s.items[0].term.as_str(), s.items[0].meaning.as_str()), ("go", "x"));
    assert_eq!((s.items[0].misses, s.items[0].answers), (2, 3));
    assert_eq!(s.items[0].deck_path, "D");
    assert_eq!(s.study_query.as_deref(), Some(format!("cid:{a},{b}").as_str()));
    // Today only; the whole collection; another deck; a wider window.
    assert_eq!(list(Some(1), Some(d.id)).items.iter().map(|i| i.card_id).collect::<Vec<_>>(), [a]);
    assert_eq!(list(None, None).total, 3);
    assert_eq!(list(Some(30), Some(d.id)).total, 3);
    let none = list(Some(1), Some(t.deck("Empty").id));
    assert!(none.items.is_empty() && none.study_query.is_none());
    // The query practises exactly those cards (a practice session: schedule untouched).
    let before = (t.card(a), t.card(b));
    let start: StartSession = serde_json::from_value(serde_json::json!({
        "kind": "custom",
        "mode": "trial",
        "deck_id": null,
        "filter": { "query": s.study_query, "due_only": false },
        "tz_offset_mins": 0,
    }))
    .unwrap();
    let session = t.ok::<StudyStart>(start);
    let mut seen = std::collections::BTreeSet::new();
    let mut cur = session.clone();
    while let Some(card) = cur.current.clone() {
        seen.insert(card.card_id);
        cur = t.ok::<StudyAnswer>(AnswerCard {
            session_id: cur.id,
            card_id: card.card_id,
            rating: Rating::Good,
            duration_ms: 1_000,
            reveal_ms: None,
        });
    }
    assert_eq!(seen, [a, b].into_iter().collect());
    assert_eq!((t.card(a), t.card(b)), before);
    // Suspended cards stay listed but are not studied.
    t.set_card(b, |c| c.suspended = true);
    let s = list(None, Some(d.id));
    assert!(s.items[1].suspended);
    assert_eq!(s.study_query.as_deref(), Some(format!("cid:{a}").as_str()));
}

/// "Her gün 50 kart": today's answers.
pub fn daily_cards_goal(f: &Factory) {
    let t = T::new(f);
    let d = t.deck("D");
    let card = t.note(d.id, "x", None);
    t.ok::<GoalsCreate>(CreateGoal {
        kind: GoalKind::DailyCards,
        target: 2.0,
        deadline: None,
        query: None,
    });
    let progress = || t.ok::<GoalsList>(ListGoals::default()).items[0].progress.clone();
    assert!(progress().daily && progress().current == 0.0);
    t.add_review(1, card, T0 - DAY, Rating::Good, CardState::Review, "normal");
    t.add_review(2, card, T0 - 1_000, Rating::Good, CardState::Review, "normal");
    assert_eq!(progress().current, 1.0);
    t.add_review(3, card, T0 - 500, Rating::Again, CardState::Review, "normal");
    assert!(progress().done);
}

macro_rules! v06_suite {
    ($factory:expr) => {
        v06_suite!(@tests $factory;
            cache_equals_recomputation,
            breakdown_by_classification,
            breakdown_cache_equals_recomputation,
            leeches_and_report_cards,
            target_retention_follows_presets,
            apply_to_leeches_covers_every_leech,
            load_balancing_spreads_equal_cards,
            exam_mode_and_exam_commands,
            goals_progress_and_undo,
            optimizer_job_saves_weights,
            optimizer_job_cancelled_before_it_runs,
            modules_can_be_switched_off,
            forecast_and_apply_pace,
            today_and_struggles,
            daily_cards_goal,
        );
    };
    (@tests $factory:expr; $($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                let factory = $factory;
                super::$name(&factory);
            }
        )*
    };
}

mod mem {
    use crate::testing::MemStore;
    use fc_storage::Store;
    use std::sync::Arc;
    v06_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite_memory {
    use fc_storage::{SqliteStore, Store};
    use std::sync::Arc;
    v06_suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

mod sqlite_file {
    use crate::testing::TempSqlite;
    use fc_storage::Store;
    use std::sync::Arc;
    v06_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });

    /// Needs a real file (background worker + readers): SQLite file only.
    #[test]
    fn optimizer_job_cancelled_while_training() {
        super::optimizer_job_cancelled_while_training(&|| -> Arc<dyn Store> {
            Arc::new(TempSqlite::new())
        });
    }
}

/// Statistics benchmark (docs/03 §17: 200k cards / 2M reviews). Builds a file SQLite
/// collection directly through the storage layer, then times what the statistics screen does.
/// Full size: `cargo test -p fc-app --release stats_2m -- --ignored --nocapture`; the CI
/// variant (1/10 size) runs with every `cargo test`.
mod bench {
    use super::*;
    use crate::testing::TempSqlite;
    use fc_core::note::searchable_text;
    use std::time::Instant;

    /// Milliseconds, best of `runs`.
    fn best<R>(runs: usize, mut f: impl FnMut() -> R) -> f64 {
        (0..runs)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(f());
                t.elapsed().as_secs_f64() * 1000.0
            })
            .fold(f64::INFINITY, f64::min)
    }

    fn review_id(n: u64) -> ReviewLogId {
        ReviewLogId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0000 | u128::from(n)))
    }

    /// `cards` review cards in 4 decks; `reviews` reviews spread over the last two years
    /// (oldest first, ~10% "again", some imported).
    fn world(cards: usize, reviews: usize) -> (T, DeckId) {
        let f: Box<Factory> = Box::new(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
        let t = T::new(&f);
        let decks: Vec<DeckId> = (0..4).map(|i| t.deck(&format!("Deste{i}")).id).collect();
        let first = t.note(decks[0], "seed", None);
        let mut rng = SplitMix64::new(2026);
        let mut roll = |n: u64| fc_core::Rng::next_u64(&mut rng) % n;
        let mut tx = t.store.write().unwrap();
        let card0 = tx.get_card(first).unwrap().unwrap();
        let note0 = tx.get_note(card0.note_id).unwrap().unwrap();
        let nt = tx.get_notetype(note0.notetype_id).unwrap().unwrap();
        let mut ids = vec![first];
        for i in 1..cards as u128 {
            let mut note = note0.clone();
            note.id = fc_core::NoteId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0001_0000_0000 | i));
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
            tx.insert_note(&note, &searchable_text(&nt, &note.fields)).unwrap();
            tx.insert_card(&card).unwrap();
            ids.push(card.id);
        }
        for (k, id) in ids.iter().enumerate() {
            let mut c = tx.get_card(*id).unwrap().unwrap();
            let s = &mut c.scheduling;
            s.state = CardState::Review;
            s.stability = Some(1.0 + roll(2_000) as f64 / 10.0);
            s.difficulty = Some(1.0 + roll(90) as f64 / 10.0);
            s.last_review = Some(Timestamp(T0 - (1 + roll(60)) as i64 * DAY));
            s.due = Some(Timestamp(T0 + (k % 90) as i64 * DAY - 10 * DAY));
            s.reps = 10;
            s.lapses = u32::try_from(roll(12)).unwrap();
            tx.update_card(&c).unwrap();
        }
        let span = 730 * DAY;
        let mut batch = Vec::with_capacity(10_000);
        for n in 0..reviews as u64 {
            let at = T0 - span + (i64::try_from(n).unwrap() * span) / i64::try_from(reviews).unwrap();
            let again = roll(10) == 0;
            batch.push(ReviewLogRecord {
                id: review_id(n),
                card_id: ids[(roll(ids.len() as u64)) as usize],
                session_id: None,
                rating: if again { Rating::Again } else { Rating::Good },
                state_before: if n % 50 == 0 { CardState::New } else { CardState::Review },
                state_after: CardState::Review,
                scheduled_days: u32::try_from(1 + roll(60)).unwrap(),
                elapsed_days: u32::try_from(roll(60)).unwrap(),
                duration_ms: u32::try_from(2_000 + roll(15_000)).unwrap(),
                answer_duration_ms: None,
                hints_used: 0,
                mode: if n % 20 == 0 { "imported" } else { "normal" }.into(),
                stability_after: Some(10.0),
                difficulty_after: Some(5.0),
                created_at: Timestamp(at),
            });
            if batch.len() == batch.capacity() {
                tx.insert_review_logs(&batch).unwrap();
                batch.clear();
            }
        }
        tx.insert_review_logs(&batch).unwrap();
        tx.commit().unwrap();
        (t, decks[1])
    }

    #[derive(Debug)]
    struct Timings {
        rebuild: f64,
        cached: f64,
        fresh: f64,
        fold_1k: f64,
        deck_scope: f64,
    }

    fn run(cards: usize, reviews: usize) -> Timings {
        let setup = Instant::now();
        let (t, deck) = world(cards, reviews);
        let setup = setup.elapsed().as_secs_f64();
        assert_eq!(t.total_reviews(), reviews);
        // Cache built from nothing (the first statistics screen after an upgrade).
        let rebuild = best(1, || {
            let info = t.refresh(0);
            assert!(info.rebuilt);
        });
        let cached = best(3, || {
            let o = t.overview(StatsScope::default(), 0, false);
            assert!(o.from_cache);
            o
        });
        let fresh = best(1, || t.overview(StatsScope::default(), 0, true));
        // A day of studying: 1000 appended answers folded into the cache.
        {
            let mut tx = t.store.write().unwrap();
            let rows: Vec<ReviewLogRecord> = (0..1_000u64)
                .map(|k| ReviewLogRecord {
                    id: review_id(reviews as u64 + k),
                    card_id: CardId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0001_0000_0001)),
                    session_id: None,
                    rating: Rating::Good,
                    state_before: CardState::Review,
                    state_after: CardState::Review,
                    scheduled_days: 5,
                    elapsed_days: 5,
                    duration_ms: 5_000,
                    answer_duration_ms: None,
                    hints_used: 0,
                    mode: "normal".into(),
                    stability_after: None,
                    difficulty_after: None,
                    created_at: Timestamp(T0 - DAY + i64::try_from(k).unwrap() * 1_000),
                })
                .collect();
            tx.insert_review_logs(&rows).unwrap();
            tx.commit().unwrap();
        }
        let fold_1k = best(1, || {
            let info = t.refresh(0);
            assert!(!info.rebuilt);
            assert_eq!(info.folded, 1_000);
        });
        let deck_scope = best(3, || {
            t.overview(
                StatsScope {
                    deck_id: Some(deck),
                    ..StatsScope::default()
                },
                0,
                false,
            )
        });
        let tm = Timings {
            rebuild,
            cached,
            fresh,
            fold_1k,
            deck_scope,
        };
        println!("stats bench: {cards} cards, {reviews} reviews (setup {setup:.1} s): {tm:?} (ms)");
        tm
    }

    /// docs/03 §17 size. Budgets for a release build on a desktop-class machine.
    #[test]
    #[ignore = "benchmark: builds a 200k-card / 2M-review database"]
    fn stats_2m_reviews() {
        let tm = run(200_000, 2_000_000);
        assert!(tm.cached < 1_000.0, "{tm:?}");
        assert!(tm.fold_1k < 1_000.0, "{tm:?}");
        assert!(tm.rebuild < 30_000.0, "{tm:?}");
        assert!(tm.deck_scope < 10_000.0, "{tm:?}");
    }

    /// CI regression check at 1/10 size (debug build, shared runners: generous budgets).
    #[test]
    fn stats_200k_reviews_scaled() {
        let tm = run(20_000, 200_000);
        assert!(tm.cached < 3_000.0, "{tm:?}");
        assert!(tm.fold_1k < 3_000.0, "{tm:?}");
        assert!(tm.rebuild < 30_000.0, "{tm:?}");
        assert!(tm.fresh < 30_000.0, "{tm:?}");
        assert!(tm.deck_scope < 10_000.0, "{tm:?}");
    }
}
