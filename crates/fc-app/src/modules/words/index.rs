//! Keeping the word index (migration 1410) in step with the collection.
//!
//! Triggers queue every note whose note, cards, note type, capture, producer language or deck
//! language changed, and every `(language, key)` whose manual mark changed. [`drain`] runs at
//! the end of **every** write transaction of the dispatcher (`App::run_command`, before the
//! oplog entry is written) and at start-up, so the index changes with the command that changed
//! its sources — undo and redo included — and is never stale after a commit. A full rebuild
//! ([`rebuild`]) empties the index, queues everything and drains: the same code path, so the
//! incremental result and the rebuilt one are equal by construction (and by test).

use super::LANGUAGE;
use crate::error::AppResult;
use crate::modules::reading::{lemmatizer_for, words::tokens};
use crate::modules::stats::facts::calendar;
use fc_core::note::strip_html;
use fc_core::notetype::{FieldKey, FieldRole, NoteTypeKind};
use fc_core::reading::{clean_headword, headword_key, WordMark};
use fc_core::settings::SettingScope;
use fc_core::words::{card_facts, index_language, NoteWordFacts, WordEvidence};
use fc_core::{DeckId, NoteId, NoteTypeId};
use fc_providers::language::Lemmatizer;
use fc_storage::repo::{WordCardRow, WordIndexNoteRecord, WordIndexRecord, WordNoteSource};
use fc_storage::{Reads, WriteTx};
use fc_study::day::StudyDay;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

/// Notes / keys read per query.
const CHUNK: usize = 500;

/// Reads the index needs, cached for one drain.
struct Indexer {
    cal: StudyDay,
    /// Note type → the field a word is read from (`None` = not a vocabulary type).
    term_fields: HashMap<NoteTypeId, Option<FieldKey>>,
    deck_languages: HashMap<DeckId, String>,
    app_language: Option<String>,
    lemmatizers: HashMap<String, Arc<dyn Lemmatizer>>,
    keys: HashMap<(String, String), Option<String>>,
}

/// Index language of a stored `words.language` value.
fn stored_language(v: Option<serde_json::Value>) -> Option<String> {
    v.and_then(|v| v.as_str().map(index_language))
        .filter(|l| !l.is_empty())
}

impl Indexer {
    fn new(reads: &dyn Reads) -> AppResult<Self> {
        Ok(Self {
            cal: calendar(reads, None)?,
            term_fields: HashMap::new(),
            deck_languages: HashMap::new(),
            app_language: stored_language(reads.get_setting(&SettingScope::App, LANGUAGE)?),
            lemmatizers: HashMap::new(),
            keys: HashMap::new(),
        })
    }

    /// The field a note type's word is read from: its `term` role; a note type without any
    /// role (Anki imports, note types made before v1.3) falls back to its sort field, as
    /// reading mode always did. Cloze types and types with roles but no term have none.
    fn term_field(&mut self, reads: &dyn Reads, nt: NoteTypeId) -> AppResult<Option<FieldKey>> {
        if let Some(f) = self.term_fields.get(&nt) {
            return Ok(f.clone());
        }
        let field = reads.get_notetype(nt)?.and_then(|t| {
            if let Some(f) = t.field_with_role(FieldRole::Term) {
                Some(f.key.clone())
            } else if !t.has_roles() && t.kind != NoteTypeKind::Cloze {
                Some(t.sort_field.clone())
            } else {
                None
            }
        });
        self.term_fields.insert(nt, field.clone());
        Ok(field)
    }

    /// Language of a note: its Subtitle capture's, else the one reading mode recorded, else
    /// `words.language` of its home deck (deck scope, then app scope); `""` = unknown.
    fn language(&mut self, reads: &dyn Reads, src: &WordNoteSource) -> AppResult<String> {
        if let Some(l) = [&src.capture_lang, &src.known_lang]
            .into_iter()
            .flatten()
            .map(|l| index_language(l))
            .find(|l| !l.is_empty())
        {
            return Ok(l);
        }
        if let Some(l) = self.deck_languages.get(&src.home_deck_id) {
            return Ok(l.clone());
        }
        let l = stored_language(
            reads.get_setting(&SettingScope::Deck(src.home_deck_id), LANGUAGE)?,
        )
        .or_else(|| self.app_language.clone())
        .unwrap_or_default();
        self.deck_languages.insert(src.home_deck_id, l.clone());
        Ok(l)
    }

    /// Key of a term in `language` (`""` = unknown language: the cleaned text in lower case,
    /// kept for the report only).
    fn key(&mut self, term: &str, language: &str) -> AppResult<Option<String>> {
        let plain = strip_html(term);
        let head = clean_headword(&plain).to_owned();
        if head.is_empty() {
            return Ok(None);
        }
        if language.is_empty() {
            return Ok(Some(head.to_lowercase()));
        }
        let cache_key = (language.to_owned(), head);
        if let Some(k) = self.keys.get(&cache_key) {
            return Ok(k.clone());
        }
        let lem = self
            .lemmatizers
            .entry(language.to_owned())
            .or_insert_with(|| lemmatizer_for(language))
            .clone();
        let lemmas: Vec<String> = tokens(&*lem, &cache_key.1, language)?
            .into_iter()
            .map(|t| t.lemma)
            .collect();
        let key = headword_key(&lemmas, language);
        self.keys.insert(cache_key, key.clone());
        Ok(key)
    }

