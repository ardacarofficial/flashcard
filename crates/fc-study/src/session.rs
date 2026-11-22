//! The study queue (docs/02 §6, docs/03 §9): which cards a session shows and in what order.
//!
//! Pure data + functions. The app persists [`SessionState`] as JSON in `sessions.state` after
//! every change (crash recovery) and puts the previous state into each answer's inverse, so
//! undo brings the session back exactly with the card.
//!
//! Queue = intraday (re)learning cards, then due reviews, then new cards. Learning entries
//! carry their due time and become available once due (or within the learn-ahead window when
//! nothing else is left). With sibling burying on, a note contributes one review/new card per
//! session (learning cards are always kept: they are in progress).

use fc_core::card::{Card, CardState, SchedulingState};
use fc_core::preset::{NewCardOrder, ReviewOrder};
use fc_core::rng::Rng;
use fc_core::{CardId, NoteId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// How long ahead a waiting learning card may be shown when nothing else is left, ms: the
/// default of the app's `study.learn_ahead_mins` setting (v1.3), which callers pass to
/// [`next_entry`] / [`available_at`].
pub const LEARN_AHEAD_MS: i64 = 20 * 60_000;

/// Longest answer time recorded for one card, ms (longer = the user walked away).
pub const MAX_ANSWER_MS: u32 = 60 * 60_000;

/// Queue category of an entry (drives the three counters).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueKind {
    /// (Re)learning, intraday.
    Learning,
    /// Due review.
    Review,
    /// Never studied.
    New,
}

impl QueueKind {
    /// Category of a scheduling state.
    pub const fn of(state: CardState) -> Self {
        match state {
            CardState::New => Self::New,
            CardState::Learning | CardState::Relearning => Self::Learning,
            CardState::Review => Self::Review,
        }
    }
}

/// One queued card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntry {
    /// Card.
    pub card_id: CardId,
    /// Its note (sibling handling).
    pub note_id: NoteId,
    /// Category.
    pub kind: QueueKind,
    /// Not before (intraday learning); `None` = available now.
    pub due: Option<Timestamp>,
}

/// Simulated scheduling state of a card in a trial session (`cards` is never written).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialCard {
    /// Card.
    pub card_id: CardId,
    /// Simulated state.
    pub state: SchedulingState,
}

/// Persistent state of a session (`sessions.state`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    /// Remaining cards.
    pub queue: Vec<QueueEntry>,
    /// Cards answered.
    pub answered: u32,
    /// Answers other than "Again".
    pub correct: u32,
    /// Total answer time, ms.
    pub time_ms: u64,
    /// Cards rated "Again" at least once, in first-failure order ("repeat mistakes").
    pub again: Vec<CardId>,
    /// Card the hint counter belongs to.
    pub hint_card: Option<CardId>,
    /// Hints shown for `hint_card`.
    pub hints: u32,
    /// Last answer (or the start): an answer can not have taken longer than since then.
    pub last_event_at: Option<Timestamp>,
    /// Trial sessions: simulated states of the cards answered so far.
    pub trial: Vec<TrialCard>,
    /// v0.10 study modes (learn progress, test paper, timed-out count); absent for plain
    /// sessions, so their persisted state is unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modes: Option<crate::modes::ModesState>,
    /// v1.3: the learn-ahead window (ms) the card on screen was chosen with — the app's
    /// `study.learn_ahead_mins` at the start or the last answer, widened (never narrowed) by
    /// saves in between. A card the session showed stays answerable when the setting is
    /// lowered while it is on screen; the new value chooses from the next card. Absent in
    /// sessions from before v1.3 (the setting alone).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learn_ahead_ms: Option<i64>,
}

/// Order of reviews and new cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Reviews by due date, new cards in the order they were added.
    #[default]
    Due,
    /// Shuffled (seeded).
    Random,
    /// Highest difficulty / most lapses first.
    HardestFirst,
    /// Most recently added first.
    NewestFirst,
    /// A deck preset's orders (`fc_core::preset`): new cards as added or shuffled; reviews by
    /// due date, shuffled, or lowest recall probability first.
    Preset {
        /// New card order.
        new: NewCardOrder,
        /// Review order.
        review: ReviewOrder,
    },
}

