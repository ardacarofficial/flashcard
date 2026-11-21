# Orchestra state — Flashcard v1.2

Goal: (1) redesign the whole desktop UI (see DESIGN-BRIEF.md), no feature removed; (2) connect the Subtitle
browser extension (`/home/user/subtitle`, contract `docs/flashcards-api.md`) via a Rust module under
`/subtitle/v1`. Branch: `claude/festive-einstein-8y0kld`. Orchestrator only plans/briefs/reviews.
User decisions: dark-first; captures go straight to a deck; prototype 4 screens first, Subtitle in parallel.
Models: workers/reviewers `opus`; effort high for design + Rust, medium for routine.

## Phases

| # | Piece | Where | Status |
|---|---|---|---|
| 0a | Design foundation: tokens, fonts, lib/ui, shell, top bar | main checkout | running |
| 0b | Subtitle Rust module + settings page | worktree | running |
| 0c | Frontend map for rollout split | read-only scout | done |
| 1 | Prototype screens: Home · Quick Add + bulk editor · Study session · Library + Deck page | worktrees | pending |
| 2 | Rollout: Search/Browse · Stats · Settings pages · modes (Learn/Test/Match/Reading) · Inbox/onboarding/lock/etc | worktrees | pending |
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
