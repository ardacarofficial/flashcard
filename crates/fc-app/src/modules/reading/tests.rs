//! `reading` module tests, run against the in-memory store and SQLite.
//!
//! Acceptance (docs/04 v0.10):
//! * "Bilinen kelime durumu FSRS verisiyle tutarlı" → `word_colours_follow_fsrs_maturity`.
//! * ".srt dosyasından cümle kartı, video varsa ses kesitiyle" →
//!   `subtitle_file_to_sentence_card_with_clip` (fake ffmpeg, unix).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::TestApp;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::CreateDeck;
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::NoteDto;
use fc_api::v1::reading::*;
use fc_api::v1::settings::SetSetting;
use fc_api::ErrorCode;
use fc_core::card::CardState;
use fc_core::reading::{ReadingPriority, WordMark, WordStatus};
use fc_core::settings::SettingScope;
use fc_core::stats::MATURE_DAYS;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{CardId, DeckId, Timestamp};
use fc_storage::Store;
use std::sync::Arc;

type Factory = dyn Fn() -> Arc<dyn Store>;

fn deck(t: &TestApp, path: &str) -> DeckId {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
    .id
}

fn word_card(t: &TestApp, deck_id: DeckId, word: &str, meaning: &str) -> NoteDto {
    t.ok::<ReadingCardsMake>(MakeReadingCard {
        kind: ReadingCardKind::Word,
        deck_id,
        word: word.into(),
        meaning: meaning.into(),
        sentence: String::new(),
        word_range: None,
        translation: String::new(),
        item_id: None,
        cue_index: None,
        audio: None,
        image: None,
        tags: vec![],
    })
}

/// Sets every card of `note` to a review card with an interval of `days`.
fn set_interval(t: &TestApp, note: &NoteDto, days: i64) {
    let mut tx = t.store.write().unwrap();
    for c in &note.cards {
        let mut card = tx.get_card(c.id).unwrap().unwrap();
        let last = Timestamp(1_767_225_600_000);
        card.scheduling.state = CardState::Review;
        card.scheduling.last_review = Some(last);
        card.scheduling.due = Some(Timestamp(last.0 + days * MILLIS_PER_DAY));
        card.scheduling.stability = Some(days as f64);
        tx.update_card(&card).unwrap();
    }
    // A raw write outside the dispatcher: bring the word index along, as every command does.
    crate::modules::words::index::drain(&mut *tx).unwrap();
    tx.commit().unwrap();
}

/// How many of `note`'s cards the **statistics module** counts as mature: its own card facts
/// (`stats::facts::card_fact` → `CardFact::is_mature`) and `stats.overview`'s state counts,
/// which must agree with each other.
fn stats_mature(t: &TestApp, note: &NoteDto) -> u32 {
    use crate::modules::stats::facts::{calendar, card_fact, Presets};
    let from_facts = {
        let reads = t.store.read().unwrap();
        let cal = calendar(&*reads, Some(0)).unwrap();
        let mut presets = Presets::new(&*reads);
        let now = Timestamp(1_767_225_600_000);
        note.cards
            .iter()
            .map(|c| reads.get_card(c.id).unwrap().unwrap())
            .filter(|card| card_fact(card, cal, now, &mut presets).unwrap().is_mature())
            .count()
    };
    let overview = t.ok::<StatsGetOverview>(fc_api::v1::stats::StatsRequest {
        tz_offset_mins: Some(0),
        fresh: Some(true),
        ..Default::default()
    });
    assert_eq!(overview.cards.states.mature as usize, from_facts);
    from_facts as u32
}

fn analyze(t: &TestApp, text: &str, lang: &str) -> TextAnalysis {
    t.ok::<ReadingAnalyze>(AnalyzeText {
        item_id: None,
        text: Some(text.into()),
        language: Some(lang.into()),
        range: None,
        tz_offset_mins: Some(0),
    })
}

/// `(surface, status)` of every token.
fn colours(a: &TextAnalysis) -> Vec<(String, WordStatus)> {
    let units: Vec<u16> = a.text.encode_utf16().collect();
    a.tokens
        .iter()
        .map(|t| {
            (
                String::from_utf16(&units[t.start as usize..t.end as usize]).unwrap(),
                t.status,
            )
        })
        .collect()
}

fn status_of(a: &TextAnalysis, surface: &str) -> WordStatus {
    colours(a)
        .into_iter()
        .find(|(s, _)| s == surface)
        .unwrap_or_else(|| panic!("no token {surface}"))
        .1
}

fn mark(t: &TestApp, lang: &str, lemmas: &[&str], m: Option<WordMark>) -> u32 {
    t.ok::<ReadingWordsMark>(MarkWords {
        language: lang.into(),
        lemmas: lemmas.iter().map(|s| (*s).to_owned()).collect(),
        mark: m,
    })
    .changed
}

fn create_text(t: &TestApp, title: &str, body: &str, queue: bool) -> ReadingItemDto {
    t.ok::<ReadingItemsCreate>(CreateReadingItem {
        title: title.into(),
        language: "en".into(),
        body: body.into(),
        cues: None,
        source: None,
        media_path: None,
        queue,
        priority: ReadingPriority::Normal,
    })
}

fn job(t: &TestApp, j: &JobDto) -> JobDto {
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: j.id })
}

fn trashed(t: &TestApp, id: fc_core::NoteId) -> bool {
    t.store
        .read()
        .unwrap()
        .get_note(id)
        .unwrap()
        .is_none_or(|n| n.deleted_at.is_some())
}

fn undo(t: &TestApp) {
    t.ok::<OplogUndo>(Empty {});
}

fn redo(t: &TestApp) {
    t.ok::<OplogRedo>(Empty {});
}

// ------------------------------------------------------------------ word colours

