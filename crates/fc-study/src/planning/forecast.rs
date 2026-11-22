//! **Workload forecast** (v1.3, docs/02 §5 "Planlama"): how many answers (and minutes) each of
//! the next days will ask for, given the cards as they are now and a new-cards-per-day pace.
//! Pure and deterministic: the cards, the FSRS weights and the pace come in as arguments, days
//! are offsets from "today" (the caller's [`StudyDay`](crate::day::StudyDay)), no randomness.
//!
//! ## Model
//!
//! An **expected-value** simulation of FSRS-6 (the same formulas as `fsrs-rs`, see
//! [`Fsrs`]): instead of drawing pass/fail at random, every review splits a card's
//! probability *mass* — `R` of it is recalled ("Good"), `1 − R` forgotten ("Again", followed by
//! the relearning steps) — where `R` is the forgetting curve at the review. Each part is
//! scheduled again at the interval its new stability gives for the desired retention. Parts
//! with the same due day and nearly the same memory (stability within ~5 %, difficulty within
//! 0.25) are merged (mass-weighted), which keeps the work bounded without changing totals:
//! **mass is conserved**, so "cards learned + cards not yet seen" always equals the input.
//!
//! * **Review cards** are due on their due day (overdue = today) with the days since their
//!   last review as elapsed time.
//! * **Learning / relearning cards** take one more answer on their due day, then graduate with
//!   their stability (or the "Good" initial stability).
//! * **New cards** are introduced `new_per_day` a day (today: minus the ones already started)
//!   until none are left. A new card costs `learning_steps + 1` answers on its first day (one
//!   per learning step plus one, the estimate `daily_workload` uses too), then starts with the
//!   "Good" initial memory.
//!
//! New cards enter **linearly**: the expected answers of one card introduced on day 0 (its
//! *profile*) is simulated once and shifted/scaled per introduction day. So the forecast is
//! exactly monotonic in the pace (more new cards a day never lowers the total work up to any
//! day) and existing cards are simulated independently of it.
//!
//! What it does not model: daily review limits (the forecast is the load that *falls due*),
//! "Hard"/"Easy" answers, same-day short-term stability changes, load balancing (it moves
//! reviews within a few days, the totals stay).

use std::collections::BTreeMap;

/// FSRS-6 default weights (`fsrs::FSRS6_DEFAULT_PARAMETERS`; same as the scheduler's).
pub const DEFAULT_WEIGHTS: [f64; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];
/// Stability bounds of `fsrs-rs`.
const S_MIN: f64 = 0.001;
const S_MAX: f64 = 36_500.0;
/// Merge buckets: `ln(stability)` steps and difficulty steps.
const LN_S_STEP: f64 = 0.05;
const D_STEP: f64 = 0.25;
/// Longest forecast (days).
pub const MAX_DAYS: u32 = 730;
/// Answer time used when the history has none (seconds).
pub const DEFAULT_SECS_PER_ANSWER: f64 = 8.0;

/// FSRS memory model (versions 4.5 / 5 / 6 by weight count) for the forecast.
#[derive(Debug, Clone, PartialEq)]
pub struct Fsrs {
    w: [f64; 21],
    /// Forgetting-curve decay (positive): `w20` for FSRS-6, 0.5 before.
    decay: f64,
    /// `0.9^(-1/decay) - 1`.
    factor: f64,
    /// Divisor of the lapse stability floor (`exp(w17·w18)`; FSRS-4.5: none).
    failure_floor_divisor: Option<f64>,
}

impl Fsrs {
    /// From preset weights: 17 (FSRS-4.5), 19 (FSRS-5) or 21 (FSRS-6) finite numbers; anything
    /// else (empty included) = the FSRS-6 defaults.
    pub fn new(weights: &[f64]) -> Self {
        let valid = matches!(weights.len(), 17 | 19 | 21) && weights.iter().all(|x| x.is_finite());
        let mut w = DEFAULT_WEIGHTS;
        let len = if valid {
            w[..weights.len()].copy_from_slice(weights);
            weights.len()
        } else {
            21
        };
        let decay = if len == 21 && w[20] > 0.0 { w[20] } else { 0.5 };
        let factor = 0.9f64.powf(-1.0 / decay) - 1.0;
        Self {
            w,
            decay,
            factor,
            failure_floor_divisor: (len >= 19).then(|| (w[17] * w[18]).exp()),
        }
    }

    /// Probability of recall `t` days after a review, for stability `s`.
    pub fn retrievability(&self, t: f64, s: f64) -> f64 {
        (1.0 + self.factor * t.max(0.0) / s.max(S_MIN)).powf(-self.decay)
    }

