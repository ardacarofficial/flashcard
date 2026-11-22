# R1 — study flow backend (Rust)
Goal: starting a study session becomes truthful and predictable. Implement CONTRACT.md §1.

Verified leads (gpt-review-1.md §A; reproduce each with a failing Rust test first):
- Home/deck counts come from deck inventory "now" (`crates/fc-storage/src/sqlite/decks.rs`), while the session queue
  (`crates/fc-app/src/modules/study/session.rs`) applies study-day boundary, subdeck settings, daily/session limits,
  sibling rules and renderability. Reproduction: 21 new cards, default limits, review 20 → Home advertises 1,
  `study.start` serves 0 and the session is closed silently.
- `study.start` closes the previous unfinished session even when the replacement is empty.
- A session with no `current` card is closed even when `next_due` says a learning card is waiting.
- Test mode has no resume path (`modes`), Learn/Match resume only same-kind.
- `reviewed:1` in the query language is a rolling 24 h, not the local study day.

OWN: `crates/fc-app/src/modules/study/**` (session, queue, mod; NOT `engine.rs` answer extraction/hints — sibling
R2 owns `engine.rs`), `crates/fc-app/src/modules/modes/**` only for start/resume/scope plumbing,
`crates/fc-storage/src/sqlite/decks.rs` + study/session storage, `crates/fc-api/src/v1/study.rs` (+ `decks.rs` DTO
fields for today counts, `method.rs`/error codes additively), `crates/fc-core` query bits for a study-day predicate,
generated TS types, `docs/02-ozellik-katalogu.md` / `docs/10-*` contract notes for the new method. Migrations if
needed: 1330–1349.
LEAVE-ALONE: `study/engine.rs`, hints, note types, notes (R2); all of `apps/desktop/src/lib/features/**` (F1 consumes
your API; you only regenerate types and, if `pnpm check` breaks because a DTO gained a required field, make the
minimal mock/type fix and list it).

Deliver
1. `study.options` exactly as in the contract, with Rust tests for every scope and every empty reason, including
   the 21/20 case, a waiting-learning-card case (next_available_at), a subdeck case and a module-off case.
2. `study.start` with `scope`, `CONFLICT_NOTHING_TO_STUDY`, practice forcing for the three practice scopes, frozen
   card set, and no closing of an unfinished session by an empty or failed start. Works for normal/learn/test/match.
3. Sessions with only waiting cards stay open and expose when the next card comes.
4. Resume: every kind can be resumed (Test included: an unfinished paper keeps its answers); `study.options.resumable`
   lists them with answered/remaining.
5. Today counts shown anywhere agree with the queue (contract invariant test).
6. "Studied today"/"struggled today" use the local study-day boundary (`study.day_start_hour`), any mode's answers.
7. Practice sessions never change `cards` scheduling (assert review_log mode and unchanged card rows in tests).
Keep everything an undoable command where it writes; keep `fc-core` pure (time via `Clock`).
