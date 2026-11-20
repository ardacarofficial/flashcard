use super::*;

fn ev(state: CardState, interval: Option<u32>) -> CardEvidence {
    CardEvidence {
        state,
        interval_days: interval,
    }
}

#[test]
fn status_follows_fsrs_maturity_then_marks() {
    let mature = ev(CardState::Review, Some(MATURE_DAYS));
    let young = ev(CardState::Review, Some(MATURE_DAYS - 1));
    let new = ev(CardState::New, None);
    let relearning = ev(CardState::Relearning, Some(40));
    assert_eq!(word_status(&[mature], None), WordStatus::Known);
    assert_eq!(word_status(&[young, mature], None), WordStatus::Known);
    assert_eq!(word_status(&[young], None), WordStatus::Learning);
    assert_eq!(word_status(&[new], None), WordStatus::Learning);
    // A lapsed card is not mature even with a long interval on record.
    assert_eq!(word_status(&[relearning], None), WordStatus::Learning);
    // Cards win over marks: FSRS data is the source of truth.
    assert_eq!(
        word_status(&[young], Some(WordMark::Known)),
        WordStatus::Learning
    );
    assert_eq!(
        word_status(&[mature], Some(WordMark::Ignored)),
        WordStatus::Known
    );
    assert_eq!(word_status(&[], Some(WordMark::Known)), WordStatus::Known);
    assert_eq!(
        word_status(&[], Some(WordMark::Ignored)),
        WordStatus::Ignored
    );
    assert_eq!(word_status(&[], None), WordStatus::New);
    assert_eq!(WordMark::parse("known"), Some(WordMark::Known));
    assert_eq!(
        WordMark::parse(WordMark::Ignored.as_str()),
        Some(WordMark::Ignored)
    );
    assert_eq!(WordMark::parse("new"), None);
}

#[test]
fn summary_counts_running_and_unique_words() {
    use WordStatus::*;
    let words = [
        ("the", Known),
        ("cat", New),
        ("the", Known),
        ("run", Learning),
        ("john", Ignored),
        ("cat", New),
    ];
    let s = summarize(words);
    assert_eq!(
        s.running,
        WordCounts {
            new: 2,
            learning: 1,
            known: 2,
            ignored: 1
        }
    );
    assert_eq!(
        s.unique,
        WordCounts {
            new: 1,
            learning: 1,
            known: 1,
            ignored: 1
        }
    );
    // 2 known of 5 countable running words.
    assert!((s.known_pct - 40.0).abs() < 1e-9);
    assert!((summarize([("a", Known), ("b", Known), ("c", New)]).known_pct - 66.7).abs() < 1e-9);
    assert_eq!(summarize([("x", Ignored)]).known_pct, 0.0);
    assert_eq!(
        summarize(Vec::<(&str, WordStatus)>::new()),
        TextSummary::default()
    );
}

#[test]
fn headwords_are_cleaned_and_keyed() {
    assert_eq!(clean_headword("run (v.)"), "run");
    assert_eq!(clean_headword(" run, ran, run "), "run");
    assert_eq!(clean_headword("give up / quit"), "give up");
    assert_eq!(clean_headword("ev - house"), "ev");
    assert_eq!(clean_headword("kitap"), "kitap");
    let k = |ls: &[&str], lang| {
        headword_key(
            &ls.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
            lang,
        )
    };
    assert_eq!(k(&["to", "run"], "en-US").as_deref(), Some("run"));
    assert_eq!(k(&["to", "run"], "tr").as_deref(), Some("to run"));
    assert_eq!(k(&["give", "up"], "en").as_deref(), Some("give up"));
    assert_eq!(k(&[], "en"), None);
    assert_eq!(k(&["a", "b", "c", "d", "e"], "en"), None);
}

