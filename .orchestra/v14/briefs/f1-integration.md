# F1 — Toparlama: ekranları birbirine bağla, eksikleri kapat

Read `.orchestra/v14/briefs/e-common.md` (it binds you; you own the whole `apps/desktop` and may touch Rust).
E1, E2, E4 are merged into your base. E3 is NOT: `git merge v14/e3` in your worktree first and resolve the
conflicts (`locales/{en,tr}/subtitle.json`, `docs/11-tasarim.md`) keeping BOTH sides. Worker reports (what
each built and left open) are the last long assistant texts in
`/tmp/claude-0/-home-user/9995e59c-6b11-5385-8893-948e6f2fb895/scratchpad/v14/logs/{e1,e2,e3,e4}.jsonl`
(type "result" → "result"); read all four.

Then, in this order (commit after each group):

1. Wiring between siblings
   - ONE "Gelişmiş" switch everywhere: `shell/advanced.svelte.ts` (`app.show_advanced`, E2). The study launcher
     (E1 `launch.ts` loadAdvanced) and İlerleme (E4 `stats/advanced.svelte.ts`, localStorage) read/write that one.
   - `/words` in `navigation.ts` PLACEMENTS so Kelimelerim sits in the sidebar (fixes E3's 2 red vitest tests);
     sidebar label "İlerleme" (not "İstatistik"); E4's "son yedek" link opens Ayarlar › Yedek.
   - Mocks: `study/mock.ts` settings schema gets the `io.*`, `subtitle.*`, `words.*` keys; words mock lives in
     `features/words`; dev mock knows `modes.quiz.*` so the Test screen appears in mock screens.
2. Left open by the builders (do them properly, backend first where needed, with tests + gen-ts):
   - Simplified "Günlük tempo" settings page as `prototypes/v14/ayarlar.html` shows; remove the old guide
     questions from the General page only if the prototype replaces them (nothing lost: move, don't delete).
   - `words.mark` (undoable command) and meanings in `words.lookup`, `total` in `words.stats`, so Kelimelerim
     does not depend on the `reading` module; an API that lists the languages present in the collection for
     the language chip.
   - Known-words history series in the stats/words API and the "bildiğim kelimeler zamanla" line chart on İlerleme.
   - AI "bu cümledeki anlam" in the note editor / study card menu (`ai.meaning_in_context`, `ai.meaning.apply`),
     shown only when the AI module is on and a provider is set.
   - Browser keyset paging as a versioned API addition (budget "browser page" 100 ms at full scale, docs/12);
     skip only if it cannot be done safely in the time, and say so.
   - Journeys on the real core for: leech repair "Düzelt" (seed the reviews through the API/dev bridge),
     encounter move + forced merge, audio fill preview.
3. Whole-app pass: run `pnpm -C apps/desktop screens` for TR/EN × dark/light × 1280/900 and LOOK at every
   screen the four builders touched; fix overflow, clipped text, missing translations, dead entries with
   modules off, inconsistent names (one word per concept: Test, Sınav, Ekle, Ara, Kelimelerim, İlerleme, Gelişmiş).
4. Version 1.4.0: bump everywhere v1.3.0 is declared (Cargo workspace, package.json files, tauri.conf.json,
   lockfiles, docs status lines, CLAUDE.md status line), turn "Unreleased (v1.4)" in CHANGELOG into the
   1.4.0 section (grouped, plain words), update `.orchestra/v14/STATE.md` and `docs/README.md`.
5. Full verification, sequentially: cargo fmt, clippy (workspace, exclude fc-desktop), `cargo test --workspace
   --exclude fc-desktop`, gen-ts --check, `pnpm check`, `test` (full, all green), `build`, `bundle:check` if it
   exists, `e2e` (full suite once at the very end, all green), `node tools/subtitle-contract/check.mjs`,
   browser-extension check. Nothing else is running on the machine now; use all cores.

Tests must be OS-independent (Windows CI runs them: no Unix-only absolute paths, no wall-clock assumptions).
Time: about 110 minutes; if short, finish groups 1, 3, 4, 5 and list what is left of group 2.
