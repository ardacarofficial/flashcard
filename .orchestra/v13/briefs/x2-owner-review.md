# X2 — the owner's-eyes review and polish (frontend)
You are an independent reviewer who did not build any of this. All eight v1.3 pieces are merged and run against
the real Rust core. Read `.orchestra/v13/STATE.md`, `gpt-review-1.md` (the owner's 14 complaints are quoted in its
brief context below) and `apps/desktop/e2e/README.md`. Your job: sit in the owner's chair, do his jobs in the REAL
app through the dev bridge (`pnpm -C apps/desktop e2e` shows how the bridge + vite are started; start them
yourself on random ports with a seeded temp profile and drive the UI with Playwright, looking at screenshots you
take as you go, in Turkish, dark theme, 1280 and 900 wide), find what is still awkward, and fix it.
OWN: all of `apps/desktop/src/**` and `apps/desktop/scripts/**`, locale files, new journey files under
`apps/desktop/e2e/` (do not edit existing journey files: a sibling, X1, is changing Rust study/queue/hint logic and
journeys S1, S2, S6, S7 right now — do not touch Rust, built-in card templates, or those journeys; if you find a
logic bug, write it in your report with a reproduction).

The owner's complaints (v1.2): settings confusing, note types "extremely confusing"; "only Claude, no OpenRouter";
MCP/automation not usable; stats/planning could be presented better; "I press Study and can't start; sometimes it
asks, sometimes not; I can't choose the mode"; Space did two jobs; hints made no sense; example sentence looked
like part of the answer; jargon (bury/suspend/new); import hidden in settings; "let me re-study what I reviewed
today"; "settings should teach, in a friendly way"; "the design is nice, make it nicer — these are only examples,
think it through yourself".

Walk these jobs end to end, keyboard first, and for each write down (for your report) the exact steps/keys it
took and anything that made you hesitate:
 1. First run on an empty profile: get from nothing to studying your first 5 words.
 2. Add one word with a sentence (N). Add 10 words by paste. Import a CSV.
 3. "I want to study English": from Home, from the library, from the deck page — Cards, then a 10-question Test,
    then Learn, then Match. Leave in the middle and come back.
 4. Study again what you got wrong today; practise the words Statistics says you forget.
 5. Find a card with a typo and fix it (search, deck term list).
 6. Read your progress: what does Statistics tell you to do? Change your pace in Planning and understand the effect.
 7. Connect an AI provider (OpenRouter preset; a fake key is fine — the error must be legible) and find where
    AI is used. Note: the "Yapay zeka" tab under Oluştur shows "Önce bir deste seç" before anything else, and mixes
    generating cards with provider setup and usage — decide where each belongs (provider setup and usage are
    settings; generating is an add-cards path) and make it so, keeping every feature reachable.
 8. Make a note type from a template and understand what each field is for; set which field is the answer.
 9. Connect an assistant via MCP; create an automation from a recipe.
10. Use only the keyboard for five minutes: is every primary action hinted, is focus always visible, does `?` help?

Then fix what you found. Standards: one predictable way to do each thing; primary action obvious and visible
without hover (e.g. deck rows' "Çalış"); every screen opens with what it is for; empty states say what to do
next; wording plain, informal Turkish ("sen"), consistent terms across screens (pick one word per concept and use
it everywhere: e.g. "Sırası gelen" vs "Vadeli" must not both exist); EN equally careful; numbers in the launcher
and Home coherent in the mock data too (the mock currently shows "Bugünün kartları 141" but "Tüm kartlar 13");
typography levels on the card back (meaning > example > source); nothing clipped at 900px; light theme legible.
A bounded visual refinement is welcome where it serves clarity (spacing rhythm, alignment between neighbouring
blocks, consistent page headers) — not a re-theme.
Verify: `pnpm -C apps/desktop check`, `test`, `build`, `bundle:check`, `e2e` (existing journeys must still pass;
add journeys for the flows you fixed as new files), `node scripts/screens.mjs --strict` full matrix. Report as
COMMON.md says, plus the ten job walkthroughs (steps/keys, before → after) in a file
`.orchestra/v13/owner-walkthrough.md`.
