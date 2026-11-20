//! **Third held-out** separator-detection set. The samples (`heldout3_samples.rs`) were
//! written by a separate author who never saw the detector or the earlier sets, and committed
//! before the v1.0 detector changes, which were made only for the known misses of the second
//! set (`heldout2_tests.rs`).
//!
//! * Before the v1.0 changes: 96/109 (88.1%), count only.
//! * **After the set-2 changes, measured once: 99/109 (90.8%)** — the honest held-out number
//!   for that revision, below the ≥95% target. The misses were: `;` with commas inside the
//!   values, ` / ` (Korean romanisation, organ names), ` - ` repeated inside a definition,
//!   `=` inside `name: formula` values, `{{c1::…}}` cloze lines, a blank-line poem list,
//!   `==`, and colon lines with a time or a second colon in the value.
//! * The detector was then tuned for those misses (separator hierarchy `;` > `,` and `:` >
//!   `=`, ` / ` and `==` candidates, a repeated pair separator stays in the definition,
//!   cloze deletions are one token, prose paragraphs): **109/109, a tuned figure**. The
//!   held-out check for that revision is the fourth set (`heldout4_tests.rs`).

use super::heldout3_samples::samples;
use super::*;

/// Correct samples before the v1.0 changes (count only).
pub(super) const HELD_OUT_3_BEFORE: usize = 96;
/// Correct samples at the one held-out measurement (see the module docs).
pub(super) const HELD_OUT_3_MEASURED: usize = 99;
/// Correct samples now (tuned); lowering it means detection got worse.
const HELD_OUT_3_FLOOR: usize = 109;

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
    let report = format!(
        "held-out 3 detection: {correct}/{} correct; misses:\n{}",
        samples.len(),
        misses.join("\n")
    );
    eprintln!("{report}");
    const {
        const { assert!(HELD_OUT_3_BEFORE <= HELD_OUT_3_MEASURED) };
    };
    assert!(correct >= HELD_OUT_3_FLOOR, "{report}");
}
