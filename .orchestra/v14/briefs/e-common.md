# E-workers — build the v1.4 screens in the app (shared rules)

Read `.orchestra/v14/COMMON.md`, `.orchestra/v14/briefs/d-common.md` (the owner's words and the five
principles; they bind you too), `docs/11-tasarim.md`, `.orchestra/v14/STATE.md`, then your brief.

The three prototypes in `prototypes/v14/` (`modlar.html`, `ayarlar.html`, `olustur-ara-istatistik.html`) are
the design. Open the one(s) your brief names with Playwright, click through every path, read its
"Taslak notları" drawer. The owner has NOT answered the A/B questions yet, so: build the variant the
prototype shows on first load (its recommended one) and keep the choice cheap to flip (one constant or
setting, named in your report). List every A/B you defaulted in the report.

Rules
- Frontend only shows. Every number, list, rule comes from the Rust API (`apps/desktop/src/lib/api/generated`).
  If the screen needs something the API lacks, add it in Rust properly (command, oplog, tests, gen-ts), small
  and inside your area; no logic in Svelte.
- Nothing is removed: whatever v1.3 could do is still reachable (behind "Gelişmiş" if the prototype says so).
  The prototype's "what moved where" list is your checklist.
- Use the design system (`apps/desktop/src/lib/ui`), not the prototype's CSS. No fixed text: i18n keys, TR and
  EN, informal "sen", plain words. Menus from `modules.list`; a switched-off module leaves no dead entry.
- Ownership: your brief lists the folders you own. Other folders: do not edit; if you need a change there,
  write it under "needs from sibling" in the report. Locale files: add keys only under your own feature's
  namespaces. `apps/desktop/src/lib/shell/**` belongs to E2 only.
- Acceptance = journeys on the real core (`pnpm -C apps/desktop e2e`, see `apps/desktop/e2e/README.md`): add
  or update journeys for your flows that read the SAVED result back through the API. Existing journeys must
  stay green (update selectors/text where your redesign changes them; never delete an assertion's meaning).
  Also update the mock screens (`pnpm -C apps/desktop screens`) and LOOK at your screens at 1280 and 900,
  dark and light, TR and EN; fix what looks broken.
- Run before the final commit: cargo fmt/clippy/test (workspace, exclude fc-desktop) if you touched Rust,
  gen-ts --check, `pnpm -C apps/desktop check`, `test`, `build`, `e2e`.
  Env: `PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium`, `FC_BIN=<repo>/target/debug/fc`,
  `NO_PROXY=127.0.0.1,localhost,.test`. The machine has 4 cores shared with three siblings: do not run
  cargo and e2e at the same time, use `-j 2`.
- Do not bump the version. CHANGELOG: one line under "Unreleased (v1.4)". Update docs/11 where a pattern is new.
- Report (Turkish, short): what was built, A/B defaults taken and where to flip each, what moved where,
  journeys added, checks actually run, needs from sibling, unfinished.
