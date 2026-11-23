//! v1.3 study launcher (CONTRACT §1): `study.options`, `study.start { scope }`,
//! `CONFLICT_NOTHING_TO_STUDY`, waiting sessions, resume of every kind and today counts that
//! agree with the queue. Every scenario runs against the three stores (`launcher_suite!`).

use super::*;
use crate::modules::study::plan::SCOPES;
use crate::modules::study::{LEARN_AHEAD_MINS, NEW_PER_SESSION};
use fc_api::v1::decks::DeckTree;
use fc_api::v1::modes::{SaveTestDraft, StartModes, TestAnswerDto, TestOptions, TestPaperRequest};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;

impl T {
    fn options(&self, deck: Option<DeckId>) -> StudyOptions {
        self.options_with(deck, None, Some(0))
    }

    fn options_with(&self, deck: Option<DeckId>, subdecks: Option<bool>, tz: Option<i32>) -> StudyOptions {
        self.ok::<StudyOptionsGet>(StudyOptionsRequest {
            deck_id: deck,
            include_subdecks: subdecks,
            tz_offset_mins: tz,
        })
    }

    fn start_req(&self, kind: StudyKind, scope: Option<StudyScope>, deck: Option<DeckId>) -> StartSession {
        StartSession {
            kind,
            mode: None,
            deck_id: deck,
            include_subdecks: None,
            filter: None,
            from_session: None,
            new_limit: None,
            order: None,
            tz_offset_mins: Some(0),
            modes: None,
            scope,
        }
    }

    fn start_scope(&self, kind: StudyKind, scope: StudyScope, deck: Option<DeckId>) -> SessionDto {
        self.ok::<StudyStart>(self.start_req(kind, Some(scope), deck))
    }

    fn tree(&self) -> DeckTree {
        self.ok::<DecksTree>(Empty {})
    }

    /// Puts a card into a state directly (the store, not a command).
    fn put_card(&self, id: CardId, f: impl FnOnce(&mut Card)) {
        let mut card = self.card(id);
        f(&mut card);
        let mut tx = self.store.write().unwrap();
        tx.update_card(&card).unwrap();
        tx.commit().unwrap();
    }

    fn scope<'a>(&self, o: &'a StudyOptions, s: StudyScope) -> &'a StudyScopeOption {
        o.scopes.iter().find(|x| x.scope == s).unwrap()
    }

    fn served(s: &SessionDto) -> u32 {
        s.counts.new + s.counts.learning + s.counts.review
    }

    fn answer_all(&self, mut s: SessionDto, rating: Rating) -> SessionDto {
        while let Some(c) = s.current.clone() {
            s = self.answer(s.id, c.card_id, rating);
        }
        s
    }

    fn nothing(&self, req: StartSession) -> (String, serde_json::Value) {
        let e = self.err::<StudyStart>(req);
        assert_eq!(e.code, ErrorCode::ConflictNothingToStudy, "{e:?}");
        let d = serde_json::to_value(&e.details).unwrap();
        (
            d["reason"].as_str().unwrap_or_default().to_owned(),
            d["next_available_at"].clone(),
        )
    }
}

fn tree_counts(tree: &DeckTree, id: DeckId) -> fc_api::v1::decks::DeckCounts {
    fn find(n: &[fc_api::v1::decks::DeckTreeNode], id: DeckId) -> Option<fc_api::v1::decks::DeckCounts> {
        n.iter()
            .find_map(|x| if x.deck.id == id { Some(x.counts) } else { find(&x.children, id) })
    }
    find(&tree.roots, id).unwrap()
}

/// The owner's report: 21 new cards, default limits (20 new a day), 20 studied → Home said 1,
/// `study.start` served 0 and the session closed silently. Now every number is the queue's.
pub fn twenty_one_new_cards_with_a_limit_of_twenty(f: &Factory) {
    let t = T::new(f(), 101);
    let deck = t.deck("English");
    for i in 0..21 {
        t.note(deck.id, &format!("w{i}"), "x");
    }
    let o = t.options(Some(deck.id));
    let due = t.scope(&o, StudyScope::Due);
    assert_eq!((due.count, due.empty_reason, due.mode), (20, None, StudyMode::Scheduled));
    assert_eq!(t.scope(&o, StudyScope::New).count, 20);
    assert_eq!(t.scope(&o, StudyScope::All).count, 21);
    assert_eq!(tree_counts(&t.tree(), deck.id).today_total, 20);
    assert_eq!(t.ok::<DecksSummaries>(Empty {}).today.today_new, 20);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(T::served(&s), due.count, "the launcher's count is what the session serves");
    let s = t.answer_all(s, Rating::Easy);
    assert!(s.current.is_none() && s.next_due.is_none());
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });

    // 1 card left in the deck, but today's limit is used up: every number says 0.
    let o = t.options(Some(deck.id));
    let due = t.scope(&o, StudyScope::Due);
    let next_day = Timestamp(T0 - 10 * HOUR + 4 * HOUR + MILLIS_PER_DAY); // 04:00Z tomorrow
    assert_eq!(
        (due.count, due.empty_reason, due.next_available_at),
        (0, Some(StudyEmptyReason::DailyLimitReached), Some(next_day))
    );
    let new = t.scope(&o, StudyScope::New);
    assert_eq!((new.count, new.empty_reason), (0, Some(StudyEmptyReason::DailyLimitReached)));
    let c = tree_counts(&t.tree(), deck.id);
    assert_eq!((c.new, c.today_new, c.today_total, c.total), (0, 0, 0, 21));
    let today = t.ok::<DecksSummaries>(Empty {}).today;
    assert_eq!((today.new, today.today_total), (0, 0));
    let (reason, next) = t.nothing(t.start_req(StudyKind::Normal, None, Some(deck.id)));
    assert_eq!((reason.as_str(), next), ("daily_limit_reached", serde_json::json!(next_day.0)));
    // Practice is still offered: the 20 studied cards.
    assert_eq!(t.scope(&o, StudyScope::StudiedToday).count, 20);

    // Tomorrow: the last new card.
    t.clock.set(next_day);
    let o = t.options(Some(deck.id));
    assert_eq!(t.scope(&o, StudyScope::Due).count, 1);
    assert_eq!(tree_counts(&t.tree(), deck.id).today_total, 1);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(T::served(&s), 1);
}

