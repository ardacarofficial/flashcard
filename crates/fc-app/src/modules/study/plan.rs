//! v1.3: which cards a session takes — one planner for `study.start`, `study.options` and
//! the "today" numbers of `decks.tree` / `decks.summaries`, so every "to study today" number
//! is the queue a session started now would serve (CONTRACT §1).
//!
//! A plan resolves a deck (sub-deck setting), a kind and a [`StudyScope`] to a queue: the
//! study day of the client's time zone, the deck preset's daily limits minus what was
//! studied today, `study.new_per_session`, sibling burying, buried/suspended and empty
//! cards. Practice scopes (`studied_today`, `struggled_today`, `all`) take a fixed card set
//! without limits.

use super::engine::*;
use super::*;
use crate::error::AppResult;
use crate::support::decks::DeckIndex;
use crate::support::invalid;
use fc_api::v1::study::*;
use fc_core::card::{CardState, Rating};
use fc_core::preset::NewCardOrder;
use fc_core::rng::SplitMix64;
use fc_core::settings::SettingScope;
use fc_core::{CardId, DeckId, NoteId, SessionId, Timestamp};
use fc_storage::repo::{NewCardsFirst, ReviewLogRecord, StudyCardFilter, REVIEW_MODE_IMPORTED};
use fc_storage::Reads;
use fc_study::day::{StudyDay, MAX_TZ_OFFSET_MINS};
use fc_study::session::{self as queue, BuildOptions, Order, QueueEntry, QueueKind};
use std::collections::{HashMap, HashSet};

/// `ux_state` key of the client's UTC offset as last given to `study.start`: queries without
/// an offset (`decks.tree`, `decks.summaries`, the query language's `reviewed:N`) use it, so
/// they share the study day of the sessions. Bookkeeping, not an undo step.
pub(crate) use fc_storage::repo::UX_CLIENT_TZ;

/// The five scopes, in the order `study.options` lists them.
pub(crate) const SCOPES: [StudyScope; 5] = [
    StudyScope::Due,
    StudyScope::New,
    StudyScope::StudiedToday,
    StudyScope::StruggledToday,
    StudyScope::All,
];

/// Whether a scope is practice (the session is `trial`, the card set fixed at start).
pub(crate) fn is_practice(scope: StudyScope) -> bool {
    matches!(
        scope,
        StudyScope::StudiedToday | StudyScope::StruggledToday | StudyScope::All
    )
}

/// Validated client offset: `given`, else the one recorded by the last `study.start`, else 0.
pub(crate) fn client_tz(reads: &dyn Reads, given: Option<i32>) -> AppResult<i32> {
    if let Some(tz) = given {
        if tz.abs() > MAX_TZ_OFFSET_MINS {
            return Err(invalid("tz_offset_mins out of range"));
        }
        return Ok(tz);
    }
    Ok(reads
        .ux_state(UX_CLIENT_TZ)?
        .and_then(|v| v.as_i64())
        .and_then(|v| i32::try_from(v).ok())
        .filter(|v| v.abs() <= MAX_TZ_OFFSET_MINS)
        .unwrap_or(0))
}

/// `study.learn_ahead_mins` in ms (v1.3 X1): how long before its step is due a waiting learning
/// card is shown when nothing else is left.
pub(crate) fn learn_ahead_ms(reads: &dyn Reads) -> AppResult<i64> {
    Ok(i64::from(setting_u32(reads, &SettingScope::App, LEARN_AHEAD_MINS)?) * 60_000)
}

/// `study.day_start_hour`.
pub(crate) fn day_start_hour(reads: &dyn Reads) -> AppResult<u8> {
    Ok(
        u8::try_from(setting_u32(reads, &SettingScope::App, DAY_START_HOUR)?)
            .unwrap_or(fc_study::day::DEFAULT_ROLLOVER_HOUR),
    )
}

