# C1 — study modes the way the owner expects them (backend + API; UI comes later)
OWN: the Learn/Test/Match mode code in `crates/fc-study` and `crates/fc-app/src/modules/study/**` (modes,
papers, options), their fc-api types, `study.options.modes`, docs/02 §6. LEAVE ALONE: Cards (flip + grade)
behaviour, scheduler, words module, the frontend except generated TS types + locale strings for mode names
and descriptions (the frontend will be rebuilt from an approved prototype: do NOT redesign screens; keep the
existing screens compiling and their journeys green by mapping old UI entry points onto the new API with the
smallest change, and say what you mapped).

The owner, in his words: "I thought Test was the one with 4 options; I click it and it asks true/false. Why?
Learn asks 4 options and shows the right answer when I pick. THAT should be Test."
Prototype decided with him (read `prototypes/v14/modlar.html` and its "Taslak notları" for behaviour detail):
- **Test** (mode id `quiz` or keep a stable id; display name "Test"): one word, FOUR options, the correct
  answer is revealed immediately after a pick, then next; ends with score + missed list + "study the missed
  ones again". Distractors come from the same deck/language and the same field role (v1.3 roles: ask `term`,
  options are `meaning`s, never the example/source), never duplicates or the same meaning; with < 4 candidate
  cards it degrades gracefully (fewer options) and says so. Default: PRACTICE (does not change the schedule);
  a setting can make it count (default off) — expose `affects_schedule` per mode in `study.options`.
- **Sınav** (today's "Test": a paper, mixed question types, score at the end): question types selectable
  (`multiple_choice`, `written`, `matching`, `true_false`); `true_false` is OFF by default; number of
  questions; add the `matching` question type if the backend lacks it. Display name "Sınav" / EN "Exam".
- **Öğren** stays: pick first, then type, until learned — one sentence description from the API.
- **Eşleştir**, **Kartlar** unchanged.
Every mode: `study.options.modes[]` gives id, name key, one-line description key, `affects_schedule`.
Existing saved sessions/papers of the old Test keep working (migration 1480–1489 if stored ids change; test).
Acceptance: Rust tests for option generation (roles, no duplicate meanings, small decks), immediate-feedback
flow, missed-list restart, exam type selection defaults; journeys (`pnpm -C apps/desktop e2e` with
`PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium`) still green; a new journey through the dev bridge
for Test: pick with keys 1–4, the answer shows, results read back from the API.
