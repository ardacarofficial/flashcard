# v1.2 Design brief (source of truth for every UI agent)

## Why this exists

The owner rejected the v1.1 UI outright: "not usable, made for the sake of making it, adding a card feels
awful, I can't get comfortable in the app". v1.1 was airy Quizlet-style: big rounded cards, lots of whitespace,
light grey + purple, a heavy form page for adding cards. The owner's other app ("Kelime", `/home/user/quiz`,
read-only reference, React) feels better to them: dark, dense, keyboard-first, one accent, 1px borders. They
do NOT want a copy of Kelime; they want something better than it. **No feature may be removed.** Behaviour
and the Rust API do not change (CLAUDE.md rule 1); this is a frontend redesign.

The test for every screen: *what does the user come here to do, and how few keystrokes/clicks does it take?*
Not "what could be put on this page".

## Feel

Dense but calm, fast, keyboard-first, a tool you live in (Linear / Raycast / a good code editor), with the
warmth of paper index cards. Dark-first; the light theme is a first-class variant of the same tokens.
The existing theme features stay: light/dark/system, ready-made themes, custom accent colour.

## Tokens (`apps/desktop/src/lib/theme/tokens.css`)

- Dark surfaces: `--surface-0 #0E0E10` (app bg), `--surface-1 #151518` (panels, sidebar), `--surface-2 #1C1C20`
  (raised: inputs, hover), `--surface-3 #25252B` (active/selected). Borders `--border #2A2A31`,
  `--border-strong #3A3A43`. Text `--text-1 #EDEDEF`, `--text-2 #A3A3AE`, `--text-3 #74747F`.
- Light surfaces: `--surface-0 #FAFAF8`, `--surface-1 #FFFFFF`, `--surface-2 #F3F3F0`, `--surface-3 #E9E9E4`,
  borders `#E2E2DC` / `#CFCFC7`, text `#1A1A1C` / `#5C5C63` / `#85858C`. (Slightly warm neutrals, not blue-grey.)
- **One accent: amber** `--accent #F5B544` with `--accent-fg #17130A` (dark text on amber fills).
  `--accent-text` is the accent used as *text/icon on a surface*: `#F5B544` in dark, `#9A5B00` in light.
  `--accent-soft` = accent at 14% alpha. Not Kelime's lime, not Quizlet blue, not v1.1 purple.
- Semantic, only where they mean something: ok `#3DD68C` (light `#147A4B`), bad `#FF6369` (light `#C8323B`),
  warn `#FF9D4D` orange (light `#B4520A`) so it never reads as the accent. All text pairs WCAG AA in both themes.
- Type: Inter (already bundled) for UI; **JetBrains Mono (OFL) bundled locally** for numbers, counts,
  durations, kbd hints, table numerics (`--font-mono`, tabular). Scale 11/12/13/14/16/20/24; **body 13px**,
  emphasis 14, page title 20/600, the study card text 28–40. Line-height 1.45.
- Density: control height 28 (sm) / 32 (md) / 36 (lg); table/list row 34; spacing 4-px grid, typical gaps 8/12/16,
  page padding 24. Radius: 6 controls, 8 panels/dialogs, 12 only for the study flashcard, 999 chips.
- Separation by 1px borders and surface steps. **No shadows** except floating layers (dialog, menu, popover,
  toast): one soft shadow token. No gradients, no glow, no glass, no emoji in chrome.
- Motion: 100–150 ms ease-out, `prefers-reduced-motion` respected.
- Focus: 2px accent ring with 2px offset, always visible for keyboard.

## Shell

- Sidebar 216px (icon-only under 1024px): app mark + name; primary nav (Bugün/Ana sayfa, Kütüphane, Ara,
  İstatistik) as compact 30px rows with icon; bottom: Gelen kutusu (count in mono), Ayarlar, profile.
  Active item = `--surface-3` + accent-coloured icon. Navigation still comes from `modules.list`.
