# W4 — every card can have sound: fill in missing audio, and know where a sound came from (wave 3)
OWN: `crates/fc-app/src/modules/tts.rs` and media origin tracking, `crates/fc-providers/src/speech.rs`,
`crates/fc-app/src/modules/reading/clips*` (clip extraction), audio fields of encounters (add columns in
migrations 1420–1429; smallest edits to `subtitle/encounters` code, listed in your report), docs.
LEAVE ALONE: encounter ranking (W3), study modes, AI meaning task, backup internals.

1. **Origin.** Every audio (and image) attached to a note/encounter records its origin: `original` (captured
   from the film by the extension), `clip` (cut from a local video file), `tts` (provider, voice, speed, text
   hash), `user`. Queryable (`ses:`/`audio:` predicate values from B3 — extend with origin if B3 left a seam,
   smallest edit) and returned by the encounters/notes API. Backfill: existing Subtitle WAVs = `original`.
2. **Fill missing audio** as a cancellable job `media.audio.fill { scope }` with a preview (how many notes,
   which source would be used): prefer the original; else re-cut from a user-chosen local video when the
   encounter has start–end ms (ffmpeg path as today's `reading.clips.extract`; a ±offset setting for cuts that
   are shifted between releases; never invent a clip when times are missing); else TTS of the TERM (and
   optionally the sentence) through the existing `TtsProvider`s. Cache by (language, voice, speed, text hash)
   so the same word is synthesised once. Undoable.
3. **Offline voice on Windows.** The desktop UI falls back to WebView speech today. Check what the core can do
   offline on Windows without bundling a model: a provider that uses the OS voices (SAPI/WinRT through a
   small, well-known crate) IF it can be added without breaking the Linux build (cfg(windows), compiled in
   Windows CI only; unverified here → say so) — otherwise design the provider seam + document, and make the
   job report clearly "no offline voice for <lang> installed" instead of failing silently.
4. Tests with a fake TTS provider and a tiny generated WAV; ffmpeg-dependent tests skip cleanly when ffmpeg is
   absent (state whether it is present here).
