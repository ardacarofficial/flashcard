# G2 — Subtitle eklentisini gerçek bir tarayıcıda Flashcard'a karşı dene

Read `.orchestra/v14/COMMON.md` first. The owner's real extension is the separate repo at `/home/user/subtitle`
(READ-ONLY: never edit or commit there; copy it to your scratch dir to build). Its CLAUDE.md explains the build
(`pnpm build`), the playground and the learning-service integration. Until now only a contract harness with the
extension's client code has been run (`tools/subtitle-contract`); the built extension has never been loaded in
a browser against the real app.

Do:
1. Build the extension from a copy. Start the real Flashcard core with its local API the way the desktop app
   does (`fc` with a temp profile; pair the extension exactly as a user would: key/pairing flow in docs/06).
2. Load the BUILT extension into real Chromium (Playwright `launchPersistentContext` with
   `--load-extension`/`--disable-extensions-except`, new headless or xvfb; executable /opt/pw-browsers/chromium)
   on a page where it runs. Prefer a page you can serve locally that the extension treats as a supported site
   (see its `src/utils/sites.ts`, the playground's service and fixtures); if no local page can be matched
   without editing the extension, say so and use the closest honest setup (e.g. its playground with the real
   background in `?background=live`), stating exactly what is and is not the shipped code path.
3. Drive it like the owner: set Flashcard as the learning service in its settings, play, hover a word, press
   "+"; do it for a second sentence with the same word; add an expression. Then read back from Flashcard's API:
   note, card, sentence, translation, picture/audio if sent, source/moment, and that the second capture became
   an encounter of the same word (default `subtitle.same_word`). Check the extension's own UI shows success,
   and what it shows when Flashcard is closed, when the key is wrong, and after Flashcard restarts.
4. Known-words: the extension does not call `known-words` today. Report precisely what the extension would need
   (file + function in its repo) to colour words from Flashcard when `subtitle.share_known_words` is on; do not
   change the extension.
Fix every bug on the Flashcard side (Rust, tests). Keep the runnable script under `tools/subtitle-real-browser/`
(skips cleanly when `/home/user/subtitle` or Chromium is missing; not in default CI). Write
`docs/14-subtitle-gercek-tarayici.md`: setup, what passed, what failed and was fixed, what needs a change in
the extension repo, what still needs the owner's own browser (YouTube/Netflix with a real account).
Usual checks. Commit on your branch (no push). Report in Turkish, short, with a table.
