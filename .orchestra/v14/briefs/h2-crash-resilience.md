# H2 — Çökme ve bozulma dayanıklılığı
Read h-common.md. Migration range 1481–1485 (if needed).
Existing: quick integrity check after an unclean shutdown, kill tests (`fc-app/src/modules/maintenance/kill_tests.rs`),
half/corrupt backup tests. Missing, do these with real processes and real files:
1. Kill during an open transaction at many points (answering, capture with media write, import, undo/redo,
   backup, word-index rebuild, migration): after restart the collection is consistent (integrity + foreign keys +
   app invariants: every card has a note, oplog matches state, media refs exist or are reported) and the last
   acknowledged write is present. Media files and DB must not disagree (orphans cleaned or adopted, never a card
   pointing to a missing file silently).
2. Disk full / write errors: inject failures at the file layer (a wrapper or a tiny quota'd temp dir) for DB
   writes, WAL, media writes, backup writes. The app must fail the command cleanly, stay usable, say what
   happened in plain words (TR/EN), and never leave a truncated backup that looks valid.
3. A database that does not open or fails integrity: no silent data loss, no auto-delete. Keep the original
   untouched (renamed copy), offer: restore the newest VERIFIED backup into a fresh profile, or export what is
   readable. Implement that recovery command and its first-run/maintenance UI entry; test with deliberately
   corrupted pages and a truncated file.
4. The background WAL checkpoint thread (fc-storage store.rs, added in v1.4): prove with a test that a crash
   during a checkpoint loses nothing, and that closing the store on Windows releases every handle.
