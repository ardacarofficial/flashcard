# B3 — language-learning queries (wave 2; word index merged)
OWN: `crates/fc-core/src/query/**` (new predicates), `crates/fc-storage/src/sqlite/search.rs` and the search
side of fc-app, saved queries, docs/02 §9. LEAVE ALONE: `fc_core::recall` semantics (A2), words module internals
(read through its public functions/tables; if you need a column or index there, add it minimally and report),
frequency data (B2 is adding rank/band: design predicates so `freq:`/`level:` can plug in; if B2's tables are
not there yet, leave those two predicates behind a clear TODO-free seam: implemented against an optional
table and tested with a fixture).

Why: "the expressions from this episode, that have audio, that I have seen in two different scenes and am
still learning" should be ONE query that can be listed, counted and studied.
Do: indexed predicates (TR + EN aliases, one word per concept as in v1.3) for: word status from the index
(yeni/öğreniyorum/biliyorum/yok say), language, source/video/episode (from Subtitle capture data and reading
sources), number of encounters/examples, has audio / has image and audio origin, added-from (subtitle, reading,
manual, anki), lemma/expression match that understands inflection (`kelime:go` finds "went"). The same query
gives identical results for count, list and "study this query" (test that equality, incl. multi-scene notes not
being double counted). Explain plans / timing test at 100k notes. Search help text updated (TR/EN locale).
