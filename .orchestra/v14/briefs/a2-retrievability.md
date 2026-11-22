# A2 — one retrievability formula everywhere (small), then A5
OWN: `crates/fc-study/src/scheduler/**` (retrievability API only), `crates/fc-core/src/query/{filter.rs,eval.rs}`
(the retrievability parts), `crates/fc-app/src/modules/browse/maintenance.rs`, `crates/fc-app/src/modules/stats/facts.rs`.

Problem (verified): stats use the preset's FSRS-6 parameters (`fc_study::scheduler::retrievability(.., &params)`),
card info uses a fixed `(1 + FSRS_FACTOR·x)^-0.5`, the search filter (`prop:r<0.9` style) inverts the fixed curve.
The same card can show different probabilities.

Do: one function is the truth (the preset's parameters, the same study-day calendar); card info, stats, search
filter and anything else that shows or filters by recall probability call it. fc-core stays pure: pass what it
needs in (decay/params per preset), no storage access. Add an equality test: for personalised weights and a
card near a threshold, card info, stats and the query agree. Grep for other copies (planning, forecast,
optimizer workload, AI prompts) and list what you found.

## Then, in the same session: A5 — "what happened to my last capture?"
OWN additionally: `crates/fc-app/src/modules/subtitle/capture.rs` and new files in that module for diagnostics,
`crates/fc-app/src/modules/maintenance/**` (diagnostic export only). Do NOT edit `subtitle/mod.rs` beyond a
minimal call/registration (worker A3 is changing the known-words part of it).
Do: a local, privacy-safe record per capture attempt (capture id, time, result code: created / duplicate /
rejected+reason / media missing+reason, matched card id, duration) kept bounded (last N, default 500;
migration 1430–1439), readable through `subtitle.captures.recent` (+ `fc call`), included in the diagnostic
export. Never log sentence text, URLs or video titles. Tests incl. the bound and the "no text stored" property.
