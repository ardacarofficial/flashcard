# H3 — Rastgele ve bozuk girdi testleri
Read h-common.md. No migrations.
Existing: property tests in `fc-core/src/query/tests.rs` (parser round-trip) and `fc-app/src/modules/words/tests.rs`
(random undo/redo). Add where a real user's files and a browser extension feed the app:
1. Import: Anki `.apkg`/`.colpkg` (corrupt zip, wrong schema versions, huge fields, invalid UTF-8, missing media,
   duplicate ids, cyclic deck parents), CSV/TSV (quotes, BOMs, mixed line ends, 1e6 columns), `.fcpack`, `.fcbackup`
   (bit flips, truncation at every section boundary, zip bombs: bounded memory and time). Never a panic, never a
   partial import left behind (one transaction, undoable), a message a person understands.
2. Subtitle capture API (`fc-http`): random/malformed JSON, oversized media, wrong content types, replayed and
   out-of-order requests, concurrent duplicates; idempotency holds, nothing written on a rejected request.
3. Undo invariant across ALL commands: generate random sequences of every undoable command in the API registry
   (derive the list from the registry so a new command cannot be forgotten); after undoing everything the
   user-data snapshot equals the start; redo returns to the end state; survives a restart in the middle.
4. Query language against the store: random valid queries never error and agree with a naive in-memory filter
   on small random collections (the new v1.4 predicates included).
Use the property-testing crate already in the workspace if there is one; otherwise `proptest`. Deterministic seeds
in CI, failing seed printed.
