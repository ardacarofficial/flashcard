# H1 — FSRS ağırlık uyuşmazlığı + yedeğin başka diske alınması ve geri dönüş
Read h-common.md. Migration range 1423–1429.
1. Real bug (verified): `fc-core/src/preset.rs` accepts 17, 19 or 21 `fsrs_weights`; `fc-study/src/scheduler/fsrs.rs`
   rejects anything but 21 ("FSRS-6 needs 21 weights"); `fc-app/src/modules/study/engine.rs` passes them through.
   A preset that can be saved can fail while studying. Decide properly: convert 17/19 to FSRS-6 the way the
   reference implementation defines the upgrade (check `fsrs` crate docs/source in ~/.cargo/registry), at one
   place, so every consumer (study, recall, forecast, optimizer, Anki import/export of deck configs) sees valid
   weights; test with real FSRS-4.5/5 parameter sets that scheduling matches the reference for those versions
   or state the documented approximation. Imported Anki presets with old weights must study without error.
   Then audit the same seam for the rest of the preset (desired retention bounds, steps, max interval): anything
   saveable must be studyable; add a property test "any preset that validates can schedule any card state".
2. Backups are kept under the collection folder: a dead disk takes them too. Add a second backup location
   (setting `io.backup_mirror_dir`, optional; e.g. another drive or a synced folder): every automatic and manual
   backup is also written there, verified there, with the same keep count; status shows both places and says
   plainly when the mirror is missing/unwritable; never blocks studying. Settings › Yedek gets the folder field.
3. Disaster path, end to end: from ONLY a `.fcbackup` in the mirror folder and an empty profile, the app restores
   notes, cards, review history, media (byte-identical), encounters, word marks, settings; a journey or CLI e2e
   proves it and it runs in the Windows CI job. First-run screen offers "Yedekten geri yükle" if it does not already.