    /// Whole days until recall falls to `retention` (at least 1, at most `max`).
    pub fn interval(&self, s: f64, retention: f64, max: u32) -> i64 {
        let raw = s / self.factor * (retention.powf(-1.0 / self.decay) - 1.0);
        let max = f64::from(max.max(1));
        // In 1..=max after clamping, so the cast is exact.
        raw.round().clamp(1.0, max) as i64
    }

    /// Initial difficulty for a first rating (1–4).
    pub fn init_difficulty(&self, rating: u8) -> f64 {
        self.w[4] - (self.w[5] * f64::from(rating.saturating_sub(1))).exp() + 1.0
    }

    /// Stability after the first "Good".
    pub fn init_stability_good(&self) -> f64 {
        self.w[2].clamp(S_MIN, S_MAX)
    }

    /// Difficulty after a rating (1 = Again, 3 = Good).
    pub fn next_difficulty(&self, d: f64, rating: u8) -> f64 {
        let delta = -self.w[6] * (f64::from(rating) - 3.0);
        let next = d + (10.0 - d) * delta / 9.0;
        (self.w[7] * (self.init_difficulty(4) - next) + next).clamp(1.0, 10.0)
    }

    /// Stability after a successful ("Good") review at recall `r`.
    pub fn stability_after_success(&self, s: f64, d: f64, r: f64) -> f64 {
        let inc = self.w[8].exp()
            * (11.0 - d)
            * s.powf(-self.w[9])
            * (((1.0 - r) * self.w[10]).exp() - 1.0);
        (s * (inc + 1.0)).clamp(S_MIN, S_MAX)
    }

    /// Stability after a lapse at recall `r`.
    pub fn stability_after_failure(&self, s: f64, d: f64, r: f64) -> f64 {
        let raw = self.w[11]
            * d.powf(-self.w[12])
            * ((s + 1.0).powf(self.w[13]) - 1.0)
            * ((1.0 - r) * self.w[14]).exp();
        let floor = self.failure_floor_divisor.map_or(s, |div| s / div);
        raw.min(floor).clamp(S_MIN, S_MAX)
    }
}

/// Where a card stands, for the forecast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForecastState {
    /// Not studied yet: introduced by the pace.
    New,
    /// In (re)learning steps.
    Learning,
    /// Graduated.
    Review,
}

/// A (non-suspended) card of the scope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForecastCard {
    /// State.
    pub state: ForecastState,
    /// Days from today until it is due (≤ 0 = due today). Ignored for new cards.
    pub due_in: i64,
    /// FSRS stability (days), if known.
    pub stability: Option<f64>,
    /// FSRS difficulty, if known.
    pub difficulty: Option<f64>,
    /// Days between its last review and today, if known.
    pub elapsed: Option<i64>,
}

/// Options of [`forecast`].
#[derive(Debug, Clone, PartialEq)]
pub struct ForecastParams {
    /// FSRS weights of the scope's preset (empty = defaults).
    pub weights: Vec<f64>,
    /// Desired retention (0.70–0.99).
    pub desired_retention: f64,
    /// Longest interval (days).
    pub max_interval: u32,
    /// Learning steps of the preset.
    pub learning_steps: u32,
    /// Relearning steps of the preset.
    pub relearning_steps: u32,
    /// New cards introduced per day (the pace being planned).
    pub new_per_day: u32,
    /// New cards already started today (today's allowance is the rest).
    pub new_done_today: u32,
    /// Days to forecast, today included (1–[`MAX_DAYS`]).
    pub days: u32,
    /// Mean time of one answer (seconds).
    pub secs_per_answer: f64,
}

/// One forecast day.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ForecastPoint {
    /// Days from today.
    pub offset: u32,
    /// Expected answers on cards already started before that day (reviews, relearning and
    /// learning steps).
    pub reviews: f64,
    /// Answers on the new cards introduced that day (their learning steps).
    pub new_answers: f64,
    /// New cards introduced that day.
    pub introduced: f64,
    /// All answers (`reviews + new_answers`).
    pub answers: f64,
    /// Minutes for `answers`.
    pub minutes: f64,
    /// Cards started by the end of the day (mass in the simulation).
    pub started: f64,
    /// New cards not introduced yet at the end of the day.
    pub unseen: f64,
}

