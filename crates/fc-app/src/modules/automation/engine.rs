//! Running rules: what a commit brings ([`observe`], [`observe_group`]), dispatching it to the
//! rules ([`dispatch`]) under the loop guard, one rule run ([`apply`], internal
//! `automation.apply`), the dry run ([`preview`]), manual runs and the scheduler ([`run_due`],
//! [`spawn_scheduler`]).
//!
//! **Undo.** A run triggered by a change is written as the continuation of the change's batch
//! ([`OplogMode::Follow`]): `oplog.undo` after answering a card reverts the answer *and* what
//! its rules did, in one step; redo replays both (without running rules again). The rules
//! a run triggers continue the same batch. If another command committed in between (another
//! thread), the run becomes its own undo step instead — the oplog keeps batches contiguous.
//! Scheduled and manual runs are their own undo step.
//!
//! **Chains.** A rule run is a command; its commit is observed like any other, so the rules its
//! actions trigger run too. They do not run *inside* it: while a dispatch is active on a thread,
//! nested commits only queue their signals (with the chain of rules that produced them) and the
//! outermost dispatch works through the queue in order, asking a
//! [`LoopGuard`](fc_core::automation::LoopGuard) about each run: a rule already in its chain is a
//! cycle, a chain at `automation.max_depth` stops, and one source change (one commit, one due
//! schedule, one manual run — each its own guard) starts at most
//! `automation.max_runs_per_event` runs. Refusals are logged, never errors: the change that
//! triggered the rules has committed already.
//!
//! Undo and redo batches trigger nothing (undoing a rule's action must not start the rule
//! again), and neither do the chunks of a long import — its finished group does
//! ([`observe_group`]).

use super::{
    action_params, action_selection, actions_of, canonical_condition, check_actions, require,
    setting, trigger_of, Selection, ID, KEEP_RUNS, MAX_DEPTH, MAX_RUNS,
};
use crate::app::OplogMode;
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, PendingOplogEntry};
use crate::registry::ensure_invocable;
use crate::support::{err_id, summary};
use crate::App;
use fc_api::method::parse_qualified_name;
use fc_api::v1::automation::*;
use fc_api::v1::bulk::SetFlag;
use fc_api::v1::cardtypes::SetCardFlag;
use fc_api::v1::common::EntityRef;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::io::ImportedIds;
use fc_api::v1::methods::*;
use fc_api::v1::oplog::{Actor, OplogKind};
use fc_api::v1::study::AnswerCard;
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest};
use fc_core::automation::{
    schedule_state, Admission, LoopGuard, RuleTrigger, ScheduleState, SignalKind,
};
use fc_core::query::compile_query;
use fc_core::query::filter::Filter;
use fc_core::rng::SplitMix64;
use fc_core::time::FixedClock;
use fc_core::{AutomationRuleId, BatchId, CardId, NoteId, Timestamp, V7IdGen};
use fc_storage::repo::{AutomationRuleRecord, AutomationRunRecord, CardFilter};
use fc_storage::Reads;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, Weak};

/// Most cards one run handles (the first ones in creation order; the rest wait for the next
/// run — a scheduled rule catches up, a preview says `truncated`).
pub const MAX_CARDS_PER_RUN: usize = 5000;

/// Cards listed in a preview.
const PREVIEW_CARDS: usize = 100;

/// How often the scheduler looks for due rules.
pub const SCHEDULER_TICK: std::time::Duration = std::time::Duration::from_secs(60);

/// One kind of thing a commit did, to which cards / notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Signal {
    kind: SignalKind,
    /// Cards concerned; `None` = the whole collection (schedules, manual runs).
    cards: Option<BTreeSet<CardId>>,
    /// Notes concerned (their live cards are checked).
    notes: BTreeSet<NoteId>,
}

impl Signal {
    fn whole_collection(kind: SignalKind) -> Self {
        Self {
            kind,
            cards: None,
            notes: BTreeSet::new(),
        }
    }
}

/// What one commit brought, ready for [`dispatch`].
#[derive(Debug, Default)]
pub(crate) struct Observed(Vec<Signal>);

impl Observed {
    fn entry(&mut self, kind: SignalKind) -> &mut Signal {
        if let Some(i) = self.0.iter().position(|s| s.kind == kind) {
            return &mut self.0[i];
        }
        self.0.push(Signal {
            kind,
            cards: Some(BTreeSet::new()),
            notes: BTreeSet::new(),
        });
        let last = self.0.len() - 1;
        &mut self.0[last]
    }