/// Contract invariant: for each scope, the count `study.options` gives is what `study.start`
/// with that scope serves (normal kind), with reviews, learning and new cards mixed, a review
/// limit and buried siblings.
pub fn every_scope_count_is_what_start_serves(f: &Factory) {
    let t = T::new(f(), 102);
    let deck = t.deck("D");
    let nt = two_way(&t);
    let mut cards = Vec::new();
    for i in 0..6 {
        cards.extend(t.note_of(nt, deck.id, &format!("q{i}"), "a").cards.iter().map(|c| c.id));
    }
    for i in 0..5 {
        cards.push(t.note(deck.id, &format!("p{i}"), "b"));
    }
    // Some study: reviews due today, a learning card, a struggled card.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    let first = s.current.clone().unwrap().card_id;
    let s = t.answer(s.id, first, Rating::Again);
    let second = s.current.clone().unwrap().card_id;
    t.answer(s.id, second, Rating::Easy);
    t.advance(30 * MIN);
    for scope in SCOPES {
        let o = t.options(Some(deck.id));
        let opt = *t.scope(&o, scope);
        if opt.count == 0 {
            let (reason, _) = t.nothing(t.start_req(StudyKind::Normal, Some(scope), Some(deck.id)));
            let want = serde_json::to_value(opt.empty_reason.unwrap()).unwrap();
            assert_eq!(reason, want.as_str().unwrap(), "{scope:?}");
            continue;
        }
        let s = t.start_scope(StudyKind::Normal, scope, Some(deck.id));
        assert_eq!(T::served(&s), opt.count, "{scope:?}");
        assert_eq!(s.mode, opt.mode, "{scope:?}");
        t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    }
    let o = t.options(Some(deck.id));
    assert_eq!(t.scope(&o, StudyScope::StudiedToday).count, 2);
    assert_eq!(t.scope(&o, StudyScope::StruggledToday).count, 1);
    assert_eq!(t.scope(&o, StudyScope::All).count, 17, "practice includes buried siblings");
    // Home (whole collection) and the deck row agree with the queue too.
    let whole = t.options(None);
    assert_eq!(
        t.ok::<DecksSummaries>(Empty {}).today.today_total,
        t.scope(&whole, StudyScope::Due).count
    );
    assert_eq!(
        tree_counts(&t.tree(), deck.id).today_total,
        t.scope(&o, StudyScope::Due).count
    );
}

/// Only learning cards waiting for their step: the launcher says when, an empty start
/// changes nothing, and a session whose cards all wait stays open with `next_due`.
pub fn waiting_learning_cards(f: &Factory) {
    let t = T::new(f(), 103);
    let deck = t.deck("D");
    let a = t.note(deck.id, "a", "1");
    let b = t.note(deck.id, "b", "2");
    let due_at = t.now().plus_millis(2 * HOUR);
    t.put_card(a, |c| {
        c.scheduling.state = CardState::Learning;
        c.scheduling.due = Some(due_at);
    });
    // `b` is studied and graduates; `a` waits.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(T::served(&s), 2);
    assert_eq!(s.current.as_ref().unwrap().card_id, b);
    let s = t.answer(s.id, b, Rating::Easy);
    assert!(s.current.is_none());
    assert_eq!(s.ended_at, None, "waiting is not finished");
    let available_at = due_at.plus_millis(-20 * MIN);
    assert_eq!(s.next_due, Some(available_at));
    // The launcher: nothing now, the waiting card later; the open session is offered.
    let o = t.options(Some(deck.id));
    let due = t.scope(&o, StudyScope::Due);
    assert_eq!(
        (due.count, due.empty_reason, due.next_available_at),
        (0, Some(StudyEmptyReason::Waiting), Some(available_at))
    );
    assert_eq!(o.resumable.len(), 1);
    assert_eq!((o.resumable[0].answered, o.resumable[0].remaining), (1, 1));
    assert_eq!(o.resumable[0].session.id, s.id);
    // An empty start returns why and leaves the session open.
    let (reason, next) = t.nothing(t.start_req(StudyKind::Normal, Some(StudyScope::Due), Some(deck.id)));
    assert_eq!((reason.as_str(), next), ("waiting", serde_json::json!(available_at.0)));
    assert_eq!(t.ok::<StudyActive>(Empty {}).session.unwrap().id, s.id);
    assert_eq!(t.session(s.id).ended_at, None);
    // Its time comes: the session shows it.
    t.clock.set(available_at);
    let s = t.session(s.id);
    assert_eq!(s.current.unwrap().card_id, a);
    assert_eq!(t.scope(&t.options(Some(deck.id)), StudyScope::Due).count, 1);
}

