# Orchestra state — Flashcard v1.2

Goal: (1) redesign the whole desktop UI (see DESIGN-BRIEF.md), no feature removed; (2) connect the Subtitle
browser extension (`/home/user/subtitle`, contract `docs/flashcards-api.md`) via a Rust module under
`/subtitle/v1`. Branch: `claude/festive-einstein-8y0kld`. Orchestrator only plans/briefs/reviews.
User decisions: dark-first; captures go straight to a deck; prototype 4 screens first, Subtitle in parallel.
Models: workers/reviewers `opus`; effort high for design + Rust, medium for routine.

## Phases

| # | Piece | Where | Status |
|---|---|---|---|
| 0a | Design foundation: tokens, fonts, lib/ui, shell, top bar | main checkout | done (7f2cf32, 2ac48b2, 6c508de) |
| 0b | Subtitle Rust module + settings page | worktree | running |
| 0c | Frontend map for rollout split | read-only scout | done |
| 1+2 | Screen workers A–F (see workstreams below), all in worktrees, launched together | worktrees | running |
| 3 | Visual review + polish rounds | reviewers | pending |
| 4 | Integrate, full tests, docs/11 rewrite, version, push | orchestrator | pending |

## Frontend map (from scout, 0c done)

Root `apps/desktop/src/lib`. Features self-register via `features/<x>/screen.ts` (ScreenDef); placement in
`shell/navigation.ts` PLACEMENTS. i18n: one JSON per feature in `i18n/locales/{en,tr}/<file>.json` (merge script
fails on duplicate keys) — workers edit only their own locale files. Shared/collision files: `shell/navigation.ts`,
`src/App.svelte`, `src/main.ts`, `features/types.ts`, locales shell/navigation/ui/kit/commands/palette.json.

Workstreams (disjoint dirs):
- A Library+Home: features/decks, features/home, lib/components
- B Create: features/notes, ai, io, onboarding (+ new Quick Add dialog; existing `inbox/QuickAdd.svelte` is the
  external-window quick add calling `notes.create` / `quick_add_prefill`)
- C Study: features/study, modes, reading
- D Search+cards: features/browse (incl. ShortcutHost palette), cards, plugins
- E Stats+Inbox: features/stats, inbox, reminders
- F Settings: core/kit, themes, presets, profiles, mcp, automation, maintenance (+ subtitle page polish)
Cross-deps: cards/CardView+NoteEditor used by B,C (D owns); core/kit used by ~15 pages (F owns);
io/JobProgress used widely (B owns); decks/TermList uses cards flags.
Keep data-testid hooks: nav-item, subnav-item, settings-group*, deck-row, deck-mode-*, study-start, flip,
study-end, test-submit, option-*, match-tile(+data-side), term-list, .view-toggle, browse-row, browse-select,
settings-select, update-open, open-dialog-*, plus unit-test hooks (test-score, match-board, match-result,
match-clock, learn-progress, learn-feedback, summary-*, study-count, timer, hint, flag, focus-toggle,
mode-options, inline-preview, ai-text, question-N).

## Running agents (ids for SendMessage)
- 0b Subtitle Rust: a85b92b23cf6b6c91 (worktree .claude/worktrees/agent-a85b92b23cf6b6c91)
- A a5f16d3d6d66cfa11 · B a9efc71eb1ab30a1e · C addeb21297ec3e065 · D aea6420ce7596aa38 · E a3d2490ffcda62ac1 · F a78fbd01aaf515efd
Common brief: .orchestra/SCREEN-WORKER.md. Foundation hooks: openQuickAdd()/setQuickAddHandler() in
src/lib/shell/commands.svelte.ts; breadcrumbs via getPageChrome()?.setTrail().
Open after merge: Subtitle settings page polish; docs/11 rewrite; sidebar mark colour (F reports); screens --strict
all configs; independent visual review; version bump + CHANGELOG.

## Progress log
- Merged into branch: foundation, A (home/library/deck), F (settings/themes/icons), Subtitle module (844c225),
  B (quick add/create), C (study/modes/reading), D (browse/cards/palette/plugins). Merged tree: check clean, 356 tests.
- screens.mjs now picks a random port (parallel runs were sharing a server on 1431).
- Still running: E (stats/inbox) a3d2490ffcda62ac1; Subtitle reviewer a5e2bb6c1b5a1b1d2.
- Polish list for the final round: "Öğreniyor" → "Öğreniliyor" on Home/Deck page; disabled "Oluştur" button shows a
  garbled kbd hint on Create; `state-cards-flipped` screenshot missing after C (75 → 73 PNGs, check hooks);
  Subtitle settings page polish; ModeSettingsScreen/IO panels/shortcuts untouched by workers (kit-driven, verify);
  mock: Cards start-panel counts ≠ session counts; mock has no undo for notes; docs/11 rewrite; CHANGELOG + version.
- All six screen workers merged (939f2ae): check clean, 358 tests.
- Final round running (worktrees): P1 a9811c29dbab40ed0 (home/decks/notes/quickadd/ai/io/onboarding + shared UI),
  P2 aed0478fb462e1f91 (study/modes/reading/browse/cards/plugins + screens.mjs + App focus), P3 acb0714005492fd0e
  (stats/inbox/settings pages/shell), Docs ac60ffe91d58afc36 (docs/11, CHANGELOG, version 1.2.0),
  Subtitle reviewer a5e2bb6c1b5a1b1d2. Common brief: .orchestra/POLISH-REVIEWER.md.
- After they merge: Subtitle settings page visual polish (small), full `screens --strict` matrix, cargo fmt/clippy/test
  (exclude fc-desktop; fc-ai needs NO_PROXY=127.0.0.1,localhost), final report to user.
