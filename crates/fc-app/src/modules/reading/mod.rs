//! `reading` module (v0.10, docs/02 §24–§26): reading mode with LingQ-style word colours,
//! subtitles and sentence mining, incremental reading.
//!
//! * [`items`]: reading items (texts, subtitles, extracts) and the reading queue. Every
//!   command is undoable: create ↔ delete, delete ↔ undelete, everything else ↔
//!   `reading.items.restore_state` with the previous columns.
//! * [`words`]: `reading.analyze` (tokens → lemmas → status, `fc_core::reading::word_status`),
//!   word marks and lookups. A word's cards are found through its **headword**: the note's
//!   `word` field if its note type has one (Vocabulary, Sentence card), else its sort field,
//!   cleaned (`fc_core::reading::clean_headword`) and lemmatized with the reading language's
//!   lemmatizer — the same one the text goes through, so `running` in a text meets a `run`
//!   card. The cards counted can be narrowed with the `reading.vocabulary_query` setting.
//! * [`cards`]: word/sentence cards and clozes from what is read (nested `notes.create`).
//! * [`jobs`]: `reading.import` (file / web page → item) and `reading.clip` (ffmpeg: a cue's
//!   audio and a screenshot from the video → media).
//!
//! All API offsets are UTF-16 code units ([`Utf16`]).

pub(crate) mod cards;
pub(crate) mod items;
pub(crate) mod jobs;
pub(crate) mod words;

#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::{ApiError, ErrorCode};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_providers::language::Lemmatizer;
use fc_providers::lemma::{builtin_lemmatizers, primary_language, Plain, ANY_LANGUAGE};
use fc_providers::Capability;
use fc_storage::Reads;
use std::sync::Arc;

/// Module id.
pub const ID: &str = "reading";
/// Job kind: file / web page → reading item.
pub const IMPORT_JOB: &str = "reading.import";
/// Job kind: ffmpeg clip of a subtitle cue.
pub const CLIP_JOB: &str = "reading.clip";

/// Setting: language of new texts.
pub const DEFAULT_LANGUAGE: &str = "reading.default_language";
/// Setting: query of the cards that count for word colours (empty = every card).
pub const VOCABULARY_QUERY: &str = "reading.vocabulary_query";
/// Setting: ffmpeg program (name on `PATH` or absolute path).
pub const FFMPEG_PATH: &str = "reading.ffmpeg_path";

/// Longest text of an item (bytes).
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

fn text_setting(key: &str, default: &str, max_len: u32, advanced: bool) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: SettingKind::Text {
            max_len: Some(max_len),
        },
        default: serde_json::Value::String(default.to_owned()),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.reading.name",
        description_key: "modules.reading.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            text_setting(DEFAULT_LANGUAGE, "en", 35, false),
            text_setting(VOCABULARY_QUERY, "", 2000, false),
            text_setting(FFMPEG_PATH, "ffmpeg", 1000, true),
        ],
        ui: vec![main_menu(
            "reading.screen",
            "nav.reading",
            "book-open",
            "/reading",
            45,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    items::register(reg);
    words::register(reg);
    cards::register(reg);
    jobs::register(reg);
}

// ------------------------------------------------------------------ helpers

/// A stored text setting, else `default`.
pub(crate) fn setting_text(reads: &dyn Reads, key: &str, default: &str) -> AppResult<String> {
    Ok(
        crate::settings::resolve_stored(reads, &SettingScope::App, key)?
            .and_then(|(v, _)| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| default.to_owned()),
    )
}

/// A BCP-47 tag as given (trimmed), the default language when empty. Validated loosely:
/// letters, digits, `-`/`_`, at most 35 chars, starting with a letter.
pub(crate) fn language_or_default(reads: &dyn Reads, given: &str) -> AppResult<String> {
    let given = given.trim();
    let lang = if given.is_empty() {
        setting_text(reads, DEFAULT_LANGUAGE, "en")?.trim().to_owned()
    } else {
        given.to_owned()
    };
    let ok = !lang.is_empty()
        && lang.len() <= 35
        && lang.starts_with(|c: char| c.is_ascii_alphabetic())
        && lang
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(lang)
    } else {
        Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationInvalidParams, format!("bad language {lang:?}"))
                .with_detail("reason", "language"),
        ))
    }
}

/// Key of word marks: the primary subtag (`en-GB` → `en`).
pub(crate) fn mark_language(language: &str) -> String {
    primary_language(language)
}

/// The lemmatizer for `language`: the first built-in declaring it, else the plain one.
pub(crate) fn lemmatizer_for(language: &str) -> Arc<dyn Lemmatizer> {
    let primary = primary_language(language);
    builtin_lemmatizers()
        .into_iter()
        .find(|l| {
            l.info().capabilities.iter().any(|c| {
                matches!(c, Capability::Language(code) if *code == primary && code != ANY_LANGUAGE)
            })
        })
        .unwrap_or_else(|| Arc::new(Plain))
}

/// `VALIDATION_READING_RANGE` for `field`.
pub(crate) fn bad_range(field: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationReadingRange, format!("bad range: {field}"))
            .with_detail("field", field),
    )
}

/// Byte ↔ UTF-16 offsets of a text.
pub(crate) struct Utf16<'a> {
    text: &'a str,
    byte: usize,
    unit: u32,
}

impl<'a> Utf16<'a> {
    /// Cursor at the start of `text`.
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            text,
            byte: 0,
            unit: 0,
        }
    }

    /// UTF-16 length of a text.
    pub(crate) fn len(text: &str) -> u32 {
        u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX)
    }

    /// UTF-16 offset of byte `byte` (a char boundary). Fast for ascending queries.
    pub(crate) fn unit_of(&mut self, byte: usize) -> u32 {
        if byte < self.byte {
            self.byte = 0;
            self.unit = 0;
        }
        for c in self.text[self.byte..].chars() {
            if self.byte >= byte {
                break;
            }
            self.byte += c.len_utf8();
            self.unit += u32::try_from(c.len_utf16()).unwrap_or(2);
        }
        self.unit
    }

    /// Byte offset of UTF-16 offset `unit` (clamped to the text; inside a surrogate pair →
    /// the start of that character).
    pub(crate) fn byte_of(text: &str, unit: u32) -> usize {
        let mut u = 0u32;
        for (b, c) in text.char_indices() {
            let next = u + u32::try_from(c.len_utf16()).unwrap_or(2);
            if next > unit {
                return b;
            }
            u = next;
        }
        text.len()
    }
}
