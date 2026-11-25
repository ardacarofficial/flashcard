# J4: offline speech on Windows

Read `j-common.md`. Read `crates/fc-app/src/modules/tts.rs`, the audio-fill path (`audio_fill`, "missing audio"
work from W4), `crates/fc-providers`, how the desktop shell wires providers, and `docs/` on TTS first.

Goal: on Windows, a card's word and sentence can be spoken and missing audio can be filled with no network and no
key, using the voices Windows already has, in the note's language (English, and whatever voices the machine has).

- Find what exists today (which providers, what the UI offers, what happens offline) and say it in the report
  before changing anything. If offline speech already works through some path, do not build a second one: close
  the gaps instead.
- Follow the architecture: the capability is a provider behind the existing TTS interface, chosen in the existing
  settings; logic in Rust; the module can be turned off; `fc-core` stays pure. The Windows API (WinRT
  `Windows.Media.SpeechSynthesis` or SAPI through the `windows` crate) lives behind `cfg(windows)` with a clear
  "not available on this system" answer elsewhere, so the workspace still builds and tests on Linux.
- It must produce audio bytes that the existing media path stores (so the audio fill, backup and export work as for
  any other audio), list the installed voices, pick a voice for the note's language, and fail with a plain TR/EN
  sentence when no voice exists for that language (with what the owner can do: install a Windows language pack).
- Tests: everything above the system call is tested on Linux with a fake synthesiser; a `cfg(windows)` test in the
  Windows CI job speaks one short English sentence and checks that it gets a non-empty, decodable audio file (GitHub's
  Windows runners have English voices; if the runner has none, the test must say "no voice" clearly, not pass
  silently and not fail the build for that reason alone).
- The crate with the Windows code must be one CI compiles and tests on Windows (check which crates the Windows job
  builds and tests); new dependencies only for `cfg(windows)`, checked against the lockfile and `cargo deny`/license
  rules if the repo has them.
- Docs: the provider in the relevant doc and the settings help text; CHANGELOG line.
