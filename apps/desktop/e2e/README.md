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

Every journey **must pass**; none is marked `test.fail` today (S7c turned green with X1's hint
fix).

| Journey | What | Status |
|---|---|---|
| J1 | Quick Add: `N`, type, `Ctrl+Enter` ×2 → two notes in the chosen deck, exactly the typed fields | must pass |
| J2 | Create page: new deck, 3 rows, `Ctrl+Enter` → deck + 3 notes; one `Ctrl+Z` (`oplog.undo`) removes all three and the deck | must pass |
| J3 | Search, `↓ Enter`, edit the back, `Ctrl+Enter` → field changed in the DB, nothing else | must pass |
| J4 | Create › From a file: existing deck, the CSV picked with the **file chooser**, mapped preview says 3 → import → notes exist with the CSV's fields | must pass |
| J5 | 21 new / limit 20 / 20 studied: Home must not advertise what `study.start` can't serve | must pass (fixed in v1.3) |
| J6 | Subtitle card typed test: the meaning alone ("gitmek") is the right answer | must pass (fixed in v1.3) |
| J7a | Note editor (Search → `Enter`): `Ctrl+A` selects the field's text, not all rows | must pass (fixed in v1.3) |
| J7b | Quick Add: `Ctrl+Z` while typing edits the text, never undoes the card saved before | must pass (fixed in v1.3) |
| S1 | Home → Enter → launcher → Enter (whole collection): Space×3 grades nothing, 2 grades one, holding 2 grades one | must pass |
| S2 | Home → Enter → Enter, answer 3, Esc (pause), reopen the launcher from Home: Continue is first (focused) and resumes the same session | must pass |
| S3 | Deck page → Test → 10 questions → answer all → `Ctrl+Enter`: score shown, no card's schedule changed | must pass |
| S4 | Two cards Again → "struggled today" counts 2, practice (`trial`) rows, schedule untouched | must pass |
| E2 | Settings: Basic only, search finds an Advanced page ("In Advanced", one-off band), Subtitle share-known-words and backup keep saved, the Advanced switch remembered (`app.show_advanced`) and Library back in the sidebar | must pass |
| S5 | 21 new / limit 20: launcher and Home show 0 with the reason; empty start refused, open session kept | must pass |
| S6 | 1 m / 10 m steps: at learn-ahead 0 → "Sıradaki kart … sonra", session open, `next_due` = due time, the card comes back when the countdown ends; within the default window (`study.learn_ahead_mins` = 20) "Again" brings the card straight back | must pass |
| S7a | Subtitle card go/went, launcher → Test (written only): the paper opens in the answer field, "gitmek" is graded correct (paper read back with the key) | must pass |
| S7b | Subtitle card back (Cards, Space): meaning, example (+ translation), source as separate parts, labelled by `cards.part.*` | must pass |
| S7c | Hint rule (X1): Subtitle front = the word only (went + "go"); H = the line with the word highlighted, never the meaning, then letters; a reverse (Vocabulary production) card blanks the word: "I ___ home after the party." | must pass (fixed in v1.3) |
| S8 | Deck page term list: editing "Definition" of a Subtitle note changes `meaning`, never `surface` | must pass |
| S9 | Create › Paste `word⇥meaning⇥sentence` ×3 into Vocabulary → example field holds the sentence; one Ctrl+Z removes all three | must pass |
| S10 | Settings › Note types: a field's role and a card's "asks for" field survive save + reload (`notetypes.get`) | must pass |
| S11 | Settings › Daily pace → "Set per deck" → Deck options (v1.4: Advanced › Studying): 5 new a day on a 15-new deck → launcher and `study.options` offer 5; reset to default → 15 | must pass |
| P1 | v1.4 Settings › Daily pace: pick the deck, "5 · Easy", 150 reviews, Strict → `presets.for_deck` reads 5 / 150 / 0.95, `study.options` offers 5 new; the toast's Undo takes back only the last change. Runs twice: as is and with a slow bridge (reads 1.2 s late), which catches out-of-order saves | must pass (v1.4) |
| S12 | Quick Add: after a save, Ctrl+A / Ctrl+Z inside the field edit the text; the saved note, the deck's note count and the oplog are untouched | must pass |
| O1 | Library → a deck → Create: "Existing deck" with that deck preselected, cursor in the first term; Ctrl+Enter saves the note in THAT deck, no deck created | must pass (fixed in v1.3) |
| O1b | Create from the sidebar: the cursor is in the first term | must pass (fixed in v1.3) |
| O2 | Library row's visible "Study" → Enter → Space, 2 → Esc: focus on Continue, Enter resumes the SAME session; Esc → Tab → Done goes to the deck page, the paused session stays open | must pass (fixed in v1.3) |
| O3 | Deck page → Learn with a paused Test of that deck: the launcher lists the Test under Continue but Enter starts Learn; the Test stays open | must pass (fixed in v1.3) |
| O4 | Test paper by keyboard, 6 questions: A–D pick and move on, typed answer + Enter moves on, the last lands on Submit, Ctrl+Enter hands it in → no blanks in the graded paper | must pass (fixed in v1.3) |
| C1 | Match on Subtitle cards: tiles are the word (`term`) ↔ its meaning (answer field), never the line, translation or source; round played, one `review_log.mode = match` answer per card read back | must pass (fixed in v1.3) |
| C2 | Planning's today = the launcher's today: 25 new (20 a day), 5 reviews a day → "Bugün ~20 kart" = `study.options`; no later day plans more than 20 new or 5 reviews | must pass (fixed in v1.3) |
| C3 | Learn-ahead lowered to 0 while a learning card (due in 10 min) is on screen: Space + 2 is saved (review read back, no "no longer in the session"); the next card is chosen with the new value and waits | must pass (fixed in v1.3) |
| C4 | A note's two cards in two decks: each deck alone offers its card; the whole-collection queue (Home → launcher → Enter) counts and serves one of them; a deck whose preset does not bury siblings keeps its card | must pass (fixed in v1.3) |
| C5 | v1.4 Test (quiz): deck page → launcher → "Test" (its one line + "practice" footer) → each word asked with four meanings of the deck; `Space` does nothing, the right key or a wrong one picks, the verdict and the right meaning show at once (a right pick goes on by itself, a wrong one waits for Enter); the end score and missed list equal `modes.quiz.result` read back (every word answered, practice: one `trial_quiz` answer per card, schedules unchanged) | must pass (v1.4) |
| E1a | v1.4 Sınav: question types are check boxes from `study.options` (true/false off), none checked → Start off with the reason, only matching + 2 questions → `modes.test.paper` has exactly two matching questions; the result says the schedule did not change | must pass (v1.4) |
| E1b | v1.4 Test: `Esc` on the feedback pauses — the session stays open in the core (`ended_at` null, `modes.quiz.result` answered 1, not finished), `Enter` continues with the next word | must pass (v1.4) |
| K1 | A key right after a grade: four buttons, four cards in a row graded `1`/`2`/`3`/`4` with `Space` pressed at once (saving is held back 400 ms on purpose; half the cards turned with Space, half by clicking the card so it has the focus): while saving, the graded card stays on its back; the next card comes up on its front; each card has exactly one review in the session, with the pressed grade | must pass (fixed in v1.3) |
| E3a | v1.4 Search chips: "Nereden → Altyazıdan" writes `nereden:altyazı` in the box; the count and the rows equal `search.cards` read back; × removes the token | must pass (v1.4) |
| E3W | v1.4 My words: `/`, type a captured word, ↓ Enter, "Zaten biliyorum" → `words.lookup` reads status known with a manual mark (mark cleared after) | must pass (v1.4) |
| E3N | v1.4 Note editor extras: a Subtitle note's encounters show; an accepted answer added in the editor is read back with `accepted_answers.list` | must pass (v1.4) |
| F1a | v1.4 Leech repair: a card lapsed 8× through `study.start`/`study.answer` (cramming sessions, typed wrong answer kept) → İlerleme › Daha fazla › "Düzelt": lapses and the typed answer shown; "Uygula" on "accept the other meanings" → `stats.leech_repairs.list` has it, `accepted_answers.list` both senses, note fields unchanged; one Ctrl+Z removes both | must pass (v1.4) |
| F1b | v1.4 Encounters (note editor): two Subtitle notes from real captures; "Başka karta taşı" + note picker moves one encounter (`subtitle.encounters.list` on both notes); "Bir kartla birleştir" with a different word → "Yine de birleştir" → one note keeps all three encounters, the other is in `browse.trash` | must pass (v1.4) |
| F1c | v1.4 Audio fill (note editor › Ses): a capture's own WAV, audio field emptied → "Ses bul" previews the original, "Bunu ekle" puts the same media back (`notes.get`, `media.origins` = original); a note without sound and no TTS provider shows the reason and writes nothing (a real TTS fill is not covered) | must pass (v1.4) |

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
   - `captureSubtitle(deckId, specs)` — Subtitle captures through the real endpoint
     (`fc api serve`, `POST /subtitle/v1/captures`, optional WAV via `/subtitle/v1/media`);
     `noteByWord(deckId, word)`; `openNoteFromSearch(page, query, rowText)` — the note editor.
3. Drive the UI by keyboard where the UI allows; prefer roles + `t()` and `data-testid` hooks.
4. Assert on what was stored (`rpc`), and where it matters on what the screen tells the user.
5. Run it alone (`pnpm e2e -- --grep JN`), then the whole suite.

The bridge by hand: `cargo run -p fc-cli -- --db /tmp/x.db dev serve` prints
`{"url":…,"token":…}`; then `VITE_FC_BRIDGE_URL=… VITE_FC_BRIDGE_TOKEN=… pnpm -C apps/desktop dev:bridge`.
**Never point it at your real collection.**
