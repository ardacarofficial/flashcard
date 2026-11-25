# J1: independent code review of the write path, with fixes

Read `j-common.md`. You did not write this code; review it as an outsider whose job is to find where the owner's
cards or study history can be lost, corrupted or wrongly changed. Three areas, in this order:

1. **SQLite writes** (`crates/fc-storage/src/sqlite/`, `store.rs`, migrations): transaction boundaries, anything
   written outside a transaction, the background WAL checkpoint thread, busy/locked handling, foreign keys,
   statements built from strings, what happens when two processes (desktop app + `fc api serve` / CLI) have the
   collection open.
2. **Commands and undo** (`crates/fc-app`: command registry, oplog, undo/redo): a command whose inverse does not
   restore everything it changed, side effects outside the transaction (files, media, events, indexes) that
   survive a rollback or an undo, validation that happens after the write, batches.
3. **Plugins** (`crates/fc-plugins/src/runtime.rs`, `crates/fc-app/src/modules/plugins/host.rs`): what a plugin can
   ask the host for, whether every host call checks the granted permission, resource limits (time, memory,
   output size), path and URL handling, what a hostile plugin package can do at install time.

For each finding: prove it with a failing test first where it can be tested, then fix it, keep the test. Rank
findings (data loss > wrong data > crash > hardening). Do not report style. A finding you cannot fix within this
task goes into the report with file:line and a concrete failure scenario. Write the review to
`docs/15-bagimsiz-inceleme.md` (Turkish, short: what was read, findings table with status, what was not read) and
link it from `docs/README.md`. You may use sub-agents to read the three areas in parallel, but verify every
finding yourself before fixing it. If you find nothing in an area, say so and say what you read.