/// Start of the current study day (client offset as [`client_tz`] without one given): the
/// query language's `added:1` / `edited:1` / `reviewed:1` mean "since then".
pub(crate) fn today_start(reads: &dyn Reads, now: Timestamp) -> AppResult<Timestamp> {
    let day = StudyDay::new(client_tz(reads, None)?, day_start_hour(reads)?);
    Ok(day.start(day.index(now)))
}

/// What to plan (the card-choosing part of `StartSession`).
pub(crate) struct PlanRequest<'r> {
    pub kind: StudyKind,
    pub scope: Option<StudyScope>,
    pub deck_id: Option<DeckId>,
    pub include_subdecks: Option<bool>,
    pub filter: &'r StudyFilter,
    pub from_session: Option<SessionId>,
    pub new_limit: Option<u32>,
    pub order: Option<StudyOrder>,
    /// Shuffle seed (the session id's: a queue is reproducible from the stored session).
    pub seed: u64,
}

impl<'r> PlanRequest<'r> {
    /// A `normal` session of `scope` on `deck` with default options (`study.options`, today
    /// counts).
    pub fn launcher(
        scope: StudyScope,
        deck_id: Option<DeckId>,
        include_subdecks: Option<bool>,
        filter: &'r StudyFilter,
    ) -> Self {
        Self {
            kind: StudyKind::Normal,
            scope: Some(scope),
            deck_id,
            include_subdecks,
            filter,
            from_session: None,
            new_limit: None,
            order: None,
            seed: 0,
        }
    }

    /// The scope the request means: its own, or what the kind does without one (`normal`:
    /// today's queue; the hand-picked kinds: every card they match).
    fn meant_scope(&self) -> StudyScope {
        match (self.scope, self.kind) {
            (Some(s), _) => s,
            (None, StudyKind::Normal) => StudyScope::Due,
            (None, _) => StudyScope::All,
        }
    }
}

/// A planned queue.
pub(crate) struct Plan {
    pub queue: Vec<QueueEntry>,
    /// The shuffle source after building (modes keep drawing from it).
    pub rng: SplitMix64,
    /// Query-language filter text of the session (`sessions.query`).
    pub query: Option<String>,
    /// Practice scope: the session is `trial` whatever was asked.
    pub practice: bool,
    /// The queue keeps one review/new card per note (the deck preset's sibling burying).
    pub bury_siblings: bool,
}

/// Plans queues at one instant. Caches what several plans share (deck index, today's
/// reviews, empty-card checks), so the today counts of a whole deck tree cost one planner.
pub(crate) struct Planner<'a> {
    v: View<'a>,
    now: Timestamp,
    tz: i32,
    day_start_hour: u8,
    /// `study.learn_ahead_mins` in ms.
    learn_ahead_ms: i64,
    index: DeckIndex,
    empty: EmptyCards<'a>,
    /// Reviews since the study day began, any mode (loaded on first use).
    today: Option<Vec<ReviewLogRecord>>,
    /// Deck of reviewed cards (daily limits per deck).
    card_deck: HashMap<CardId, Option<DeckId>>,
    /// Live cards per deck (empty reasons; loaded on first use).
    inventory: Option<HashMap<DeckId, u32>>,
}

impl<'a> Planner<'a> {
    pub fn new(v: View<'a>, now: Timestamp, tz: i32) -> AppResult<Self> {
        if tz.abs() > MAX_TZ_OFFSET_MINS {
            return Err(invalid("tz_offset_mins out of range"));
        }
        Ok(Self {
            v,
            now,
            tz,
            day_start_hour: day_start_hour(v.reads)?,
            learn_ahead_ms: learn_ahead_ms(v.reads)?,
            index: DeckIndex::load(v.reads)?,
            empty: EmptyCards::new(v),
            today: None,
            card_deck: HashMap::new(),
            inventory: None,
        })
    }

    pub fn day_start_hour(&self) -> u8 {
        self.day_start_hour
    }

    pub fn index(&self) -> &DeckIndex {
        &self.index
    }

