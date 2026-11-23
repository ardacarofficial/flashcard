# I1 — Sağlamlık turunun kapanışı ve 1.4.1

Read `.orchestra/v14/briefs/h-common.md` (binds you). H1–H4 are merged into your base. Their reports are the
last long assistant texts in `/tmp/claude-0/-home-user/9995e59c-6b11-5385-8893-948e6f2fb895/scratchpad/v14/logs/{h1,h2,h3,h4}*.jsonl`;
read them. Close what they left open, in this order (commit after each):

1. Undo invariant coverage: H3's random undo test generates 39 of 124 undoable commands (`NOT_GENERATED` list).
   For every command left out, either add a generator or point (in the list, checked by a test) to an existing
   test that proves undo restores state for it; where no such test exists, write it. Fix what breaks.
2. Local API hardening: `/v1` `routes.rs::read_json` answers a truncated/malformed body with 413 like the
   Subtitle path did (now 422): align. Add a body-read timeout so a client that announces a long body and stalls
   cannot hold a connection forever (test with a stalled socket).
3. Leftover files: media rows with `ref_count=0` after an undone import (prove maintenance cleans them or make
   it so, without deleting anything still referenced by the undo/redo log); `backups/.verify-*` temp folders
   left by a crash during verification are removed at next start.
4. Mirror catch-up: when the mirror folder was missing at backup time and is back, the next backup also copies
   the verified backups it lacks (within the keep count), and status says so.
5. In-app rescue: a collection that opens but fails the integrity check offers the same two rescue choices as
   the start-up dialog (Bakım page), using the existing `rescue` core command. Desktop `src-tauri` cannot be
   compiled here: keep that part minimal, mirror existing command wiring exactly, and say what only Windows CI
   can confirm.
6. Version 1.4.1 everywhere 1.4.0 is declared; CHANGELOG "Unreleased (v1.4.1)" becomes the 1.4.1 section,
   grouped, plain words; STATE.md, docs/README status, CLAUDE.md status line.
7. Full verification, sequentially, nothing else runs on the machine: fmt, clippy, `cargo test --workspace
   --exclude fc-desktop`, gen-ts --check, `pnpm check`, `test`, `build`, `bundle:check`, full `e2e`, subtitle
   contract check, `tools/subtitle-real-browser` (unpatched and with SUBTITLE_PATCH), the kill matrix and the
   fuzz suites once with a larger case count (`FC_FUZZ_CASES`, `FC_SOAK=1`), and the full-scale perf suite
   (docs/12 command) to confirm no budget regressed; update docs/12 if numbers moved.
If time runs short: 6 and 7 are mandatory; list what is left of 1–5.
