//! # fc-interop
//!
//! Import/export file formats (docs/02 §15–16). Every function here is **bytes in, bytes
//! out**: no filesystem, no clock, no randomness. Callers (the `io` module in `fc-app`) read
//! and write files, pass `now`, and turn the results into commands.
//!
//! * [`text`]: flexible delimited text (CSV/TSV/`term: definition`/Quizlet) — parse, write
//!   and separator auto-detection.
//! * [`json`]: our own JSON note format.
//! * [`pack`]: the in-memory package every structured import produces (`fc-core` models
//!   plus media and review history); applied by `fc-app` in one undoable command.
//! * [`anki`]: Anki `.apkg`/`.colpkg` reading (legacy `collection.anki2`/`.anki21` and the
//!   latest zstd `collection.anki21b` schema) and writing (legacy schema 11).
//! * [`fcpack`]: our deck package (`.fcpack`), [`fcbackup`]: full collection backup.
//! * [`media`]: media references inside field HTML. [`print`]: printable HTML.
//! * [`subtitle`]: `.srt` / `.vtt` subtitles → cues (reading mode, v0.10).
#![forbid(unsafe_code)]

pub mod anki;
pub mod error;
pub mod fcbackup;
pub mod fcpack;
pub mod json;
pub mod media;
pub mod pack;
pub mod print;
pub mod subtitle;
pub mod text;
mod zipio;

pub use error::{InteropError, InteropResult};

#[cfg(test)]
mod fuzz_tests;