    /// The learn-ahead window (`study.learn_ahead_mins`), ms.
    pub fn learn_ahead_ms(&self) -> i64 {
        self.learn_ahead_ms
    }

    fn day(&self) -> StudyDay {
        StudyDay::new(self.tz, self.day_start_hour)
    }

    /// Start of the next study day (when daily limits reset).
    pub fn next_day_start(&self) -> Timestamp {
        let day = self.day();
        day.start(day.index(self.now) + 1)
    }

    /// Decks a session on `deck` covers (`None` = every deck): the deck, with its live
    /// sub-decks unless `include_subdecks` (default: the deck's `decks.include_subdecks`) is
    /// off. Fails for a missing or trashed deck.
    pub fn decks(
        &self,
        deck: Option<DeckId>,
        include_subdecks: Option<bool>,
    ) -> AppResult<Option<Vec<DeckId>>> {
        let Some(d) = deck else { return Ok(None) };
        self.index.require_live(d)?;
        let subdecks = match include_subdecks {
            Some(b) => b,
            None => self.includes_subdecks(d)?,
        };
        Ok(Some(if subdecks {
            self.index.live_subtree(d)
        } else {
            vec![d]
        }))
    }

    /// The deck's `decks.include_subdecks` setting (default on).
    fn includes_subdecks(&self, deck: DeckId) -> AppResult<bool> {
        Ok(crate::settings::resolve_stored(
            self.v.reads,
            &SettingScope::Deck(deck),
            "decks.include_subdecks",
        )?
        .and_then(|(v, _)| v.as_bool())
        .unwrap_or(true))
    }

    fn today_reviews(&mut self) -> AppResult<&[ReviewLogRecord]> {
        if self.today.is_none() {
            let day = self.day();
            let since = day.start(day.index(self.now));
            self.today = Some(self.v.reads.review_logs_since_any(since)?);
        }
        Ok(self.today.as_deref().unwrap_or_default())
    }

    fn deck_of(&mut self, card: CardId) -> AppResult<Option<DeckId>> {
        if let Some(d) = self.card_deck.get(&card) {
            return Ok(*d);
        }
        let d = self.v.reads.get_card(card)?.map(|c| c.deck_id);
        self.card_deck.insert(card, d);
        Ok(d)
    }

    /// New and review cards still allowed today by the preset of `deck`: its `new_per_day` /
    /// `reviews_per_day` minus the distinct cards first studied / reviewed (by a
    /// schedule-changing answer, any mode — v0.10) since the study day began, counted over
    /// `decks` (all decks when `None`).
    fn daily_left(
        &mut self,
        deck: Option<DeckId>,
        decks: Option<&[DeckId]>,
    ) -> AppResult<(u32, u32)> {
        let preset = effective_options(self.v.reads, deck)?;
        let mut new = HashSet::new();
        let mut reviewed = HashSet::new();
        let logs: Vec<(CardId, CardState)> = self
            .today_reviews()?
            .iter()
            .filter(|r| fc_storage::repo::SCHEDULED_REVIEW_MODES.contains(&r.mode.as_str()))
            .map(|r| (r.card_id, r.state_before))
            .collect();
        for (card, before) in logs {
            let set = match before {
                CardState::New => &mut new,
                CardState::Review => &mut reviewed,
                CardState::Learning | CardState::Relearning => continue,
            };
            if set.contains(&card) {
                continue;
            }
            let in_scope = match decks {
                None => true,
                Some(decks) => self.deck_of(card)?.is_some_and(|d| decks.contains(&d)),
            };
            if in_scope {
                set.insert(card);
            }
        }
        let count = |s: &HashSet<CardId>| u32::try_from(s.len()).unwrap_or(u32::MAX);
        Ok((
            preset.new_per_day.saturating_sub(count(&new)),
            preset.reviews_per_day.saturating_sub(count(&reviewed)),
        ))
    }

