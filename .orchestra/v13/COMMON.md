# v1.3 "usable" — common rules for every worker

You are one of several agents working in parallel, each in its own git worktree and branch of
`/home/user/flashcard` (Rust core + Tauri 2 shell + Svelte 5 UI flashcard app). An orchestrator merges the
branches. Read first: `CLAUDE.md` (binding project rules), this file, `.orchestra/v13/CONTRACT.md`, your brief,
and the two review documents `.orchestra/v13/gpt-review-1.md` / `gpt-review-2.md` (an independent review with file
references; treat its findings as leads to verify, not as truth).

## Why this release exists
The owner (the only user; learns English vocabulary, many cards come from a browser extension that captures a
word + the subtitle sentence) tried the v1.2 Windows build: "Looks better, infrastructure seems solid, but I
can't actually use it. I want real user experience." v1.2 was verified with screenshots against a MOCK backend;
that is how unusable flows shipped. v1.3 adds no new features for their own sake: it makes what exists usable,
predictable and self-explanatory. The test for everything: can he sit down and do the job with the keyboard,
without guessing, and does the PERSISTED RESULT match what he did?

## Hard rules
- Project rules in CLAUDE.md hold: business logic in Rust; `fc-core` pure; every write is a validated,
  transactional, oplog-recorded, undoable command; API types in `fc-api` (TS types are generated — run the
  project's generator, never hand-write DTOs); schema/data change = numbered migration + migration test (use the
  v1.3 range 1300–1399; pick the sub-range your brief names); modules can be switched off; no hard-coded UI text
  (TR + EN locale files; the app addresses the user informally — "sen", never "siz"); numbers/dates via
  `$lib/format`; only `$lib/ui` controls; no telemetry.
- No feature, setting or shortcut is removed unless your brief says so. Things may move; they stay reachable.
- Stay inside the files/directories your brief says you OWN. Files under LEAVE-ALONE belong to a sibling worker:
  if something there is broken or you need a change, write it in your report instead of editing. Shared
  registration files (`apps/desktop/src/lib/shell/navigation.ts`, `src/App.svelte`, `src/main.ts`,
  `crates/fc-api/src/method.rs`, error-code lists, generated TS types, shared locale files): smallest possible
  additive edit, listed in your report.
- One key = one job. Never give a shortcut a second meaning that depends on hidden state.
- Never delete or skip a test. Update an assertion only where the behaviour deliberately changed, and say so.
- Do not push. Commit on your worktree branch. Commit messages end with exactly:
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
  `Claude-Session: https://claude.ai/code/session_015HsE62NZHWhisCVzcargRj`
- Do not create GitHub issues/PRs, do not touch `/home/user/subtitle` or `/home/user/quiz` (read-only references).

## Environment
- The machine is shared (4 cores, other agents are building). Be patient; a UI test that times out once under
  load and passes when rerun alone is load, not a defect.
- Rust: export `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` before cargo commands
  (disk is limited; debug info fills it). Use `cargo test -p <crate>` for the crates you touch while iterating;
  before reporting run `cargo fmt --all --check`, `cargo clippy --workspace --exclude fc-desktop --all-targets -- -D warnings`
  and `cargo test --workspace --exclude fc-desktop` with
  `NO_PROXY="127.0.0.1,localhost,.test" no_proxy="127.0.0.1,localhost,.test"` (two fc-ai tests need it here).
  `fc-desktop` (the Tauri shell) cannot compile in this container (no gtk/webkit): if you edit
  `apps/desktop/src-tauri`, re-read your diff carefully and say it is unverified.
- Frontend: `pnpm -C apps/desktop install` (each worktree needs its own node_modules), `pnpm -C apps/desktop check`,
  `pnpm -C apps/desktop test`. To look at the UI: `cd apps/desktop && PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium node scripts/screens.mjs --langs tr --themes dark --widths 1280 --out screens`
  (random port per run; `screens/` is git-ignored) and open the PNGs; or run
  `node node_modules/vite/bin/vite.js --mode mock --host 127.0.0.1 --port <random 20000-40000>` after
  `node scripts/merge-locales.mjs` and drive it with Playwright (`require('playwright')` works globally; Chromium
  at /opt/pw-browsers/chromium). Stop servers you start. Put scratch files under /tmp, not in the repo.
- Design system: `docs/11-tasarim.md` (v1.2: dark-first, dense, amber accent, mono numbers, borders not shadows).
  Keep it; this release fixes flows and wording, not the theme.

## Report (your final message; ≤ 250 words, no code or diffs)
Branch name and worktree path · what changed, per area · every file you edited outside your OWN list ·
exact results of the checks you ran (and which you could not run) · what you verified by actually running
it vs only by reading · anything unfinished, deviations from CONTRACT.md, risks, and things siblings must know.
