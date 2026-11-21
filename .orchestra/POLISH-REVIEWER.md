# Common rules for v1.2 review + polish agents

Context: the desktop UI (`apps/desktop`, Svelte 5) was just redesigned by six parallel workers, each owning some
feature directories, on top of a shared foundation. Read `.orchestra/DESIGN-BRIEF.md` (the binding direction and
the four core flows) and `.orchestra/SCREEN-WORKER.md` (the rules the workers had; the same hard rules bind you).
You did not build these screens. You are the independent reviewer for a set of them: judge them as the app's
owner would — someone who rejected the previous design as "uncomfortable, made for the sake of making it" —
and then fix what you find, inside your directories.

How to review
1. `pnpm -C apps/desktop install`, then take the full screenshot set for your review:
   `cd apps/desktop && PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium node scripts/screens.mjs --langs tr,en --themes dark,light --widths 1280,900 --out screens`
   (random port per run; safe to run in parallel with other agents; the machine is shared, be patient).
2. Open with the Read tool EVERY PNG that belongs to your screens in `tr-dark-1280`, and the same screens in
   `tr-light-1280`, `tr-dark-900` and `en-dark-1280` (at least). Write down concrete defects as you go.
   Look for: unclear primary action; wasted space or, conversely, cramped/colliding elements; misalignment
   between neighbouring blocks; inconsistent spacing, row heights, heading sizes or button styles compared with
   the other screens; clipped or wrapped labels (EN strings are often longer); low contrast, especially in light
   theme (amber on white); numbers not in mono; leftover v1.1 look (big rounded boxes, shadows, purple);
   boxes inside boxes; raw i18n keys; broken empty/loading/error states; dialogs too wide/tall; anything that
   would make a keyboard user reach for the mouse (missing kbd hints on primary actions, no visible focus).
3. Also drive the real interactions for the core flows of your screens with a short Playwright script against
   `pnpm -C apps/desktop dev:mock` (use `--port <random>`; `require('playwright')` works globally, chromium needs
   no install): keyboard-only walkthroughs, checking focus order, shortcuts, Esc behaviour, that nothing throws
   in the console. Screenshots alone don't show interaction bugs.
4. Check that no feature was lost: compare with the pre-redesign version of each screen
   (`git show a80e1cc:<path>`) and list every action, setting, shortcut and state that existed; each must still
   be reachable. Restore anything missing.
5. Fix what you found (your directories + your locale files only; smallest possible edits elsewhere, listed in
   your report). Re-shoot and re-look until clean. `pnpm -C apps/desktop check` and `pnpm -C apps/desktop test`
   must pass; `report.json` 0 findings for the full matrix. `App.test.ts` can time out under machine load; rerun
   it alone before calling it a failure.

Cross-screen consistency rules (apply them; they were found inconsistent):
- Terminology: card states are "Yeni / Öğreniliyor / Vadeli / Ustalaşıldı" in TR ("Öğreniyor" is wrong) and
  "New / Learning / Due / Mastered" in EN, everywhere.
- A disabled Button with a `kbd` hint must still render the hint legibly (it currently looks garbled on the
  Create page's disabled "Oluştur" button) — if the cause is in `$lib/ui/Button.svelte`, the agent whose brief
  names "shared UI" fixes it; others report it.
- Page structure: page title row (title left, primary action right), optional one-line description, then
  content; section headings 14px/600; tables/lists 34px rows; same panel border/radius everywhere.

Commit on your worktree branch (do not push). End commit messages with:
`Co-Authored-By: Claude Fable 5.2 <noreply@anthropic.com>` and
`Claude-Session: https://claude.ai/code/session_015HsE62NZHWhisCVzcargRj` (use exactly this attribution).

Report (≤220 words, no code): branch; the defects you found per screen (one line each) and whether fixed;
features you found missing and restored; shared files touched; check/test/screens results; what you could not
fix or verify. Be candid: "looks fine" without having opened the PNGs is not a review.
