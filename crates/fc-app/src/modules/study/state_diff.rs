//! How a session's queue is stored and undone (v1.0).
//!
//! The queue used to be a JSON array inside `sessions.state`, rewritten on every answer and
//! copied whole into each answer's undo entry: tens of thousands of entries with many cards
//! in learning. Now:
//!
//! * the queue is stored as `session_queue` rows (migration 1071); a save diffs the queue
//!   against what is stored and deletes/inserts only the rows that changed ([`plan_rows`]);
//! * the undo entry holds the rest of the state (counters, mistakes, trial and mode state —
//!   small) and the queue as edits back to the stored queue ([`Op`]), so undo is still exact.
//!
//! [`diff`] finds the edits of the usual changes (an answered card leaves, a learning card is
//! queued again, siblings are buried) in one pass with a short lookahead; anything else is
//! still a correct diff, only a longer one.

use crate::error::{AppError, AppResult};
use crate::support::from_json;
use fc_api::v1::study::SessionStateSnapshot;
use fc_api::ErrorCode;
use fc_core::SessionId;
use fc_storage::repo::SessionQueueRow;
use fc_study::session::{QueueEntry, QueueKind, SessionState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Gap between the `seq` keys of consecutive rows of a fully written queue (room for
/// insertions between them).
pub(super) const SEQ_GAP: i64 = 1 << 20;

/// How far [`diff`] looks ahead for the entry it is out of step with.
const LOOKAHEAD: usize = 16;

/// Key of the queue edits in a v1.0 undo snapshot (older snapshots are whole states).
const OPS_KEY: &str = "queue_ops";
/// Key of the rest of the state in a v1.0 undo snapshot.
const REST_KEY: &str = "rest";

/// One step of turning a queue into another, walking both from the front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Op {
    /// The next `n` entries stay.
    Keep(usize),
    /// The next entries (exactly these) go.
    Del(Vec<QueueEntry>),
    /// These entries come in here.
    Ins(Vec<QueueEntry>),
}

fn push_keep(ops: &mut Vec<Op>, n: usize) {
    if let Some(Op::Keep(k)) = ops.last_mut() {
        *k += n;
    } else {
        ops.push(Op::Keep(n));
    }
}

fn push_del(ops: &mut Vec<Op>, entries: &[QueueEntry]) {
    if let Some(Op::Del(d)) = ops.last_mut() {
        d.extend_from_slice(entries);
    } else {
        ops.push(Op::Del(entries.to_vec()));
    }
}

fn push_ins(ops: &mut Vec<Op>, entries: &[QueueEntry]) {
    if let Some(Op::Ins(d)) = ops.last_mut() {
        d.extend_from_slice(entries);
    } else {
        ops.push(Op::Ins(entries.to_vec()));
    }
}

/// Edits turning `old` into `new` ([`apply_ops`]`(old, diff(old, new)) == new`).
pub(super) fn diff(old: &[QueueEntry], new: &[QueueEntry]) -> Vec<Op> {
    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() && j < new.len() {
        // The common run in one step.
        let run = old[i..]
            .iter()
            .zip(&new[j..])
            .take_while(|(a, b)| a == b)
            .count();
        if run > 0 {
            push_keep(&mut ops, run);
            i += run;
            j += run;
            continue;
        }
        let del = (1..=LOOKAHEAD).find(|d| old.get(i + d) == Some(&new[j]));
        let ins = (1..=LOOKAHEAD).find(|d| new.get(j + d) == Some(&old[i]));
        match (del, ins) {
            (Some(d), Some(n)) if n < d => {
                push_ins(&mut ops, &new[j..j + n]);
                j += n;
            }
            (Some(d), _) => {
                push_del(&mut ops, &old[i..i + d]);
                i += d;
            }
            (None, Some(n)) => {
                push_ins(&mut ops, &new[j..j + n]);
                j += n;
            }
            (None, None) => {
                push_del(&mut ops, &old[i..=i]);
                push_ins(&mut ops, &new[j..=j]);
                i += 1;
                j += 1;
            }
        }
    }
    if i < old.len() {
        push_del(&mut ops, &old[i..]);
    }
    if j < new.len() {
        push_ins(&mut ops, &new[j..]);
    }
    ops
}

/// The edits that undo `ops` (from the new queue back to the old one).
pub(super) fn invert(ops: Vec<Op>) -> Vec<Op> {
    ops.into_iter()
        .map(|op| match op {
            Op::Keep(n) => Op::Keep(n),
            Op::Del(e) => Op::Ins(e),
            Op::Ins(e) => Op::Del(e),
        })
        .collect()
}

fn mismatch() -> AppError {
    AppError::api(
        ErrorCode::Internal,
        "the session queue changed since this undo step was recorded",
    )
}

/// Applies `ops` to `queue`; fails when a deleted entry is not the one recorded.
pub(super) fn apply_ops(queue: &[QueueEntry], ops: &[Op]) -> AppResult<Vec<QueueEntry>> {
    let mut out = Vec::with_capacity(queue.len() + 8);
    let mut i = 0;
    for op in ops {
        match op {
            Op::Keep(n) => {
                let kept = queue.get(i..i + n).ok_or_else(mismatch)?;
                out.extend_from_slice(kept);
                i += n;
            }
            Op::Del(gone) => {
                if queue.get(i..i + gone.len()) != Some(gone.as_slice()) {
                    return Err(mismatch());
                }
                i += gone.len();
            }
            Op::Ins(new) => out.extend_from_slice(new),
        }
    }
    out.extend_from_slice(queue.get(i..).unwrap_or_default());
    Ok(out)
}

/// What undoes one [`super::engine::Session::save`]: the stored state before it (without
/// its queue) and the queue edits back to it. Serialized only when an inverse needs it.
#[derive(Debug, Clone)]
pub(crate) struct SessionUndo {
    pub(super) id: SessionId,
    pub(super) rest: Value,
    pub(super) ops: Vec<Op>,
}

impl SessionUndo {
    /// The `study.restore` snapshot (`state` is opaque to clients).
    pub fn snapshot(self) -> AppResult<SessionStateSnapshot> {
        let ops = serde_json::to_value(invert(self.ops))
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        Ok(SessionStateSnapshot {
            id: self.id,
            state: serde_json::json!({ REST_KEY: self.rest, OPS_KEY: ops }),
        })
    }
}

/// The state `snapshot` describes, given the current state: a v1.0 snapshot's rest with its
/// queue edits applied to the current queue; an older snapshot is the whole state.
pub(super) fn apply(current: &SessionState, snapshot: &Value) -> AppResult<SessionState> {
    match (snapshot.get(REST_KEY), snapshot.get(OPS_KEY)) {
        (Some(rest), Some(ops)) => {
            let ops: Vec<Op> = from_json(ops)?;
            let mut state: SessionState = from_json(rest)?;
            state.queue = apply_ops(&current.queue, &ops)?;
            Ok(state)
        }
        _ => from_json(snapshot),
    }
}

/// Row edits of one save.
#[derive(Debug, Default)]
pub(super) struct RowPlan {
    /// `seq`s of the rows to delete.
    pub remove: Vec<i64>,
    /// Rows to insert.
    pub add: Vec<SessionQueueRow>,
    /// `seq` of every entry of the new queue.
    pub seqs: Vec<i64>,
}

/// The row edits that turn the stored rows (`seqs`, parallel to the old queue) into `new`
/// by `ops`. `None` when there is no room between two kept rows (the caller rewrites all).
pub(super) fn plan_rows(seqs: &[i64], ops: &[Op], new: &[QueueEntry]) -> Option<RowPlan> {
    let mut plan = RowPlan {
        seqs: Vec::with_capacity(new.len()),
        ..RowPlan::default()
    };
    let mut i = 0;
    for (k, op) in ops.iter().enumerate() {
        match op {
            Op::Keep(n) => {
                plan.seqs.extend_from_slice(seqs.get(i..i + n)?);
                i += n;
            }
            Op::Del(gone) => {
                plan.remove.extend_from_slice(seqs.get(i..i + gone.len())?);
                i += gone.len();
            }
            Op::Ins(entries) => {
                // Between the last row of the new queue so far and the next kept row, spread
                // evenly over every entry inserted before that row.
                let mut ahead = i;
                let mut next = None;
                let mut pending = entries.len() as i128;
                for later in &ops[k + 1..] {
                    match later {
                        Op::Del(gone) => ahead += gone.len(),
                        Op::Keep(_) => {
                            next = Some(*seqs.get(ahead)?);
                            break;
                        }
                        Op::Ins(more) => pending += more.len() as i128,
                    }
                }
                let span = (pending + 1) * i128::from(SEQ_GAP);
                let (lo, hi) = match (plan.seqs.last().copied(), next) {
                    (None, None) => (0, span),
                    (Some(lo), None) => (i128::from(lo), i128::from(lo) + span),
                    (None, Some(hi)) => (i128::from(hi) - span, i128::from(hi)),
                    (Some(lo), Some(hi)) => (i128::from(lo), i128::from(hi)),
                };
                let step = (hi - lo) / (pending + 1);
                if step < 1 {
                    return None;
                }
                for (m, e) in entries.iter().enumerate() {
                    let seq = i64::try_from(lo + step * (m as i128 + 1)).ok()?;
                    plan.seqs.push(seq);
                    plan.add.push(row(seq, e));
                }
            }
        }
    }
    (i == seqs.len() && plan.seqs.len() == new.len()).then_some(plan)
}

/// Storage form of a queue kind.
pub(super) fn kind_str(kind: QueueKind) -> &'static str {
    match kind {
        QueueKind::Learning => "learning",
        QueueKind::Review => "review",
        QueueKind::New => "new",
    }
}

