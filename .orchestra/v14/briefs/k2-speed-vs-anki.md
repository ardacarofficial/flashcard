# K2: a budget that broke, and the places where Anki is clearly faster

Read `j-common.md` and `docs/12-performans.md` (especially the new "Anki ile karşılaştırma" section and how the perf
suite is run) first. The machine is shared with one sibling (K1, `-j 2`), so judge every change by before/after
measured under the same conditions, several runs, medians; the orchestrator runs the full-scale suite on a quiet
machine afterwards.

## 1. A budget is broken on the merged tree (do this first)

`FC_PERF_DIR=<dir> cargo test -p fc-app --release --lib perf_suite_100k -- --ignored --nocapture` on commit
`2a19ed9`, quiet machine:

```
| browser page (worst)   |    110.1 ms | budget    100 ms |     <- FAILS (was 79 ms on 2ad9c8a, v1.4.0)
| … by keyset cursor     |     18.5 ms | budget    100 ms |
| study.start (all)      |    391.2 ms | budget    500 ms |     (was 467)
| stats cached overview  |    644.3 ms | budget   1000 ms |
```

Find which change made the browser page slower (v1.4.1's robustness round or v1.4.2's merges: the write-path review
in fc-storage, the Anki work, the speech provider), with a measurement per suspect, and fix the cause. Do not raise
the budget. If it turns out to be noise, show it with numbers from both commits on the same machine state.

## 2. Answering a card: Anki 0.6 ms, we 5.6 ms (p50)

The comparison found more than half of our time in the load balancer, with a cost that grows with the number of due
cards per day. Make that cost independent of collection size (keep per-day counts incrementally, or compute them once
per session and update on answer; decide from the code), without changing what the load balancer decides: a test
must show the chosen intervals are identical before and after for a seeded sequence. Then profile what is left of the
answer path and take what is cheap and safe. The answer stays durable when the command returns (that is a deliberate
difference from Anki; do not trade it away).

## 3. Building the day's queue: Anki 44–64 ms, we 399 ms

We read every due card and its note (and render templates to detect empty cards) and write the session to the
database; Anki reads up to the limits. Make `study.start` read what it needs for the first screen and the limits
(counts by SQL, cards up to the day's limit, the rest lazily or in pages), keeping the order and the counts exactly as
today: a test compares the full ordered queue and the counts before and after on a seeded collection, for the default
session and for a custom one. The budgets `study.start (all)` and the film-layer `study.options / first card` must
improve, not just hold.

## 4. If time remains: `.apkg` import (862 ms vs 3 498 ms) and export (1 104 vs 2 169 ms)

Profile first; fix what is structural (per-row statements, repeated lookups, compression settings), with the real-Anki
round-trip tests still passing.

Report the before/after table per item and update `docs/12-performans.md` (our numbers and the comparison table, with
the caveats kept honest) and the CHANGELOG under "Unreleased (v1.4.2)". Do not bump the version. A sibling (K1) works
in `fc-storage` (media write, migrations) and `fc-app/src/app.rs` start-up: stay out of those.
