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
    tx.commit().unwrap();
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
    set_interval(&t, &run, i64::from(MATURE_DAYS) - 1);
    assert_eq!(status_of(&analyze(&t, text, "en"), "ran"), WordStatus::Learning);
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
    let text = "Evlerimizden çıktık. Kitapları masaya koyduk.";
    let a = analyze(&t, text, "tr-TR");
    assert_eq!(a.lemmatizer, "builtin.tr");
    word_card(&t, d, "ev", "house");
    let a = analyze(&t, text, "tr-TR");
    assert_eq!(status_of(&a, "Evlerimizden"), WordStatus::Learning);
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
    // Missing program.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: super::FFMPEG_PATH.into(),
        value: serde_json::json!(dir.path().join("nope").display().to_string()),
    });
    let missing = job(
        &t,
        &t.ok::<ReadingClipsExtract>(ExtractClip {
            item_id: item.id,
            cue_index: 0,
            image: false,
        }),
    );
    assert_eq!(missing.error.unwrap().details["reason"], "missing");
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