fn word_colours_follow_fsrs_maturity(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "English");
    let text = "She runs fast. He ran! The cats were running, and John saw them.";
    let a = analyze(&t, text, "en");
    assert_eq!(a.lemmatizer, "builtin.en");
    assert!(colours(&a).iter().all(|(_, s)| *s == WordStatus::New));
    assert_eq!(a.summary.known_pct, 0.0);
    assert_eq!(a.sentences.len(), 3);

    // A card for "run" (any form in the text) → learning.
    let run = word_card(&t, d, "to run", "koşmak");
    let a = analyze(&t, text, "en");
    for w in ["runs", "ran", "running"] {
        assert_eq!(status_of(&a, w), WordStatus::Learning, "{w}");
    }
    // Interval 20 days: still learning; 21 (= MATURE_DAYS, the statistics' "mature"): known.
    // At each step the v0.6 statistics are asked directly (their own `card_fact` →
    // `CardFact::is_mature`, and `stats.overview`'s mature count): the colour must agree.
    assert_eq!(stats_mature(&t, &run), 0);
    for (days, want) in [
        (1, WordStatus::Learning),
        (i64::from(MATURE_DAYS) - 1, WordStatus::Learning),
        (i64::from(MATURE_DAYS), WordStatus::Known),
        (i64::from(MATURE_DAYS) + 100, WordStatus::Known),
    ] {
        set_interval(&t, &run, days);
        let mature = stats_mature(&t, &run);
        let status = status_of(&analyze(&t, text, "en"), "ran");
        assert_eq!(status, want, "{days} days");
        assert_eq!(status == WordStatus::Known, mature > 0, "{days} days: stats say {mature}");
    }
    set_interval(&t, &run, i64::from(MATURE_DAYS));
    let a = analyze(&t, text, "en");
    assert_eq!(status_of(&a, "running"), WordStatus::Known);
    // Consistent with what the statistics count as mature.
    let info = t.ok::<ReadingWordsLookup>(LookupWord {
        language: "en".into(),
        lemma: "run".into(),
        tz_offset_mins: Some(0),
    });
    assert_eq!(info.status, WordStatus::Known);
    assert_eq!(info.cards.len(), 2, "recognition + production");
    assert!(info.cards.iter().all(|c| c.mature && c.interval_days == Some(MATURE_DAYS)));
    assert_eq!(info.cards[0].word, "to run");
    assert_eq!(info.cards[0].meaning.as_deref(), Some("koşmak"));

    // Marks only matter without cards.
    assert_eq!(mark(&t, "en", &["run", "cat", "john"], Some(WordMark::Ignored)), 3);
    let a = analyze(&t, text, "en");
    assert_eq!(status_of(&a, "ran"), WordStatus::Known, "cards win over marks");
    assert_eq!(status_of(&a, "cats"), WordStatus::Ignored);
    assert_eq!(status_of(&a, "John"), WordStatus::Ignored);
    // 3 run-forms known out of 11 countable words (13 words − 2 ignored).
    assert_eq!(a.summary.running.known, 3);
    assert_eq!(a.summary.running.ignored, 2);
    assert!((a.summary.known_pct - 27.3).abs() < 1e-9);

    // A lapse makes it learning again.
    {
        let mut tx = t.store.write().unwrap();
        for c in &run.cards {
            let mut card = tx.get_card(c.id).unwrap().unwrap();
            card.scheduling.state = CardState::Relearning;
            tx.update_card(&card).unwrap();
        }
        crate::modules::words::index::drain(&mut *tx).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(status_of(&analyze(&t, text, "en"), "ran"), WordStatus::Learning);

    // The vocabulary query narrows the cards that count.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: super::VOCABULARY_QUERY.into(),
        value: serde_json::json!("deck:Nothing"),
    });
    assert_eq!(status_of(&analyze(&t, text, "en"), "ran"), WordStatus::Ignored);
}

