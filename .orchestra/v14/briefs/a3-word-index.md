# A3 — one word index: the single truth for "which words does he know"
OWN: new crate module(s) for the index (fc-core pure types/logic; fc-storage tables + queries, migrations
1410–1429; fc-app module `words` or inside `reading`), `crates/fc-app/src/modules/reading/words.rs`,
`crates/fc-storage/src/sqlite/reading.rs`, the known-words part of `crates/fc-app/src/modules/subtitle/mod.rs`
(~line 984), fc-api methods for it, lemma provider use.
LEAVE ALONE: backup/io, fc-study scheduler, `subtitle/capture.rs` (A2/A5), the frontend except generated types.

Problem (verified): two different calculations. Reading mode loads live card rows on every call, looks at a
`word` field or the sort field and builds an in-memory lemma index (misses custom note types' `term` role).
The Subtitle `known-words` endpoint starts from its own capture rows and calls a word known when a card's
interval ≥ 21 days. A word marked known in reading, learned from an Anki import or typed by hand may be
invisible to the other side.

Do:
1. A persisted index keyed by (language, lemma-or-expression): status with its EVIDENCE kept separately:
   manual mark (known / ignored), learning (has cards), known-by-maturity (card interval ≥ threshold, setting,
   default 21 d), with counts and the note/card ids behind it. A manual mark wins over derived status.
   Source: every note whose type has a `term` role (v1.3 roles) — hand-made, Anki-imported, Subtitle, reading —
   plus reading-mode marks. Language comes from the deck/note type/capture; notes with no language are
   reported, not guessed (a setting gives a default language per deck; say what you chose).
2. Kept up to date incrementally by the commands that change notes/cards/reviews/marks (and undo/redo), with a
   full `words.index.rebuild` job that must give the identical result (property/equality test).
3. Inflection ≠ derivation: "went"→"go" joins the lemma; "actor" does not make "act" known. Keep them distinct.
4. Reading mode (`reading.words.lookup/mark`, known-word percentage) and the Subtitle `known-words` endpoint
   both read the index (the HTTP response shape of `/subtitle/v1/known-words` must not change: check
   `/home/user/subtitle/docs/flashcards-api.md` and run `node tools/subtitle-contract/check.mjs`).
5. API: `words.lookup` (batch: forms → lemma, status, evidence), `words.list` (filters: language, status,
   paging), `words.stats` (counts per status per language). Read-only for now except the existing mark command.
6. Performance: no full table scan per lookup; test with ≥ 50k notes that a 500-word lookup stays fast (assert a
   generous bound, print the measured time).