#[test]
fn sentences_split_on_terminators_and_lines() {
    let text = "Hello there! Mr. smith paid 3.5 dollars. \"Really?\" she asked…\nNew line here\n\n  Last one";
    let got: Vec<&str> = sentence_spans(text)
        .into_iter()
        .map(|(s, e)| &text[s..e])
        .collect();
    assert_eq!(
        got,
        [
            "Hello there!",
            "Mr. smith paid 3.5 dollars.",
            "\"Really?\" she asked…",
            "New line here",
            "Last one"
        ]
    );
    let turkish = "Kediler koşuyor. İstanbul'a gittik! Çok güzeldi.";
    assert_eq!(sentence_spans(turkish).len(), 3);
    let at = text.find("paid").unwrap();
    let (s, e) = sentence_at(text, at).unwrap();
    assert_eq!(&text[s..e], "Mr. smith paid 3.5 dollars.");
    // Between sentences → the next one.
    let gap = text.find(" Mr.").unwrap();
    assert_eq!(&text[sentence_at(text, gap).unwrap().0..][..3], "Mr.");
    assert_eq!(sentence_spans("   \n "), vec![]);
    assert_eq!(sentence_at("", 0), None);
}

#[test]
fn reading_intervals_grow_by_priority_and_cap() {
    use ReadingPriority::*;
    assert_eq!(next_reading_interval(None, Normal), 1);
    assert_eq!(next_reading_interval(None, Low), 2);
    assert_eq!(next_reading_interval(Some(0), High), 1);
    assert_eq!(next_reading_interval(Some(1), High), 2);
    assert_eq!(next_reading_interval(Some(4), High), 6);
    assert_eq!(next_reading_interval(Some(3), High), 5);
    assert_eq!(next_reading_interval(Some(4), Normal), 8);
    assert_eq!(next_reading_interval(Some(4), Low), 12);
    assert_eq!(
        next_reading_interval(Some(300), Low),
        MAX_READING_INTERVAL_DAYS
    );
    assert_eq!(
        next_reading_interval(Some(u32::MAX), Normal),
        MAX_READING_INTERVAL_DAYS
    );
    for p in [High, Normal, Low] {
        assert_eq!(ReadingPriority::parse(p.as_str()), Some(p));
    }
}

#[test]
fn cloze_from_spans_numbers_and_escapes() {
    let text = "Ankara is the capital of <Türkiye>.";
    let a = text.find("Ankara").unwrap();
    let t = text.find("<Türkiye>").unwrap();
    assert_eq!(
        cloze_from_spans(text, &[(t, t + "<Türkiye>".len()), (a, a + 6)], false).unwrap(),
        "{{c1::Ankara}} is the capital of {{c2::&lt;Türkiye&gt;}}."
    );
    assert_eq!(
        cloze_from_spans(text, &[(a, a + 6), (t, t + 2)], true).unwrap(),
        "{{c1::Ankara}} is the capital of {{c1::&lt;T}}ürkiye&gt;."
    );
    assert_eq!(
        cloze_from_spans(text, &[], false),
        Err(ClozeSpanError::Empty)
    );
    assert_eq!(
        cloze_from_spans(text, &[(0, 6), (3, 9)], false),
        Err(ClozeSpanError::Overlap)
    );
    assert_eq!(
        cloze_from_spans(text, &[(5, 5)], false),
        Err(ClozeSpanError::OutOfRange(5, 5))
    );
    // Inside the two-byte `ü`.
    let u = text.find('ü').unwrap();
    assert!(matches!(
        cloze_from_spans(text, &[(u + 1, u + 2)], false),
        Err(ClozeSpanError::OutOfRange(..))
    ));
    assert_eq!(
        cloze_from_spans("a {{b}} c", &[(2, 7)], false),
        Err(ClozeSpanError::Syntax)
    );
}

#[test]
fn emphasis_and_timestamps() {
    assert_eq!(emphasize("I <3 cats", 5, 9), "I &lt;3 <b>cats</b>");
    assert_eq!(emphasize("cats", 2, 99), "cats");
    assert_eq!(format_timestamp(0), "0:00");
    assert_eq!(format_timestamp(62_345), "1:02");
    assert_eq!(format_timestamp(3_723_000), "1:02:03");
}
