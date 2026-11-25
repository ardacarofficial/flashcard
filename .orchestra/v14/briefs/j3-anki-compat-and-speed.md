# J3: Anki compatibility on real decks, and an honest speed comparison

Read `j-common.md`. Read `crates/fc-interop/src/anki/`, `crates/fc-app/src/modules/io/real_anki_tests.rs`,
`docs/12-performans.md` and how CI opens our exports in real Anki first.

## A. Compatibility

1. `crates/fc-interop/src/anki/convert.rs` (around line 701) skips review-log entries made by manual rescheduling.
   Decide what is right for the owner's history (import them as what they are, so intervals and due dates after
   import match Anki's), implement it, test it with a package made by real Anki.
2. Build a wider set of real-Anki fixtures with the `anki` Python package (the one CI already uses; install it in
   a venv in your scratch dir, not in the repo): cloze, image occlusion, several templates per note type, sub-decks,
   deck options with FSRS on and off, suspended/buried/flagged cards, tags with hierarchy, media with non-ASCII
   names, filtered decks, a large review history, a scheduler-v3 collection, an old `.apkg` (schema 11) and a new
   one (`.anki21b`, zstd). For each: import → compare with what Anki itself reports (card counts per state, due
   dates, intervals, ease or FSRS memory state, review counts, media, tags) → export → open in Anki → compare
   again. Every difference is either fixed, or listed with the reason it is acceptable.
3. Write `docs/16-anki-uyum.md` (Turkish, short): the table of what round-trips exactly, what changes and why,
   what is not supported. Link it from `docs/README.md`. Keep fixtures small enough for the repo (generate large
   ones in the test, do not commit them).

## B. Speed comparison

Same machine, same data: build the 100k-note / 2M-review collection the perf suite uses, export it for Anki (or
generate the same shape with the `anki` package), and measure Anki's own backend (through the `anki` Python
package: open collection, build the day's queue / get the next card, answer a card, search queries equivalent to
ours, browser page, stats, import, backup/export) against our numbers from `perf_suite_100k`. Be honest about what
is and is not comparable (Python call overhead, UI not included on either side, different feature work per call);
run each several times, report medians. Add the table to `docs/12-performans.md` under its own heading with the
method and the caveats. Where Anki is clearly faster on a comparable operation, find out why and either fix it
(with before/after numbers and no budget regressions) or explain it. No marketing sentences: numbers and caveats.

If the `anki` package cannot be installed here (network policy), stop that part, say exactly which host was
refused, and finish everything that does not need it.