/// Sub-decks: the deck's setting (or the request) decides, the same way for the launcher,
/// the session and the deck tree.
pub fn subdecks_follow_the_setting(f: &Factory) {
    let t = T::new(f(), 104);
    let parent = t.deck("P");
    let child = t.deck("P::C");
    for i in 0..3 {
        t.note(parent.id, &format!("p{i}"), "x");
    }
    for i in 0..2 {
        t.note(child.id, &format!("c{i}"), "x");
    }
    let due = |o: &StudyOptions| t.scope(o, StudyScope::Due).count;
    assert_eq!(due(&t.options(Some(parent.id))), 5);
    assert_eq!(due(&t.options_with(Some(parent.id), Some(false), Some(0))), 3);
    assert_eq!(due(&t.options(Some(child.id))), 2);
    let tree = t.tree();
    assert_eq!(tree_counts(&tree, parent.id).today_total, 5);
    assert_eq!(tree_counts(&tree, child.id).today_total, 2);
    // Sub-decks off for P: everything agrees on 3.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::Deck(parent.id),
        key: "decks.include_subdecks".into(),
        value: serde_json::json!(false),
    });
    assert_eq!(due(&t.options(Some(parent.id))), 3);
    assert_eq!(tree_counts(&t.tree(), parent.id).today_total, 3);
    assert_eq!(tree_counts(&t.tree(), parent.id).total, 5, "inventory still counts sub-decks");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(parent.id));
    assert_eq!(T::served(&s), 3);
    assert_eq!(t.options(Some(parent.id)).deck_name.as_deref(), Some("P"));
    assert_eq!(t.options(Some(child.id)).deck_name.as_deref(), Some("P::C"));
}

