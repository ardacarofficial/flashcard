# X3 — docs, version 1.3.0, CI
Documentation and release plumbing only; no feature code. Read `.orchestra/v13/STATE.md`, COMMON.md, CONTRACT.md,
both gpt-review files, `git log --oneline 7f9b95e..HEAD`, and the code that was actually built (do not document
plans). Docs are Turkish, in the existing style.
OWN: `docs/**`, `CHANGELOG.md`, `README.md`, `CLAUDE.md` (status line + a short "Doğrulama" note only),
version files, `.github/workflows/**`. Two siblings are changing code right now (X1: collection queue honours deck
presets; subtitle forward card shows the word only and the hint shows the sentence as context, reverse card blanks
the term; learn-ahead window becomes a setting. X2: UI polish, AI provider setup moves into settings) — describe
those in general terms and mark the lines to re-check with an HTML comment `<!-- v1.3: verify after X1/X2 -->`.

1. `CHANGELOG.md`: a v1.3.0 entry grouped as the owner would read it: Çalışma (launcher, scopes, keyboard, waiting,
   resume), Kelime kartları (roles, grading, hint, card back), Kart ekleme (one place; import), Ayarlar (kit, note
   types, MCP, automation), Yapay zekâ (presets, model list, test), İstatistik ve Planlama, Düzeltmeler (the bugs:
   advertised-vs-served count, Ctrl+Z while typing deleted a saved card, Esc ended a session, Space graded, wrong
   answer key for subtitle cards, Definition edit changed the wrong field, …), Geliştirici (dev bridge, journeys).
2. `docs/11-tasarim.md` → v1.3: add "Akışlar" as the organising idea (launcher, keyboard model table with the
   one-key-one-job rule, settings kit rules, add-cards page), keep the design system section accurate, and replace
   the verification section with the real one: mock screenshots are a visual check only; acceptance = journeys on
   the real core through the dev bridge (how to run, how to add one, the list of journeys and what each asserts).
3. `docs/02-ozellik-katalogu.md`, `03-mimari.md`, `06-ai-ve-entegrasyonlar.md`, `07-gelistirme-sureci.md`,
   `04-yol-haritasi.md`, `docs/README.md`, user guides under `docs/guide`: bring them in line with what exists
   (study.options/scopes, field roles + answer_field, hint kinds, provider presets, planning forecast, reminders
   counting the queue (02 §19), dev bridge). Fix anything stale you notice.
4. A short user-facing "v1.3'te ne değişti" page in the guide (TR + EN): how to start studying, the keys, how to
   re-study, where adding/importing lives, how to connect OpenRouter.
5. Version 1.3.0 everywhere the v1.2.0 bump touched (`git show --stat 96047a3`), lockfiles refreshed
   (`cargo check -p fc-api`; do not run the full workspace build).
6. CI: (a) `.github/workflows/windows-build.yml` stays (builds the NSIS installer artifact for this branch).
   (b) Add a NON-gating pilot job to it (or a separate workflow on the same branch trigger), `continue-on-error: true`:
   on `windows-2025`, install the built NSIS installer silently, then run three journeys against the INSTALLED app
   through WebDriver (`tauri-driver` + matching `msedgedriver`, see https://v2.tauri.app/develop/tests/webdriver/ci/):
   app launches and Home renders; Quick Add (N, type, Ctrl+Enter) creates a note visible in the library; Home →
   Enter → launcher → Enter → Space, Space, 2 → the session advanced by exactly one card. Log the runner image,
   WebView2 runtime and driver versions. Put the WebDriver test code under `apps/desktop/e2e-windows/`. You cannot
   run Windows here: write it carefully from the documentation, keep it isolated so a failure cannot break the
   installer artifact, and say plainly in your report that it is untested until CI runs it.
   (c) Check the Linux `journeys` job in `ci.yml` that a sibling added is coherent with the current scripts.
Report as COMMON.md says.
