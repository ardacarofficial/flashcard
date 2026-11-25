# K1: close the open findings of the independent review

Read `j-common.md` and `docs/15-bagimsiz-inceleme.md` first. Four findings were left open; each gets a failing test
first where a test is possible, then the fix.

1. **Finding 9** (`crates/fc-app/src/app.rs` around 244 and 285): while the `fc` CLI runs a long import, the desktop
   app starts, treats the CLI's open command group as crashed and undoes it; the CLI keeps writing. Prove it with two
   real processes (the kill-matrix tests show how to drive real processes), then fix it so a group that belongs to a
   live process is never undone by another one (and a group of a dead process still is). Must work on Windows.
2. **Finding 10** (`crates/fc-storage/src/sqlite/content.rs` around 296): a media file is renamed into place without
   fsync, so a power cut right after can leave the row with an empty file. Measure the cost of syncing the file (and
   the directory where the platform has that) on the capture path with the scaled perf suite, several runs before and
   after. If the capture budgets hold, fix it. If they do not, find the design that keeps both (for example sync
   before the row's transaction commits, off the answer path) and say what you chose and why, with the numbers.
3. **Finding 11** (`crates/fc-storage/src/migrate.rs` 124–145): two processes opening at once both enter the same
   migration and one fails to open once. Make the second wait and then see the migration done.
4. **`crates/fc-app/src/modules/ai/meaning.rs` around 374** swallows the error of a nested call. With review finding 5
   fixed (a rolled-back transaction now answers `StorageError::RolledBack`), check what a command does when its
   statement is cut short (disk full, timeout): the command must fail as a whole, nothing half-written, and the user
   gets the right error. Test it at the application layer (finding 5 was only tested in the storage layer), fix what
   breaks. Look for other places that swallow a storage error inside a command the same way.

Finding 12 stays open (structural); leave it. Update `docs/15-bagimsiz-inceleme.md` (status of each finding) and the
CHANGELOG under "Unreleased (v1.4.2)". Do not bump the version. A sibling (K2) works on speed in `fc-app`'s study,
browser and Anki paths at the same time: stay out of those.