fn turkish_words_and_marks_undo(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Türkçe");
    let text = "Kalemlerimizden birini aldık. Kitapları masaya koyduk.";
    let a = analyze(&t, text, "tr-TR");
    assert_eq!(a.lemmatizer, "builtin.tr");
    word_card(&t, d, "kalem", "pen");
    let a = analyze(&t, text, "tr-TR");
    assert_eq!(status_of(&a, "Kalemlerimizden"), WordStatus::Learning);
    assert_eq!(status_of(&a, "Kitapları"), WordStatus::New);

    // Marks are keyed by the primary subtag; undo/redo restore them.
    assert_eq!(mark(&t, "tr", &["kitap", "masa", "kitap"], Some(WordMark::Known)), 2);
    let a = analyze(&t, text, "tr-TR");
    assert_eq!(status_of(&a, "Kitapları"), WordStatus::Known);
    assert_eq!(status_of(&a, "masaya"), WordStatus::Known);
    undo(&t);
    assert_eq!(status_of(&analyze(&t, text, "tr"), "Kitapları"), WordStatus::New);
    redo(&t);
    assert_eq!(status_of(&analyze(&t, text, "tr"), "Kitapları"), WordStatus::Known);
    assert_eq!(mark(&t, "tr", &["kitap"], Some(WordMark::Known)), 0, "no change");
    mark(&t, "tr", &["kitap"], None);
    assert_eq!(status_of(&analyze(&t, text, "tr"), "Kitapları"), WordStatus::New);
    let events = t.take_events();
    assert!(events.iter().any(|e| matches!(
        &e.event,
        fc_api::v1::events::DomainEvent::WordMarksChanged { language } if language == "tr"
    )));
    // Unknown language → plain lowercase lemmatizer.
    assert_eq!(analyze(&t, "Größe", "de").lemmatizer, "builtin.plain");
    let e = t.err::<ReadingWordsMark>(MarkWords {
        language: "en".into(),
        lemmas: vec![" ".into()],
        mark: Some(WordMark::Known),
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

// ------------------------------------------------------------------ items & queue

fn items_queue_and_extracts(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Notes");
    // An emoji takes two UTF-16 units: offsets are JS string indices.
    let body = "🙂 Ankara is the capital of Türkiye. It has many parks.";
    let item = create_text(&t, "  ", body, false);
    assert_eq!(item.title, "🙂 Ankara is the capital of Türkiye. It has many parks.");
    assert_eq!(item.length, body.encode_utf16().count() as u32);
    assert_eq!(item.kind, ReadingItemKind::Text);
    assert!(!item.queued);

    let a = t.ok::<ReadingAnalyze>(AnalyzeText {
        item_id: Some(item.id),
        ..AnalyzeText::default()
    });
    assert_eq!(a.tokens[0].start, 3, "after the emoji and a space");
    assert_eq!(a.sentences, vec![[0, 36], [37, 55]]);
    // A page of the text.
    let page = t.ok::<ReadingAnalyze>(AnalyzeText {
        item_id: Some(item.id),
        range: Some([37, 55]),
        ..AnalyzeText::default()
    });
    assert_eq!(page.tokens.len(), 4);
    assert_eq!(page.sentences, vec![[37, 55]]);
    assert_eq!(page.summary, a.summary, "summary covers the whole text");

    // Position, update and their undo.
    let moved = t.ok::<ReadingItemsSetPosition>(SetReadingPosition {
        id: item.id,
        position: 10_000,
    });
    assert_eq!(moved.position, moved.length);
    assert!((moved.progress_pct - 100.0).abs() < 1e-9);
    undo(&t);
    assert_eq!(t.ok::<ReadingItemsGet>(ReadingItemRef { id: item.id }).position, 0);
    let renamed = t.ok::<ReadingItemsUpdate>(UpdateReadingItem {
        id: item.id,
        title: "Ankara".into(),
        language: "en-GB".into(),
        media_path: None,
    });
    assert_eq!(renamed.title, "Ankara");
    assert_eq!(renamed.language, "en-GB");

    // Queue: due now, then 1, 2, 4 days (normal priority); postpone; out.
    let q = t.ok::<ReadingQueueSet>(SetReadingQueue {
        id: item.id,
        queued: true,
        priority: None,
    });
    assert!(q.queued && q.is_due);
    let list = t.ok::<ReadingItemsList>(ListReadingItems { queued_only: true });
    assert_eq!((list.items.len(), list.due_count), (1, 1));
    let mut intervals = vec![];
    for _ in 0..3 {
        let r = t.ok::<ReadingQueueRead>(ReadingSessionDone {
            id: item.id,
            position: Some(20),
        });
        assert!(!r.is_due);
        intervals.push(r.interval_days);
    }
    assert_eq!(intervals, [1, 2, 4]);
    let r = t.ok::<ReadingItemsGet>(ReadingItemRef { id: item.id });
    assert_eq!((r.read_count, r.position), (3, 20));
    undo(&t);
    assert_eq!(t.ok::<ReadingItemsGet>(ReadingItemRef { id: item.id }).interval_days, 2);
    let p = t.ok::<ReadingQueuePostpone>(PostponeReading {
        id: item.id,
        days: 10,
    });
    assert!(p.due.unwrap().0 > r.due.unwrap().0);
    assert_eq!(
        t.err::<ReadingQueuePostpone>(PostponeReading { id: item.id, days: 0 }).code,
        ErrorCode::ValidationReadingRange
    );

    // Extract "Ankara is the capital of Türkiye." → queued, high priority, child of the item.
    let start = 3;
    let end = 36;
    let ex = t.ok::<ReadingExtractsCreate>(CreateExtract {
        item_id: item.id,
        start,
        end,
        title: String::new(),
    });
    assert_eq!(ex.kind, ReadingItemKind::Extract);
    assert_eq!(ex.parent_id, Some(item.id));
    assert_eq!(ex.priority, ReadingPriority::High);
    assert!(ex.is_due);
    assert_eq!(ex.title, "Ankara is the capital of Türkiye.");
    let queue = t.ok::<ReadingItemsList>(ListReadingItems { queued_only: true });
    assert_eq!(queue.items[0].id, ex.id, "due first");
    assert_eq!(
        t.err::<ReadingExtractsCreate>(CreateExtract {
            item_id: item.id,
            start: 36,
            end: 37,
            title: String::new(),
        })
        .code,
        ErrorCode::ValidationReadingRange,
        "only white space"
    );

    // Cloze from the extract: hide "Ankara" and "Türkiye".
    let note = t.ok::<ReadingExtractsCloze>(ClozeFromItem {
        item_id: ex.id,
        range: None,
        hide: vec![[0, 6], [25, 32]],
        together: false,
        deck_id: d,
        tags: vec!["okuma".into()],
    });
    assert_eq!(
        note.fields["text"],
        "{{c1::Ankara}} is the capital of {{c2::Türkiye}}."
    );
    assert_eq!(note.cards.len(), 2);
    undo(&t);
    assert!(trashed(&t, note.id));
    assert_eq!(
        t.err::<ReadingExtractsCloze>(ClozeFromItem {
            item_id: ex.id,
            range: None,
            hide: vec![[0, 6], [3, 9]],
            together: false,
            deck_id: d,
            tags: vec![],
        })
        .code,
        ErrorCode::ValidationReadingRange
    );

    // Delete → gone from lists; undo brings it back; redo trashes again.
    t.ok::<ReadingItemsDelete>(ReadingItemRef { id: ex.id });
    assert_eq!(
        t.err::<ReadingItemsGet>(ReadingItemRef { id: ex.id }).code,
        ErrorCode::NotFoundReadingItem
    );
    undo(&t);
    assert_eq!(t.ok::<ReadingItemsGet>(ReadingItemRef { id: ex.id }).id, ex.id);
    redo(&t);
    let all = t.ok::<ReadingItemsList>(ListReadingItems::default());
    assert_eq!(all.items.len(), 1);
    // Empty texts are refused.
    assert_eq!(
        t.err::<ReadingItemsCreate>(CreateReadingItem {
            title: "x".into(),
            language: String::new(),
            body: " \n ".into(),
            cues: None,
            source: None,
            media_path: None,
            queue: false,
            priority: ReadingPriority::Normal,
        })
        .code,
        ErrorCode::ValidationReadingText
    );
}

// ------------------------------------------------------------------ cards

fn word_and_sentence_cards(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "English");
    let sentence = "I saw 🙂 cats running.";
    let units: Vec<u16> = sentence.encode_utf16().collect();
    let at = sentence.find("running").unwrap();
    let start = sentence[..at].encode_utf16().count() as u32;
    let range = Some([start, start + 7]);
    assert_eq!(String::from_utf16(&units[start as usize..start as usize + 7]).unwrap(), "running");
    let note = t.ok::<ReadingCardsMake>(MakeReadingCard {
        kind: ReadingCardKind::Word,
        deck_id: d,
        word: "run".into(),
        meaning: "koşmak".into(),
        sentence: sentence.into(),
        word_range: range,
        translation: String::new(),
        item_id: None,
        cue_index: None,
        audio: None,
        image: None,
        tags: vec!["okuma".into()],
    });
    assert_eq!(
        note.notetype_id,
        fc_core::content::builtin::find("vocabulary").unwrap().id
    );
    assert_eq!(note.fields["word"], "run");
    assert_eq!(note.fields["example"], "I saw 🙂 cats <b>running</b>.");
    // Duplicates are refused like any note.
    assert_eq!(
        t.err::<ReadingCardsMake>(MakeReadingCard {
            kind: ReadingCardKind::Word,
            deck_id: d,
            word: "run".into(),
            meaning: "x".into(),
            sentence: String::new(),
            word_range: None,
            translation: String::new(),
            item_id: None,
            cue_index: None,
            audio: None,
            image: None,
            tags: vec![],
        })
        .code,
        ErrorCode::ConflictNoteDuplicate
    );
    // A word card needs a meaning (required field of Vocabulary).
    assert_eq!(
        t.err::<ReadingCardsMake>(MakeReadingCard {
            kind: ReadingCardKind::Word,
            deck_id: d,
            word: "walk".into(),
            meaning: " ".into(),
            sentence: String::new(),
            word_range: None,
            translation: String::new(),
            item_id: None,
            cue_index: None,
            audio: None,
            image: None,
            tags: vec![],
        })
        .code,
        ErrorCode::ValidationFieldRequired
    );
    // Undo removes the note in one step.
    undo(&t);
    assert!(trashed(&t, note.id));
    // Media must be references.
    assert_eq!(
        t.err::<ReadingCardsMake>(MakeReadingCard {
            kind: ReadingCardKind::Sentence,
            deck_id: d,
            word: "run".into(),
            meaning: String::new(),
            sentence: sentence.into(),
            word_range: None,
            translation: String::new(),
            item_id: None,
            cue_index: None,
            audio: Some("C:/x.mp3".into()),
            image: None,
            tags: vec![],
        })
        .code,
        ErrorCode::ValidationInvalidParams
    );
}

fn srt_bytes() -> &'static str {
    "1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>The cats were running.</i>\r\n\r\n2\r\n00:01:02,000 --> 00:01:04,000\r\nThey ran home!\r\n"
}

fn subtitle_file_to_sentence_card(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Films");
    let dir = tempfile::tempdir().unwrap();
    let srt = dir.path().join("Cats S01E01.srt");
    std::fs::write(&srt, srt_bytes()).unwrap();
    let started = t.ok::<ReadingItemsImport>(ImportReadingItem {
        source: ReadingSource::File {
            path: srt.display().to_string(),
        },
        title: String::new(),
        language: "en".into(),
        media_path: None,
        queue: false,
        priority: ReadingPriority::Normal,
    });
    let done = job(&t, &started);
    assert_eq!(done.status, JobStatus::Succeeded, "{:?}", done.error);
    let item: ReadingItemDto = serde_json::from_value(done.result.unwrap()).unwrap();
    assert_eq!(item.kind, ReadingItemKind::Subtitle);
    assert_eq!(item.title, "Cats S01E01");
    assert_eq!(item.source.as_deref(), Some("Cats S01E01.srt"));
    assert_eq!(item.cue_count, 2);
    let listed = t.ok::<ReadingItemsList>(ListReadingItems::default());
    assert_eq!(listed.items[0].cue_count, 2);

    let a = t.ok::<ReadingAnalyze>(AnalyzeText {
        item_id: Some(item.id),
        ..AnalyzeText::default()
    });
    assert_eq!(a.text, "The cats were running.\nThey ran home!");
    assert_eq!(a.cues.len(), 2);
    assert_eq!(a.cues[1].start_ms, 62_000);

    let note = t.ok::<ReadingCardsMake>(MakeReadingCard {
        kind: ReadingCardKind::Sentence,
        deck_id: d,
        word: "run".into(),
        meaning: "koşmak".into(),
        sentence: "They ran home!".into(),
        word_range: Some([5, 8]),
        translation: "Eve koştular!".into(),
        item_id: Some(item.id),
        cue_index: Some(1),
        audio: None,
        image: None,
        tags: vec![],
    });
    assert_eq!(
        note.notetype_id,
        fc_core::content::builtin::find("sentence").unwrap().id
    );
    assert_eq!(note.fields["sentence"], "They <b>ran</b> home!");
    assert_eq!(note.fields["translation"], "Eve koştular!");
    assert_eq!(note.fields["source"], "Cats S01E01 · 1:02");
    // The sentence card's `word` field counts for the word colours.
    let a = t.ok::<ReadingAnalyze>(AnalyzeText {
        item_id: Some(item.id),
        ..AnalyzeText::default()
    });
    assert_eq!(status_of(&a, "running"), WordStatus::Learning);

    // No video: clips are refused before a job starts.
    assert_eq!(
        t.err::<ReadingClipsExtract>(ExtractClip {
            item_id: item.id,
            cue_index: 0,
            image: false,
        })
        .code,
        ErrorCode::ValidationReadingClip
    );
    // Bad sources.
    let bad = t.ok::<ReadingItemsImport>(ImportReadingItem {
        source: ReadingSource::File {
            path: dir.path().join("missing.txt").display().to_string(),
        },
        title: String::new(),
        language: String::new(),
        media_path: None,
        queue: false,
        priority: ReadingPriority::Normal,
    });
    let bad = job(&t, &bad);
    assert_eq!(bad.status, JobStatus::Failed);
    assert_eq!(bad.error.unwrap().code, ErrorCode::ValidationReadingSource);
    assert_eq!(
        t.err::<ReadingItemsImport>(ImportReadingItem {
            source: ReadingSource::Url {
                url: "file:///etc/passwd".into(),
            },
            title: String::new(),
            language: String::new(),
            media_path: None,
            queue: false,
            priority: ReadingPriority::Normal,
        })
        .code,
        ErrorCode::ValidationReadingSource
    );
    assert_eq!(
        t.err::<ReadingItemsImport>(ImportReadingItem {
            source: ReadingSource::File {
                path: "relative.srt".into(),
            },
            title: String::new(),
            language: String::new(),
            media_path: None,
            queue: false,
            priority: ReadingPriority::Normal,
        })
        .code,
        ErrorCode::ValidationReadingSource
    );
    // A plain text file in Windows-1254 (Turkish Notepad / Excel).
    let txt = dir.path().join("hikaye.txt");
    std::fs::write(&txt, [0xC7, b'o', b'k', b' ', b'g', 0xFC, b'z', b'e', b'l', b'.']).unwrap();
    let j = t.ok::<ReadingItemsImport>(ImportReadingItem {
        source: ReadingSource::File {
            path: txt.display().to_string(),
        },
        title: String::new(),
        language: "tr".into(),
        media_path: None,
        queue: true,
        priority: ReadingPriority::Low,
    });
    let j = job(&t, &j);
    let story: ReadingItemDto = serde_json::from_value(j.result.unwrap()).unwrap();
    assert_eq!(story.title, "hikaye");
    assert!(story.queued && story.priority == ReadingPriority::Low);
    let a = t.ok::<ReadingAnalyze>(AnalyzeText {
        item_id: Some(story.id),
        ..AnalyzeText::default()
    });
    assert_eq!(a.text, "Çok güzel.");
}

/// A fake ffmpeg: writes a tiny mp3 or jpeg to its last argument.
#[cfg(unix)]
fn fake_ffmpeg(dir: &std::path::Path, fail: bool) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(if fail { "ffmpeg-broken" } else { "ffmpeg-fake" });
    let script = if fail {
        "#!/bin/sh\necho 'Invalid data found when processing input' >&2\nexit 1\n".to_owned()
    } else {
        format!(
            "#!/bin/sh\necho \"$@\" >> '{}'\nfor last; do :; done\ncase \"$last\" in\n  *.mp3) printf 'ID3\\003\\000\\000\\000\\000\\000\\000' > \"$last\" ;;\n  *.jpg) printf '\\377\\330\\377\\340\\000\\020JFIF' > \"$last\" ;;\nesac\n",
            dir.join("calls.log").display()
        )
    };
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn subtitle_file_to_sentence_card_with_clip(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Films");
    let dir = tempfile::tempdir().unwrap();
    let srt = dir.path().join("film.srt");
    let video = dir.path().join("film.mp4");
    std::fs::write(&srt, srt_bytes()).unwrap();
    std::fs::write(&video, b"not really a video").unwrap();
    let ffmpeg = fake_ffmpeg(dir.path(), false);
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: super::FFMPEG_PATH.into(),
        value: serde_json::json!(ffmpeg.display().to_string()),
    });
    let j = t.ok::<ReadingItemsImport>(ImportReadingItem {
        source: ReadingSource::File {
            path: srt.display().to_string(),
        },
        title: "Film".into(),
        language: "en".into(),
        media_path: Some(video.display().to_string()),
        queue: false,
        priority: ReadingPriority::Normal,
    });
    let item: ReadingItemDto = serde_json::from_value(job(&t, &j).result.unwrap()).unwrap();
    assert_eq!(item.media_path.as_deref(), Some(video.to_str().unwrap()));

    let clip_job = t.ok::<ReadingClipsExtract>(ExtractClip {
        item_id: item.id,
        cue_index: 1,
        image: true,
    });
    let done = job(&t, &clip_job);
    assert_eq!(done.status, JobStatus::Succeeded, "{:?}", done.error);
    let clip: ReadingClip = serde_json::from_value(done.result.unwrap()).unwrap();
    assert_eq!((clip.start_ms, clip.end_ms), (61_750, 64_250));
    assert_eq!(clip.audio.mime, "audio/mpeg");
    assert_eq!(clip.image.as_ref().unwrap().mime, "image/jpeg");
    let calls = std::fs::read_to_string(dir.path().join("calls.log")).unwrap();
    assert!(calls.contains("-ss 61.750"), "{calls}");
    assert!(calls.contains("-t 2.500"), "{calls}");
    assert!(calls.contains("-ss 63.000"), "{calls}");

    let note = t.ok::<ReadingCardsMake>(MakeReadingCard {
        kind: ReadingCardKind::Sentence,
        deck_id: d,
        word: "run".into(),
        meaning: String::new(),
        sentence: "They ran home!".into(),
        word_range: Some([5, 8]),
        translation: String::new(),
        item_id: Some(item.id),
        cue_index: Some(1),
        audio: Some(clip.audio.reference.clone()),
        image: Some(clip.image.unwrap().reference),
        tags: vec![],
    });
    assert_eq!(note.fields["audio"], clip.audio.reference);
    assert!(note.fields["image"].starts_with("media:"));

    // A broken ffmpeg fails the job with PROVIDER_FFMPEG.
    let broken = fake_ffmpeg(dir.path(), true);
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: super::FFMPEG_PATH.into(),
        value: serde_json::json!(broken.display().to_string()),
    });
    let failed = job(
        &t,
        &t.ok::<ReadingClipsExtract>(ExtractClip {
            item_id: item.id,
            cue_index: 0,
            image: false,
        }),
    );
    assert_eq!(failed.status, JobStatus::Failed);
    let e = failed.error.unwrap();
    assert_eq!(e.code, ErrorCode::ProviderFfmpeg);
    assert_eq!(e.details["reason"], "failed");
    // A missing program cannot even be set…
    let e = t.err::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: super::FFMPEG_PATH.into(),
        value: serde_json::json!(dir.path().join("nope").display().to_string()),
    });
    assert_eq!(e.code, ErrorCode::ValidationSettingValue);
    // …and one written behind the commands' back (a hand edit) does not run either.
    {
        let mut tx = t.store.write().unwrap();
        let nope = dir.path().join("nope").display().to_string();
        let v = serde_json::json!(nope);
        tx.set_setting(&SettingScope::App, super::FFMPEG_PATH, &v, Timestamp(1))
            .unwrap();
        tx.commit().unwrap();
    }
    let missing = job(
        &t,
        &t.ok::<ReadingClipsExtract>(ExtractClip {
            item_id: item.id,
            cue_index: 0,
            image: false,
        }),
    );
    let e = missing.error.unwrap();
    assert_eq!(e.code, ErrorCode::ProviderFfmpeg);
    assert_eq!(e.details["reason"], "path");
    // Out-of-range cue.
    assert_eq!(
        t.err::<ReadingClipsExtract>(ExtractClip {
            item_id: item.id,
            cue_index: 9,
            image: false,
        })
        .code,
        ErrorCode::ValidationReadingClip
    );
}

