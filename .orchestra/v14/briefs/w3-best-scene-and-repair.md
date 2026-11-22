# W3 — the most teachable scene, and repairing a card that keeps failing (wave 3; encounters are merged)
OWN: new pure scoring in `fc-core` (e.g. `fc-core/src/encounter_score.rs`), an fc-app module for it
(`modules/examples` or inside `subtitle/encounters`: add files, smallest edits to existing ones), the leech
repair flow in `crates/fc-app/src/modules/stats/**` (leeches) + its fc-api types, docs. Migrations 1470–1479.
LEAVE ALONE: encounter storage schema and commands (use `subtitle.encounters.*`; W4 is adding audio to them),
scheduler, study modes, AI task internals (call `ai.meaning_in_context` only through its public path).

1. **Best scene.** For a note with several encounters (and for candidate sentences in reading mode), rank the
   examples and say WHY: number of unknown words besides the target (from the word index; proper names must not
   inflate it: capitalised mid-sentence tokens and tokens unknown to the frequency pack are treated as names),
   sentence completeness (cut-off lines, dangling "—", too short/too long), the target really occurring,
   audio present, image present, frequency of the surrounding words. Pure function + reasons list;
   `encounters.rank { note_id }` returns the ordered list with reasons; `encounters.suggest_primary` proposes
   (never switches by itself; a setting `subtitle.auto_best_example`, default OFF, may apply it on capture —
   undoable, schedule untouched). Near-duplicate sentences (punctuation/case/overlapping subtitle lines) are
   detected and reported as duplicates of each other. Tests with realistic film lines.
2. **Leech repair loop.** A card forgotten N times (existing leech threshold) gets a REPAIR proposal instead
   of only "set aside": evidence = the last failed reviews (answers typed, time taken, hint use), the current
   example and its score from (1), other available encounters ranked, the meaning vs accepted answers,
   whether another note of the same word exists (confusable). Proposed repairs (each an existing undoable
   command, previewed): switch to a better example, add accepted answers, split/move a meaning, AI rewrite
   (existing task) — and it records which repair was applied and reports the card's next real reviews
   afterwards (`stats.leeches.repairs`: before/after pass rate with the honest caveat "n is small").
   API only; no screens.