    fn card(&mut self, kind: SignalKind, card: CardId) {
        if let Some(c) = &mut self.entry(kind).cards {
            c.insert(card);
        }
    }

    fn note(&mut self, kind: SignalKind, note: NoteId) {
        self.entry(kind).notes.insert(note);
    }

    fn finish(self) -> Option<Self> {
        (!self.0.is_empty()).then_some(self)
    }
}

/// What a committed **do** batch brought: notes added / edited and classification values
/// changed (from its events), cards answered and flags set (from its commands). `None` when
/// the module is off, for undo/redo batches, or when nothing relevant happened. Cheap: no
/// storage access.
pub(crate) fn observe(
    app: &App,
    kind: OplogKind,
    entries: &[PendingOplogEntry],
    events: &[DomainEvent],
) -> Option<Observed> {
    if kind != OplogKind::Do || !app.is_module_enabled(ID) {
        return None;
    }
    let mut o = Observed::default();
    for e in events {
        match e {
            DomainEvent::NoteCreated { note_id, .. } => o.note(SignalKind::NoteAdded, *note_id),
            DomainEvent::NoteUpdated { note_id } => o.note(SignalKind::NoteUpdated, *note_id),
            DomainEvent::AttributeValuesChanged { entities } => {
                for en in entities {
                    match en {
                        EntityRef::Note(n) => o.note(SignalKind::ClassificationChanged, *n),
                        EntityRef::Card(c) => o.card(SignalKind::ClassificationChanged, *c),
                    }
                }
            }
            _ => {}
        }
    }
    for e in entries {
        let Some(name) = parse_qualified_name(&e.command) else {
            continue;
        };
        match name {
            n if n == StudyAnswer::NAME => {
                if let Ok(a) = serde_json::from_value::<AnswerCard>(e.payload.clone()) {
                    o.card(SignalKind::CardAnswered(a.rating), a.card_id);
                }
            }
            // v0.10 modes rate through the same engine (`answer_core`): the rating the backend
            // judged (learn grade, confidence map, timed "again") is the summary's `rating`.
            n if n == ModesLearnAnswer::NAME
                || n == ModesConfidenceAnswer::NAME
                || n == ModesTimedExpire::NAME =>
            {
                let card = e
                    .payload
                    .get("card_id")
                    .and_then(|v| serde_json::from_value::<CardId>(v.clone()).ok());
                let rating = e
                    .summary
                    .params
                    .get("rating")
                    .and_then(|r| serde_json::from_value(Value::String(r.clone())).ok());
                if let (Some(card), Some(rating)) = (card, rating) {
                    o.card(SignalKind::CardAnswered(rating), card);
                }
            }
            n if n == CardsSetFlag::NAME => {
                if let Ok(f) = serde_json::from_value::<SetCardFlag>(e.payload.clone()) {
                    if f.on {
                        for id in f.ids {
                            o.card(SignalKind::CardFlagged(f.flag), id);
                        }
                    }
                }
            }
            n if n == BulkSetFlag::NAME => {
                if let Ok(f) = serde_json::from_value::<SetFlag>(e.payload.clone()) {
                    if let (true, Ok(flag @ 1..=7)) = (f.on, u8::try_from(f.flag)) {
                        for id in f.card_ids {
                            o.card(SignalKind::CardFlagged(flag), id);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    o.finish()
}

/// What a committed undo group (a long job's single undo step) brought: a finished import
/// signals [`SignalKind::Imported`] with the notes it added and updated.
pub(crate) fn observe_group(app: &App, entries: &[PendingOplogEntry]) -> Option<Observed> {
    if !app.is_module_enabled(ID) {
        return None;
    }
    let mut import = false;
    let mut o = Observed::default();
    for e in entries {
        match parse_qualified_name(&e.command) {
            Some(n) if n == IoImportApply::NAME => {
                import = true;
                for inv in &e.inverse {
                    if parse_qualified_name(&inv.method) == Some(IoImportTrash::NAME) {
                        if let Ok(ids) = serde_json::from_value::<ImportedIds>(inv.params.clone())
                        {
                            for n in ids.notes {
                                o.note(SignalKind::Imported, n);
                            }
                        }
                    }
                }
            }
            Some(n) if n == NotesUpdate::NAME => {
                if let Some(id) = e
                    .payload
                    .get("id")
                    .and_then(|v| serde_json::from_value::<NoteId>(v.clone()).ok())
                {
                    o.note(SignalKind::Imported, id);
                }
            }
            _ => {}
        }
    }
    if import {
        o.finish()
    } else {
        None
    }
}

// ------------------------------------------------------------------ dispatch

/// Signals waiting for rules, with the chain of rule runs that produced them.
struct Job {
    signal: Signal,
    chain: Vec<AutomationRuleId>,
    /// Only this rule (schedules, manual runs).
    only: Option<AutomationRuleId>,
    /// A manual run (runs the rule even if disabled).
    manual: bool,
    /// The committed batch whose change this is. A rule run continues it
    /// ([`OplogMode::Follow`]): the change and what its rules did are **one undo step**.
    /// `None` (schedules, manual runs): the run is its own batch.
    follows: Option<BatchId>,
    /// Loop guard of the source job this job descends from (set by [`run_jobs`]).
    guard: usize,
}

impl Job {
    fn new(
        signal: Signal,
        only: Option<AutomationRuleId>,
        manual: bool,
        follows: Option<BatchId>,
    ) -> Self {
        Self {
            signal,
            chain: Vec::new(),
            only,
            manual,
            follows,
            guard: 0,
        }
    }
}

/// The dispatch active on this thread.
struct Frame {
    queue: VecDeque<Job>,
    /// Chain of the rule run in progress (signals committed now were produced by it).
    chain: Vec<AutomationRuleId>,
    /// Loop guard of the rule run in progress (signals committed now count against it).
    guard: usize,
}

thread_local! {
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

/// Ends the thread's dispatch (also on panic).
struct EndFrame;

impl Drop for EndFrame {
    fn drop(&mut self) {
        FRAME.with(|f| *f.borrow_mut() = None);
    }
}

fn set_running(chain: Vec<AutomationRuleId>, guard: usize) {
    FRAME.with(|f| {
        if let Some(fr) = f.borrow_mut().as_mut() {
            fr.chain = chain;
            fr.guard = guard;
        }
    });
}

fn next_job() -> Option<Job> {
    FRAME.with(|f| f.borrow_mut().as_mut().and_then(|fr| fr.queue.pop_front()))
}

/// Runs the rules a change committed as batch `source` triggers (see the module docs). Never
/// fails: problems end up in the run log.
///
/// It runs on the committing thread, after the commit, before the triggering call returns.
/// That is deliberate: a rule's run continues `source` only while `source` is still the
/// newest batch, so running later (on another thread) would let the next answer slip in
/// between and split the undo step. The cost is bounded instead: rules and compiled
/// conditions are loaded once per dispatch, conditions are evaluated only over the cards the
/// change touched, rules sharing a condition share one search (benchmark:
/// `tests::bench`, < 20 ms per answer with 100 rules over 100k notes).
pub(crate) fn dispatch(app: &App, observed: Option<Observed>, source: BatchId) {
    let Some(observed) = observed else { return };
    let jobs = observed
        .0
        .into_iter()
        .map(|signal| Job::new(signal, None, false, Some(source)))
        .collect();
    let _ = run_jobs(app, vec![jobs]);
}

/// A rule as one dispatch sees it (loaded once).
struct LoadedRule {
    record: AutomationRuleRecord,
    trigger: RuleTrigger,
}

/// Compiled conditions of one dispatch (the catalog they resolve against is loaded once).
/// Forgotten after every rule run that wrote something: an action may create the tag or deck
/// a later condition names.
struct Conditions {
    now: Timestamp,
    catalog: Option<crate::modules::browse::CatalogData>,
    filters: HashMap<String, Result<Filter, ApiError>>,
}

impl Conditions {
    fn new(now: Timestamp) -> Self {
        Self {
            now,
            catalog: None,
            filters: HashMap::new(),
        }
    }

    fn invalidate(&mut self) {
        self.catalog = None;
        self.filters.clear();
    }

    /// The condition's filter ('' = every card).
    fn filter(&mut self, reads: &dyn Reads, condition: &str) -> AppResult<Filter> {
        if let Some(f) = self.filters.get(condition) {
            return f.clone().map_err(AppError::Api);
        }
        let compiled = if condition.trim().is_empty() {
            Ok(Filter::All)
        } else {
            if self.catalog.is_none() {
                self.catalog = Some(crate::modules::browse::CatalogData::load(reads, self.now)?);
            }
            let data = self
                .catalog
                .as_ref()
                .ok_or_else(|| ApiError::new(ErrorCode::Internal, "catalog not loaded"))?;
            compile_query(condition, &data.catalog())
                .map_err(|e| crate::modules::browse::query_error(&e).into_api())
        };
        self.filters.insert(condition.to_owned(), compiled.clone());
        compiled.map_err(AppError::Api)
    }

    /// Live cards matching `condition` — among `scope` when given, else in the whole
    /// collection — at most [`MAX_CARDS_PER_RUN`], in creation order.
    fn matches(
        &mut self,
        app: &App,
        condition: &str,
        scope: Option<&[CardId]>,
    ) -> AppResult<Vec<CardId>> {
        let tx = app.store().read()?;
        let filter = self.filter(&*tx, condition)?;
        let mut ids = match scope {
            Some(cards) => tx.search_card_ids_among(&filter, self.now, cards)?,
            None => tx.search_card_ids(&filter, self.now)?,
        };
        ids.truncate(MAX_CARDS_PER_RUN);
        Ok(ids)
    }
}

/// The cards a signal concerns (its cards and its notes' cards); `None` = the whole
/// collection.
fn scope_of(app: &App, signal: &Signal) -> AppResult<Option<Vec<CardId>>> {
    let Some(cards) = &signal.cards else {
        return Ok(None);
    };
    let mut ids: Vec<CardId> = cards.iter().copied().collect();
    if !signal.notes.is_empty() {
        let tx = app.store().read()?;
        let of_notes = tx.list_cards(&CardFilter {
            note_ids: Some(signal.notes.iter().copied().collect()),
            ..CardFilter::default()
        })?;
        ids.extend(of_notes.into_iter().map(|c| c.id));
    }
    Ok(Some(ids))
}

/// Works through `sources` (and everything they trigger), or — inside an active dispatch —
/// queues them behind the run in progress. Every source job (the signals of one commit, one
/// due schedule, one manual run) has its **own** loop guard, which the runs it triggers share.
/// Returns the runs logged by this call.
fn run_jobs(app: &App, sources: Vec<Vec<Job>>) -> AppResult<Vec<RunDto>> {
    let n_sources = sources.len();
    let mut queue: VecDeque<Job> = VecDeque::new();
    for (i, jobs) in sources.into_iter().enumerate() {
        for mut j in jobs {
            j.guard = i;
            queue.push_back(j);
        }
    }
    let nested = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        match f.as_mut() {
            Some(fr) => {
                for mut j in queue.drain(..) {
                    j.chain = fr.chain.clone();
                    j.guard = fr.guard;
                    fr.queue.push_back(j);
                }
                true
            }
            None => {
                *f = Some(Frame {
                    queue: std::mem::take(&mut queue),
                    chain: Vec::new(),
                    guard: 0,
                });
                false
            }
        }
    });
    if nested {
        return Ok(Vec::new());
    }
    let _end = EndFrame;
    let now = app.now();
    // Rules and limits: once per dispatch (actions cannot change rules or settings).
    let (rules, mut guards) = {
        let tx = app.store().read()?;
        let rules: Vec<LoadedRule> = tx
            .list_automation_rules(false)?
            .into_iter()
            .filter_map(|record| {
                let trigger = trigger_of(&record).ok()?;
                Some(LoadedRule { record, trigger })
            })
            .collect();
        let guard =
            LoopGuard::new(setting(&*tx, MAX_DEPTH, 3, 10)?, setting(&*tx, MAX_RUNS, 20, 100)?);
        (rules, vec![guard; n_sources.max(1)])
    };
    let mut conditions = Conditions::new(now);
    let mut out = Vec::new();
    let mut logged_outside = false;
    while let Some(job) = next_job() {
        let selected: Vec<&LoadedRule> = rules
            .iter()
            .filter(|r| {
                let on = match job.only {
                    Some(id) => r.record.id == id && (r.record.enabled || job.manual),
                    None => r.record.enabled,
                };
                on && r.trigger.accepts(&job.signal.kind)
            })
            .collect();
        if selected.is_empty() {
            continue;
        }
        let depth = u32::try_from(job.chain.len()).unwrap_or(u32::MAX);
        let scope = scope_of(app, &job.signal).map_err(AppError::into_api);
        if matches!(&scope, Ok(Some(s)) if s.is_empty()) {
            continue;
        }
        // Rules sharing a condition share one search — until a run writes something.
        let mut found: HashMap<&str, Vec<CardId>> = HashMap::new();
        for rule in selected {
            let rule_ref = &rule.record;
            let log = |status: &str, matched: usize, error: Option<ApiError>| {
                log_run(app, rule_ref, job.signal.kind, depth, matched, status, error)
            };
            let condition = rule.record.condition.as_str();
            let cards = match (found.get(condition), &scope) {
                (Some(c), _) => Ok(c.clone()),
                (None, Ok(scope)) => conditions.matches(app, condition, scope.as_deref()),
                (None, Err(e)) => Err(AppError::Api(e.clone())),
            };
            let cards = match cards {
                Ok(c) => {
                    found.insert(condition, c.clone());
                    c
                }
                Err(e) => {
                    out.push(log("failed", 0, Some(e.into_api()))?);
                    logged_outside = true;
                    continue;
                }
            };
            if cards.is_empty() {
                if job.signal.cards.is_none() {
                    // A scheduled or manual run that found nothing still ran.
                    out.push(log("no_match", 0, None)?);
                    logged_outside = true;
                }
                continue;
            }
            let guard = guards.get_mut(job.guard).ok_or_else(|| {
                AppError::api(ErrorCode::Internal, "automation: unknown loop guard")
            })?;
            let admission = guard.admit(rule.record.id, &job.chain);
            if let Some(status) = admission.status() {
                out.push(log(status, cards.len(), None)?);
                logged_outside = true;
                continue;
            }
            debug_assert_eq!(admission, Admission::Run);
            let mut chain = job.chain.clone();
            chain.push(rule.record.id);
            set_running(chain, job.guard);
            let request = apply_request(&rule.record, &job, &cards, depth)?;
            let actor = Actor::Automation(rule.record.id.to_string());
            let mode = match job.follows {
                Some(batch) => OplogMode::Follow(batch),
                None => OplogMode::Write,
            };
            let result = app.invoke_with(&request, &actor, mode);
            set_running(Vec::new(), job.guard);
            match result.and_then(|v| crate::support::from_json::<ApplyResult>(&v)) {
                Ok(r) => {
                    if r.batch_id.is_some() {
                        // It wrote: later conditions see its changes.
                        found.clear();
                        conditions.invalidate();
                    }
                    out.push(RunDto {
                        seq: r.seq,
                        rule_id: rule.record.id,
                        at: app.now(),
                        trigger: job.signal.kind.as_str().to_owned(),
                        depth,
                        matched: count(cards.len()),
                        status: "applied".into(),
                        batch_id: r.batch_id,
                        error: None,
                    });
                }
                Err(e) => {
                    out.push(log("failed", cards.len(), Some(e.into_api()))?);
                    logged_outside = true;
                }
            }
        }
    }
    if logged_outside {
        app.publish(vec![DomainEvent::AutomationChanged], None);
    }
    Ok(out)
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn apply_request(
    rule: &AutomationRuleRecord,
    job: &Job,
    cards: &[CardId],
    depth: u32,
) -> AppResult<InvokeRequest> {
    let params = ApplyRule {
        rule_id: rule.id,
        card_ids: cards.to_vec(),
        trigger: job.signal.kind.as_str().to_owned(),
        depth,
        manual: job.manual,
    };
    Ok(InvokeRequest {
        method: format!("{}.{}", fc_api::API_VERSION, AutomationApply::NAME),
        params: serde_json::to_value(params)
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
    })
}

/// Logs a run that wrote nothing (no match, refused by the guard, failed) in its own small
/// transaction — for a scheduled run, together with its `last_run_at` (see [`run_due`]).
fn log_run(
    app: &App,
    rule: &AutomationRuleRecord,
    kind: SignalKind,
    depth: u32,
    matched: usize,
    status: &str,
    error: Option<ApiError>,
) -> AppResult<RunDto> {
    let mut record = AutomationRunRecord {
        seq: 0,
        rule_id: rule.id,
        at: app.now(),
        trigger: kind.as_str().to_owned(),
        depth,
        matched: count(matched),
        status: status.to_owned(),
        batch_id: None,
        error: error.as_ref().and_then(|e| serde_json::to_value(e).ok()),
    };
    let mut tx = app.store().write()?;
    if kind == SignalKind::Schedule {
        tx.set_automation_rule_last_run(rule.id, record.at)?;
    }
    record.seq = tx.insert_automation_run(&record, KEEP_RUNS)?;
    tx.commit()?;
    Ok(run_dto(record))
}

/// Wire form of a log row.
pub(crate) fn run_dto(r: AutomationRunRecord) -> RunDto {
    RunDto {
        seq: r.seq,
        rule_id: r.rule_id,
        at: r.at,
        trigger: r.trigger,
        depth: r.depth,
        matched: r.matched,
        status: r.status,
        batch_id: r.batch_id,
        error: r.error.and_then(|e| serde_json::from_value(e).ok()),
    }
}

/// The condition's filter ('' = every card).
fn condition_filter(reads: &dyn Reads, condition: &str, now: Timestamp) -> AppResult<Filter> {
    if condition.trim().is_empty() {
        Ok(Filter::All)
    } else {
        crate::modules::browse::compile(reads, condition, now)
    }
}

// ------------------------------------------------------------------ one run

/// Runs `actions` over `cards` as nested commands of `ctx`'s batch. Each passes the same access
/// check as a client call ([`ensure_invocable`]: the automation actor's permissions and action
/// allow-list, the module switch as it stands in this transaction).
fn run_actions(
    ctx: &mut CommandCtx<'_>,
    actions: &[RuleAction],
    cards: &[CardId],
) -> AppResult<Vec<Value>> {
    let mut live = Vec::with_capacity(cards.len());
    let mut notes = Vec::new();
    let mut seen = HashSet::new();
    for id in cards {
        if let Some(c) = ctx.reads().get_card(*id)? {
            if c.deleted_at.is_none() {
                live.push(c.id);
                if seen.insert(c.note_id) {
                    notes.push(c.note_id);
                }
            }
        }
    }
    let mut results = Vec::with_capacity(actions.len());
    for (i, a) in actions.iter().enumerate() {
        let selection = action_selection(&a.method).ok_or_else(|| {
            AppError::Api(
                ApiError::new(
                    ErrorCode::PermissionDenied,
                    format!("{} is not an automation action", a.method),
                )
                .with_detail("method", a.method.as_str())
                .with_detail("index", i),
            )
        })?;
        let ids = match selection {
            Selection::Cards => serde_json::to_value(&live),
            Selection::Notes => serde_json::to_value(&notes),
        }
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        if ids.as_array().is_some_and(Vec::is_empty) {
            results.push(Value::Null);
            continue;
        }
        let d = *ctx.registry.descriptor(&a.method).ok_or_else(|| {
            crate::registry::unknown_method(&a.method)
        })?;
        let enabled = crate::support::module_enabled(ctx.reads(), ctx.registry, d.module)?;
        ensure_invocable(ctx.registry, &d, ctx.actor, enabled)?;
        results.push(ctx.call(&InvokeRequest {
            method: d.qualified_name(),
            params: action_params(a, selection, ids),
        })?);
    }
    Ok(results)
}

/// Handler of internal `automation.apply`: one rule run. Its actions' oplog entries form this
/// dispatch's batch (one undo step, actor `automation:<rule id>`); the run is logged in the
/// same transaction.
pub(crate) fn apply(
    req: ApplyRule,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ApplyResult>> {
    let rule = require(ctx.reads(), req.rule_id)?;
    if !rule.enabled && !req.manual {
        return Err(err_id(
            ErrorCode::NotFoundAutomationRule,
            format!("automation rule {} is disabled", rule.id),
            rule.id,
        ));
    }
    let actions = actions_of(&rule)?;
    let results = run_actions(ctx, &actions, &req.card_ids)?;
    let batch_id: Option<BatchId> = ctx.has_oplog_entries().then_some(ctx.batch_id);
    if req.trigger == SignalKind::Schedule.as_str() {
        // Marked in the run's own transaction: a scheduled run is never lost (see `run_due`).
        ctx.tx.set_automation_rule_last_run(rule.id, ctx.now)?;
    }
    let seq = ctx.tx.insert_automation_run(
        &AutomationRunRecord {
            seq: 0,
            rule_id: rule.id,
            at: ctx.now,
            trigger: req.trigger.clone(),
            depth: req.depth,
            matched: count(req.card_ids.len()),
            status: "applied".into(),
            batch_id,
            error: None,
        },
        KEEP_RUNS,
    )?;
    Ok(CommandOutcome {
        response: ApplyResult {
            seq,
            batch_id,
            results,
        },
        inverse: Vec::new(),
        summary: summary::<AutomationApply>().param("name", &rule.name),
        events: vec![DomainEvent::AutomationChanged],
    })
}

// ------------------------------------------------------------------ manual runs, schedules

/// Handler of `automation.rules.run`: runs the rule now (even if disabled) over every card
/// matching its condition — one undo step — followed by whatever it triggers.
pub(crate) fn run_by_hand(req: RuleRef, app: &App, actor: &Actor) -> AppResult<RunResult> {
    super::ensure_manager(actor)?;
    {
        let tx = app.store().read()?;
        require(&*tx, req.id)?;
    }
    let job = Job::new(
        Signal::whole_collection(SignalKind::Manual),
        Some(req.id),
        true,
        None,
    );
    let runs = run_jobs(app, vec![vec![job]])?;
    Ok(RunResult { runs })
}

/// One scheduler pass at a time (two would both find the same rules due).
static SCHEDULER_PASS: Mutex<()> = Mutex::new(());

/// Runs every enabled scheduled rule whose interval has passed since its last run (a new rule
/// is due at once).
///
/// * Each due rule is its own source job, with its own loop-guard budget: however many rules
///   are due, none is refused for the others' runs.
/// * A rule's `last_run_at` is written in the transaction that applies or logs its run, never
///   before: a run that did not happen is not marked, so it is not lost (it runs on the next
///   pass). A failing run is logged — and marked — so it waits for its next interval.
/// * Monotonic guard: a last run in the future means the wall clock went back; it is reset to
///   now ([`ScheduleState::ClockWentBack`]) so the rule waits one interval at most, instead of
///   until the clock catches up.
pub fn run_due(app: &App) -> AppResult<Vec<RunDto>> {
    if !app.is_module_enabled(ID) {
        return Ok(Vec::new());
    }
    let _pass = SCHEDULER_PASS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let now = app.now();
    let mut due = Vec::new();
    let mut rebase = Vec::new();
    {
        let tx = app.store().read()?;
        for r in tx.list_automation_rules(false)? {
            let Some(every) = trigger_of(&r).ok().and_then(|t| t.schedule_ms()) else {
                continue;
            };
            if !r.enabled {
                continue;
            }
            match schedule_state(r.last_run_at, now, every) {
                ScheduleState::Due => due.push(r.id),
                ScheduleState::ClockWentBack => rebase.push(r.id),
                ScheduleState::Waiting => {}
            }
        }
    }
    if !rebase.is_empty() {
        let mut tx = app.store().write()?;
        for id in &rebase {
            tx.set_automation_rule_last_run(*id, now)?;
        }
        tx.commit()?;
    }
    if due.is_empty() {
        return Ok(Vec::new());
    }
    let sources = due
        .into_iter()
        .map(|id| {
            vec![Job::new(
                Signal::whole_collection(SignalKind::Schedule),
                Some(id),
                false,
                None,
            )]
        })
        .collect();
    run_jobs(app, sources)
}

/// Starts the scheduler thread (the desktop shell calls this once): every
/// [`SCHEDULER_TICK`] it runs [`run_due`]. It stops when the app is dropped.
pub fn spawn_scheduler(app: &Arc<App>) -> std::io::Result<std::thread::JoinHandle<()>> {
    let weak: Weak<App> = Arc::downgrade(app);
    std::thread::Builder::new()
        .name("fc-automation".into())
        .spawn(move || loop {
            std::thread::sleep(SCHEDULER_TICK);
            let Some(app) = weak.upgrade() else { break };
            let _ = run_due(&app);
        })
}

// ------------------------------------------------------------------ preview

/// Handler of `automation.preview`: a **dry run**. The condition is evaluated over the whole
/// collection and the actions run exactly as a real run would (same access checks, same
/// commands) in a write transaction that is dropped — rolled back — at the end: nothing is
/// written, no oplog, no event. This is side-effect free because every action is undo-safe
/// (database only: no files, no jobs).
pub(crate) fn preview(req: PreviewRule, app: &App, actor: &Actor) -> AppResult<RulePreview> {
    super::ensure_manager(actor)?;
    let now = app.now();
    let (rule_id, condition, actions) = {
        let tx = app.store().read()?;
        match (req.rule_id, req.draft) {
            (Some(id), None) => {
                let r = require(&*tx, id)?;
                (Some(id), r.condition.clone(), actions_of(&r)?)
            }
            (None, Some(d)) => {
                check_draft_trigger(&d.trigger)?;
                check_actions(app.registry(), &d.actions)?;
                (None, canonical_condition(&*tx, &d.condition, now)?, d.actions)
            }
            _ => {
                return Err(crate::support::invalid(
                    "preview takes either rule_id or draft",
                ))
            }
        }
    };
    let mut tx = app.store().write()?;
    let filter = condition_filter(&*tx, &condition, now)?;
    let all = tx.search_card_ids(&filter, now)?;
    let truncated = all.len() > MAX_CARDS_PER_RUN;
    let cards: Vec<CardId> = all.iter().copied().take(MAX_CARDS_PER_RUN).collect();
    let actor = Actor::Automation(rule_id.map_or_else(|| "preview".to_owned(), |id| id.to_string()));
    let mut ids = V7IdGen::new(
        Arc::new(FixedClock::new(now)),
        SplitMix64::new(now.0.unsigned_abs()),
    );
    let batch_id = BatchId::generate(&mut ids);
    let mut ctx = CommandCtx {
        tx: &mut *tx,
        now,
        ids: &mut ids,
        actor: &actor,
        locale: app.locale(),
        batch_id,
        batch_kind: OplogKind::Do,
        reverts_batch_id: None,
        registry: app.registry(),
        oplog: Vec::new(),
        events: Vec::new(),
        check_modules: true,
    };
    let results = run_actions(&mut ctx, &actions, &cards)?;
    let events = std::mem::take(&mut ctx.events);
    drop(ctx);
    // Never committed: dropping the transaction rolls every action back.
    drop(tx);

    let (changed_cards, changed_notes) = changes(&events);
    let reads = app.store().read()?;
    let index = crate::support::decks::DeckIndex::load(&*reads)?;
    let mut rows = Vec::new();
    let mut changed = 0u32;
    for id in &cards {
        let Some(card) = reads.get_card(*id)? else {
            continue;
        };
        let changes = changed_cards.contains(id) || changed_notes.contains(&card.note_id);
        if changes {
            changed += 1;
        }
        rows.push((changes, card));
    }
    // Changing cards first, then creation order (stable).
    rows.sort_by_key(|(changes, _)| !*changes);
    let mut list = Vec::new();
    for (changes, card) in rows.into_iter().take(PREVIEW_CARDS) {
        let text = reads
            .get_note(card.note_id)?
            .map(|n| n.sort_field)
            .unwrap_or_default();
        list.push(PreviewCard {
            card_id: card.id,
            note_id: card.note_id,
            text,
            deck: index.path(card.deck_id),
            changes,
        });
    }
    Ok(RulePreview {
        matched: count(all.len()),
        changed,
        cards: list,
        actions: actions
            .iter()
            .zip(results)
            .map(|(a, response)| ActionPreview {
                method: a.method.clone(),
                response,
            })
            .collect(),
        truncated,
    })
}

fn check_draft_trigger(t: &RuleTrigger) -> AppResult<()> {
    t.validate().map_err(|field| {
        AppError::Api(
            ApiError::new(
                ErrorCode::ValidationAutomationRule,
                format!("invalid trigger parameter {field}"),
            )
            .with_detail("field", field),
        )
    })
}

/// Cards and notes the events of a (dry) run say changed.
fn changes(events: &[DomainEvent]) -> (HashSet<CardId>, HashSet<NoteId>) {
    let mut cards = HashSet::new();
    let mut notes = HashSet::new();
    for e in events {
        match e {
            DomainEvent::CardsChanged { ids } | DomainEvent::CardsUpdated { ids } => {
                cards.extend(ids.iter().copied());
            }
            DomainEvent::StudyCardsChanged { card_ids } => cards.extend(card_ids.iter().copied()),
            DomainEvent::NoteUpdated { note_id } => {
                notes.insert(*note_id);
            }
            DomainEvent::AttributeValuesChanged { entities } => {
                for en in entities {
                    match en {
                        EntityRef::Note(n) => {
                            notes.insert(*n);
                        }
                        EntityRef::Card(c) => {
                            cards.insert(*c);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    (cards, notes)
}