/// Module switches decide the kinds offered; a switched-off kind cannot start.
pub fn modes_follow_module_switches(f: &Factory) {
    let t = T::new(f(), 105);
    let deck = t.deck("D");
    t.note(deck.id, "a", "1");
    // v1.4: the modes are described (`StudyModeOption`); the quiz ("Test") is one of them.
    let kinds = |o: StudyOptions| o.modes.into_iter().map(|m| m.kind).collect::<Vec<_>>();
    assert_eq!(
        kinds(t.options(Some(deck.id))),
        [StudyKind::Normal, StudyKind::Quiz, StudyKind::Learn, StudyKind::Test, StudyKind::Match]
    );
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "learn".into(),
        enabled: false,
    });
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "match".into(),
        enabled: false,
    });
    assert_eq!(
        kinds(t.options(Some(deck.id))),
        [StudyKind::Normal, StudyKind::Quiz, StudyKind::Test]
    );
    let e = t.err::<StudyStart>(t.start_req(StudyKind::Learn, Some(StudyScope::All), Some(deck.id)));
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "study".into(),
        enabled: false,
    });
    let e = t.err::<StudyOptionsGet>(StudyOptionsRequest {
        deck_id: Some(deck.id),
        include_subdecks: None,
        tz_offset_mins: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
}

/// Every empty reason, and that neither an empty nor a failed start closes the unfinished
/// session.
pub fn empty_reasons_and_failed_starts(f: &Factory) {
    let t = T::new(f(), 106);
    let empty = t.deck("Empty");
    let o = t.options(Some(empty.id));
    for s in &o.scopes {
        assert_eq!((s.count, s.empty_reason), (0, Some(StudyEmptyReason::NoCards)), "{:?}", s.scope);
    }
    let deck = t.deck("D");
    let cards: Vec<CardId> = (0..3).map(|i| t.note(deck.id, &format!("q{i}"), "a")).collect();
    let o = t.options(Some(deck.id));
    assert_eq!(t.scope(&o, StudyScope::StudiedToday).empty_reason, Some(StudyEmptyReason::NoneStudiedToday));
    assert_eq!(
        t.scope(&o, StudyScope::StruggledToday).empty_reason,
        Some(StudyEmptyReason::NoneStruggledToday)
    );
    // An unfinished session…
    let open = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    let open = t.answer(open.id, open.current.clone().unwrap().card_id, Rating::Good);
    // …survives empty starts of every kind and scope…
    for (kind, scope) in [
        (StudyKind::Normal, None),
        (StudyKind::Normal, Some(StudyScope::All)),
        (StudyKind::Learn, Some(StudyScope::All)),
        (StudyKind::Test, None),
        (StudyKind::Match, Some(StudyScope::Due)),
    ] {
        let (reason, _) = t.nothing(t.start_req(kind, scope, Some(empty.id)));
        assert_eq!(reason, "no_cards", "{kind:?} {scope:?}");
    }
    // …and failed ones.
    let e = t.err::<StudyStart>(t.start_req(StudyKind::Custom, Some(StudyScope::Due), Some(deck.id)));
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    let after = t.session(open.id);
    assert_eq!(after.ended_at, None);
    assert_eq!(after, open);
    assert_eq!(t.ok::<StudyActive>(Empty {}).session.unwrap().id, open.id);
    // Nothing due: everything in review, due later.
    for c in &cards {
        t.put_card(*c, |c| {
            c.scheduling.state = CardState::Review;
            c.scheduling.due = Some(Timestamp(T0 + 5 * MILLIS_PER_DAY));
        });
    }
    let o = t.options(Some(deck.id));
    assert_eq!(t.scope(&o, StudyScope::Due).empty_reason, Some(StudyEmptyReason::NothingDue));
    assert_eq!(t.scope(&o, StudyScope::New).empty_reason, Some(StudyEmptyReason::NothingDue));
    assert_eq!(t.scope(&o, StudyScope::All).count, 3);
}

/// "Studied today" / "struggled today": distinct cards answered (rated Again) during the
/// local study day in any mode; practice that never touches `cards`.
pub fn practice_scopes_use_the_local_study_day(f: &Factory) {
    const IST: i32 = 180;
    let t = T::new(f(), 107);
    let deck = t.deck("D");
    let cards: Vec<CardId> = (0..4).map(|i| t.note(deck.id, &format!("q{i}"), "a")).collect();
    // 13:00 in Istanbul: study two cards (one Again), and one more in a trial session.
    let mut req = t.start_req(StudyKind::Normal, None, Some(deck.id));
    req.tz_offset_mins = Some(IST);
    let s = t.ok::<StudyStart>(req);
    let s = t.answer(s.id, s.current.clone().unwrap().card_id, Rating::Again);
    let s = t.answer(s.id, s.current.clone().unwrap().card_id, Rating::Easy);
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    let trial = t.ok::<StudyStart>(StartSession {
        mode: Some(StudyMode::Trial),
        filter: Some(StudyFilter {
            query: Some(format!("cid:{}", cards[3])),
            ..StudyFilter::default()
        }),
        tz_offset_mins: Some(IST),
        ..t.start_req(StudyKind::Custom, None, Some(deck.id))
    });
    t.answer(trial.id, cards[3], Rating::Good);
    let opts = |tz| t.options_with(Some(deck.id), None, tz);
    // The offset given to `study.start` is remembered for queries without one.
    let o = opts(None);
    assert_eq!(t.scope(&o, StudyScope::StudiedToday).count, 3, "any mode counts");
    assert_eq!(t.scope(&o, StudyScope::StruggledToday).count, 1);
    // 03:30 local next morning: still the same study day (it starts at 04:00).
    t.clock.set(Timestamp(T0 - 10 * HOUR + MILLIS_PER_DAY + 30 * MIN)); // 00:30Z
    assert_eq!(t.scope(&opts(Some(IST)), StudyScope::StudiedToday).count, 3);
    // 04:30 local: a new study day (a rolling 24 h would still count them).
    t.clock.set(Timestamp(T0 - 10 * HOUR + MILLIS_PER_DAY + 90 * MIN)); // 01:30Z
    let o = opts(Some(IST));
    let studied = t.scope(&o, StudyScope::StudiedToday);
    assert_eq!((studied.count, studied.empty_reason), (0, Some(StudyEmptyReason::NoneStudiedToday)));
    // Back on the day: practice the struggled card; asking for scheduled gets trial.
    t.clock.set(Timestamp(T0 + HOUR));
    let before = t.all_cards();
    let mut req = t.start_req(StudyKind::Normal, Some(StudyScope::StruggledToday), Some(deck.id));
    req.mode = Some(StudyMode::Scheduled);
    let p = t.ok::<StudyStart>(req);
    assert_eq!((p.mode, T::served(&p)), (StudyMode::Trial, 1));
    let p = t.answer_all(p, Rating::Good);
    assert!(p.answered >= 1);
    assert_eq!(t.all_cards(), before, "practice never changes cards");
    let last = t.history(cards[0]).reviews.into_iter().chain(t.history(cards[1]).reviews).max_by_key(|r| r.created_at).unwrap();
    assert_eq!(last.mode, "trial");
    // The set is frozen: studying more does not grow a running practice session.
    let all = t.start_scope(StudyKind::Normal, StudyScope::All, Some(deck.id));
    assert_eq!((all.mode, T::served(&all)), (StudyMode::Trial, 4));
    let before = t.all_cards();
    t.answer_all(all, Rating::Good);
    assert_eq!(t.all_cards(), before);
}

/// Every kind resumes: one open session per kind, a test keeps its saved answers, and the
/// launcher lists what is left.
pub fn every_kind_resumes(f: &Factory) {
    let t = T::new(f(), 108);
    let deck = t.deck("English");
    for i in 0..6 {
        t.note(deck.id, &format!("word{i}"), &format!("anlam{i}"));
    }
    let mut req = t.start_req(StudyKind::Test, Some(StudyScope::All), Some(deck.id));
    req.modes = Some(StartModes {
        test: Some(TestOptions {
            questions: Some(4),
            kinds: None,
        }),
        ..StartModes::default()
    });
    let test = t.ok::<StudyStart>(req);
    assert_eq!(test.mode, StudyMode::Trial);
    t.advance(MIN);
    let normal = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    let normal = t.answer(normal.id, normal.current.clone().unwrap().card_id, Rating::Easy);
    t.advance(MIN);
    let learn = t.start_scope(StudyKind::Learn, StudyScope::Due, Some(deck.id));
    // A flashcard session did not end the test or the learn session.
    for s in [test.id, normal.id, learn.id] {
        assert_eq!(t.session(s).ended_at, None);
    }
    // Save two answers on the paper.
    let paper = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: test.id,
        with_key: None,
    });
    assert_eq!(paper.draft, Some(Vec::new()));
    let answers: Vec<TestAnswerDto> = paper.questions[..2]
        .iter()
        .map(|q| TestAnswerDto {
            index: q.index,
            choice: Some(0),
            text: Some("x".into()),
            truth: Some(true),
            pairs: None,
        })
        .collect();
    t.ok::<ModesTestSaveDraft>(SaveTestDraft {
        session_id: test.id,
        answers,
    });
    let e = t.err::<ModesTestSaveDraft>(SaveTestDraft {
        session_id: test.id,
        answers: vec![TestAnswerDto {
            index: 99,
            ..TestAnswerDto::default()
        }],
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    // After a restart the paper still has them.
    let store = t.store.clone();
    drop(t);
    let t = T::new(store, 109);
    t.clock.set(Timestamp(T0 + 10 * MIN)); // the new process's clock goes on, not back

    let paper = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: test.id,
        with_key: None,
    });
    let draft = paper.draft.clone().unwrap();
    assert_eq!(draft.len(), 4);
    assert!(draft[..2].iter().all(|a| a.choice.is_some() || a.text.is_some() || a.truth.is_some()));
    assert!(draft[2..].iter().all(|a| *a == TestAnswerDto { index: a.index, ..TestAnswerDto::default() }));
    let o = t.options(Some(deck.id));
    let kinds: Vec<StudyKind> = o.resumable.iter().map(|r| r.session.kind).collect();
    assert_eq!(kinds, [StudyKind::Learn, StudyKind::Normal, StudyKind::Test], "newest first");
    let r = |k| o.resumable.iter().find(|r| r.session.kind == k).unwrap();
    assert_eq!((r(StudyKind::Test).answered, r(StudyKind::Test).remaining), (2, 2));
    assert_eq!((r(StudyKind::Normal).answered, r(StudyKind::Normal).remaining), (1, 5));
    assert_eq!(r(StudyKind::Normal).deck_name.as_deref(), Some("English"));
    assert_eq!(t.options(None).resumable.len(), 3, "the whole collection lists every deck's");
    let other = t.deck("Other");
    assert!(t.options(Some(other.id)).resumable.is_empty());
    // Submitted: nothing left to continue. A new start of a kind replaces that kind's.
    t.ok::<ModesTestSubmit>(fc_api::v1::modes::SubmitTest {
        session_id: test.id,
        answers: Vec::new(),
        duration_ms: 1_000,
    });
    let again = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert!(t.session(normal.id).ended_at.is_some());
    let kinds: Vec<SessionId> = t.options(Some(deck.id)).resumable.iter().map(|r| r.session.id).collect();
    assert_eq!(kinds, [again.id, learn.id]);
    // Blank test answers count as struggled today.
    assert_eq!(t.scope(&t.options(Some(deck.id)), StudyScope::StruggledToday).count, 4);
}

