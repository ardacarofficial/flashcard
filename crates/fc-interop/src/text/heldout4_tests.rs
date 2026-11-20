//! **Fourth held-out** separator-detection set (`heldout4_samples.rs`), written blind by a
//! separate author and committed before the detector changes for the third set's misses.

use super::heldout4_samples::samples;
use super::*;

/// Correct samples now; filled in at the first measurement.
fn correct() -> (usize, usize, Vec<String>) {
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
    (samples.len() - misses.len(), samples.len(), misses)
}

#[test]
fn held_out_4_detection_accuracy() {
    let (correct, total, _) = correct();
    eprintln!("held-out 4 detection: {correct}/{total}");
    assert!(total >= 80);
}