/// A queue kind from its storage form.
pub(super) fn queue_kind(kind: &str) -> AppResult<QueueKind> {
    match kind {
        "learning" => Ok(QueueKind::Learning),
        "review" => Ok(QueueKind::Review),
        "new" => Ok(QueueKind::New),
        other => Err(AppError::api(
            ErrorCode::Internal,
            format!("unknown session_queue.kind {other:?}"),
        )),
    }
}

/// The row of `entry` at `seq`.
pub(super) fn row(seq: i64, entry: &QueueEntry) -> SessionQueueRow {
    SessionQueueRow {
        seq,
        card_id: entry.card_id,
        note_id: entry.note_id,
        kind: kind_str(entry.kind).to_owned(),
        due: entry.due,
    }
}

/// `sessions.state` key of the queue's revision: a fresh value on every save.
pub(super) const REV_KEY: &str = "queue_rev";

/// A new queue revision, unique across saves and processes (a rolled-back save's revision
/// is never stored, so a cached queue tagged with it is never taken).
pub(super) fn new_rev() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NONCE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nonce = *NONCE.get_or_init(|| {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    });
    format!("{nonce:016x}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// The queue of the session saved last, in memory (one per app, v1.0): loading that session
/// again takes its queue from here when the stored revision is the cached one, instead of
/// reading every row. Purely a cache — a miss reads the rows.
#[derive(Debug, Default)]
pub(crate) struct QueueCache {
    last: std::sync::Mutex<Option<Cached>>,
}

#[derive(Debug)]
struct Cached {
    session: SessionId,
    rev: String,
    queue: Vec<QueueEntry>,
    seqs: Vec<i64>,
}

impl QueueCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Cached>> {
        self.last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The cached queue and row seqs of `session` at revision `rev`.
    pub(super) fn get(&self, session: SessionId, rev: &str) -> Option<(Vec<QueueEntry>, Vec<i64>)> {
        let last = self.lock();
        let c = last.as_ref().filter(|c| c.session == session && c.rev == rev)?;
        Some((c.queue.clone(), c.seqs.clone()))
    }

    /// Remembers the queue `session` was saved with at `rev`.
    pub(super) fn put(&self, session: SessionId, rev: String, queue: &[QueueEntry], seqs: &[i64]) {
        *self.lock() = Some(Cached {
            session,
            rev,
            queue: queue.to_vec(),
            seqs: seqs.to_vec(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_core::{CardId, NoteId, Timestamp};

    fn e(n: u128) -> QueueEntry {
        QueueEntry {
            card_id: CardId(uuid::Uuid::from_u128(n)),
            note_id: NoteId(uuid::Uuid::from_u128(1_000 + n)),
            kind: QueueKind::Review,
            due: None,
        }
    }

    fn q(ns: &[u128]) -> Vec<QueueEntry> {
        ns.iter().map(|n| e(*n)).collect()
    }

    /// Diff, inverse and row plan agree for the shapes study produces and for arbitrary ones.
    #[test]
    fn diffs_round_trip_and_plan_rows() {
        let mut learning = e(7);
        learning.kind = QueueKind::Learning;
        learning.due = Some(Timestamp(5));
        let mut answered: Vec<QueueEntry> = q(&[1, 2, 3, 4, 5, 6, 8, 9]);
        answered.push(learning);
        let cases: Vec<(Vec<QueueEntry>, Vec<QueueEntry>)> = vec![
            (q(&[]), q(&[1, 2, 3])),
            (q(&[1, 2, 3]), q(&[])),
            (q(&[1, 2, 3, 4, 5, 6, 7, 8, 9]), answered),
            (q(&[1, 2, 3, 4]), q(&[2, 4])),
            (q(&[1, 2, 3]), q(&[1, 9, 2, 3])),
            (q(&[1, 2, 3]), q(&[9, 1, 2, 3])),
            (q(&[1, 2, 3]), q(&[3, 2, 1])),
            (q(&[1, 2, 3]), q(&[4, 5, 6, 7])),
            (
                (0..100).collect::<Vec<u128>>().iter().map(|n| e(*n)).collect(),
                (0..100).rev().collect::<Vec<u128>>().iter().map(|n| e(*n)).collect(),
            ),
        ];
        for (old, new) in cases {
            let ops = diff(&old, &new);
            assert_eq!(apply_ops(&old, &ops).unwrap(), new, "{ops:?}");
            assert_eq!(apply_ops(&new, &invert(ops.clone())).unwrap(), old);
            let seqs: Vec<i64> = (1..=old.len() as i64).map(|i| i * SEQ_GAP).collect();
            let plan = plan_rows(&seqs, &ops, &new).unwrap();
            assert_eq!(plan.seqs.len(), new.len());
            assert!(plan.seqs.windows(2).all(|w| w[0] < w[1]), "{:?}", plan.seqs);
            // The stored rows after the edits are the new queue in seq order.
            let mut rows: std::collections::BTreeMap<i64, QueueEntry> =
                seqs.iter().copied().zip(old.iter().cloned()).collect();
            for s in &plan.remove {
                assert!(rows.remove(s).is_some());
            }
            for r in &plan.add {
                let entry = QueueEntry {
                    card_id: r.card_id,
                    note_id: r.note_id,
                    kind: queue_kind(&r.kind).unwrap(),
                    due: r.due,
                };
                assert!(rows.insert(r.seq, entry).is_none());
            }
            assert_eq!(rows.into_values().collect::<Vec<_>>(), new);
        }
    }

    /// An answer is a delete and an append, whatever the queue's length.
    #[test]
    fn an_answer_is_a_small_diff() {
        let old: Vec<QueueEntry> = (0..20_000).map(e).collect();
        let mut new = old.clone();
        let answered = new.remove(0);
        new.push(answered);
        let ops = diff(&old, &new);
        assert_eq!(ops.len(), 3, "{ops:?}");
        let seqs: Vec<i64> = (1..=old.len() as i64).map(|i| i * SEQ_GAP).collect();
        let plan = plan_rows(&seqs, &ops, &new).unwrap();
        assert_eq!((plan.remove.len(), plan.add.len()), (1, 1));
    }

    /// No room between two rows: the caller rewrites the queue.
    #[test]
    fn no_gap_left_asks_for_a_rewrite() {
        let old = q(&[1, 2]);
        let new = q(&[1, 9, 2]);
        assert!(plan_rows(&[1, 2], &diff(&old, &new), &new).is_none());
    }

    /// Snapshots written before v1.0 are whole states.
    #[test]
    fn old_snapshots_are_whole_states() {
        let before = SessionState {
            queue: q(&[1, 2]),
            answered: 3,
            ..SessionState::default()
        };
        let v = serde_json::to_value(&before).unwrap();
        assert_eq!(apply(&SessionState::default(), &v).unwrap(), before);
    }
}
