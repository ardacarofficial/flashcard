# Z1 — backend closing pass: measured speed, loose ends, one consistent whole (the machine is quiet now)
You are the only worker running. All three backend waves are merged (see CHANGELOG "Unreleased (v1.4)" and
`.orchestra/v14/briefs/*`). Do, in this order, committing after each:

1. **Measured speed at scale** (was deferred to a quiet machine). Extend the existing full-scale perf suite
   (`crates/fc-app/src/perf_tests.rs`: 100k notes / 200k cards / 2M reviews; CI runs a scaled version) with the
   film-watching workload: 20k Subtitle encounters over 6k notes (several encounters per note), realistic
   media (thousands of small WAV/JPEG files; generate tiny valid ones), mixed languages, the English word pack.
   Measure and assert generous-but-meaningful bounds, reporting p50/p95/max and peak RSS where you can:
   a capture arriving while a study answer is being saved; `study.options` + next card; `words.lookup` of 500
   forms; `words.list` sorted by frequency; language queries (`durum:`, `karşılaşma:`, `ses:`, `sıklık<`);
   `subtitle.encounters.rank`; index rebuild; backup create + verify-by-restore time and peak memory with
   ~1 GB-equivalent media (use the streaming test knob if a real GB is too slow; say which); app start.
   Fix what is clearly wrong (missing index, N+1) with the smallest change; report before/after numbers.
   Write the table into `docs/12-performans.md` (new) with the machine description and "debug vs release".
2. **Typed answers are not stored** (W3 found it: leech repair evidence has `typed_answers_recorded: false`).
   Store what was typed for written answers (Learn written stage, Exam written, Cards type-in) with the
   review/answer record: migration 1421–1429, bounded per card (keep the last 20), included in backup, never
   in logs/diagnostics, a setting to switch it off. Feed the leech repair evidence. Tests.
3. **Consistency sweep** across the 14 merged pieces: every new setting has TR/EN label + one-line plain
   explanation and appears in the settings catalogue the UI reads; every new module is in `modules.list` and
   can be switched off without breaking the others (test each OFF: app starts, study works, captures work);
   every new method is classified for plugins and MCP; docs/02, 03, 06, 10 agree with the code (fix the docs,
   or the code when the docs are right); CHANGELOG "Unreleased (v1.4)" is one coherent, grouped Turkish list.
   `.orchestra/v14/STATE.md`: replace with the true final state of the backend.
4. **Full verification**, then report exact numbers: `cargo fmt --all --check`; clippy `-D warnings`;
   `cargo test --workspace --exclude fc-desktop --no-fail-fast`; the full-scale perf suite once; gen-ts
   `--check`; `pnpm -C apps/desktop install`, `check`, `test`, `build`, `bundle:check`;
   `FC_BIN=$PWD/target/debug/fc PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium pnpm -C apps/desktop e2e`;
   `node tools/subtitle-contract/check.mjs`; `pnpm -C apps/browser-extension install && pnpm -C apps/browser-extension check`.
Do not bump the version (the design track is not built yet). Report ≤250 words + the perf table.