/// Due texts come in priority order, however long each has been due.
fn queue_orders_due_texts_by_priority(f: &Factory) {
    let t = TestApp::new(f());
    let mut ids = vec![];
    // Queued low first, so it has been due longest.
    for p in [ReadingPriority::Low, ReadingPriority::Normal, ReadingPriority::High] {
        let item = create_text(&t, p.as_str(), "Some text.", false);
        let q = t.ok::<ReadingQueueSet>(SetReadingQueue {
            id: item.id,
            queued: true,
            priority: Some(p),
        });
        assert!(q.is_due);
        ids.push(q.id);
    }
    // One not due yet, high priority: after every due one.
    let later = create_text(&t, "later", "Later.", false);
    t.ok::<ReadingQueueSet>(SetReadingQueue {
        id: later.id,
        queued: true,
        priority: Some(ReadingPriority::High),
    });
    t.ok::<ReadingQueuePostpone>(PostponeReading {
        id: later.id,
        days: 3,
    });
    let queue = t.ok::<ReadingItemsList>(ListReadingItems { queued_only: true });
    let order: Vec<_> = queue.items.iter().map(|i| i.id).collect();
    assert_eq!(order, [ids[2], ids[1], ids[0], later.id]);
    assert_eq!(queue.due_count, 3);
}