/// A preset with `new_per_day` new cards a day, assigned to `deck`.
fn new_per_day(t: &T, deck: DeckId, n: u32) {
    let preset = t.ok::<PresetsCreate>(fc_api::v1::presets::CreatePreset {
        name: format!("new {n} {deck}"),
        config: Some(fc_core::preset::PresetConfig {
            new_per_day: n,
            ..fc_core::preset::PresetConfig::default()
        }),
    });
    t.ok::<PresetsAssign>(fc_api::v1::presets::AssignPreset {
        deck_ids: vec![deck],
        preset_id: Some(preset.id),
    });
}

/// X1: the whole collection's queue (Home's headline, the launcher without a deck,
/// `decks.summaries.today`) is the union of what each deck serves under its own options —
/// not one built-in limit for all decks together.
pub fn the_collection_queue_is_each_decks_own(f: &Factory) {
    let t = T::new(f(), 110);
    let a = t.deck("A");
    let b = t.deck("B");
    let p = t.deck("P");
    let pc = t.deck("P::C");
    let e = t.deck("E");
    let ef = t.deck("E::F");
    for i in 0..21 {
        t.note(a.id, &format!("a{i}"), "x");
    }
    for (deck, n) in [(b.id, 3), (p.id, 2), (pc.id, 2), (e.id, 1), (ef.id, 2)] {
        for i in 0..n {
            t.note(deck, &format!("{deck}-{i}"), "x");
        }
    }
    // E studies without its sub-deck: F counts on its own.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::Deck(e.id),
        key: "decks.include_subdecks".into(),
        value: serde_json::json!(false),
    });
    // B allows only 2 new cards a day; A the default 20 (21 cards).
    new_per_day(&t, b.id, 2);
    let due = |o: &StudyOptions| t.scope(o, StudyScope::Due).count;
    let per_deck = |tree: &DeckTree| {
        // Every deck the collection is made of, once (E without F; F on its own).
        [a.id, b.id, p.id, e.id, ef.id].map(|d| tree_counts(tree, d))
    };
    let check = |want: u32| {
        let tree = t.tree();
        let decks = per_deck(&tree);
        let sum = |f: fn(&fc_api::v1::decks::DeckCounts) -> u32| decks.iter().map(f).sum::<u32>();
        let today = t.ok::<DecksSummaries>(Empty {}).today;
        assert_eq!(sum(|c| c.today_total), want, "sum of the decks");
        assert_eq!(
            (today.today_new, today.today_learning, today.today_due, today.today_total),
            (sum(|c| c.today_new), sum(|c| c.today_learning), sum(|c| c.today_due), want),
            "Home = the decks' sum"
        );
        assert_eq!(due(&t.options(None)), want, "the launcher without a deck");
    };
    assert_eq!(tree_counts(&t.tree(), a.id).today_total, 20);
    assert_eq!(tree_counts(&t.tree(), b.id).today_total, 2);
    assert_eq!(tree_counts(&t.tree(), p.id).today_total, 4, "P with its sub-deck");
    assert_eq!(tree_counts(&t.tree(), e.id).today_total, 1);
    assert_eq!(tree_counts(&t.tree(), ef.id).today_total, 2);
    // 20 + 2 + 4 + 1 + 2: more than one built-in limit of 20 for everything.
    check(29);

    // Studying A to its limit leaves every other deck's count as it was.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(a.id));
    assert_eq!(T::served(&s), 20);
    t.answer_all(s, Rating::Easy);
    assert_eq!(tree_counts(&t.tree(), a.id).today_total, 0);
    assert_eq!(tree_counts(&t.tree(), b.id).today_total, 2);
    check(9);

    // A collection session serves exactly that: cards of every deck, taken in turn.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    assert_eq!(T::served(&s), 9);
    let mut decks = Vec::new();
    let mut s = s;
    while let Some(c) = s.current.clone() {
        decks.push(t.card(c.card_id).deck_id);
        s = t.answer(s.id, c.card_id, Rating::Easy);
    }
    assert_eq!(decks.len(), 9, "every card the launcher promised was served");
    assert_ne!(decks[0], decks[1], "decks are interleaved");
    check(0);
    let o = t.options(None);
    assert_eq!(t.scope(&o, StudyScope::Due).empty_reason, Some(StudyEmptyReason::DailyLimitReached));
}

