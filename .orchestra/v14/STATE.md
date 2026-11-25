# v1.4 state — 1.4.2 closing round released (L1, 2026-11-25)

Version **1.4.2** (`v14/l1`): the closing round on top of 1.4.1. J1 (independent review of the write path:
salvage loses fewer rows and counts them, damaged media file repaired, no write after SQLite's own rollback,
plugin model URL local files only), J2 (installed-app Windows journeys J4–J7 incl. rescue; main window created
after the app state exists; Subtitle client check cannot pass silently), J3 (Anki manual reschedules kept as
`manual` rows, round trip checked with real Anki in CI, speed comparison with Anki's backend), J4 (offline
speech with Windows voices, `builtin.system`), K1 (the review's four open findings closed: process locks,
media fsync before commit, concurrent migration, AI capture hook) and K2 (answer + next card p50 ~6 → ~2 ms,
browser page budget was measurement noise) merged. No migration. Record: CHANGELOG 1.4.2, docs/12, 15, 16.

Still pending, all with the owner: manual use on Windows; Subtitle on a real YouTube/Netflix account; adding
the `SUBTITLE_REPO_TOKEN` repository secret so CI checks the real Subtitle client (then
`SUBTITLE_CHECK_REQUIRED=true`). The `webdriver-pilot` job is not a gate yet (`continue-on-error` until J4–J6
were green on three runs in a row).

## 1.4.1 (I1, 2026-11-23)

Version **1.4.1** (`v14/i1`): the robustness round. H1 (FSRS weights of every version, mirrored backups,
disaster restore), H2 (kill matrix, disk full, damaged-collection rescue, WAL checkpoint), H3 (fuzz/property
tests, 12 fixes), H4 (Subtitle "Flashcard'dan oku" patch) merged; I1 closed what they left open: every undoable
command proves undo (generator or a named, existence-checked test), `/v1` body reading aligned with Subtitle
(malformed = 400, stalled body = 408 after 30 s), unused media rows cleaned by the trash purge (never one
undo/redo can still reach), `backups/.verify-*` leftovers removed at start, mirror catch-up, in-app rescue on
the Bakım page. No migration. Verification record: CHANGELOG 1.4.1 and the I1 report.

## 1.4.0 (F1, 2026-11-23)


Version **1.4.0** (`v14/f1`): backend (below) + design track built in the app (E1 study modes, E2 settings and
navigation, E3 Add/Search/My words, E4 Progress/Home) + F1 integration (one Advanced switch, simple Günlük tempo,
`words.mark/languages/history`, meaning in context in editor and study, browser cursor paging, journeys F1a–c).
F1 details and what is left: see "F1 integration" at the end.

Backend history: all backend work of v1.4 was merged on `v14/z1`:
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
- Windows manual run and the Subtitle extension in a real browser are still pending (from v1.3).
- The owner has not answered the prototypes' A/B questions; the app builds each prototype's first-load variant,
  each flippable by one constant (listed in the E1–E4 and F1 reports / docs/11).

## F1 integration (2026-11-23)
- One Advanced switch (`app.show_advanced`) everywhere; "Gelişmiş" names only it (page disclosures: "Daha fazla ayar").
- Names: Ekle, İlerleme, Test (quiz) / Sınav (paper) also on the deck page; Kelimelerim in the sidebar.
- New API (additive, v1): `words.mark` (+ internal `words.restore`), `words.languages`, `words.history`,
  `words.lookup` meanings, `words.stats.total`, `planning.apply_pace` `reviews_per_day`/`desired_retention`,
  `search.cards` `cursor`/`next_cursor`. No migration.
- New screens/parts: Basic › Günlük tempo (`/settings/pace`), "Bu cümledeki anlam" (editor + study menu), known-words
  line chart on İlerleme. Journeys P1, F1a–F1c. Dialog keeps Esc/Tab when the focused action removes itself.
- Open: no MCP tools for the new words methods; MCP `search_cards` still pages by offset; history uses today's
  threshold and the current mark only; "Genel tempo" for all decks needs a core default (the pace page writes one
  deck); no real-core journey for "Bu cümledeki anlam" (needs a provider); full-scale perf suite not re-run on a
  quiet machine (study.start 663 ms under load in the keyset run).
