//! v1.4.2 K2: the load balancer's per-day counts kept on the writer connection
//! (`sqlite::due_load`) equal what the counting query and `due_times` give, whatever is
//! written in between.

use super::{add_note, deck, did, nid, note, Fx};
use crate::repo::{Reads, Writes};
use crate::sqlite::due_load::LOADS;
use crate::store::Store;
use fc_core::card::CardState;
use fc_core::rng::{Rng, SplitMix64};
use fc_core::time::MILLIS_PER_DAY as DAY;
use fc_core::{CardId, Timestamp};

/// The counts from `due_times` (the trait's reference implementation, another query).
fn reference(r: &dyn Reads, from: Timestamp, to: Timestamp, shift: i64) -> Vec<(i64, u32)> {
    let mut out: Vec<(i64, u32)> = Vec::new();
    for t in r.due_times(from, to).unwrap() {
        let day = (t.0 + shift).div_euclid(DAY);
        match out.last_mut() {
            Some((d, n)) if *d == day => *n += 1,
            _ => out.push((day, 1)),
        }
    }
    out
}

const SHIFTS: [i64; 3] = [0, 3 * 3_600_000 - 4 * 3_600_000, -5 * 3_600_000 - 1_800_000];

/// A few ranges around day 0 of `shift`: aligned (served from the kept counts), unaligned and
/// too long (both answered by the query).
fn check(r: &dyn Reads, rng: &mut SplitMix64, shift: i64, what: &str) {
    for _ in 0..3 {
        let first = rng.below(50) as i64 - 25;
        let len = 1 + rng.below(12) as i64;
        let (mut from, mut to) = (first * DAY - shift, (first + len) * DAY - shift);
        match rng.below(10) {
            0 => from += 1 + rng.below(5_000) as i64,
            1 => to -= 1 + rng.below(5_000) as i64,
            2 => to += 2_000 * DAY,
            _ => {}
        }
        assert_eq!(
            r.due_counts_by_day(Timestamp(from), Timestamp(to), shift)
                .unwrap(),
            reference(r, Timestamp(from), Timestamp(to), shift),
            "{what}: [{from}, {to}) shift {shift}"
        );
    }
}

fn mutate(tx: &mut dyn Writes, rng: &mut SplitMix64, notes: &mut u128, at: i64) -> String {
    let pick = |rng: &mut SplitMix64, notes: u128| 1 + u128::from(rng.below(notes as u64));
    match rng.below(12) {
        0 | 1 => {
            *notes += 1;
            add_note(tx, &note(*notes, "a", "b", &[], at), 1);
            format!("add {notes}")
        }
        2..=7 => {
            let n = pick(rng, *notes);
            let Some(mut c) = tx.get_card(CardId(nid(n).0)).unwrap() else {
                return "update of a purged card".into();
            };
            c.scheduling.state = [
                CardState::New,
                CardState::Learning,
                CardState::Review,
                CardState::Review,
                CardState::Relearning,
            ][rng.below(5) as usize];
            c.scheduling.due = (c.scheduling.state != CardState::New || rng.below(4) == 0)
                .then(|| Timestamp(rng.below(40 * DAY as u64) as i64 - 20 * DAY));
            c.suspended = rng.below(8) == 0;
            c.buried_until = (rng.below(5) == 0).then_some(Timestamp(at));
            tx.update_card(&c).unwrap();
            format!(
                "update {n}: {:?} {:?} suspended {}",
                c.scheduling.state, c.scheduling.due, c.suspended
            )
        }
        8 => {
            let n = pick(rng, *notes);
            let gone = (rng.below(2) == 0).then_some(Timestamp(at));
            tx.set_cards_deleted(&[CardId(nid(n).0)], gone).unwrap();
            format!("card {n} deleted {gone:?}")
        }
        9 | 10 => {
            let n = pick(rng, *notes);
            let gone = (rng.below(2) == 0).then_some(Timestamp(at));
            tx.set_notes_deleted(&[nid(n)], gone).unwrap();
            format!("note {n} deleted {gone:?}")
        }
        _ => {
            let purged = tx.purge_trash(Timestamp(at - 3)).unwrap();
            format!("purge {purged:?}")
        }
    }
}