    /// Distinct cards answered (`struggled`: rated Again / answered wrong) during the
    /// current study day, any mode except imported history, in first-answer order.
    fn studied_today(&mut self, struggled: bool) -> AppResult<Vec<CardId>> {
        let mut seen = HashSet::new();
        Ok(self
            .today_reviews()?
            .iter()
            .filter(|r| r.mode != REVIEW_MODE_IMPORTED)
            .filter(|r| !struggled || r.rating == Rating::Again)
            .filter(|r| seen.insert(r.card_id))
            .map(|r| r.card_id)
            .collect())
    }

    /// Live cards (any state, suspended included) in `decks` (`None` = all).
    fn inventory(&mut self, decks: Option<&[DeckId]>) -> AppResult<u32> {
        if self.inventory.is_none() {
            self.inventory = Some(
                self.v
                    .reads
                    .card_counts_by_deck(self.now)?
                    .into_iter()
                    .map(|(d, c)| (d, c.total))
                    .collect(),
            );
        }
        let Some(inv) = &self.inventory else {
            return Ok(0);
        };
        Ok(match decks {
            None => inv.values().sum(),
            Some(decks) => decks.iter().filter_map(|d| inv.get(d)).sum(),
        })
    }

    /// Plans the queue of `req`. Today's queue of the whole collection (no deck, daily
    /// limits) is the union of what each deck serves under its own options
    /// ([`Planner::plan_collection`]).
    pub fn plan(&mut self, req: &PlanRequest<'_>) -> AppResult<Plan> {
        if req.deck_id.is_none() && is_limited(req) {
            return self.plan_collection(req);
        }
        self.plan_inner(req, false)
    }

    /// The decks whose own plans make up the whole collection's (v1.3 X1): every top-level
    /// deck, and below a deck that does not include its sub-decks, those sub-decks (the same
    /// rule further down). Together they cover every live deck exactly once, so the sum of
    /// their today counts is the collection's.
    pub fn collection_roots(&self) -> AppResult<Vec<DeckId>> {
        let mut roots = Vec::new();
        let mut pending: Vec<Option<DeckId>> = vec![None];
        while let Some(parent) = pending.pop() {
            let mut below = Vec::new();
            for d in self.index.children(parent) {
                roots.push(d.id);
                if !self.includes_subdecks(d.id)? {
                    below.push(Some(d.id));
                }
            }
            pending.extend(below.into_iter().rev());
        }
        Ok(roots)
    }