// ------------------------------------------------------------------ executable settings

/// `reading.ffmpeg_path` names a program the app runs: only the user, the CLI and the app
/// hold `settings:exec`; plugins, automations, local-API and MCP clients cannot set or reset
/// it — directly or wrapped in `batch.execute` — while ordinary settings stay open to them.
#[cfg(unix)]
fn exec_settings_are_user_only(f: &Factory) {
    use fc_api::v1::oplog::Actor;
    use serde_json::json;
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let good = fake_ffmpeg(dir.path(), false).display().to_string();
    let set = |v: &str| json!({ "scope": { "kind": "app" }, "key": super::FFMPEG_PATH, "value": v });
    let reset = json!({ "scope": { "kind": "app" }, "key": super::FFMPEG_PATH });
    let stored = |t: &TestApp| {
        t.store
            .read()
            .unwrap()
            .get_setting(&SettingScope::App, super::FFMPEG_PATH)
            .unwrap()
    };
    let outsiders = [
        Actor::Plugin("p".into()),
        Actor::Automation("a".into()),
        Actor::LocalApi("l".into()),
        Actor::Mcp("m".into()),
    ];
    for actor in &outsiders {
        for (method, params) in [("v1.settings.set", set(&good)), ("v1.settings.reset", reset.clone())] {
            let e = t.app.invoke(method, params.clone(), actor).unwrap_err();
            assert_eq!(e.code, ErrorCode::PermissionDenied, "{method} {actor:?}");
            // A batch does not launder it.
            let batch = json!({ "calls": [{ "method": method, "params": params }] });
            let e = t.app.invoke("v1.batch.execute", batch, actor).unwrap_err();
            assert_eq!(e.code, ErrorCode::PermissionDenied, "batch {method} {actor:?}");
            // Since v0.7 none of them holds `settings:write` either; the handler's own check
            // is the second wall, for trusted paths (undo/redo, a batch's inner calls).
            let request = fc_api::InvokeRequest { method: method.to_owned(), params };
            let e = t.app.invoke_trusted(&request, actor).unwrap_err();
            assert_eq!(e.code, ErrorCode::PermissionDenied, "trusted {method} {actor:?}");
            assert_eq!(e.details["permission"], "settings:exec", "{method} {actor:?}");
        }
        assert_eq!(stored(&t), None);
    }
    // On a trusted path other settings stay writable for them: only exec settings are
    // checked by actor in the handler.
    for actor in &outsiders[..3] {
        let v = json!({ "scope": { "kind": "app" }, "key": super::VOCABULARY_QUERY, "value": "deck:X" });
        let request = fc_api::InvokeRequest { method: "v1.settings.set".into(), params: v };
        t.app.invoke_trusted(&request, actor).unwrap();
    }
    // The user and the CLI may — with a real executable only.
    for actor in [Actor::User, Actor::Cli] {
        t.app.invoke("v1.settings.set", set(&good), &actor).unwrap();
        assert_eq!(stored(&t), Some(json!(good)));
        t.app.invoke("v1.settings.reset", reset.clone(), &actor).unwrap();
        assert_eq!(stored(&t), None);
    }
    let plain = dir.path().join("plain.txt");
    std::fs::write(&plain, "x").unwrap();
    let bad = [
        "ffmpeg".to_owned(),                         // not absolute
        "relative/ffmpeg".to_owned(),                // not absolute
        dir.path().join("nope").display().to_string(), // missing
        dir.path().display().to_string(),            // a folder
        plain.display().to_string(),                 // not executable
        format!(" {good}"),                          // padded
    ];
    for v in &bad {
        let e = t.app.invoke("v1.settings.set", set(v), &Actor::User).unwrap_err();
        assert_eq!(e.code, ErrorCode::ValidationSettingValue, "{v:?}");
    }
    // Empty = the default (`ffmpeg` on PATH).
    t.app.invoke("v1.settings.set", set(""), &Actor::User).unwrap();
    assert_eq!(super::jobs::ffmpeg_program(Some("")).unwrap(), "ffmpeg");
    assert_eq!(super::jobs::ffmpeg_program(None).unwrap(), "ffmpeg");
    assert_eq!(super::jobs::ffmpeg_program(Some(&good)).unwrap(), good);
    let e = super::jobs::ffmpeg_program(Some("/bin/../nope/sh")).unwrap_err();
    assert_eq!(fc_api::ApiError::from(e).details["reason"], "path");
    // Undoing the user's change as a plugin is refused too (the inverse is checked).
    t.app.invoke("v1.settings.set", set(&good), &Actor::User).unwrap();
    let e = t
        .app
        .invoke("v1.oplog.undo", json!({}), &Actor::Plugin("p".into()))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    assert_eq!(stored(&t), Some(json!(good)));
}