- **Top bar 44px, always present** (except in a running study session): page title / breadcrumb on the left;
  on the right a search-or-command field ("Ara ya da komut çalıştır", `Ctrl K` kbd hint) that opens the
  existing command palette, and a primary **"Kart ekle" button with `N` hint** that opens Quick Add.
  "Oluştur" stops being a loud sidebar block; the bulk editor stays reachable (sidebar item + palette + from
  Quick Add "Toplu ekle").
- Content: one container, left-aligned next to the sidebar, max 1120 (wide variant for tables/charts).

## The four core flows (these define success)

1. **Add a card (the owner's biggest pain).** `N` anywhere (not while typing in a field) or the top-bar button
   opens the **Quick Add dialog**: deck picker (remembers the last deck), note type (remembers), the note type's
   fields (front/back…) as plain inputs with Tab order, tags, optional collapsed "Diğer" (classification etc.).
   `Ctrl+Enter` saves, **keeps the dialog open**, clears the fields, focuses the first field and shows a small
   "Eklendi · 3" counter with Undo; `Esc` closes. Zero mouse needed. Uses the existing Rust commands.
   The **bulk editor** (Oluştur page) becomes a spreadsheet-like grid: a single compact header row
   (deck, note type, "Diğer" popover for classification) and then numbered term | definition rows at row
   height ~36, Tab/Enter navigation as today, paste-to-import and AI tabs kept. No big form before the rows.
2. **Study today's cards.** Home answers "what now": first line "Bugün · 141 kart" with mono breakdown
   (Vadeli 88 · Öğreniliyor 11 · Yeni 42) and primary "Çalışmaya başla" (`Enter`). Below: decks with due cards
   as a dense list (name, due, new, mastery bar, per-row start). Streak/goal as a compact stat strip, inbox
   notice as a single row. In a **running session** the shell disappears entirely: slim top line (progress bar,
   "12 / 40" mono, close), the card centred (max ~720 wide), answer buttons exactly as wide as the card, kbd
   hints as small mono text under them. 2-button and 4-button modes both kept.
3. **Find and fix a card.** `Ctrl K` / `/` search; Library is a dense tree-table (name, cards, due, mastery,
   chips; hover/keyboard row actions); the old card-grid view stays as the secondary toggle. Deck page: title
   row with primary "Çalış" and the study modes as a compact row of mode buttons **above the fold**, a smaller
   preview card, then the term list as a real two-column table with inline edit.
4. **See progress.** Stats: compact stat tiles with big mono numbers, small-square heatmap (GitHub-like cells
   ~12px), readable charts using accent + semantic colours.

## Hard rules

- No feature, screen, setting, shortcut or module switch is removed. If something moves, it stays reachable
  and the command palette still finds it.
- All screens use only `$lib/ui` components (no raw `<select>`/checkbox/radio); components keep `data-fc-ui`.
- No hard-coded text: every string is an i18n key in TR and EN. Terminology is consistent
  ("Öğreniliyor" everywhere, never "Öğreniyor").
- Numbers/dates only through `$lib/format`.
- Keep `data-testid` hooks that `apps/desktop/scripts/screens.mjs` and the tests rely on.
- Keep the existing lint-style tests green (`tests/*.test.ts`: raw-controls, contrast, eager-imports,
  bundle-budget ≤ 320 kB initial JS, no-hardcoded-text, number-format). If a test asserts a v1.1 visual detail
  that the new design deliberately changes, update the test, don't bend the design. Never delete or skip a test.
- No business logic in the frontend. If a screen needs data the API doesn't give, say so in your report
  instead of computing it in TS.
- Verification is visual: run `pnpm -C apps/desktop screens` (see docs/11-tasarim.md §5; set
  `PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium`; pass `--out` as a path *relative to
  apps/desktop*, e.g. `--out screens`), then **open the PNGs and look at them** in dark and light.