#[test]
fn due_load_cache_equals_the_query() {
    for seed in [1_u64, 7, 20_261_125] {
        let fx = Fx::seeded();
        fx.write(|tx| tx.insert_deck(&deck(1, None, "D")).unwrap());
        let mut rng = SplitMix64::new(seed);
        let mut notes = 0_u128;
        fx.write(|tx| {
            for _ in 0..40 {
                notes += 1;
                add_note(tx, &note(notes, "a", "b", &[], 5), 1);
            }
        });
        let mut shift = SHIFTS[0];
        for step in 0..500_i64 {
            let at = 100 + step;
            if rng.below(40) == 0 {
                shift = SHIFTS[rng.below(3) as usize];
            }
            if rng.below(25) == 0 {
                // Another connection (a second process on the file) moves cards.
                let raw = fx.raw();
                raw.execute(
                    "UPDATE cards SET due = due + ?1 WHERE due IS NOT NULL AND rowid % 3 = ?2",
                    rusqlite::params![DAY * (1 + rng.below(3) as i64), rng.below(3) as i64],
                )
                .unwrap();
            }
            let mut tx = fx.store.write().unwrap();
            check(
                &*tx,
                &mut rng,
                shift,
                &format!("seed {seed} step {step} before"),
            );
            let mut did_what = Vec::new();
            for _ in 0..1 + rng.below(3) {
                did_what.push(mutate(&mut *tx, &mut rng, &mut notes, at));
                check(
                    &*tx,
                    &mut rng,
                    shift,
                    &format!("seed {seed} step {step} after {did_what:?}"),
                );
            }
            if rng.below(5) == 0 {
                // Rolled back: the counts go back with the rows.
                drop(tx);
            } else {
                tx.commit().unwrap();
            }
            // A read snapshot runs the query; the next write transaction's counts equal it.
            let committed = fx.read(|r| {
                r.due_counts_by_day(
                    Timestamp(-30 * DAY - shift),
                    Timestamp(30 * DAY - shift),
                    shift,
                )
                .unwrap()
            });
            let tx = fx.store.write().unwrap();
            assert_eq!(
                tx.due_counts_by_day(
                    Timestamp(-30 * DAY - shift),
                    Timestamp(30 * DAY - shift),
                    shift
                )
                .unwrap(),
                committed,
                "seed {seed} step {step} after {did_what:?}"
            );
            drop(tx);
        }
    }
    let _ = did(1);
}

/// The counts are really kept: a day is counted by the query once, and answering a card
/// (an update of its row) or committing does not count it again; a trashed note, another
/// connection's commit and a rollback of the transaction that filled them do.
#[test]
fn due_load_cache_counts_a_day_once() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "D")).unwrap();
        for n in 1..=6_u128 {
            add_note(tx, &note(n, "a", "b", &[], 5), 1);
            let mut c = tx.get_card(CardId(nid(n).0)).unwrap().unwrap();
            c.scheduling.state = CardState::Review;
            c.scheduling.due = Some(Timestamp(n as i64 * DAY + 5));
            tx.update_card(&c).unwrap();
        }
    });
    let ask = |r: &dyn Reads| {
        r.due_counts_by_day(Timestamp(0), Timestamp(10 * DAY), 0)
            .unwrap()
    };
    let loads = || LOADS.with(std::cell::Cell::get);
    let all: Vec<(i64, u32)> = (1..=6).map(|d| (d, 1)).collect();

    // Filled in a transaction that is rolled back: nothing is kept.
    let start = loads();
    let tx = fx.store.write().unwrap();
    assert_eq!(ask(&*tx), all);
    drop(tx);
    assert_eq!(loads(), start + 1);

    let mut tx = fx.store.write().unwrap();
    assert_eq!(ask(&*tx), all);
    assert_eq!(ask(&*tx), all);
    assert_eq!(loads(), start + 2, "asked twice, counted once");
    // An answer moves a card from day 2 to day 3.
    let mut c = tx.get_card(CardId(nid(2).0)).unwrap().unwrap();
    c.scheduling.due = Some(Timestamp(3 * DAY + 9));
    tx.update_card(&c).unwrap();
    assert_eq!(ask(&*tx), vec![(1, 1), (3, 2), (4, 1), (5, 1), (6, 1)]);
    // A narrower range and a wider one: only the days not kept yet are counted.
    assert_eq!(
        tx.due_counts_by_day(Timestamp(3 * DAY), Timestamp(5 * DAY), 0)
            .unwrap(),
        vec![(3, 2), (4, 1)]
    );
    assert_eq!(loads(), start + 2);
    assert_eq!(
        tx.due_counts_by_day(Timestamp(-2 * DAY), Timestamp(12 * DAY), 0)
            .unwrap(),
        vec![(1, 1), (3, 2), (4, 1), (5, 1), (6, 1)]
    );
    assert_eq!(loads(), start + 3);
    tx.commit().unwrap();

    // Across commits of this connection.
    for n in 4..=6_u128 {
        let mut tx = fx.store.write().unwrap();
        let mut c = tx.get_card(CardId(nid(n).0)).unwrap().unwrap();
        c.scheduling.due = Some(Timestamp(8 * DAY));
        tx.update_card(&c).unwrap();
        tx.commit().unwrap();
    }
    let mut tx = fx.store.write().unwrap();
    assert_eq!(ask(&*tx), vec![(1, 1), (3, 2), (8, 3)]);
    assert_eq!(loads(), start + 3, "commits keep the counts");
    // A trashed note forgets them.
    tx.set_notes_deleted(&[nid(1)], Some(Timestamp(50)))
        .unwrap();
    assert_eq!(ask(&*tx), vec![(3, 2), (8, 3)]);
    assert_eq!(loads(), start + 4);
    tx.commit().unwrap();

    // Another connection's commit forgets them.
    fx.raw()
        .execute("UPDATE cards SET suspended = 1 WHERE due = ?1", [8 * DAY])
        .unwrap();
    let tx = fx.store.write().unwrap();
    assert_eq!(ask(&*tx), vec![(3, 2)]);
    assert_eq!(loads(), start + 5);
    drop(tx);

    // Switched off: the query, every time.
    crate::set_due_load_cache(false);
    let tx = fx.store.write().unwrap();
    assert_eq!(ask(&*tx), vec![(3, 2)]);
    crate::set_due_load_cache(true);
    drop(tx);
}