/// A subtitle's media file is a free file path the app reads and hands to ffmpeg: only the
/// user (and the CLI) may name it (`io:read`); others — even on a trusted path — are refused,
/// and their updates keep the item's file.
fn media_paths_are_user_only(f: &Factory) {
    use fc_api::v1::oplog::Actor;
    use serde_json::json;
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("film.mp4").display().to_string();
    let cues = json!([{ "start_ms": 0, "end_ms": 1000, "text": "Hello there." }]);
    let trusted = |method: &str, params: serde_json::Value, actor: &Actor| {
        let request = fc_api::InvokeRequest { method: method.to_owned(), params };
        t.app.invoke_trusted(&request, actor)
    };
    let plugin = Actor::Plugin("p".into());
    for actor in [plugin.clone(), Actor::Automation("a".into()), Actor::LocalApi("l".into()), Actor::Mcp("m".into())] {
        let e = trusted(
            "v1.reading.items.create",
            json!({ "title": "S", "body": "", "cues": cues, "media_path": video }),
            &actor,
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "{actor:?}");
        assert_eq!(e.details["field"], "media_path");
    }
    let item = t
        .app
        .invoke(
            "v1.reading.items.create",
            json!({ "title": "S", "body": "", "cues": cues, "media_path": video }),
            &Actor::User,
        )
        .unwrap();
    assert_eq!(item["media_path"], json!(video));
    let id = item["id"].clone();
    // A plugin's update (it never sees the path) keeps the user's file.
    let updated = trusted(
        "v1.reading.items.update",
        json!({ "id": id, "title": "Renamed", "language": "en" }),
        &plugin,
    )
    .unwrap();
    assert_eq!((updated["title"].clone(), updated["media_path"].clone()), (json!("Renamed"), json!(video)));
    let e = trusted(
        "v1.reading.items.update",
        json!({ "id": id, "title": "X", "language": "en", "media_path": "/etc/passwd" }),
        &plugin,
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    // The user still may change or clear it.
    let cleared = t
        .app
        .invoke("v1.reading.items.update", json!({ "id": id, "title": "S", "language": "en" }), &Actor::User)
        .unwrap();
    assert_eq!(cleared["media_path"], serde_json::Value::Null);
}

/// A backup is data from outside: a restored `reading.ffmpeg_path` is dropped (back to the
/// default) and must be set again by the user.
#[cfg(unix)]
#[test]
fn restoring_a_backup_clears_exec_settings() {
    use fc_api::v1::io::{CreateBackup, ExportReport, RestoreBackup, RestoreReport};
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::new(Arc::new(crate::testing::TempSqlite::new()));
    t.app.set_data_dir(dir.path().join("app-data"));
    let ffmpeg = fake_ffmpeg(dir.path(), false).display().to_string();
    for (key, value) in [(super::FFMPEG_PATH, ffmpeg.as_str()), (super::VOCABULARY_QUERY, "deck:X")] {
        t.ok::<SettingsSet>(SetSetting {
            scope: SettingScope::App,
            key: key.into(),
            value: serde_json::json!(value),
        });
    }
    let backup = dir.path().join("b.fcbackup").display().to_string();
    let j = job(&t, &t.ok::<IoBackupCreate>(CreateBackup { path: backup.clone() }));
    let _: ExportReport = serde_json::from_value(j.result.unwrap()).unwrap();
    let j = job(
        &t,
        &t.ok::<IoBackupRestore>(RestoreBackup {
            path: backup,
            safety_backup_path: None,
        }),
    );
    assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error);
    let _: RestoreReport = serde_json::from_value(j.result.unwrap()).unwrap();
    let reads = t.store.read().unwrap();
    assert_eq!(reads.get_setting(&SettingScope::App, super::FFMPEG_PATH).unwrap(), None);
    assert_eq!(
        reads.get_setting(&SettingScope::App, super::VOCABULARY_QUERY).unwrap(),
        Some(serde_json::json!("deck:X")),
        "ordinary settings come back"
    );
}

