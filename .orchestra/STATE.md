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
| 0c | Frontend map for rollout split | read-only scout | running |
| 1 | Prototype screens: Home · Quick Add + bulk editor · Study session · Library + Deck page | worktrees | pending |
| 2 | Rollout: Search/Browse · Stats · Settings pages · modes (Learn/Test/Match/Reading) · Inbox/onboarding/lock/etc | worktrees | pending |
| 3 | Visual review + polish rounds | reviewers | pending |
| 4 | Integrate, full tests, docs/11 rewrite, version, push | orchestrator | pending |
