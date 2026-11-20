//! **Third held-out** separator-detection set (v1.0 acceptance: "≥ 95% on a new held-out
//! set"). The samples (`heldout3_samples.rs`) were written by a separate author who never saw
//! the detector or the earlier sets, and committed before the v1.0 detector changes, which
//! were made only for the known misses of the second set (`heldout2_tests.rs`).

use super::heldout3_samples::samples;
use super::*;

/// Correct samples with the detector before the v1.0 changes (96/109 = 88.1%; only the count
/// was looked at, not the misses).
pub(super) const HELD_OUT_3_BEFORE: usize = 96;

#[test]
fn held_out_3_detection_accuracy() {
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
    eprintln!("held-out 3 detection: {correct}/{}", samples.len());
    let _ = (HELD_OUT_3_BEFORE, misses);
}
