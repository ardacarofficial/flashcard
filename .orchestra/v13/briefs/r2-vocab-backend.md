# R2 — vocabulary correctness (Rust) : field roles, grading, hints, card backs, context
Goal: a word+sentence card behaves like a vocabulary card. Implement CONTRACT.md §2.

Verified leads (gpt-review-1.md §B, gpt-review-2.md §2; reproduce each with a failing test first):
- Typed/test answer for a subtitle note is extracted as `gitmek\nEve gittim.\n\nFilm` (meaning + example + source):
  `crates/fc-app/src/modules/study/engine.rs` (~line 499). A learner who types "gitmek" is marked wrong.
- Deck-page term list: Rust returns `word` and `surface` as the two columns for subtitle notes, so editing
  "Definition" changes the surface form: `crates/fc-app/src/modules/notes.rs` (~527) and
  `apps/desktop/src/lib/features/decks/TermList.svelte` (inline save).
- Hints only unmask letters (`crates/fc-study/src/hint.rs`); the owner wants the sentence with the word hidden.
- Card backs append the example unlabelled (`crates/fc-core/src/content/builtin.rs` ~518): "it feels like the
  continuation of the answer".
- Inbox acceptance drops the captured sentence for classic notes (`crates/fc-app/src/modules/inbox/mod.rs` ~542);
  CSV mapping offers only front/back (Rust side of import mapping).

OWN: `crates/fc-core/src/notetype.rs`, `crates/fc-core/src/content/**`, `crates/fc-core/src/subtitle.rs`,
`crates/fc-study/src/hint.rs`, `crates/fc-app/src/modules/study/engine.rs`, `crates/fc-app/src/modules/notes.rs`,
`notetypes.rs`, `cardtypes.rs`, `inbox/**`, `io/**` (Rust import mapping), `crates/fc-app/src/support/content.rs`
(seeding), `crates/fc-api/src/v1/{notetypes,notes,inbox,io}.rs` + the hint DTO in `study.rs` (additive only — sibling
R1 owns the rest of `study.rs`; keep your edit to the hint type and tell me), generated TS types, migrations
1300–1329, and on the frontend ONLY: `apps/desktop/src/lib/features/decks/TermList.svelte` (use `term_field` /
`definition_field` / `ambiguous`), the hint rendering spot in the study screen if the DTO shape forces it (smallest
edit; F1 owns that screen), locale keys `cards.part.*`.
LEAVE-ALONE: `study/session.rs` and queue logic (R1), study/mode screens (F1), `NoteTypesScreen.svelte` (S2 — the
full role editor UI is NOT in scope; roles just need to round-trip through the existing note-type API and be
settable via the API), `features/io` UI (S3).

Deliver
1. `role` on fields and `answer_field` on templates, serde-optional, pure in fc-core; round-trip tests; old JSON loads.
2. Idempotent, tested data migration for built-in note types (do not touch user-customised definitions; define
   and test how "customised" is detected). The owner's existing collection must get the fix, not only fresh ones.
3. Grading: typed answers and Test use `answer_field`; reverse cards ask for the term; cloze/choice unchanged.
   Test "went → gitmek" end to end: note with word=go, surface=went, meaning=gitmek, sentence "I went home."
4. Term projection + inline edit keys + `ambiguous` flag; TermList uses them.
5. Context hint as specified (UTF-16 offsets from captures when stored, else a whole-word, case-insensitive match
   of surface then term; never leak the answer; fall back to letters). Keep the hint-strategy setting; add the
   context strategy as the default first level when an example exists.
6. Built-in backs: meaning / example (+ translation) / source as distinct labelled parts with classes the UI can
   style (`.fc-part-meaning`, `.fc-part-example`, `.fc-part-source`); add minimal CSS in the built-in template CSS
   so it reads correctly even before F1 styles it.
7. Context preserved through inbox acceptance and import mapping by role.
Document the model in `docs/03-mimari.md` (short) and the migration in the changelog draft at
`.orchestra/v13/changelog-r2.md`.