    /// Today's queue of the whole collection (v1.3 X1): the union of the plans of
    /// [`Planner::collection_roots`] — each deck with its own preset (new/reviews a day,
    /// order, sibling burying), sub-deck setting and `study.new_per_session` — interleaved:
    /// learning cards by due time, then reviews and new cards taken in turn from each deck.
    /// A collection-wide cap applies only when one is asked for (`new_limit`) or the user
    /// set `study.new_per_session` app-wide.
    fn plan_collection(&mut self, req: &PlanRequest<'_>) -> AppResult<Plan> {
        let roots = self.collection_roots()?;
        let filter = StudyFilter {
            limit: None,
            ..req.filter.clone()
        };
        let mut learning: Vec<QueueEntry> = Vec::new();
        let mut reviews: Vec<Vec<(QueueEntry, bool)>> = Vec::new();
        let mut new: Vec<Vec<(QueueEntry, bool)>> = Vec::new();
        let mut query = None;
        let mut rng = SplitMix64::new(req.seed);
        for root in roots {
            let plan = self.plan_inner(
                &PlanRequest {
                    kind: req.kind,
                    scope: req.scope,
                    deck_id: Some(root),
                    include_subdecks: None,
                    filter: &filter,
                    from_session: req.from_session,
                    new_limit: req.new_limit,
                    order: req.order,
                    seed: req.seed,
                },
                false,
            )?;
            query = plan.query;
            rng = plan.rng;
            let (mut r, mut n) = (Vec::new(), Vec::new());
            for e in plan.queue {
                match e.kind {
                    QueueKind::Learning => learning.push(e),
                    QueueKind::Review => r.push((e, plan.bury_siblings)),
                    QueueKind::New => n.push((e, plan.bury_siblings)),
                }
            }
            reviews.push(r);
            new.push(n);
        }
        // Learning cards shown now (no due time) first, then by due time (stable: each
        // deck's own order otherwise).
        learning.sort_by_key(|e| e.due.map_or((0, 0), |d| (1, d.0)));
        let cap = match req.new_limit {
            Some(n) => Some(n),
            None => crate::settings::resolve_stored(self.v.reads, &SettingScope::App, NEW_PER_SESSION)?
                .and_then(|(v, _)| v.as_u64())
                .map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
        };
        // Sibling burying across decks (v1.3): a note's cards in different decks are not both
        // studied today — as within one deck, a review/new card whose deck buries siblings is
        // left out when a card of its note is already queued (learning cards always stay).
        let mut notes: HashSet<NoteId> = learning.iter().map(|e| e.note_id).collect();
        let mut keep = |list: Vec<(QueueEntry, bool)>| -> Vec<QueueEntry> {
            list.into_iter()
                .filter(|(e, bury)| notes.insert(e.note_id) || !bury)
                .map(|(e, _)| e)
                .collect()
        };
        let reviews = keep(interleave(reviews));
        let mut new = keep(interleave(new));
        if let Some(cap) = cap {
            new.truncate(cap as usize);
        }
        let mut queue = learning;
        queue.extend(reviews);
        queue.extend(new);
        if let Some(limit) = req.filter.limit {
            queue.truncate(limit as usize);
        }
        Ok(Plan {
            queue,
            rng,
            query,
            practice: false,
            bury_siblings: false,
        })
    }

    /// Whether a card of `queue` can be shown now (a learning card counts within the
    /// learn-ahead window, as the session shows it).
    pub fn available(&self, queue: &[QueueEntry]) -> bool {
        queue::next_entry(queue, self.now, self.learn_ahead_ms).is_some()
    }

    /// The number a launcher shows for a plan: its cards when one can be shown now, else 0
    /// (only learning cards still waiting for their step: `waiting`).
    pub fn count(&self, queue: &[QueueEntry]) -> u32 {
        if self.available(queue) {
            u32::try_from(queue.len()).unwrap_or(u32::MAX)
        } else {
            0
        }
    }

    /// Why `req` gives nothing to study now (`queue` = its plan) and when that changes.
    pub fn empty_reason(
        &mut self,
        req: &PlanRequest<'_>,
        queue: &[QueueEntry],
    ) -> AppResult<(StudyEmptyReason, Option<Timestamp>)> {
        if let Some(due) = queue::next_due(queue) {
            // Every card waits for its learning step: served from `due - learn-ahead` (the
            // moment a session shows it).
            return Ok((
                StudyEmptyReason::Waiting,
                Some(queue::available_at(due, self.learn_ahead_ms)),
            ));
        }
        let decks = self.decks(req.deck_id, req.include_subdecks)?;
        if self.inventory(decks.as_deref())? == 0 {
            return Ok((StudyEmptyReason::NoCards, None));
        }
        Ok(match req.meant_scope() {
            StudyScope::StudiedToday => (StudyEmptyReason::NoneStudiedToday, None),
            StudyScope::StruggledToday => (StudyEmptyReason::NoneStruggledToday, None),
            StudyScope::All => (StudyEmptyReason::NothingDue, None),
            StudyScope::Due | StudyScope::New => {
                let unlimited = self.plan_inner(req, true)?;
                if unlimited.queue.is_empty() {
                    (StudyEmptyReason::NothingDue, None)
                } else {
                    (StudyEmptyReason::DailyLimitReached, Some(self.next_day_start()))
                }
            }
        })
    }