/// Result of [`forecast`].
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    /// One point per day, today first.
    pub days: Vec<ForecastPoint>,
    /// Cards in the forecast (new + started).
    pub total_cards: f64,
    /// Day (offset) on which the last new card is introduced (`None`: none left to introduce,
    /// or not within the forecast).
    pub new_finished_on: Option<u32>,
}

impl Forecast {
    /// Mean answers and minutes per day over the (up to) 7 days ending at `offset`.
    pub fn week_ending(&self, offset: u32) -> (f64, f64) {
        let end = (offset as usize).min(self.days.len().saturating_sub(1));
        let start = end.saturating_sub(6);
        let slice = self.days.get(start..=end).unwrap_or(&[]);
        if slice.is_empty() {
            return (0.0, 0.0);
        }
        let n = slice.len() as f64;
        (
            slice.iter().map(|p| p.answers).sum::<f64>() / n,
            slice.iter().map(|p| p.minutes).sum::<f64>() / n,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct Cohort {
    mass: f64,
    s: f64,
    d: f64,
    /// Day (offset) of the last review; may be negative or fractional after merging.
    last: f64,
}

/// The expected-value simulation of one group of cards.
struct Sim<'a> {
    fsrs: &'a Fsrs,
    retention: f64,
    max_interval: u32,
    relearning_steps: f64,
    /// (due day, ln-stability bucket, difficulty bucket) → merged cohort.
    queue: BTreeMap<(i64, i64, i64), Cohort>,
    answers: Vec<f64>,
    /// Mass in the simulation after each day.
    mass: Vec<f64>,
    /// Mass not yet in `queue` that joins on a day (learning cards graduating), for the mass
    /// count: started mass = queue + pending learning.
    pending: f64,
}

impl<'a> Sim<'a> {
    fn new(fsrs: &'a Fsrs, p: &ForecastParams, days: usize) -> Self {
        Self {
            fsrs,
            retention: p.desired_retention,
            max_interval: p.max_interval,
            relearning_steps: f64::from(p.relearning_steps),
            queue: BTreeMap::new(),
            answers: vec![0.0; days],
            mass: vec![0.0; days],
            pending: 0.0,
        }
    }

    fn push(&mut self, due: i64, c: Cohort) {
        if c.mass <= 0.0 {
            return;
        }
        let key = (
            due,
            (c.s.ln() / LN_S_STEP).round() as i64,
            (c.d / D_STEP).round() as i64,
        );
        match self.queue.get_mut(&key) {
            Some(m) => {
                let total = m.mass + c.mass;
                m.s = (m.s * m.mass + c.s * c.mass) / total;
                m.d = (m.d * m.mass + c.d * c.mass) / total;
                m.last = (m.last * m.mass + c.last * c.mass) / total;
                m.mass = total;
            }
            None => {
                self.queue.insert(key, c);
            }
        }
    }

    /// A card that was just answered "Good" for the last time in its steps on `day`.
    fn graduate(&mut self, day: i64, mass: f64, s: f64, d: f64) {
        let ivl = self.fsrs.interval(s, self.retention, self.max_interval);
        self.push(
            day + ivl,
            Cohort {
                mass,
                s,
                d,
                last: day as f64,
            },
        );
    }

    fn review(&mut self, day: i64, c: Cohort) {
        let f = self.fsrs;
        let r = f.retrievability(day as f64 - c.last, c.s);
        let idx = day as usize;
        self.answers[idx] += c.mass * (1.0 + (1.0 - r) * self.relearning_steps);
        let pass = Cohort {
            mass: c.mass * r,
            s: f.stability_after_success(c.s, c.d, r),
            d: f.next_difficulty(c.d, 3),
            last: day as f64,
        };
        let fail = Cohort {
            mass: c.mass * (1.0 - r),
            s: f.stability_after_failure(c.s, c.d, r),
            d: f.next_difficulty(c.d, 1),
            last: day as f64,
        };
        let pass_ivl = f.interval(pass.s, self.retention, self.max_interval);
        let fail_ivl = f.interval(fail.s, self.retention, self.max_interval);
        self.push(day + pass_ivl, pass);
        self.push(day + fail_ivl, fail);
    }

    /// Runs every day; `learning[d]` = (mass, s, d) of learning cards answered on day `d`.
    fn run(&mut self, learning: &BTreeMap<i64, Vec<(f64, f64, f64)>>) {
        let days = self.answers.len() as i64;
        for day in 0..days {
            let due: Vec<(i64, i64, i64)> = self
                .queue
                .range((day, i64::MIN, i64::MIN)..=(day, i64::MAX, i64::MAX))
                .map(|(k, _)| *k)
                .collect();
            for key in due {
                if let Some(c) = self.queue.remove(&key) {
                    self.review(day, c);
                }
            }
            if let Some(items) = learning.get(&day) {
                for &(mass, s, d) in items {
                    // Their next answer finishes the steps.
                    self.answers[day as usize] += mass;
                    self.pending -= mass;
                    self.graduate(day, mass, s, d);
                }
            }
            self.mass[day as usize] =
                self.queue.values().map(|c| c.mass).sum::<f64>() + self.pending;
        }
    }
}

/// Forecast of the next `p.days` days (see the module docs).
pub fn forecast(cards: &[ForecastCard], p: &ForecastParams) -> Forecast {
    let fsrs = Fsrs::new(&p.weights);
    let days = p.days.clamp(1, MAX_DAYS) as usize;
    let retention = p.desired_retention.clamp(0.5, 0.999);
    let p = &ForecastParams {
        desired_retention: retention,
        ..p.clone()
    };
    let good_s = fsrs.init_stability_good();
    let good_d = fsrs.init_difficulty(3).clamp(1.0, 10.0);

    // Cards already started.
    let mut existing = Sim::new(&fsrs, p, days);
    let mut learning: BTreeMap<i64, Vec<(f64, f64, f64)>> = BTreeMap::new();
    let mut unseen = 0.0f64;
    for c in cards {
        let s = c.stability.filter(|s| s.is_finite() && *s > 0.0);
        let d = c
            .difficulty
            .filter(|d| d.is_finite())
            .map(|d| d.clamp(1.0, 10.0));
        let due = c.due_in.max(0);
        match c.state {
            ForecastState::New => unseen += 1.0,
            ForecastState::Learning => {
                existing.pending += 1.0;
                learning.entry(due).or_default().push((
                    1.0,
                    s.unwrap_or(good_s),
                    d.unwrap_or(good_d),
                ));
            }
            ForecastState::Review => {
                let s = s.unwrap_or(good_s).clamp(S_MIN, S_MAX);
                // Last review (day offset, ≤ 0); unknown: one interval before its due day.
                let last = match c.elapsed {
                    Some(e) => -e.max(0),
                    None => (c.due_in - fsrs.interval(s, retention, p.max_interval)).min(0),
                };
                existing.push(
                    due,
                    Cohort {
                        mass: 1.0,
                        s,
                        d: d.unwrap_or(good_d),
                        last: last as f64,
                    },
                );
            }
        }
    }
    existing.run(&learning);

    // One new card introduced on day 0.
    let mut unit = Sim::new(&fsrs, p, days);
    unit.graduate(0, 1.0, good_s, good_d);
    unit.answers[0] += f64::from(p.learning_steps) + 1.0;
    unit.run(&BTreeMap::new());

    // Introductions: the pace, today minus what was already started, until none are left.
    let mut intro = vec![0.0f64; days];
    let mut left = unseen;
    let mut new_finished_on = None;
    for (day, slot) in intro.iter_mut().enumerate() {
        let allowance = if day == 0 {
            f64::from(p.new_per_day.saturating_sub(p.new_done_today))
        } else {
            f64::from(p.new_per_day)
        };
        let n = allowance.min(left);
        *slot = n;
        left -= n;
        if n > 0.0 && left <= 0.0 && new_finished_on.is_none() {
            new_finished_on = Some(day as u32);
        }
    }

    let secs = if p.secs_per_answer.is_finite() && p.secs_per_answer > 0.0 {
        p.secs_per_answer
    } else {
        DEFAULT_SECS_PER_ANSWER
    };
    let mut out = Vec::with_capacity(days);
    let mut not_yet = unseen;
    for day in 0..days {
        let mut reviews = existing.answers[day];
        let mut started = existing.mass[day];
        for k in 0..=day {
            if intro[k] > 0.0 {
                started += intro[k] * unit.mass[day - k];
                if k < day {
                    reviews += intro[k] * unit.answers[day - k];
                }
            }
        }
        let new_answers = intro[day] * unit.answers[0];
        not_yet -= intro[day];
        let answers = reviews + new_answers;
        out.push(ForecastPoint {
            offset: day as u32,
            reviews,
            new_answers,
            introduced: intro[day],
            answers,
            minutes: answers * secs / 60.0,
            started,
            unseen: not_yet.max(0.0),
        });
    }
    Forecast {
        days: out,
        total_cards: cards.len() as f64,
        new_finished_on,
    }
}

#[cfg(test)]
mod tests;
