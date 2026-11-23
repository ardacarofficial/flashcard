# H4 — Subtitle eklentisi için "Flashcard'dan oku" yaması (eklenti deposuna dokunmadan)
Read h-common.md and `docs/14-subtitle-gercek-tarayici.md` §4 (what the extension needs).
`/home/user/subtitle` is READ-ONLY: never edit or commit there. Work in a scratch copy
(`cp -r` without node_modules/dist into your scratchpad, `git init` there if the copy has no .git, or
`git worktree`-free plain copy + `git diff --no-index`).
Build the change in the copy, following the extension's own CLAUDE.md conventions (Effector models, Turkish UI
texts, tests with Vitest, settings backup rules: a new persisted store must be in the backup or NOT_BACKED_UP):
"Flashcard'dan oku" beside "Anki'den oku" — reads the known/learning words from Flashcard's `known-words`
endpoint (paged by cursor, only when Flashcard says `shared: true`; a clear message when sharing is off in
Flashcard), merges them like `syncAnkiFx` does (a user's own mark wins), so subtitles colour by what Flashcard
knows. Unit tests in the copy; `pnpm test`, `pnpm lint`, `pnpm build` green in the copy; then run
`tools/subtitle-real-browser` against the PATCHED build and extend it: share on in Flashcard → press
"Flashcard'dan oku" → a learning word is coloured in the subtitles; share off → message, nothing coloured.
Deliver in the flashcard repo: `tools/subtitle-patches/0001-flashcard-known-words.patch` (applies with
`git apply` on the extension's current HEAD; record that commit hash), a short README there (how to apply, what
it does, how it was tested), docs/14 updated. Flashcard-side fixes if the endpoint needs any (Rust, tests).
