# J-workers — v1.4.2: the rest of the independent reviewer's plan

Read `.orchestra/v14/COMMON.md` first; it binds you. State: v1.4.1 is released on `claude/festive-einstein-8y0kld`
(robustness round done, CI and Windows build green). Four siblings work in parallel on the items that were left.
Rules for this round:
- Tests must pass on Windows CI too (no Unix-only paths, no wall-clock assumptions, files closed before delete).
- `fc-desktop` (apps/desktop/src-tauri) cannot be compiled in this container. Code you write there is read by
  nobody but CI: keep it small, re-read it as a reviewer (clippy `-D warnings`, items after test modules, unused
  imports), and say in the report exactly what only CI can confirm.
- No features removed, no UI redesign, sync out of scope. Text in the UI is i18n TR/EN.
- Do not bump the version. CHANGELOG: lines under "Unreleased (v1.4.2)". Siblings share a 4-core machine: `-j 2`,
  `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
- `/home/user/subtitle` and `/home/user/quiz` are read-only references: never edit or commit there.
- Never put a real key or token in the repo or in output. No model names in the repo.
- Usual full checks before the final commit (fmt, clippy `--workspace --exclude fc-desktop --all-targets`,
  tests you touched plus the crate's suite, gen-ts check if the API changed, pnpm check/test if the UI changed).
  Commit on your branch, no push. Report in Turkish, short: table of what you found or built, what you ran,
  what you could not check.
