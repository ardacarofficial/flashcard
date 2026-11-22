# W1 — a capture is an ENCOUNTER, not a card (wave 3; the big one)
OWN: `crates/fc-core/src/subtitle.rs`, `crates/fc-app/src/modules/subtitle/**` (capture → note mapping),
`crates/fc-storage` subtitle tables (migrations 1490–1499), fc-api subtitle/encounter types and methods,
docs/06 + docs/02 §25. LEAVE ALONE: word index internals (use its public API; it must stay correct: its
incremental = rebuild test must still pass), query language internals (add the encounter count source B3's
`karşılaşma:`/`seen:` predicate should read, with the smallest edit there), study modes (C1), backup internals.
Read `.orchestra/v14/gpt-backend-review.md` (top item 1 and the first structural weakness) first.

Why: today three scenes with "pick up" can make three notes. Two are better examples of the same meaning,
the third may be another meaning. And the capture contract is richer than what is stored: `video_id`,
`t_end_ms`, definition source, capture kind are not kept as queryable fields.

Do:
1. Persist every capture as an encounter with its source: capture id, language, lemma/expression + surface,
   sentence, term position in the sentence, video/episode id + title, start–end ms, capture kind, definition
   + its source, media (image/audio) links, received-at. Old captures are backfilled from what exists
   (missing source facts stay NULL; never invented). Privacy: this is the user's own local data (unlike the
   diagnostics log, text IS stored here).
2. An encounter is linked to a note (term + meaning). Default behaviour on a new capture of a term that already
   has a note in the same language: attach it as ANOTHER EXAMPLE of that note (no new card, schedule and
   history untouched) UNLESS the captured definition/meaning differs (documented comparison rule; when unsure
   → keep it separate and flag `needs_review`, the user decides: "aynı anlam" merges, "farklı anlam" keeps a
   separate note). A setting chooses the default (`subtitle.same_word`: add_example [default] / always_new_card
   = today's behaviour). The HTTP response stays contract-compatible (`created` / `duplicate` semantics in
   `/home/user/subtitle/docs/flashcards-api.md`; if a third outcome is needed, map it onto what the real
   client tolerates and prove with `node tools/subtitle-contract/check.mjs`, extending it).
3. Commands (undoable, oplog): `encounters.list` (by note / word / video), `encounters.set_primary` (the
   example shown on the card changes; card id, schedule and review history stay — test that due date and
   stability are identical before/after), `encounters.move` (to another or a new note = "this is a different
   meaning"), `encounters.merge_notes` (two notes of one word become one; reviews: keep the note with more
   history, the other's review log is preserved and reattached or archived — choose, document, test).
4. Card rendering: the note's example/source/audio/image fields show the PRIMARY encounter; others are
   available to templates and the API as a list (smallest change to the v1.3 Subtitle note type; migration +
   test; existing cards render the same as before).
5. Backup/restore and `.fcpack`/Anki export keep working (encounters in backup; Anki export gets the primary
   example; say what is lost). Word index and capture diagnostics stay correct.
Acceptance (Rust + through HTTP/`fc call`, reading persisted results back): resend does not add a card; two
scenes of one meaning → one card, two examples; a different meaning stays separate; switching the primary
example leaves the schedule identical; restore into a clean profile plays the same audio.