/// Inputs of [`build_queue`].
pub struct BuildOptions<'a> {
    /// Now.
    pub now: Timestamp,
    /// End of the study day ([`crate::day::day_end`]).
    pub day_end: Timestamp,
    /// Only cards due today (`false` = every candidate: cramming / custom sessions).
    pub due_only: bool,
    /// At most this many new cards (`None` = no limit).
    pub new_limit: Option<u32>,
    /// At most this many review cards (`None` = no limit). Learning cards are never limited.
    pub review_limit: Option<u32>,
    /// Order.
    pub order: Order,
    /// One review/new card per note.
    pub bury_siblings: bool,
    /// Randomness for [`Order::Random`].
    pub rng: &'a mut dyn Rng,
}

fn shuffle<T>(items: &mut [T], rng: &mut dyn Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        items.swap(i, j);
    }
}

/// Builds the queue from candidate cards (live, not suspended — storage pre-filters; buried
/// cards are skipped here too). `state_of` gives the scheduling state to use per card (the
/// card's own, or the simulated one in trial sessions).
pub fn build_queue(
    cards: &[Card],
    state_of: impl Fn(&Card) -> SchedulingState,
    opts: BuildOptions<'_>,
) -> Vec<QueueEntry> {
    let mut learning: Vec<(&Card, SchedulingState)> = Vec::new();
    let mut reviews: Vec<(&Card, SchedulingState)> = Vec::new();
    let mut new: Vec<(&Card, SchedulingState)> = Vec::new();
    for card in cards {
        if card.suspended || card.deleted_at.is_some() {
            continue;
        }
        if card.buried_until.is_some_and(|b| b > opts.now) {
            continue;
        }
        let s = state_of(card);
        let due_today = s.due.is_none_or(|d| d < opts.day_end);
        match s.state {
            CardState::New => new.push((card, s)),
            CardState::Learning | CardState::Relearning if due_today || !opts.due_only => {
                learning.push((card, s));
            }
            CardState::Review if due_today || !opts.due_only => reviews.push((card, s)),
            _ => {}
        }
    }
    let key = |c: &Card| (c.created_at, c.id);
    learning.sort_by_key(|(c, s)| (s.due, key(c)));
    match opts.order {
        Order::Due => {
            reviews.sort_by_key(|(c, s)| (s.due, key(c)));
            new.sort_by_key(|(c, _)| key(c));
        }
        Order::Random => {
            reviews.sort_by_key(|(c, _)| key(c));
            new.sort_by_key(|(c, _)| key(c));
            shuffle(&mut reviews, opts.rng);
            shuffle(&mut new, opts.rng);
        }
        Order::HardestFirst => {
            // Most lapses first, then highest difficulty (unknown last).
            let d = |s: &SchedulingState| s.difficulty.unwrap_or(f64::NEG_INFINITY);
            reviews.sort_by(|(a, sa), (b, sb)| {
                sb.lapses
                    .cmp(&sa.lapses)
                    .then_with(|| d(sb).total_cmp(&d(sa)))
                    .then_with(|| key(a).cmp(&key(b)))
            });
            new.sort_by_key(|(c, _)| key(c));
        }
        Order::NewestFirst => {
            reviews.sort_by_key(|(c, _)| std::cmp::Reverse(key(c)));
            new.sort_by_key(|(c, _)| std::cmp::Reverse(key(c)));
        }
        Order::Preset { new: n, review } => {
            match review {
                ReviewOrder::Due => reviews.sort_by_key(|(c, s)| (s.due, key(c))),
                ReviewOrder::Random => {
                    reviews.sort_by_key(|(c, _)| key(c));
                    shuffle(&mut reviews, opts.rng);
                }
                ReviewOrder::Retrievability => {
                    // Recall probability falls monotonically with elapsed time / stability
                    // (FSRS forgetting curve), so the largest ratio is the least likely
                    // recalled. Cards without a stability go last.
                    let overdue = |s: &SchedulingState| match (s.stability, s.last_review) {
                        (Some(st), Some(last)) if st > 0.0 => {
                            (opts.now.0 - last.0).max(0) as f64 / st
                        }
                        _ => f64::NEG_INFINITY,
                    };
                    reviews.sort_by(|(a, sa), (b, sb)| {
                        overdue(sb)
                            .total_cmp(&overdue(sa))
                            .then_with(|| key(a).cmp(&key(b)))
                    });
                }
            }
            new.sort_by_key(|(c, _)| key(c));
            if n == NewCardOrder::Random {
                shuffle(&mut new, opts.rng);
            }
        }
    }
    let mut notes: HashSet<NoteId> = HashSet::new();
    let mut queue = Vec::new();
    for (c, s) in learning {
        notes.insert(c.note_id);
        queue.push(QueueEntry {
            card_id: c.id,
            note_id: c.note_id,
            kind: QueueKind::Learning,
            due: s.due.filter(|d| *d > opts.now),
        });
    }
    let mut take = |c: &Card, kind: QueueKind, queue: &mut Vec<QueueEntry>| -> bool {
        if opts.bury_siblings && !notes.insert(c.note_id) {
            return false;
        }
        queue.push(QueueEntry {
            card_id: c.id,
            note_id: c.note_id,
            kind,
            due: None,
        });
        true
    };
    let mut reviews_taken = 0u32;
    for (c, _) in reviews {
        if opts.review_limit.is_some_and(|l| reviews_taken >= l) {
            break;
        }
        if take(c, QueueKind::Review, &mut queue) {
            reviews_taken += 1;
        }
    }
    let mut new_taken = 0u32;
    for (c, _) in new {
        if opts.new_limit.is_some_and(|l| new_taken >= l) {
            break;
        }
        if take(c, QueueKind::New, &mut queue) {
            new_taken += 1;
        }
    }
    queue
}

