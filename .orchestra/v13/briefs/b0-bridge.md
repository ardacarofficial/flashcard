# B0 — dev bridge + journey harness
Goal: make it impossible to ship "works in the mock, broken in the real app" again. Build the dev bridge and
journey harness of CONTRACT.md §3, then write the first journeys against the REAL Rust core.

OWN: `crates/fc-cli` (new `dev serve` subcommand + its module), a new small crate if cleaner (e.g. `crates/fc-devbridge`),
`apps/desktop/src/lib/api/` transport selection + a new bridge transport file, `apps/desktop/e2e/**`,
`apps/desktop/package.json` (script `e2e`), `apps/desktop/vite.config.ts` (mode `bridge` only), `.github/workflows/ci.yml`
(add a Linux job running the journeys; keep existing jobs), `docs/07-gelistirme-sureci.md` (a short section).
LEAVE-ALONE: every feature screen (`apps/desktop/src/lib/features/**`), `crates/fc-app/src/modules/**` logic,
`apps/desktop/src-tauri/**` (read `src-tauri/src/ipc.rs` to mirror its dispatch; if the dispatch must be factored
into a shared function to be reused, do the smallest extraction into `fc-app` and report it as unverified for the
shell build).

Requirements
1. The bridge reuses the same dispatch path as the desktop shell (method lookup, actor, envelope, error
   serialization, lock/profile gating, post-commit events) — not the restricted extension-facing `fc-http` API.
   Loopback only, random token required, refuses non-loopback Host, compiled out of release builds, clearly
   documented as a development tool. Events reach the browser so screens refresh as they do in the app.
2. Frontend transport: selected at build/dev time; the mock and Tauri transports keep working unchanged; initial-JS
   budget test still passes (the bridge code must not be in the production bundle).
3. `pnpm -C apps/desktop e2e`: builds/locates the `fc` binary, creates a temp profile (seeded through real
   commands: an "İngilizce" deck with ~30 classic notes, a "Subtitle" deck with ~10 subtitle-type notes that have
   word, surface, meaning, sentence, translation — see `crates/fc-core/src/subtitle.rs` and the Subtitle module for
   how captures become notes), starts bridge + vite (random ports), runs Playwright journeys headless, tears down.
4. Journeys to write now (keyboard-only where the UI allows; assert persisted state through RPC, e.g. review-log
   rows, note fields, session rows):
   J1 Quick Add: press N, type term/definition, Ctrl+Enter twice → two notes exist in the chosen deck.
   J2 Bulk create: Create page, 3 rows, Ctrl+Enter → deck with 3 notes; one undo removes all three.
   J3 Search and fix: find a card, edit the note, save → field changed in the DB.
   J4 CSV import through the UI → notes exist.
   J5 (expected to FAIL today — mark it `test.fail` with a comment naming the bug, do not hide it) "advertised vs
      served": create 21 new cards with the default 20/day new limit, study 20, then Home must not advertise cards
      that `study.start` cannot serve. See gpt-review-1.md §A.
   J6 (expected to FAIL today, same treatment) subtitle card typed test: the expected answer for a subtitle note
      is the meaning only.
   Sibling workers are rewriting the study launcher and keyboard model right now, so do NOT write journeys that
   click through the study start UI yet; expose helpers (`rpc()`, `seed()`, `openApp()`, `press()`) so a later
   worker can add them quickly. Document how to add a journey in `apps/desktop/e2e/README.md`.
5. Prove it: run the journeys and paste the pass/fail summary (J1–J4 pass, J5–J6 fail as expected) in your report.
