# v1.4 state — backend closed (Z1, 2026-11-23)

Version stays 1.3.0 (the design track is not built yet). All backend work of v1.4 is merged on `v14/z1`:
A1 backup, A2/A5 recall + capture diagnostics, A3 word index, A6 interrupted delivery, B2 frequency/CEFR,
B3 language queries, B4 words on every surface, C0 deterministic tests, C1 study modes, W1 encounters,
W2 meaning in context + accepted answers, W3 best scene + leech repair, W4 media origin + audio fill,
Z1 closing pass (measured speed, typed answers, consistency).

## Backend facts
- Migrations 1410–1490 (+ Z1's typed answers in 1421–1429); all in `crates/fc-storage/migrations/`, listed in
  docs/03 §15.
- New modules (on by default, switchable, tested OFF one by one with restart + study + capture in
  `fc-app/src/module_switch_tests.rs`): `words`, `word_data`, `quiz`, `accepted_answers`, `examples`, `audio_fill`.
- New settings are in `settings.schema` with TR/EN label + one-line description (Subtitle's only while the
  Subtitle module is on: the schema lists enabled modules only). The i18n lint now walks every module ON.
- Plugins: every method classified (`redact::every_grantable_method_is_classified`). MCP: only `words_*`
  added; the rest of v1.4 deliberately unreachable (docs/06); tools of a switched-off module are not listed.
- Performance: docs/12-performans.md (full-scale film-watching workload, release, this machine).

## Known limits / next
- Backups hold the whole collection dump in memory twice over a create (write + verify-by-restore): peak memory
  grows with the review history (see docs/12). Streaming the dump table by table is the next backup step.
- Settings UI for the v1.4 keys waits for the approved D2 prototype (only the schema exists).
- Windows manual run and the Subtitle extension in a real browser are still pending (from v1.3).
- Design track (D1–D3 prototypes) awaits the owner's approval; nothing of it is built.
