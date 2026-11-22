# A1 — backups that always restore
OWN: `crates/fc-interop/src/{fcbackup.rs,zipio.rs}`, `crates/fc-app/src/modules/io/backup.rs` (+ new files beside
them), their tests, backup settings/commands in fc-api. Migrations 1400–1409 if needed.
LEAVE ALONE: subtitle and reading modules, fc-study, the frontend (except generated TS types and the two locale
lines a new setting needs).

Problem (verified): the writer builds the dump as JSON and the finished archive in memory (`Vec<u8>`) and can
skip a media row whose file is missing; the reader caps the inflated archive at 1 GiB (`zipio.rs` MAX_BUDGET).
A collection with a lot of captured audio can be backed up and then refused on restore.

Do:
1. Streaming write and read of `.fcbackup` (file to file, bounded memory); the size the app can write it can
   read. Keep zip-bomb protection for foreign input (per-entry ratio / declared-vs-actual checks), but no fixed
   total ceiling for our own verified format; old backups still restore.
2. The manifest lists every media file with hash; a missing file fails the backup (or is reported in the
   result as `missing_media[]` and the backup is marked incomplete; pick one, document it); restore verifies
   every listed entry exists and matches.
3. `io.backup.verify`: restores into a temporary profile/DB and compares counts (notes, cards, review log,
   media) and hashes, without touching the live data.
4. Automatic backups: setting (off / daily / weekly, default daily), keeps the last N verified copies (default
   7), never overwrites the last good one, runs as a cancellable job; a "last verified backup" fact is readable
   through the API (`io.backup.status`). Scheduling uses the injected Clock.
5. Tests: a >1 GiB-equivalent path without allocating it all (e.g. lower the test threshold through an internal
   knob and prove streaming), interrupted write leaves the previous backup intact, missing media, corrupt entry,
   restore into an empty profile brings notes + review history + a media file back byte-identical.
