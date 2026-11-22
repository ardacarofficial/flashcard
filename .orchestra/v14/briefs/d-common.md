# D-workers — clickable prototypes (shared rules)

You build ONE self-contained HTML file per brief under `prototypes/v14/` (inline CSS + JS, no network, no
external fonts/scripts/images; system font stack; works at 1280 and at 900 px wide; dark theme by default with
a light toggle). It is a prototype for the OWNER to click through and say "this / not this" BEFORE anything is
built in the app. Do not touch app code. All visible text in Turkish, informal "sen", plain words.

Look: use the app's design system so it feels like the same product (read `docs/11-tasarim.md` and
`apps/desktop/src/lib/ui` tokens/CSS: dark-first, amber accent, mono numbers, borders not shadows, dense but
calm). Look at how the app looks now: `cd apps/desktop && pnpm install && PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium node scripts/screens.mjs --langs tr --themes dark --widths 1280 --out screens` and open the relevant PNGs.

The owner's words (translated), which every prototype must answer:
"Study cards are OK but settings are still very confusing. Statistics are confusing, Search is odd, Create is
odd. We really have design problems. I thought Test was the one with 4 options; I click it and it asks
true/false questions. Why? Learn mode asks 4 options and shows the right answer when I pick. THAT should be
Test. A person gets very confused while using it." Earlier: "I want real user experience; teach me how to use
things in a friendly way; no jargon; think it through yourself, my complaints are only examples."

Principles (decided with the owner):
1. SUBTRACT before adding. A first-time user sees only what he needs daily. Everything else is behind ONE
   "Gelişmiş" switch (features stay, they just get out of the way). Show both states in the prototype.
2. Names mean what people expect. One word per concept (Sırası gelen, Kenara koy, Yarına bırak).
3. One key = one job (Space only flips; digits grade/pick).
4. Every screen answers "what do I do here?" in one line, and its empty state teaches.
5. No lorem ipsum: realistic English→Turkish vocabulary from films (e.g. "give up — vazgeçmek", with the
   subtitle sentence and source "Breaking Bad S1E3").

Deliver in the SAME file: a small fixed "Taslak notları" drawer listing (a) each decision you made and the
alternative you rejected, as questions the owner can answer with A/B, (b) what moved where compared with v1.3
(so nothing is lost). Where a choice is genuinely open, build BOTH variants with a visible A/B switch.
Verify by opening the file with Playwright (`require('playwright')`, Chromium at /opt/pw-browsers/chromium),
clicking through every path, checking no console errors, and looking at screenshots at 1280 and 900.
Commit the HTML (and nothing else outside `prototypes/v14/`). Report: file path, the paths through it, the
A/B questions, what you checked.
