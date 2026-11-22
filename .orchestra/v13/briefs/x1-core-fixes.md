# X1 — last core fixes (Rust first), decided by the orchestrator
State: all eight v1.3 pieces are merged and run against the real core (`pnpm -C apps/desktop e2e`: 22 journeys
pass, S7c left red on purpose). Read `.orchestra/v13/STATE.md` and `apps/desktop/e2e/README.md`. Three things are
decided; implement them with tests and turn every journey green. You may edit any Rust crate and the journey
files; on the frontend only what these three items need (a reviewer agent is polishing the UI in parallel: do not
restyle screens, do not touch settings/stats/add-cards screens).

1. **"All decks" queue must honour each deck's own settings.** Today the collection-wide queue (Home headline,
   launcher with no deck, `decks.summaries.today`) applies one fixed default new-cards/day limit instead of the
   decks' presets, so after 20 new cards in ANY deck Home shows 0 for everything. Correct rule: the collection
   queue is the union of what each deck would serve today under its own preset limits (new/day, reviews/day,
   subdeck rules, siblings), in a sensible interleaved order; a collection-level cap applies only if the user has
   set one. Invariants (tests): sum of per-deck `today_*` == collection `today_*`; studying deck A to its limit
   leaves deck B's count unchanged; `study.options(deck=null).scopes[due].count` == cards actually served by a
   collection session. Then restore journeys S1/S2 to start from Home with no ordering tricks.
2. **Context hint rule (S7c).** Rule: *a hint may reveal context but never the answer; blank exactly the answer
   when it occurs in the example; if it does not occur, show the example as context.* Concretely:
   - Built-in vocabulary/subtitle FORWARD card (asks for the meaning): front shows the word only (surface form
     large; dictionary form small beside it when different) — NOT the sentence. First hint = the example sentence
     with the word highlighted (context, `HintDto.kind = "context"`); next levels = letter hints of the meaning.
   - REVERSE card (shows the meaning, asks for the term): first hint = the example sentence with the term/surface
     blanked ("I ___ home."); never leak it.
   - Back unchanged: meaning / example (+ translation) / source.
   Change the built-in templates accordingly through a new idempotent data migration (next free number after
   1300; untouched for user-customised templates, same detection as migration 1300) with tests, update seeds,
   the mock card HTML, and S7c (make it assert the real rule for both directions). Check Learn/Test/Match prompts
   still make sense with the wordless-sentence front.
3. **Learn-ahead and the waiting screen.** The core shows a waiting learning card up to 20 minutes early, so with
   short steps the "Sıradaki kart N dk sonra" state is never seen and `next_available_at` is "due − 20 min".
   Keep learn-ahead (it avoids pointless waiting) but make it truthful: `next_available_at` is the moment the card
   will actually be served; when the only remaining cards are within the learn-ahead window serve them (no
   waiting screen); the waiting screen appears only when the user would really have to wait, and its countdown
   matches. Make the learn-ahead window a documented setting with the current value as default (settings schema +
   i18n TR/EN, described in plain words). Tests for both sides of the boundary; journey S6 on a realistic step.

Also: re-read `apps/desktop/src-tauri/src/ipc.rs` and `crates/fc-app/src/transport.rs` once more as a compiler
would (the Tauri shell cannot be built here; a Windows CI build is the first real compile) and fix anything
doubtful. Run the full checks from COMMON.md plus `pnpm -C apps/desktop check`, `test`, `e2e`. Report as COMMON.md
says, listing every journey's result.