/// X1: a collection-wide cap applies only when the user set one (`study.new_per_session`
/// app-wide), and then the launcher and the session agree on it.
pub fn a_collection_cap_only_when_set(f: &Factory) {
    let t = T::new(f(), 111);
    let a = t.deck("A");
    let b = t.deck("B");
    for i in 0..5 {
        t.note(a.id, &format!("a{i}"), "x");
        t.note(b.id, &format!("b{i}"), "x");
    }
    assert_eq!(t.scope(&t.options(None), StudyScope::Due).count, 10);
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: NEW_PER_SESSION.into(),
        value: serde_json::json!(6),
    });
    let o = t.options(None);
    assert_eq!(t.scope(&o, StudyScope::Due).count, 6);
    assert_eq!(t.scope(&o, StudyScope::New).count, 6);
    assert_eq!(t.ok::<DecksSummaries>(Empty {}).today.today_total, 6);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    assert_eq!(T::served(&s), 6);
    // Each deck alone still offers its own five.
    assert_eq!(t.scope(&t.options(Some(a.id)), StudyScope::Due).count, 5);
}

/// X1: the learn-ahead window is a setting (`study.learn_ahead_mins`, default 20) and every
/// "when" is the moment the card is really served — both sides of the boundary.
pub fn learn_ahead_is_a_setting_and_the_wait_is_truthful(f: &Factory) {
    let t = T::new(f(), 112);
    let deck = t.deck("D");
    let a = t.note(deck.id, "a", "1");
    let due_at = t.now().plus_millis(10 * MIN);
    t.put_card(a, |c| {
        c.scheduling.state = CardState::Learning;
        c.scheduling.due = Some(due_at);
    });
    // Default 20 minutes: a card due in 10 is served now, no waiting screen.
    let o = t.options(Some(deck.id));
    assert_eq!(t.scope(&o, StudyScope::Due).count, 1);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(s.current.as_ref().map(|c| c.card_id), Some(a));
    assert_eq!(s.next_due, None);
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });

    // 5 minutes: the card really waits, until due − 5 min.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: LEARN_AHEAD_MINS.into(),
        value: serde_json::json!(5),
    });
    let available_at = due_at.plus_millis(-5 * MIN);
    let o = t.options(Some(deck.id));
    let due = t.scope(&o, StudyScope::Due);
    assert_eq!(
        (due.count, due.empty_reason, due.next_available_at),
        (0, Some(StudyEmptyReason::Waiting), Some(available_at))
    );
    let (reason, next) = t.nothing(t.start_req(StudyKind::Normal, None, Some(deck.id)));
    assert_eq!((reason.as_str(), next), ("waiting", serde_json::json!(available_at.0)));
    // A session whose last card waits: its countdown ends exactly when the card comes.
    let b = t.note(deck.id, "b", "2");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(s.current.as_ref().unwrap().card_id, b);
    let s = t.answer(s.id, b, Rating::Easy);
    assert_eq!((s.current.is_none(), s.ended_at, s.next_due), (true, None, Some(available_at)));
    t.clock.set(available_at.plus_millis(-1));
    assert!(t.session(s.id).current.is_none(), "one ms early: still waiting");
    t.clock.set(available_at);
    assert_eq!(t.session(s.id).current.unwrap().card_id, a, "on time: served");

    // 0: always wait for the step itself.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: LEARN_AHEAD_MINS.into(),
        value: serde_json::json!(0),
    });
    let o = t.options(Some(deck.id));
    assert_eq!(t.scope(&o, StudyScope::Due).next_available_at, Some(due_at));
    let e = t.err::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: LEARN_AHEAD_MINS.into(),
        value: serde_json::json!(-1),
    });
    assert_eq!(e.code, ErrorCode::ValidationSettingValue);
}