/// The card to show now: a learning card that is due, else the next card without a due time,
/// else the earliest learning card within `learn_ahead_ms`. `None` = nothing to show now (the
/// session is finished, or only learning cards due later remain — see [`next_due`]).
pub fn next_entry(
    queue: &[QueueEntry],
    now: Timestamp,
    learn_ahead_ms: i64,
) -> Option<&QueueEntry> {
    let due_now = queue
        .iter()
        .filter(|e| e.due.is_some_and(|d| d <= now))
        .min_by_key(|e| e.due);
    due_now
        .or_else(|| queue.iter().find(|e| e.due.is_none()))
        .or_else(|| {
            queue
                .iter()
                .filter(|e| e.due.is_some_and(|d| d <= now.plus_millis(learn_ahead_ms)))
                .min_by_key(|e| e.due)
        })
}

/// When a waiting learning entry due at `due` is shown by [`next_entry`] with nothing else
/// left: `learn_ahead_ms` before its due time. A session's "next card at" and a launcher's
/// "available at" are this moment, so a countdown ends exactly when the card comes.
pub fn available_at(due: Timestamp, learn_ahead_ms: i64) -> Timestamp {
    due.plus_millis(-learn_ahead_ms.max(0))
}

/// Earliest due time among waiting learning entries.
pub fn next_due(queue: &[QueueEntry]) -> Option<Timestamp> {
    queue.iter().filter_map(|e| e.due).min()
}

/// Counts `(new, learning, review)` of the queue.
pub fn counts(queue: &[QueueEntry]) -> (u32, u32, u32) {
    queue.iter().fold((0, 0, 0), |(n, l, r), e| match e.kind {
        QueueKind::New => (n + 1, l, r),
        QueueKind::Learning => (n, l + 1, r),
        QueueKind::Review => (n, l, r + 1),
    })
}

/// Records an answer of `card` in the session: removes its entry and re-queues it as a
/// learning entry when `requeue_at` is set (an intraday step due before the day ends).
pub fn record_answer(
    state: &mut SessionState,
    card: CardId,
    note: NoteId,
    again: bool,
    duration_ms: u32,
    now: Timestamp,
    requeue_at: Option<Timestamp>,
) {
    state.queue.retain(|e| e.card_id != card);
    if let Some(due) = requeue_at {
        state.queue.push(QueueEntry {
            card_id: card,
            note_id: note,
            kind: QueueKind::Learning,
            due: Some(due),
        });
    }
    state.answered = state.answered.saturating_add(1);
    if again {
        if !state.again.contains(&card) {
            state.again.push(card);
        }
    } else {
        state.correct = state.correct.saturating_add(1);
    }
    state.time_ms = state.time_ms.saturating_add(u64::from(duration_ms));
    state.hint_card = None;
    state.hints = 0;
    state.last_event_at = Some(now);
}