/// A process that writes far more to stderr than a pipe holds still finishes (stderr is
/// drained while it runs); timeouts and cancellation kill it; a missing program is reported.
#[cfg(unix)]
#[test]
fn ffmpeg_process_handling() {
    use super::jobs::run_process;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let script = |name: &str, body: &str| {
        let p = dir.path().join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.display().to_string()
    };
    // ~1 MiB of stderr (a pipe holds 64 KiB), then success.
    let chatty = script("chatty", "head -c 1048576 /dev/zero | tr '\\0' x >&2\nexit 0");
    let t0 = Instant::now();
    run_process(&chatty, &[], Duration::from_secs(20), &|| false).unwrap();
    assert!(t0.elapsed() < Duration::from_secs(10));
    // Chatty and failing: the error carries the tail of stderr, not all of it.
    let noisy = script("noisy", "head -c 1048576 /dev/zero | tr '\\0' x >&2\necho LAST >&2\nexit 3");
    let e = run_process(&noisy, &[], Duration::from_secs(20), &|| false).map_err(fc_api::ApiError::from).unwrap_err();
    assert_eq!(e.details["reason"], "failed");
    assert!(e.message.contains("LAST") && e.message.len() < 1000, "{}", e.message.len());
    // Hangs → timeout / stop.
    let slow = script("slow", "exec sleep 30");
    let t0 = Instant::now();
    let e = run_process(&slow, &[], Duration::from_millis(200), &|| false).map_err(fc_api::ApiError::from).unwrap_err();
    assert_eq!(e.details["reason"], "timeout");
    let e = run_process(&slow, &[], Duration::from_secs(60), &|| t0.elapsed() > Duration::from_millis(400))
        .map_err(fc_api::ApiError::from)
        .unwrap_err();
    assert_eq!(e.details["reason"], "cancelled");
    assert!(t0.elapsed() < Duration::from_secs(10));
    let e = run_process("/nonexistent/ffmpeg", &[], Duration::from_secs(1), &|| false)
        .map_err(fc_api::ApiError::from)
        .unwrap_err();
    assert_eq!(e.details["reason"], "missing");
}

fn module_switch(f: &Factory) {
    let t = TestApp::new(f());
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: super::ID.into(),
        enabled: false,
    });
    assert_eq!(
        t.err::<ReadingItemsList>(ListReadingItems::default()).code,
        ErrorCode::ConflictModuleDisabled
    );
}

#[test]
fn ffmpeg_arguments() {
    let a = super::jobs::audio_args(
        std::path::Path::new("/v/film.mp4"),
        61_750,
        64_250,
        std::path::Path::new("/t/clip.mp3"),
    );
    assert_eq!(
        a.join(" "),
        "-nostdin -hide_banner -loglevel error -y -ss 61.750 -i /v/film.mp4 -t 2.500 -vn -ac 1 -c:a libmp3lame -q:a 5 /t/clip.mp3"
    );
    let i = super::jobs::image_args(
        std::path::Path::new("/v/film.mp4"),
        5,
        std::path::Path::new("/t/f.jpg"),
    );
    assert!(i.join(" ").contains("-ss 0.005 -i /v/film.mp4 -frames:v 1"));
}

/// A Windows path (drive letter, backslashes, spaces, non-ASCII) is passed to ffmpeg as one
/// argument, untouched: no shell, no quoting.
#[test]
fn ffmpeg_arguments_windows_paths() {
    let input = std::path::Path::new(r"C:\Users\Ayşe Yılmaz\Videos\Film (2024).mkv");
    let out = std::path::Path::new(r"C:\Users\AYE~1\AppData\Local\Temp\fc-clip-x1\clip.mp3");
    let a = super::jobs::audio_args(input, 1_000, 2_000, out);
    let i = a.iter().position(|x| x == "-i").unwrap();
    assert_eq!(a[i + 1], input.display().to_string());
    assert_eq!(a.last().unwrap(), &out.display().to_string());
    let img = super::jobs::image_args(input, 1_500, out);
    assert!(img.iter().any(|x| x == r"C:\Users\Ayşe Yılmaz\Videos\Film (2024).mkv"));
    let cmd = super::jobs::ffmpeg_command(r"C:\Program Files\ffmpeg\bin\ffmpeg.exe", &a);
    assert_eq!(cmd.get_program(), r"C:\Program Files\ffmpeg\bin\ffmpeg.exe");
    let args: Vec<_> = cmd.get_args().map(|x| x.to_string_lossy().into_owned()).collect();
    assert_eq!(args, a);
}