/// v1.3 closing pass: a note's cards in different decks are not both in the collection's
/// queue when their deck buries siblings (as within one deck); each deck alone still offers
/// its own card.
pub fn siblings_across_decks_in_the_collection_queue(f: &Factory) {
    let t = T::new(f(), 113);
    let nt = super::two_way(&t);
    let a = t.deck("A");
    let b = t.deck("B");
    let note = t.note_of(nt, a.id, "dog", "köpek");
    assert_eq!(note.cards.len(), 2);
    t.put_card(note.cards[1].id, |c| c.deck_id = b.id);
    let due = |d: Option<DeckId>| t.scope(&t.options(d), StudyScope::Due).count;
    assert_eq!((due(Some(a.id)), due(Some(b.id))), (1, 1));
    assert_eq!(due(None), 1, "siblings buried across decks");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    assert_eq!(T::served(&s), 1);
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    // B does not bury siblings: its card stays next to A's.
    let preset = t.ok::<PresetsCreate>(fc_api::v1::presets::CreatePreset {
        name: "no bury".into(),
        config: Some(fc_core::preset::PresetConfig {
            bury_siblings: false,
            ..fc_core::preset::PresetConfig::default()
        }),
    });
    t.ok::<PresetsAssign>(fc_api::v1::presets::AssignPreset {
        deck_ids: vec![b.id],
        preset_id: Some(preset.id),
    });
    assert_eq!(due(None), 2);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    assert_eq!(T::served(&s), 2);
}

/// v1.3 closing pass: the planning forecast's day 0 is the launcher's count (same planner),
/// for a deck and for the whole collection, and later days keep the presets' limits.
pub fn forecast_today_is_the_launchers_count(f: &Factory) {
    use fc_api::v1::planning::ForecastRequest;
    let t = T::new(f(), 114);
    let a = t.deck("A");
    let b = t.deck("B");
    for i in 0..25 {
        t.note(a.id, &format!("a{i}"), "x");
    }
    let now = t.now();
    for i in 0..12 {
        let id = t.note(b.id, &format!("b{i}"), "x");
        t.put_card(id, |c| {
            c.scheduling.state = CardState::Review;
            c.scheduling.due = Some(now.plus_millis(-MILLIS_PER_DAY));
            c.scheduling.stability = Some(10.0);
            c.scheduling.difficulty = Some(5.0);
            c.scheduling.last_review = Some(now.plus_millis(-11 * MILLIS_PER_DAY));
            c.scheduling.reps = 3;
        });
    }
    // B: 5 reviews a day; A: the default 20 new a day.
    let preset = t.ok::<PresetsCreate>(fc_api::v1::presets::CreatePreset {
        name: "5 reviews".into(),
        config: Some(fc_core::preset::PresetConfig {
            reviews_per_day: 5,
            ..fc_core::preset::PresetConfig::default()
        }),
    });
    t.ok::<PresetsAssign>(fc_api::v1::presets::AssignPreset {
        deck_ids: vec![b.id],
        preset_id: Some(preset.id),
    });
    let forecast = |deck: Option<DeckId>| {
        t.ok::<PlanningForecast>(ForecastRequest {
            deck_id: deck,
            tz_offset_mins: Some(0),
            days: Some(10),
            ..ForecastRequest::default()
        })
    };
    let due = |d: Option<DeckId>| t.scope(&t.options(d), StudyScope::Due).count;
    for deck in [None, Some(a.id), Some(b.id)] {
        let f = forecast(deck);
        assert_eq!(f.days[0].cards, f64::from(due(deck)), "day 0 of {deck:?}");
    }
    assert_eq!(due(None), 25);
    let fb = forecast(Some(b.id));
    assert_eq!(fb.days[0].reviews, 5.0);
    assert!(fb.days.iter().all(|d| d.reviews <= 5.0 + 1e-9), "reviews/day kept");
    assert_eq!(fb.days[1].reviews, 5.0, "the rest of today's reviews come tomorrow");
    let fa = forecast(Some(a.id));
    assert!(fa.days.iter().all(|d| d.new_cards <= 20.0 + 1e-9), "new/day kept");
    // After studying, day 0 follows the queue.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(b.id));
    t.answer_all(s, Rating::Good);
    assert_eq!(due(None), 20);
    assert_eq!(forecast(None).days[0].cards, 20.0);
}

