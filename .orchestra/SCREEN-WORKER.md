# Common rules for v1.2 screen workers

You redesign a set of feature screens of the desktop app (`apps/desktop`, Svelte 5) in an isolated git worktree.
Several other workers do the same for other feature directories in parallel; an orchestrator merges the branches.

Read first: `.orchestra/DESIGN-BRIEF.md` (binding design direction and the four core flows), `CLAUDE.md`,
`apps/desktop/src/lib/ui/README.md`, `docs/11-tasarim.md` §5 (screenshot tool). The foundation is done:
`src/lib/theme/tokens.css` (dark-first, amber accent, `--font-mono`, `.num` class), restyled `$lib/ui/*`,
new `$lib/ui/ListRow.svelte`, `$lib/ui/StatTile.svelte`, Button `kbd` prop, Table `numeric` columns, the shell with
a 44px top bar (`src/lib/shell/TopBar.svelte`; screens set breadcrumbs with `getPageChrome()?.setTrail(crumbs, up)`,
a `PageHeader` with `back` does it automatically), and `openQuickAdd()` / `setQuickAddHandler()` in
`src/lib/shell/commands.svelte.ts`. Look at how the foundation looks before designing: run the screenshot tool once.

What "redesign" means: not a recolour. The owner found v1.1 uncomfortable and "made for the sake of making it".
For each screen ask what the user comes to do and make that fast: dense rows instead of big cards, the primary
action obvious and keyboard-reachable, secondary things collapsed or in menus, numbers in mono, no decorative
boxes, no empty hero areas. Layout changes are expected. `/home/user/quiz` (React, read-only) shows the density
and polish level the owner likes; do not copy it, beat it.

Hard rules
- **No feature, setting, action, shortcut or state is removed.** Everything stays reachable.
- Edit only your own directories and your own locale files (`src/lib/i18n/locales/{en,tr}/<yours>.json`). Do not
  edit `src/lib/ui`, `src/lib/theme`, `src/lib/shell`, `src/App.svelte`, `src/main.ts`, shared locale files or other
  workers' feature directories unless your brief says so. If you need a small change in a shared file or a new
  shared component, prefer building it inside your feature directory; if it truly must be shared, make the
  smallest possible edit and list it in your report. New generic components may be ADDED as new files in `src/lib/ui`
  (add `data-fc-ui`, do not edit the barrel `index.ts` unless needed; eager code must import `$lib/ui/X.svelte` directly).
- Only `$lib/ui` controls (no raw select/checkbox/radio). All text through i18n keys, TR and EN, consistent terms
  ("Öğreniliyor", never "Öğreniyor"). Numbers/dates only via `$lib/format`.
- Keep every `data-testid` hook used by `scripts/screens.mjs` and the unit tests.
- No business logic in TS; do not touch Rust unless your brief says so. If the API lacks something, report it.
- Never delete or skip a test. Update assertions only where the new design deliberately changes what they checked.

Verify before reporting
- `pnpm -C apps/desktop install` (worktree needs its own node_modules), `pnpm -C apps/desktop check`,
  `pnpm -C apps/desktop test`.
- Screenshots: `cd apps/desktop && PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium node scripts/screens.mjs --langs tr --themes dark,light --widths 1280,900 --out screens`
  (check the script for exact flag syntax). **Open your screens' PNGs with the Read tool and critique them
  yourself** in dark and light and at 900px: hierarchy, alignment, density, clipped text, contrast, empty states.
  Iterate until you would be proud to show them. `report.json` must show 0 findings for your screens.
- Commit on your worktree branch (do not push). End commit messages with:
  `Co-Authored-By: Claude Fable 5.2 <noreply@anthropic.com>` and
  `Claude-Session: https://claude.ai/code/session_015HsE62NZHWhisCVzcargRj`

Report (≤180 words, no code/diffs): branch + worktree path; per screen, what the layout is now and why it is
faster to use; shared files you touched; check/test/screens results; anything unfinished, API gaps, risks.
