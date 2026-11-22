# F1 — study flow frontend: one launcher, one keyboard model
Goal: "I want to study English" works the same way from everywhere, lets him choose the mode and what to study,
and every key does one thing. Consume CONTRACT.md §1 (R1 is implementing the backend in parallel: code against
the contract, extend the mock transport to implement `study.options` and the new `study.start` rules faithfully —
including an empty-scope case and a resumable-session case — so the UI can be exercised now).

The owner's words: "When a person presses Study he can't even choose which mode he wants. Sometimes it asks,
sometimes it doesn't. Show answer is Space; I press Space again to close it and it moves to the next question —
as if the keyboard had run out of keys. Let me easily study again what I reviewed today."

OWN: `apps/desktop/src/lib/features/study/**`, `features/modes/**`, `features/reading/**` (only if a start entry
lives there), `features/home/**`, `features/decks/**` EXCEPT `TermList.svelte` (R2), `features/browse/ShortcutHost.svelte`,
`palette.ts`, `ShortcutsScreen.svelte` and the shortcut definitions (`features/study/keys.ts` etc.), locale files
study/modes/home/decks/palette/commands, the focus-layout part of `src/App.svelte`.
LEAVE-ALONE: Rust crates (R1/R2), `TermList.svelte`, `features/cards/**` templates (R2 styles parts minimally; you
may add presentation CSS for `.fc-part-*` in the study card container only), quick add / notes / io (S3),
settings pages (S2), stats (S4), ai (S1).

Deliver
1. **Study launcher** — one keyboard-first dialog (`$lib/ui` Dialog), opened by EVERY generic entry: Home primary
   button and deck rows, Library row action, deck page primary "Çalış", palette "Çalış", top-level Study nav if any.
   Mode buttons (deck page Kartlar/Öğren/Test/Eşleştir, palette entries) open the SAME dialog with that mode
   preselected — never a silent start (gpt-review-2.md §4). Contents, top to bottom:
   - "Devam et · <deck> · <mode> · 7 cevaplandı" row(s) from `resumable`, first and focused when present.
   - Mode: segmented control Kartlar / Öğren / Test / Eşleştir (only `modes` returned), with one line saying what
     the selected mode does and whether it changes the schedule.
   - What to study: the five scopes as radio rows with their exact counts in mono; an empty scope is disabled and
     shows its reason in plain words ("Günlük yeni kart sınırın doldu", "Sıradaki kart 14 dk sonra", "Bugün henüz
     çalışmadın"); practice scopes carry a small "pratik · takvimi değiştirmez" tag.
   - Count (optional stepper, default all) and, where the old setup panels had options (order, typed answers,
     test question types, match size…), a collapsed "Seçenekler" section containing them — no option is lost.
   - Primary "Başla" with Enter; shows deck + count on the button ("İngilizce · 31 kart"). Esc closes.
   Remembers last mode per deck and last scope per mode (per-viewer preference; use the app's settings/ux-state
   API if one exists, see `1000_ux_state` — not business logic). Revalidates on Start; on
   `CONFLICT_NOTHING_TO_STUDY` stays open and shows the reason. The old separate setup panels of Cards/Learn/Test/
   Match are replaced by this dialog; their end/summary screens offer "Tekrar çalış" which reopens the launcher
   with sensible preselection (e.g. "Bugün zorlandıklarım").
2. **No dead clicks**: Home's Start is never enabled for something that cannot start; with the study module off the
   entry is hidden or explains itself; router misses show a message instead of silently staying; the raw palette
   command no longer runs a start and discards the session (gpt-review-1.md §A table).
3. **Keyboard model for Cards** (gpt-review-2.md §3): Space = flip, repeatedly (front⇄back), never grades.
   `1`/`2` (2-button) or `1`–`4` (4-button) grade, only when the back has been seen at least once. `H` hint,
   `Z` undo last grade, `F` focus, `Esc` leave (with the existing confirm rules). Enter activates the focused
   control only. Holding a key = one action (ignore `event.repeat`). Arrows are not grading keys. Update the
   on-screen legend, `aria-keyshortcuts`, the Shortcuts screen and the shortcut editor defaults so they tell the
   truth; migrate users who customised keys without breaking them. Audit Learn/Test/Match/Reading and the rest of
   the app's shortcut table for any other key with two meanings and fix or report each.
4. **Waiting state**: when the session has only waiting cards, show "Sıradaki kart N dk sonra" with a live
   countdown and the choices "Bekle" / "Başka bir şey çalış" (launcher) / "Bitir" — not a closed session.
5. **Plain words**: rename the learner-facing actions and states and explain each in one line where it is offered
   (tooltip/description): bury → "Yarına bırak" (bugün bir daha sorma), suspend → "Durdur" (sen açana kadar
   sorma), new → "Yeni" stays but is explained ("henüz hiç çalışmadığın"), leech/lapse terms likewise. Same care in EN.
6. **Card back**: style `.fc-part-meaning` / `.fc-part-example` / `.fc-part-source` inside the study card so the
   meaning is the answer, the example is a clearly separate labelled block (sentence, then translation, muted),
   source is a small caption. Context hint (`HintDto.kind == "context"`) is shown as the sentence with a visible
   blank, letter hints as today.
7. Tests: unit tests for the key handler (space toggles and never grades; repeat ignored; digits before reveal do
   nothing), launcher tests (preselection, disabled empty scope with reason, resume first, Enter starts), updated
   existing tests. Keep every `data-testid` the screenshot tool uses or update `scripts/screens.mjs` so the
   launcher, a running session and the waiting state are captured (you may edit screens.mjs for this).
Look at the result in a browser (mock) and iterate until the flow feels obvious; describe in your report the exact
key sequence for "study 10 Test questions from İngilizce" and "re-study what I struggled with today".