/// v1.3 closing pass: lowering `study.learn_ahead_mins` while a learning card is on screen
/// never takes that card away or makes its answer fail; the new value counts from the next
/// card.
pub fn lowering_learn_ahead_keeps_the_card_on_screen(f: &Factory) {
    let t = T::new(f(), 115);
    let deck = t.deck("D");
    let a = t.note(deck.id, "a", "1");
    let b = t.note(deck.id, "b", "2");
    let due_at = t.now().plus_millis(10 * MIN);
    for id in [a, b] {
        t.put_card(id, |c| {
            c.scheduling.state = CardState::Learning;
            c.scheduling.due = Some(due_at);
        });
    }
    // Default 20 min: a card due in 10 is shown now.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    let shown = s.current.as_ref().unwrap().card_id;
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: LEARN_AHEAD_MINS.into(),
        value: serde_json::json!(0),
    });
    // The session read again still shows it, and its answer is taken.
    assert_eq!(t.session(s.id).current.map(|c| c.card_id), Some(shown));
    let s = t.answer(s.id, shown, Rating::Good);
    assert_eq!(t.history(shown).reviews.len(), 1, "the answer was saved");
    // The next card is chosen with the new value: the other one waits for its step.
    assert!(s.current.is_none());
    assert_eq!(s.ended_at, None);
    assert_eq!(s.next_due, Some(due_at));
    // Raising it again applies at once.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: LEARN_AHEAD_MINS.into(),
        value: serde_json::json!(20),
    });
    assert!(t.session(s.id).current.is_some());
}

/// v1.4 Z2: the planner reads the cards in progress once and shares them across the plans of
/// a deck tree. On random worlds its candidates equal the storage read of the same filter
/// (the old path, kept as the reference), and every tree node's numbers equal a planner made
/// for that deck alone.
pub fn shared_candidates_equal_the_storage_read(f: &Factory) {
    use crate::modules::study::ext::View;
    use crate::modules::study::plan::{today_counts, Planner};
    use fc_core::card::Card;
    use fc_core::rng::{Rng, SplitMix64};
    use fc_storage::repo::{NewCardsFirst, StudyCardFilter};
    for seed in 0..4u64 {
        let t = T::new(f(), 130 + seed);
        let mut rng = SplitMix64::new(seed);
        let p = t.deck("P");
        let decks = [p.id, t.deck("P::A").id, t.deck("P::A::X").id, t.deck("Q").id];
        let mut cards = Vec::new();
        for i in 0..60 {
            let d = decks[(rng.next_u64() % 4) as usize];
            cards.push(t.note(d, &format!("w{seed}-{i}"), "x"));
        }
        for c in &cards {
            let roll = rng.next_u64() % 5;
            let due = Timestamp(T0 + (rng.next_u64() % 48) as i64 * HOUR - 24 * HOUR);
            let suspended = rng.next_u64() % 10 == 0;
            t.put_card(*c, |card| {
                match roll {
                    0 => card.scheduling.state = CardState::Review,
                    1 => card.scheduling.state = CardState::Learning,
                    2 => card.scheduling.state = CardState::Relearning,
                    _ => {}
                }
                if roll < 3 {
                    card.scheduling.due = Some(due);
                }
                card.suspended = suspended;
            });
        }
        let tree = t.tree();
        let tx = t.store.read().unwrap();
        let ctx = crate::handler::QueryCtx {
            tx: &*tx,
            now: fc_core::time::Clock::now(&*t.clock),
            locale: fc_core::i18n::Locale::default(),
            registry: t.app.registry(),
        };
        let now = ctx.now;
        let mut planner = Planner::new(View::query(&ctx), now, 0).unwrap();
        let day_end = Timestamp(now.0 + 6 * HOUR);
        for subset in [&decks[..1], &decks[1..3], &decks[..], &decks[3..]] {
            for states in [None, Some(vec![CardState::New]), Some(vec![CardState::Review, CardState::Learning])] {
                for new_first in [None, Some(NewCardsFirst { n: 3, newest: false }), Some(NewCardsFirst { n: 2, newest: true })] {
                    let filter = StudyCardFilter {
                        deck_ids: Some(subset.to_vec()),
                        states: states.clone(),
                        due_before: Some(day_end),
                        new_first,
                        ..StudyCardFilter::default()
                    };
                    let ids = |v: Vec<Card>| v.into_iter().map(|c| c.id).collect::<Vec<_>>();
                    assert_eq!(
                        ids(planner.candidates(&filter).unwrap()),
                        ids(ctx.tx.study_cards(&filter).unwrap()),
                        "seed {seed}"
                    );
                }
            }
        }
        for d in decks {
            let mut alone = Planner::new(View::query(&ctx), now, 0).unwrap();
            let (new, learning, due) = today_counts(&mut alone, Some(d)).unwrap();
            let c = tree_counts(&tree, d);
            assert_eq!((c.new, c.learning, c.due), (new, learning, due), "seed {seed}");
        }
    }
}

macro_rules! launcher_suite {
    ($factory:expr) => {
        launcher_suite!(@tests $factory;
            twenty_one_new_cards_with_a_limit_of_twenty,
            every_scope_count_is_what_start_serves,
            waiting_learning_cards,
            subdecks_follow_the_setting,
            modes_follow_module_switches,
            empty_reasons_and_failed_starts,
            practice_scopes_use_the_local_study_day,
            every_kind_resumes,
            the_collection_queue_is_each_decks_own,
            a_collection_cap_only_when_set,
            learn_ahead_is_a_setting_and_the_wait_is_truthful,
            siblings_across_decks_in_the_collection_queue,
            forecast_today_is_the_launchers_count,
            lowering_learn_ahead_keeps_the_card_on_screen,
            shared_candidates_equal_the_storage_read,
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
    launcher_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite_memory {
    use fc_storage::{SqliteStore, Store};
    use std::sync::Arc;
    launcher_suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

mod sqlite_file {
    use crate::testing::TempSqlite;
    use fc_storage::Store;
    use std::sync::Arc;
    launcher_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}