    /// `lift_limits`: without the daily / per-session limits (empty reasons).
    #[allow(clippy::too_many_lines)]
    fn plan_inner(&mut self, req: &PlanRequest<'_>, lift_limits: bool) -> AppResult<Plan> {
        if req.scope.is_some()
            && matches!(req.kind, StudyKind::Custom | StudyKind::Mistakes)
        {
            return Err(invalid("scope is for normal, learn, test and match sessions"));
        }
        let practice = req.scope.is_some_and(is_practice);
        let reads = self.v.reads;
        let deck_ids = self.decks(req.deck_id, req.include_subdecks)?;
        let filter = req.filter;
        // Query-language filter (v0.4): text, or a saved query ("filtered deck").
        let query = match (filter.query.as_deref().map(str::trim), filter.saved_query_id) {
            (Some(q), Some(_)) if !q.is_empty() => {
                return Err(invalid("give either filter.query or filter.saved_query_id"))
            }
            (_, Some(id)) => Some(crate::modules::browse::require_saved_query(reads, id)?.query),
            (Some(q), None) if !q.is_empty() => Some(q.to_owned()),
            _ => None,
        };
        if query.is_some() && req.kind == StudyKind::Mistakes {
            return Err(invalid("a mistakes session takes no query"));
        }
        let matching = match &query {
            Some(q) => {
                let compiled = crate::modules::browse::compile(reads, q, self.now)?;
                Some(reads.search_card_ids(&compiled, self.now)?)
            }
            None => None,
        };
        let mut card_ids = match req.kind {
            StudyKind::Mistakes => {
                let from = req
                    .from_session
                    .ok_or_else(|| invalid("a mistakes session needs from_session"))?;
                Some(Session::load(reads, self.v.registry, from)?.state.again)
            }
            // v1.4: "study the missed ones again" — the words a quiz missed (and the query's,
            // when both are given).
            StudyKind::Quiz => match req.from_session {
                Some(from) => {
                    let s = Session::load(reads, self.v.registry, from)?;
                    let missed = crate::modules::modes::quiz::missed_cards(&s)?;
                    Some(match matching {
                        Some(ids) => {
                            let ids: HashSet<CardId> = ids.into_iter().collect();
                            missed.into_iter().filter(|c| ids.contains(c)).collect()
                        }
                        None => missed,
                    })
                }
                None => matching,
            },
            StudyKind::Normal
            | StudyKind::Custom
            | StudyKind::Learn
            | StudyKind::Test
            | StudyKind::Match => matching,
        };
        // v1.3 practice scopes: today's cards (and the query's, when both are given).
        if let Some(s @ (StudyScope::StudiedToday | StudyScope::StruggledToday)) = req.scope {
            let today = self.studied_today(s == StudyScope::StruggledToday)?;
            card_ids = Some(match card_ids {
                Some(ids) => {
                    let ids: HashSet<CardId> = ids.into_iter().collect();
                    today.into_iter().filter(|c| ids.contains(c)).collect()
                }
                None => today,
            });
        }
        // Deck options (v0.4 presets, inherited from parent decks): order, sibling burying.
        let preset = effective_options(reads, req.deck_id)?;
        let order = match req.order {
            Some(o) => order_of(o),
            None => Order::Preset {
                new: preset.new_order,
                review: preset.review_order,
            },
        };
        // Today's queue respects the deck preset's daily limits (minus what was studied
        // today); hand-picked sets (custom, mistakes, learn/test/match without a scope,
        // practice scopes) are not limited — their schedule-changing answers still count
        // towards later sessions (docs/10 §24).
        let limited = !lift_limits && is_limited(req);
        let daily = if limited {
            Some(self.daily_left(req.deck_id, deck_ids.as_deref())?)
        } else {
            None
        };
        let new_limit = match (req.new_limit, daily) {
            _ if lift_limits => None,
            (Some(n), _) => Some(n),
            (None, Some((new_left, _))) => Some(
                setting_u32(reads, &deck_scope(req.deck_id), NEW_PER_SESSION)?.min(new_left),
            ),
            (None, None) => None,
        };
        let review_limit = daily.map(|(_, reviews_left)| reviews_left);
        let bury_siblings = preset.bury_siblings
            && match req.scope {
                Some(StudyScope::Due | StudyScope::New) => true,
                Some(_) => false,
                None => matches!(req.kind, StudyKind::Normal | StudyKind::Custom),
            };
        let due_only = match req.scope {
            Some(StudyScope::Due) => true,
            Some(_) => false,
            None => match req.kind {
                StudyKind::Normal => filter.due_only.unwrap_or(true),
                StudyKind::Custom
                | StudyKind::Learn
                | StudyKind::Test
                | StudyKind::Match
                | StudyKind::Quiz => filter.due_only.unwrap_or(false),
                StudyKind::Mistakes => false,
            },
        };
        let states = match req.scope {
            // Unseen cards only (within the filter's states, when it names some).
            Some(StudyScope::New) => Some(
                filter
                    .states
                    .as_ref()
                    .map_or_else(|| vec![CardState::New], |s| {
                        s.iter().copied().filter(|s| *s == CardState::New).collect()
                    }),
            ),
            _ => filter.states.clone(),
        };
        let day_end = self.day().end(self.now);
        // v1.0: only what the queue can take is read — cards in progress due today (through
        // the due index) and, when new cards come in a fixed order, the first ones up to the
        // limit (more are read only if burying or empty cards leave the limit unfilled).
        let newest_first = match order {
            Order::Due | Order::HardestFirst => Some(false),
            Order::NewestFirst => Some(true),
            Order::Preset { new: NewCardOrder::Added, .. } => Some(false),
            Order::Random | Order::Preset { new: NewCardOrder::Random, .. } => None,
        };
        let mut new_read = new_limit
            .zip(newest_first)
            .map(|(n, newest)| NewCardsFirst { n: n.saturating_mul(2).saturating_add(32), newest });
        let mut card_filter = StudyCardFilter {
            deck_ids,
            card_ids,
            states,
            attributes: filter
                .attributes
                .iter()
                .map(|c| (c.attr_id, c.values.clone()))
                .collect(),
            include_suspended: false,
            due_before: due_only.then_some(day_end),
            new_first: None,
        };
        // Empty cards (v0.3: the note no longer yields them) are never studied: an empty one
        // is left out and the queue built again (v1.3: every queued card is checked, so the
        // count a launcher shows is the count a session serves).
        let mut excluded: HashSet<CardId> = HashSet::new();
        // Bounded: every round leaves out a card or reads more new cards.
        let (mut queue, rng) = loop {
            card_filter.new_first = new_read;
            let mut candidates = reads.study_cards(&card_filter)?;
            let new_read_all = new_read.is_none_or(|f| {
                (candidates
                    .iter()
                    .filter(|c| c.scheduling.state == CardState::New)
                    .count() as u64)
                    < u64::from(f.n)
            });
            candidates.retain(|c| !excluded.contains(&c.id));
            if practice {
                // Practice is outside the schedule: cards hidden until tomorrow (buried
                // siblings) are practised too; suspended ones stay out.
                for c in &mut candidates {
                    c.buried_until = None;
                }
            }
            // v1.4: "lowest recall first" ranks by the recall card info shows: each card's
            // own preset decay (`stats::facts::decay_of`).
            let decays: HashMap<fc_core::DeckId, f64> = if matches!(
                order,
                Order::Preset {
                    review: fc_core::preset::ReviewOrder::Retrievability,
                    ..
                }
            ) {
                let mut presets = crate::modules::stats::facts::Presets::new(reads);
                let mut m = HashMap::new();
                for c in &candidates {
                    if let std::collections::hash_map::Entry::Vacant(e) = m.entry(c.deck_id) {
                        e.insert(crate::modules::stats::facts::decay_of(presets.of(c.deck_id)?));
                    }
                }
                m
            } else {
                HashMap::new()
            };
            let decay_of = |c: &fc_core::card::Card| {
                decays
                    .get(&c.deck_id)
                    .copied()
                    .unwrap_or(fc_core::recall::DEFAULT_DECAY)
            };
            // Seeded from the session id: shuffles are reproducible from the stored session.
            let mut rng = SplitMix64::new(req.seed);
            let queue = queue::build_queue(
                &candidates,
                |c| c.scheduling.clone(),
                BuildOptions {
                    now: self.now,
                    day_end,
                    due_only,
                    new_limit,
                    review_limit,
                    order,
                    bury_siblings,
                    rng: &mut rng,
                    decay_of: Some(&decay_of),
                },
            );
            let by_id: HashMap<CardId, &fc_core::card::Card> =
                candidates.iter().map(|c| (c.id, c)).collect();
            self.empty
                .prefetch(queue.iter().filter_map(|e| by_id.get(&e.card_id).copied()))?;
            let mut found = false;
            for e in &queue {
                if let Some(card) = by_id.get(&e.card_id) {
                    if self.empty.is_empty(card)? {
                        excluded.insert(card.id);
                        found = true;
                    }
                }
            }
            if found {
                continue;
            }
            let new_taken = queue.iter().filter(|e| e.kind == queue::QueueKind::New).count() as u64;
            match (new_read, new_limit) {
                (Some(f), Some(limit)) if !new_read_all && new_taken < u64::from(limit) => {
                    new_read = Some(NewCardsFirst {
                        n: f.n.saturating_mul(4),
                        ..f
                    });
                }
                _ => break (queue, rng),
            }
        };
        if practice {
            // Practice shows every card now: a learning step waits only in scheduled study.
            for e in &mut queue {
                e.due = None;
            }
        }
        if let Some(limit) = filter.limit {
            queue.truncate(limit as usize);
        }
        Ok(Plan {
            queue,
            rng,
            query,
            practice,
            bury_siblings,
        })
    }
}