/// Removes cards from the queue (suspended/buried during the session). Returns how many.
pub fn remove_cards(state: &mut SessionState, cards: &[CardId]) -> usize {
    let before = state.queue.len();
    state.queue.retain(|e| !cards.contains(&e.card_id));
    before - state.queue.len()
}

/// Simulated state of `card` in a trial session (`None` = use the card's own).
pub fn trial_state(state: &SessionState, card: CardId) -> Option<&SchedulingState> {
    state
        .trial
        .iter()
        .find(|t| t.card_id == card)
        .map(|t| &t.state)
}

/// Stores the simulated state of `card` after a trial answer.
pub fn set_trial_state(state: &mut SessionState, card: CardId, s: SchedulingState) {
    match state.trial.iter_mut().find(|t| t.card_id == card) {
        Some(t) => t.state = s,
        None => state.trial.push(TrialCard {
            card_id: card,
            state: s,
        }),
    }
}

/// Validates a client-reported duration against the backend's clock: at most the time since
/// the session's last event, at most [`MAX_ANSWER_MS`].
pub fn clamp_duration(reported_ms: u32, last_event_at: Option<Timestamp>, now: Timestamp) -> u32 {
    let since = last_event_at.map_or(i64::from(MAX_ANSWER_MS), |t| {
        (now.as_millis() - t.as_millis()).max(0)
    });
    let cap = u32::try_from(since).unwrap_or(u32::MAX).min(MAX_ANSWER_MS);
    reported_ms.min(cap)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use fc_core::card::CardSpec;
    use fc_core::rng::SplitMix64;
    use fc_core::DeckId;
    use uuid::Uuid;

    const NOW: Timestamp = Timestamp(1_000_000_000);
    const END: Timestamp = Timestamp(1_000_000_000 + 10 * 3_600_000);

    fn card(n: u128, note: u128, state: CardState, due: Option<i64>) -> Card {
        let mut c = Card::new(
            CardId(Uuid::from_u128(n)),
            NoteId(Uuid::from_u128(note)),
            DeckId(Uuid::from_u128(1)),
            CardSpec {
                template_key: format!("t{n}"),
                card_type: "classic".into(),
            },
            Timestamp(n as i64),
        );
        c.scheduling.state = state;
        c.scheduling.due = due.map(Timestamp);
        c
    }

    fn ids(q: &[QueueEntry]) -> Vec<u128> {
        q.iter().map(|e| e.card_id.0.as_u128()).collect()
    }

    fn opts(rng: &mut SplitMix64) -> BuildOptions<'_> {
        BuildOptions {
            now: NOW,
            day_end: END,
            due_only: true,
            new_limit: None,
            review_limit: None,
            order: Order::Due,
            bury_siblings: true,
            rng,
        }
    }

    #[test]
    fn review_limit_caps_reviews_but_never_learning_cards() {
        let cards = vec![
            card(1, 1, CardState::Review, Some(NOW.0 - 3)),
            card(2, 2, CardState::Review, Some(NOW.0 - 2)),
            card(3, 3, CardState::Review, Some(NOW.0 - 1)),
            card(4, 4, CardState::Learning, Some(NOW.0 - 1)),
            card(5, 5, CardState::Relearning, Some(NOW.0)),
            card(6, 6, CardState::New, None),
        ];
        let mut rng = SplitMix64::new(1);
        let q = build_queue(
            &cards,
            |c| c.scheduling.clone(),
            BuildOptions {
                review_limit: Some(1),
                new_limit: Some(0),
                ..opts(&mut rng)
            },
        );
        assert_eq!(ids(&q), vec![4, 5, 1]);
    }

    #[test]
    fn learning_then_reviews_then_new_with_sibling_burying() {
        let cards = vec![
            card(1, 1, CardState::New, None),
            card(2, 1, CardState::New, None), // sibling of 1
            card(3, 2, CardState::Review, Some(NOW.0 - 5)),
            card(4, 3, CardState::Review, Some(NOW.0 - 9)),
            card(5, 4, CardState::Review, Some(END.0 + 1)), // tomorrow
            card(6, 5, CardState::Learning, Some(NOW.0 + 60_000)),
            card(7, 2, CardState::New, None), // sibling of review 3
        ];
        let mut rng = SplitMix64::new(1);
        let q = build_queue(&cards, |c| c.scheduling.clone(), opts(&mut rng));
        assert_eq!(ids(&q), vec![6, 4, 3, 1]);
        assert_eq!(counts(&q), (1, 1, 2));
        // Without burying every sibling is queued; cramming takes tomorrow's card too.
        let mut rng = SplitMix64::new(1);
        let q = build_queue(
            &cards,
            |c| c.scheduling.clone(),
            BuildOptions {
                bury_siblings: false,
                due_only: false,
                new_limit: Some(2),
                ..opts(&mut rng)
            },
        );
        assert_eq!(ids(&q), vec![6, 4, 3, 5, 1, 2]);
    }

    #[test]
    fn suspended_and_buried_are_skipped_and_orders_apply() {
        let mut a = card(1, 1, CardState::Review, Some(NOW.0));
        a.suspended = true;
        let mut b = card(2, 2, CardState::Review, Some(NOW.0));
        b.buried_until = Some(END);
        let mut c = card(3, 3, CardState::Review, Some(NOW.0));
        c.scheduling.difficulty = Some(3.0);
        let mut d = card(4, 4, CardState::Review, Some(NOW.0 - 1));
        d.scheduling.difficulty = Some(9.0);
        let cards = vec![a, b, c, d];
        let mut rng = SplitMix64::new(1);
        let hard = build_queue(
            &cards,
            |c| c.scheduling.clone(),
            BuildOptions {
                order: Order::HardestFirst,
                ..opts(&mut rng)
            },
        );
        assert_eq!(ids(&hard), vec![4, 3]);
        let mut rng = SplitMix64::new(1);
        let newest = build_queue(
            &cards,
            |c| c.scheduling.clone(),
            BuildOptions {
                order: Order::NewestFirst,
                ..opts(&mut rng)
            },
        );
        assert_eq!(ids(&newest), vec![4, 3]);
        // Random is a permutation, deterministic per seed.
        let many: Vec<Card> = (10..30).map(|n| card(n, n, CardState::New, None)).collect();
        let run = |seed| {
            let mut rng = SplitMix64::new(seed);
            ids(&build_queue(
                &many,
                |c| c.scheduling.clone(),
                BuildOptions {
                    order: Order::Random,
                    ..opts(&mut rng)
                },
            ))
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5), run(6));
        let mut sorted = run(5);
        sorted.sort_unstable();
        assert_eq!(sorted, (10..30).collect::<Vec<u128>>());
    }

    #[test]
    fn next_entry_prefers_due_learning_then_queue_then_learn_ahead() {
        let e = |n: u128, due: Option<i64>| QueueEntry {
            card_id: CardId(Uuid::from_u128(n)),
            note_id: NoteId(Uuid::from_u128(n)),
            kind: if due.is_some() {
                QueueKind::Learning
            } else {
                QueueKind::New
            },
            due: due.map(Timestamp),
        };
        let q = vec![
            e(1, Some(NOW.0 + 5 * 60_000)),
            e(2, None),
            e(3, Some(NOW.0 - 1)),
        ];
        assert_eq!(
            next_entry(&q, NOW, LEARN_AHEAD_MS)
                .unwrap()
                .card_id
                .0
                .as_u128(),
            3
        );
        let q = vec![e(1, Some(NOW.0 + 5 * 60_000)), e(2, None)];
        assert_eq!(
            next_entry(&q, NOW, LEARN_AHEAD_MS)
                .unwrap()
                .card_id
                .0
                .as_u128(),
            2
        );
        let q = vec![e(1, Some(NOW.0 + 5 * 60_000))];
        assert_eq!(
            next_entry(&q, NOW, LEARN_AHEAD_MS)
                .unwrap()
                .card_id
                .0
                .as_u128(),
            1
        );
        let q = vec![e(1, Some(NOW.0 + 60 * 60_000))];
        assert!(next_entry(&q, NOW, LEARN_AHEAD_MS).is_none());
        assert_eq!(next_due(&q), Some(Timestamp(NOW.0 + 60 * 60_000)));
        assert!(next_entry(&[], NOW, LEARN_AHEAD_MS).is_none());
    }

    #[test]
    fn answers_update_counters_and_requeue() {
        let c = CardId(Uuid::from_u128(1));
        let n = NoteId(Uuid::from_u128(1));
        let mut s = SessionState {
            queue: vec![QueueEntry {
                card_id: c,
                note_id: n,
                kind: QueueKind::New,
                due: None,
            }],
            hint_card: Some(c),
            hints: 2,
            ..SessionState::default()
        };
        record_answer(
            &mut s,
            c,
            n,
            true,
            1500,
            NOW,
            Some(Timestamp(NOW.0 + 60_000)),
        );
        assert_eq!(s.queue.len(), 1);
        assert_eq!(s.queue[0].kind, QueueKind::Learning);
        assert_eq!(
            (s.answered, s.correct, s.again.len(), s.hints),
            (1, 0, 1, 0)
        );
        record_answer(&mut s, c, n, false, 500, NOW, None);
        assert!(s.queue.is_empty());
        assert_eq!(
            (s.answered, s.correct, s.again.len(), s.time_ms),
            (2, 1, 1, 2000)
        );
        set_trial_state(&mut s, c, SchedulingState::NEW);
        set_trial_state(
            &mut s,
            c,
            SchedulingState {
                reps: 3,
                ..SchedulingState::NEW
            },
        );
        assert_eq!(trial_state(&s, c).unwrap().reps, 3);
        assert_eq!(s.trial.len(), 1);
        s.queue.push(QueueEntry {
            card_id: c,
            note_id: n,
            kind: QueueKind::New,
            due: None,
        });
        assert_eq!(remove_cards(&mut s, &[c]), 1);
        // JSON roundtrip (persisted state).
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(serde_json::from_value::<SessionState>(json).unwrap(), s);
        assert_eq!(
            serde_json::from_value::<SessionState>(serde_json::json!({})).unwrap(),
            SessionState::default()
        );
    }

    #[test]
    fn durations_are_clamped_by_the_backend_clock() {
        assert_eq!(
            clamp_duration(5_000, Some(Timestamp(NOW.0 - 2_000)), NOW),
            2_000
        );
        assert_eq!(
            clamp_duration(1_000, Some(Timestamp(NOW.0 - 2_000)), NOW),
            1_000
        );
        assert_eq!(clamp_duration(u32::MAX, None, NOW), MAX_ANSWER_MS);
        assert_eq!(clamp_duration(10, Some(Timestamp(NOW.0 + 5)), NOW), 0);
    }

    #[test]
    fn preset_orders() {
        let mk = |n: u128, state: CardState, due: i64, stability: f64, last: i64| {
            let mut c = card(n, n, state, Some(due));
            c.scheduling.stability = Some(stability);
            c.scheduling.last_review = Some(Timestamp(last));
            c
        };
        // R falls with elapsed / stability: card 2 (10 days over 1) < card 1 (20 over 100).
        let cards = vec![
            mk(1, CardState::Review, 10, 100.0, 0),
            mk(2, CardState::Review, 20, 1.0, 90),
            card(3, 3, CardState::New, None),
            card(4, 4, CardState::New, None),
        ];
        let build = |order: Order| {
            let mut rng = fc_core::rng::SplitMix64::new(7);
            build_queue(
                &cards,
                |c| c.scheduling.clone(),
                BuildOptions {
                    now: Timestamp(100),
                    day_end: Timestamp(1_000),
                    due_only: true,
                    new_limit: None,
                    review_limit: None,
                    order,
                    bury_siblings: false,
                    rng: &mut rng,
                },
            )
            .into_iter()
            .map(|e| e.card_id)
            .collect::<Vec<_>>()
        };
        let ids = |ns: &[u128]| {
            ns.iter()
                .map(|n| CardId(Uuid::from_u128(*n)))
                .collect::<Vec<_>>()
        };
        let due = build(Order::Preset {
            new: NewCardOrder::Added,
            review: ReviewOrder::Due,
        });
        assert_eq!(due, ids(&[1, 2, 3, 4]));
        let weakest = build(Order::Preset {
            new: NewCardOrder::Added,
            review: ReviewOrder::Retrievability,
        });
        assert_eq!(weakest, ids(&[2, 1, 3, 4]));
    }
}