    /// The row a live note contributes (`None` = it is not a word).
    fn note_row(
        &mut self,
        reads: &dyn Reads,
        src: &WordNoteSource,
        cards: &[WordCardRow],
    ) -> AppResult<Option<WordIndexNoteRecord>> {
        let Some(field) = self.term_field(reads, src.notetype_id)? else {
            return Ok(None);
        };
        let Some(term) = src.fields.get(&field) else {
            return Ok(None);
        };
        let language = self.language(reads, src)?;
        let Some(key) = self.key(term, &language)? else {
            return Ok(None);
        };
        let facts: Vec<_> = cards
            .iter()
            .map(|c| (c.state, self.interval(c), c.last_review))
            .collect();
        let (count, max_interval_days, last_review) = card_facts(&facts);
        Ok(Some(WordIndexNoteRecord {
            note_id: src.note_id,
            language,
            key,
            cards: count,
            max_interval_days,
            last_review,
            created_at: src.created_at,
        }))
    }

    /// Interval of a review card in study days (as the statistics compute it).
    fn interval(&self, c: &WordCardRow) -> Option<u32> {
        match (c.due, c.last_review) {
            (Some(due), Some(last)) => {
                u32::try_from((self.cal.index(due) - self.cal.index(last)).max(0)).ok()
            }
            _ => None,
        }
    }
}

fn facts(r: &WordIndexNoteRecord) -> NoteWordFacts {
    NoteWordFacts {
        language: r.language.clone(),
        key: r.key.clone(),
        cards: r.cards,
        max_interval_days: r.max_interval_days,
        last_review: r.last_review,
        created_at: r.created_at,
    }
}

/// The `word_index` row of a word's evidence.
fn entry(language: &str, key: &str, ev: &WordEvidence) -> WordIndexRecord {
    WordIndexRecord {
        language: language.to_owned(),
        key: key.to_owned(),
        mark: ev.mark.map(|(m, _)| m.as_str().to_owned()),
        mark_at: ev.mark.map(|(_, t)| t),
        notes: ev.notes,
        cards: ev.cards,
        max_interval_days: ev.max_interval_days,
        last_review: ev.last_review,
        first_seen: ev.first_seen,
    }
}

/// Brings the index up to date with the queued changes. Cheap when nothing is queued (two
/// small reads).
pub(crate) fn drain(tx: &mut dyn WriteTx) -> AppResult<()> {
    let (notes, keys) = tx.take_word_index_dirty()?;
    if notes.is_empty() && keys.is_empty() {
        return Ok(());
    }
    let mut dirty: BTreeSet<(String, String)> = keys.into_iter().collect();
    let mut indexer = Indexer::new(&*tx)?;
    for chunk in notes.chunks(CHUNK) {
        update_notes(tx, &mut indexer, chunk, &mut dirty)?;
    }
    let mut by_language: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (l, k) in dirty {
        if !l.is_empty() {
            by_language.entry(l).or_default().push(k);
        }
    }
    for (language, keys) in by_language {
        for chunk in keys.chunks(CHUNK) {
            update_words(tx, &language, chunk)?;
        }
    }
    Ok(())
}

fn update_notes(
    tx: &mut dyn WriteTx,
    indexer: &mut Indexer,
    ids: &[NoteId],
    dirty: &mut BTreeSet<(String, String)>,
) -> AppResult<()> {
    let old: HashMap<NoteId, WordIndexNoteRecord> = tx
        .word_index_notes_by_id(ids)?
        .into_iter()
        .map(|r| (r.note_id, r))
        .collect();
    let sources = tx.word_note_sources(ids)?;
    let mut cards: HashMap<NoteId, Vec<WordCardRow>> = HashMap::new();
    for c in tx.word_card_rows(ids)? {
        cards.entry(c.note_id).or_default().push(c);
    }
    let mut new: HashMap<NoteId, WordIndexNoteRecord> = HashMap::new();
    for src in &sources {
        let of = cards.get(&src.note_id).map_or(&[][..], Vec::as_slice);
        if let Some(row) = indexer.note_row(&*tx, src, of)? {
            new.insert(src.note_id, row);
        }
    }
    for id in ids {
        let before = old.get(id);
        let after = new.get(id);
        if before == after {
            continue;
        }
        if let Some(b) = before {
            dirty.insert((b.language.clone(), b.key.clone()));
        }
        match after {
            Some(a) => {
                dirty.insert((a.language.clone(), a.key.clone()));
                tx.put_word_index_note(a)?;
            }
            None => tx.delete_word_index_note(*id)?,
        }
    }
    Ok(())
}

fn update_words(tx: &mut dyn WriteTx, language: &str, keys: &[String]) -> AppResult<()> {
    let mut notes: HashMap<String, Vec<NoteWordFacts>> = HashMap::new();
    for r in tx.word_index_notes_of(language, keys)? {
        notes.entry(r.key.clone()).or_default().push(facts(&r));
    }
    let marks: HashMap<String, (WordMark, fc_core::Timestamp)> = tx
        .word_marks_of(language, keys)?
        .into_iter()
        .filter_map(|m| WordMark::parse(&m.status).map(|s| (m.lemma, (s, m.updated_at))))
        .collect();
    let current: HashMap<String, WordIndexRecord> = tx
        .word_index_get(language, keys)?
        .into_iter()
        .map(|r| (r.key.clone(), r))
        .collect();
    for key in keys {
        let of = notes.get(key).map_or(&[][..], Vec::as_slice);
        let row = WordEvidence::collect(of, marks.get(key).copied())
            .map(|ev| entry(language, key, &ev));
        match (current.get(key), row) {
            (Some(c), Some(r)) if *c == r => {}
            (_, Some(r)) => tx.put_word_index_entry(&r)?,
            (Some(_), None) => tx.delete_word_index_entry(language, key)?,
            (None, None) => {}
        }
    }
    Ok(())
}

/// Empties the index and builds it again from every note and mark.
pub(crate) fn rebuild(tx: &mut dyn WriteTx) -> AppResult<()> {
    tx.reset_word_index()?;
    drain(tx)
}