/// Whether `req` is today's queue under the daily limits (scopes `due`/`new`, a normal
/// session without a scope); hand-picked sets (custom, mistakes, learn/test/match without a
/// scope, practice scopes) are not limited.
fn is_limited(req: &PlanRequest<'_>) -> bool {
    match req.scope {
        Some(StudyScope::Due | StudyScope::New) => true,
        Some(_) => false,
        None => req.kind == StudyKind::Normal,
    }
}

/// Takes one entry from each list in turn (lists keep their own order).
fn interleave<T>(lists: Vec<Vec<T>>) -> Vec<T> {
    let total = lists.iter().map(Vec::len).sum();
    let mut out = Vec::with_capacity(total);
    let mut iters: Vec<_> = lists.into_iter().map(Vec::into_iter).collect();
    while out.len() < total {
        for it in &mut iters {
            if let Some(e) = it.next() {
                out.push(e);
            }
        }
    }
    out
}

fn order_of(o: StudyOrder) -> Order {
    match o {
        StudyOrder::Due => Order::Due,
        StudyOrder::Random => Order::Random,
        StudyOrder::HardestFirst => Order::HardestFirst,
        StudyOrder::NewestFirst => Order::NewestFirst,
    }
}

/// Today's queue of `deck` (`None` = whole collection) as `(new, learning, review)`: what
/// `study.start { kind: normal }` on it serves now (0s while every card waits).
pub(crate) fn today_counts(planner: &mut Planner<'_>, deck: Option<DeckId>) -> AppResult<(u32, u32, u32)> {
    let filter = StudyFilter::default();
    let plan = planner.plan(&PlanRequest::launcher(StudyScope::Due, deck, None, &filter))?;
    Ok(if planner.available(&plan.queue) {
        queue::counts(&plan.queue)
    } else {
        (0, 0, 0)
    })
}
