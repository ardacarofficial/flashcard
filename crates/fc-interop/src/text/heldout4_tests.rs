//! **Fourth held-out** separator-detection set (`heldout4_samples.rs`), written blind by a
//! separate author and committed before the detector changes for the third set's misses.
//!
//! **Measured once, after those changes: 100/104 (96.2%)** — the v1.0 held-out figure (target
//! ≥ 95%). The detector was not tuned against this set. Misses:
//!
//! * two blank-line samples whose records are `term⏎definition` (one value per line, no
//!   separator) — they read as a single-column word list;
//! * `dog - köpek, cat - kedi, …` on one line (`,` between records, ` - ` between fields);
//! * `15 - 7 = 8` (a ` - ` inside the question beats the ` = `).

use super::heldout4_samples::samples;
use super::*;

/// Correct samples at the one held-out measurement; lowering it means detection got worse.
const HELD_OUT_4_MEASURED: usize = 100;

#[test]
fn held_out_4_detection_accuracy() {
    let samples = samples();
    let mut misses = Vec::new();
    for (label, text, expected) in &samples {
        let d = detect(text);
        let got = parse_delimited(text, &d.options).unwrap_or_default();
        if &got != expected {
            misses.push(format!(
                "{label}: got {:?}/{:?}/{:?} → {got:?}",
                d.options.field_separator, d.options.record_separator, d.options.max_fields
            ));
        }
    }
    let correct = samples.len() - misses.len();
    let report = format!(
        "held-out 4 detection: {correct}/{} correct; misses:\n{}",
        samples.len(),
        misses.join("\n")
    );
    eprintln!("{report}");
    assert!(samples.len() >= 80);
    assert!(correct >= HELD_OUT_4_MEASURED, "{report}");
    assert!(correct * 100 >= samples.len() * 95, "{report}");
}
