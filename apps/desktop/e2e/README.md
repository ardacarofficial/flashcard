# Journeys (v1.3)

End-to-end journeys of the desktop UI against the **real Rust core**: Playwright drives the
Svelte app in headless Chromium; the app talks to `fc dev serve` (the dev bridge, same dispatch
as the desktop IPC) on a temporary, freshly seeded collection. Assertions are on the
**persisted result** (note fields, session rows, review answers read back through the API),
not on pixels. See `docs/07-gelistirme-sureci.md` §3.1.

```bash
pnpm -C apps/desktop install
pnpm -C apps/desktop e2e                      # build fc, seed, run every journey
pnpm -C apps/desktop e2e -- --grep J3         # one journey (args go to `playwright test`)
FC_BIN=target/debug/fc pnpm -C apps/desktop e2e   # skip `cargo build`
FC_E2E_KEEP=1 pnpm -C apps/desktop e2e        # keep the temp profile (path is printed)
```

Chromium: `PLAYWRIGHT_CHROMIUM_EXECUTABLE=/path/to/chromium`, else Playwright's own
(`pnpm -C apps/desktop exec playwright install chromium`). Traces and screenshots of failed
journeys: `apps/desktop/e2e/.results/` (git-ignored; `pnpm exec playwright show-trace …`).

## What runs

`run.mjs` builds `fc` (debug: it has `dev serve`), creates a temp profile, starts the bridge
(random port + token), seeds it (`seed.mjs`), starts `vite --mode bridge` on a random port, runs
`playwright test -c e2e/playwright.config.ts`, then stops everything and deletes the profile.

Seed (`seed.mjs`, real commands only): deck **İngilizce** with 30 Classic notes
(front English, back Turkish), deck **Subtitle** with 10 notes made exactly as the Subtitle
browser extension makes them (`POST /subtitle/v1/captures` on `fc api serve`: word, surface,
meaning, highlighted sentence, translation, source), the first-run guide skipped. `seed()` in a
journey returns the ids and the data (`e2e/seed.json` in the profile).

| Journey | What | Status |
|---|---|---|
| J1 | Quick Add: `N`, type, `Ctrl+Enter` ×2 → two notes in the chosen deck | must pass |
| J2 | Create page: 3 rows, `Ctrl+Enter` → deck + 3 notes; one `Ctrl+Z` removes all | must pass |
| J3 | Search, `↓ Enter`, edit the back, `Ctrl+Enter` → field changed in the DB | must pass |
| J4 | CSV import through the UI (path prompt outside Tauri) → notes exist | must pass |
| J5 | 21 new / limit 20 / 20 studied: Home must not advertise what `study.start` can't serve | must pass (fixed in v1.3) |
| J6 | Subtitle card typed test: the meaning alone is the right answer | must pass (fixed in v1.3) |
| J7a/b | Ctrl+A / Ctrl+Z inside a text field edit the text (not "select all rows" / `oplog.undo`) | must pass (fixed in v1.3) |
| S1 | Home → Enter → launcher → Enter: Space×3 grades nothing, 2 grades one, holding 2 grades one | must pass |
| S2 | Answer 3, Esc (pause), reopen the launcher: Continue is first and resumes the same session | must pass |
| S3 | Deck page → Test → 10 questions → submit: score shown, no card's schedule changed | must pass |
| S4 | Two cards Again → "struggled today" counts 2, practice (`trial`) rows, schedule untouched | must pass |
| S5 | 21 new / limit 20: launcher and Home show 0 with the reason; empty start refused, open session kept | must pass |
| S6 | Waiting state: a learning card showable in minutes → "Sıradaki kart … sonra", session stays open | must pass |

A journey that shows a known bug is **never deleted or skipped**: the bug assertion comes after
`test.fail(true, '<bug>')`, called only once the journey's own preconditions have passed (so a
broken selector still fails loudly). When the bug is fixed Playwright reports "expected to fail
but passed": remove the `test.fail` line in the same commit as the fix.

## Adding a journey

1. A file `journeys/jN-<what>.spec.ts`; journeys run one at a time, in file order, on one shared
   profile — make your own deck/notes with `uniq()` names, never rely on another journey's data.
2. Helpers from `../fixtures`:
   - `rpc(method, params)` — any public method through the bridge as the user (throws
     `RpcError` with `.api.code`). Use it to set up state and to **assert the persisted result**.
   - `seed()` — the seeded decks/notes; `makeDeck(name, pairs)` — a deck with Classic notes;
     `notesIn(deckId)`, `deckIdByPath(path)`.
   - `openApp(page, { lang })` — open the app (TR by default) and wait for the shell.
   - `press(page, ...keys)` — keyboard first; `focused(page)` — the focused element.
   - `goToSection(page, 'shell.section.create')`, `choose(page, label, option, scope)` (the
     `$lib/ui` Select).
   - `t(key, params, lang)` — the UI text of an i18n key: select by what the user reads
     without hard-coding words (`page.getByRole('button', { name: t('io.choose_file') })`).
   - `env.profile` (write fixture files there), `env.fc`, `env.db`.
3. Drive the UI by keyboard where the UI allows; prefer roles + `t()` and `data-testid` hooks.
4. Assert on what was stored (`rpc`), and where it matters on what the screen tells the user.
5. Run it alone (`pnpm e2e -- --grep JN`), then the whole suite.

The bridge by hand: `cargo run -p fc-cli -- --db /tmp/x.db dev serve` prints
`{"url":…,"token":…}`; then `VITE_FC_BRIDGE_URL=… VITE_FC_BRIDGE_TOKEN=… pnpm -C apps/desktop dev:bridge`.
**Never point it at your real collection.**