// ------------------------------------------------------------------ colouring benchmark

/// A collection with word cards for `cards` distinct words (some mature, some marked) and a
/// `words`-word text drawn from 4 000 distinct ones (English inflections included).
fn colour_world(words: usize, cards: usize) -> (TestApp, String) {
    let t = TestApp::new(Arc::new(crate::testing::TempSqlite::new()));
    let d = deck(&t, "Bench");
    let syl = ["ka", "lo", "mi", "ner", "tas", "vu", "pel", "dor", "sin", "bra"];
    let vocab: Vec<String> = (0..4000usize)
        .map(|i| format!("{}{}{}", syl[i % 10], syl[(i / 10) % 10], syl[(i / 100) % 10]) + &"x".repeat(i / 1000))
        .collect();
    for (i, w) in vocab.iter().take(cards).enumerate() {
        let note = word_card(&t, d, w, "m");
        if i % 3 == 0 {
            set_interval(&t, &note, i64::from(MATURE_DAYS));
        }
    }
    let marked: Vec<&str> = vocab[cards..cards + 200].iter().map(String::as_str).collect();
    mark(&t, "en", &marked, Some(WordMark::Known));
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut text = String::with_capacity(words * 9);
    for n in 0..words {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let w = &vocab[(seed % 4000) as usize];
        text.push_str(w);
        text.push_str(match seed % 4 {
            0 => "s",
            1 => "ing",
            _ => "",
        });
        text.push_str(if n % 12 == 11 { ". " } else { " " });
    }
    (t, text)
}

/// Best of `runs` `reading.analyze` calls over the whole text, in µs.
fn bench_colouring(t: &TestApp, text: &str, runs: usize) -> u128 {
    let mut best = u128::MAX;
    for _ in 0..runs {
        let start = std::time::Instant::now();
        let a = analyze(t, text, "en");
        best = best.min(start.elapsed().as_micros());
        assert_eq!(a.summary.running.total() as usize, text.split_whitespace().count());
        assert!(a.summary.running.known > 0 && a.summary.running.learning > 0);
    }
    println!("colouring {} words: {:.2} ms", text.split_whitespace().count(), best as f64 / 1000.0);
    best
}

/// docs/04 v0.10 performance: colouring a 10 000-word text (tokenize, lemmatize, look up
/// 1 000 word cards and 200 marks, statuses, summary) in < 50 ms. Run with
/// `cargo test -p fc-app --release -- --ignored --nocapture bench_colouring_10k`.
#[test]
#[ignore = "benchmark: run in release"]
fn bench_colouring_10k() {
    let (t, text) = colour_world(10_000, 1_000);
    let best = bench_colouring(&t, &text, 5);
    assert!(best < 50_000, "colouring took {best} µs (budget 50 ms)");
}

/// The CI-sized variant (runs with every `cargo test`): the same 10 000 words and 300 cards.
/// Debug builds are ~10× slower than release and CI runners are noisy, so the budget is
/// 50 ms × 10 × 2 in debug and 50 ms × 2 in release.
#[test]
fn bench_colouring_10k_scaled() {
    let (t, text) = colour_world(10_000, 300);
    let budget_us: u128 = if cfg!(debug_assertions) { 1_000_000 } else { 100_000 };
    let best = bench_colouring(&t, &text, 3);
    assert!(best < budget_us, "colouring took {best} µs (budget {budget_us} µs)");
}

#[test]
fn utf16_offsets() {
    use super::Utf16;
    let s = "a🙂ç";
    assert_eq!(Utf16::len(s), 4);
    let mut c = Utf16::new(s);
    assert_eq!(c.unit_of(1), 1);
    assert_eq!(c.unit_of(5), 3);
    assert_eq!(c.unit_of(7), 4);
    assert_eq!(c.unit_of(0), 0, "rewinds");
    assert_eq!(Utf16::byte_of(s, 1), 1);
    assert_eq!(Utf16::byte_of(s, 2), 1, "inside the surrogate pair → its start");
    assert_eq!(Utf16::byte_of(s, 3), 5);
    assert_eq!(Utf16::byte_of(s, 99), s.len());
    let _ = CardId::from_uuid(uuid::Uuid::nil());
}

macro_rules! reading_suite {
    ($factory:expr) => {
        #[test]
        fn word_colours_follow_fsrs_maturity() {
            super::word_colours_follow_fsrs_maturity(&$factory);
        }
        #[test]
        fn turkish_words_and_marks_undo() {
            super::turkish_words_and_marks_undo(&$factory);
        }
        #[test]
        fn items_queue_and_extracts() {
            super::items_queue_and_extracts(&$factory);
        }
        #[test]
        fn word_and_sentence_cards() {
            super::word_and_sentence_cards(&$factory);
        }
        #[test]
        fn subtitle_file_to_sentence_card() {
            super::subtitle_file_to_sentence_card(&$factory);
        }
        #[cfg(unix)]
        #[test]
        fn subtitle_file_to_sentence_card_with_clip() {
            super::subtitle_file_to_sentence_card_with_clip(&$factory);
        }
        #[cfg(unix)]
        #[test]
        fn exec_settings_are_user_only() {
            super::exec_settings_are_user_only(&$factory);
        }
        #[test]
        fn media_paths_are_user_only() {
            super::media_paths_are_user_only(&$factory);
        }
        #[test]
        fn queue_orders_due_texts_by_priority() {
            super::queue_orders_due_texts_by_priority(&$factory);
        }
        #[test]
        fn module_switch() {
            super::module_switch(&$factory);
        }
    };
}

mod mem {
    use crate::testing::MemStore;
    use fc_storage::Store;
    use std::sync::Arc;
    reading_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite {
    use crate::testing::TempSqlite;
    use fc_storage::Store;
    use std::sync::Arc;
    reading_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}
