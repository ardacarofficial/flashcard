//! Best scene: realistic film lines.

use super::*;
use std::collections::HashMap;

/// A tiny world: lemmas of some forms, the word index and the frequency pack.
struct World {
    lemmas: HashMap<&'static str, &'static str>,
    status: HashMap<&'static str, WordStatus>,
    ranks: HashMap<&'static str, u32>,
}

fn world() -> World {
    let lemmas = HashMap::from([
        ("picked", "pick"),
        ("picks", "pick"),
        ("picking", "pick"),
        ("scrounging", "scrounge"),
        ("dumpsters", "dumpster"),
        ("was", "be"),
        ("is", "be"),
        ("got", "get"),
        ("gonna", "gonna"),
        ("kids", "kid"),
    ]);
    let status = HashMap::from([
        ("kid", WordStatus::Known),
        ("school", WordStatus::Known),
        ("tomorrow", WordStatus::Known),
        ("leave", WordStatus::Learning),
        ("pick", WordStatus::Learning),
    ]);
    let ranks = HashMap::from([
        ("i", 5),
        ("the", 1),
        ("you", 3),
        ("up", 40),
        ("at", 20),
        ("pick", 600),
        ("kid", 700),
        ("school", 500),
        ("from", 30),
        ("will", 25),
        ("tomorrow", 900),
        ("be", 2),
        ("get", 60),
        ("gonna", 800),
        ("we", 10),
        ("have", 9),
        ("to", 4),
        ("leave", 400),
        ("now", 70),
        ("scrounge", 18_000),
        ("dumpster", 15_000),
        ("for", 12),
        ("food", 800),
        ("in", 6),
        ("eight", 900),
        ("o'clock", 2_500),
        ("if", 33),
        ("it", 8),
        ("me", 14),
        ("my", 16),
        ("and", 7),
        ("then", 90),
        ("he", 11),
        ("she", 13),
        ("where", 120),
        ("here", 110),
        ("are", 2),
        ("going", 300),
        ("can", 40),
        ("should", 140),
        ("call", 350),
        ("phone", 1_100),
        ("they", 15),
    ]);
    World {
        lemmas,
        status,
        ranks,
    }
}

impl World {
    fn scene(&self, id: &str, sentence: &str) -> Scene {
        let mut words = Vec::new();
        let mut start = None;
        let bytes: Vec<(usize, char)> = sentence.char_indices().collect();
        for (k, &(i, c)) in bytes.iter().enumerate() {
            let inner = c == '\''
                && start.is_some()
                && bytes.get(k + 1).is_some_and(|(_, n)| n.is_alphanumeric());
            if c.is_alphanumeric() || inner {
                start.get_or_insert(i);
            } else if let Some(s) = start.take() {
                words.push(self.word(sentence, s, i));
            }
        }
        if let Some(s) = start {
            words.push(self.word(sentence, s, sentence.len()));
        }
        Scene {
            id: id.into(),
            sentence: sentence.into(),
            words,
            ..Scene::default()
        }
    }

    fn word(&self, sentence: &str, s: usize, e: usize) -> SceneWord {
        let text = &sentence[s..e];
        let lower = text.to_lowercase();
        let lemma = self
            .lemmas
            .get(lower.as_str())
            .map_or(lower.clone(), |l| (*l).to_owned());
        SceneWord {
            start: s,
            end: e,
            text: text.into(),
            status: self
                .status
                .get(lemma.as_str())
                .copied()
                .unwrap_or(WordStatus::New),
            rank: self.ranks.get(lemma.as_str()).copied(),
            lemma,
        }
    }
}

fn opts(target: &str) -> ScoreOptions {
    ScoreOptions {
        language: "en".into(),
        target: target.into(),
        frequency_pack: true,
        basic_rank: DEFAULT_BASIC_RANK,
    }
}

fn codes(s: &SceneScore) -> Vec<SceneReasonCode> {
    s.reasons.iter().map(|r| r.code).collect()
}

#[test]
fn a_clean_line_beats_a_cut_one_with_rare_words() {
    let w = world();
    let good = w.scene("good", "I'll pick you up from school tomorrow.");
    let bad = w.scene("bad", "...scrounging in dumpsters, and then he picks —");
    let ranked = rank_scenes(&[bad, good], &opts("pick up"));
    assert_eq!(ranked[0].id, "good");
    let g = &ranked[0];
    assert!(g.target_found);
    assert!(codes(g).contains(&SceneReasonCode::NoOtherUnknown));
    assert!(codes(g).contains(&SceneReasonCode::GoodLength));
    assert!(codes(g).contains(&SceneReasonCode::CommonWords));
    let b = &ranked[1];
    // "picks" without "up": the expression does not occur.
    assert!(!b.target_found);
    assert!(codes(b).contains(&SceneReasonCode::TargetMissing));
    assert!(codes(b).contains(&SceneReasonCode::CutStart));
    assert!(codes(b).contains(&SceneReasonCode::DanglingDash));
    assert_eq!(b.unknown, vec!["scrounging", "dumpsters"]);
    assert_eq!(b.score, b.reasons.iter().map(|r| r.points).sum::<i32>());
}

#[test]
fn an_expression_may_be_split_by_its_object() {
    let w = world();
    let s = w.scene("a", "Can you pick the kids up at eight?");
    let r = score_scene(&s, &opts("pick up"));
    assert!(r.target_found, "{r:?}");
    // "pick" and "up" are the target, not learning/unknown words.
    assert!(!codes(&r).contains(&SceneReasonCode::LearningWords));
    let far = w.scene(
        "b",
        "Pick the phone and call them if they are going to be up.",
    );
    assert!(!score_scene(&far, &opts("pick up")).target_found);
}

#[test]
fn proper_names_do_not_count_as_unknown() {
    let w = world();
    // "Walter" (capitalised mid-sentence) and "Heisenberg" (not in the pack) are names.
    let s = w.scene("a", "I'll pick you up, Walter, and then Heisenberg.");
    let r = score_scene(&s, &opts("pick up"));
    assert!(r.unknown.is_empty(), "{:?}", r.unknown);
    let names = r
        .reasons
        .iter()
        .find(|x| x.code == SceneReasonCode::NamesIgnored)
        .expect("names reason");
    assert_eq!(names.words, vec!["Walter", "Heisenberg"]);
    assert_eq!(names.points, 0);
    // A capital at the start of a sentence is not a name: "Scrounging" is a real unknown word.
    let s = w.scene(
        "b",
        "Where are you going? Scrounging for food in dumpsters.",
    );
    let r = score_scene(&s, &opts("scrounge"));
    assert!(r.target_found);
    assert_eq!(r.unknown, vec!["dumpsters"]);
}

#[test]
fn without_a_frequency_pack_only_capitals_make_names() {
    let w = world();
    let mut o = opts("pick up");
    o.frequency_pack = false;
    let s = w.scene("a", "I'll pick you up, Walter, and then Heisenberg.");
    let r = score_scene(&s, &o);
    // Both capitalised mid-sentence: still names.
    assert!(r.unknown.is_empty());
    let s = w.scene("b", "I'll pick you up and then zorblax.");
    assert_eq!(score_scene(&s, &o).unknown, vec!["zorblax"]);
    // With the pack, the made-up word is taken for a name.
    assert!(score_scene(&s, &opts("pick up")).unknown.is_empty());
}

#[test]
fn completeness_short_long_and_two_speakers() {
    let w = world();
    let short = score_scene(&w.scene("a", "Pick it up!"), &opts("pick up"));
    assert!(codes(&short).contains(&SceneReasonCode::TooShort));
    let long = score_scene(
        &w.scene(
            "b",
            "I should call you and then you can pick me up at eight o'clock and then we have to leave now if we are going to get there before they leave.",
        ),
        &opts("pick up"),
    );
    assert!(codes(&long).contains(&SceneReasonCode::TooLong));
    let two = score_scene(
        &w.scene("c", "- Where are you going? - I'll pick you up."),
        &opts("pick up"),
    );
    assert!(codes(&two).contains(&SceneReasonCode::TwoSpeakers));
    let cut = score_scene(
        &w.scene("d", "and then I'll pick you up,"),
        &opts("pick up"),
    );
    assert!(codes(&cut).contains(&SceneReasonCode::CutStart));
    assert!(codes(&cut).contains(&SceneReasonCode::CutEnd));
    let open = score_scene(&w.scene("e", "I'll pick you up at eight"), &opts("pick up"));
    assert!(codes(&open).contains(&SceneReasonCode::NoEndPunctuation));
    let quoted = score_scene(
        &w.scene("f", "♪ I'll pick you up at eight. ♪"),
        &opts("pick up"),
    );
    assert!(!codes(&quoted).contains(&SceneReasonCode::NoEndPunctuation));
}

#[test]
fn media_adds_points_and_breaks_ties() {
    let w = world();
    let a = w.scene("a", "I'll pick you up at eight.");
    let mut b = w.scene("b", "We have to pick you up now.");
    b.has_audio = true;
    b.has_image = true;
    let ranked = rank_scenes(&[a, b], &opts("pick up"));
    assert_eq!(ranked[0].id, "b");
    assert_eq!(
        ranked[0].score - ranked[1].score,
        points::HAS_AUDIO + points::HAS_IMAGE
    );
}

/// v1.4 W4: the film's own sound beats a synthesised one (a TTS fill still beats none).
#[test]
fn original_audio_beats_synthesized_audio() {
    let w = world();
    let mut tts = w.scene("tts", "I'll pick you up at eight.");
    tts.has_audio = true;
    tts.audio_synthesized = true;
    let mut film = w.scene("film", "I'll pick you up at eight.");
    film.has_audio = true;
    let none = w.scene("none", "I'll pick you up at eight.");
    let s = |x| score_scene(x, &opts("pick up"));
    let (t, f, n) = (s(&tts), s(&film), s(&none));
    assert!(codes(&t).contains(&SceneReasonCode::SynthesizedAudio));
    assert!(!codes(&t).contains(&SceneReasonCode::HasAudio));
    assert_eq!(
        f.score - t.score,
        points::HAS_AUDIO - points::SYNTHESIZED_AUDIO
    );
    assert_eq!(t.score - n.score, points::SYNTHESIZED_AUDIO);
    let ranked = rank_scenes(&[tts, film], &opts("pick up"));
    assert_eq!(ranked[0].id, "film");
}

#[test]
fn learning_words_count_a_little() {
    let w = world();
    let r = score_scene(
        &w.scene("a", "We have to leave now, pick me up."),
        &opts("pick up"),
    );
    let l = r
        .reasons
        .iter()
        .find(|x| x.code == SceneReasonCode::LearningWords)
        .unwrap();
    assert_eq!(l.words, vec!["leave"]);
    assert_eq!(l.points, points::PER_LEARNING);
}

#[test]
fn a_stale_offset_is_checked_not_trusted() {
    let w = world();
    let mut s = w.scene("a", "Where are you going now?");
    s.surface = Some("picked".into());
    s.target = Some((0, 5));
    assert!(!score_scene(&s, &opts("pick")).target_found);
    let mut s = w.scene("b", "She picked me up.");
    s.surface = Some("picked".into());
    s.target = Some((4, 10));
    assert!(score_scene(&s, &opts("pick")).target_found);
}

#[test]
fn near_duplicates_are_reported_both_ways() {
    let w = world();
    let scenes = [
        w.scene("a", "I'll pick you up at eight."),
        w.scene("b", "I'll pick you up at eight o'clock, and then—"),
        w.scene("c", "i'll PICK you up... at eight!"),
        w.scene("d", "We have to leave now."),
    ];
    let ranked = rank_scenes(&scenes, &opts("pick up"));
    let of = |id: &str| ranked.iter().find(|r| r.id == id).unwrap();
    let mut a = of("a").duplicates.clone();
    a.sort();
    assert_eq!(a, vec!["b", "c"]);
    assert!(of("b").duplicates.contains(&"a".to_owned()));
    assert!(of("c").duplicates.contains(&"b".to_owned()));
    assert!(of("d").duplicates.is_empty());
    assert!(near_duplicate("Pick it up.", "pick it up"));
    assert!(!near_duplicate("Pick it up.", "Put it down."));
    // Two words in common are not enough for containment.
    assert!(!near_duplicate(
        "Pick up",
        "Pick up the phone and call me now."
    ));
}

#[test]
fn ranking_is_stable_for_equal_scores() {
    let w = world();
    let scenes = [
        w.scene("first", "I'll pick you up at eight."),
        w.scene("second", "I'll pick you up at eight."),
    ];
    let ranked = rank_scenes(&scenes, &opts("pick up"));
    assert_eq!(ranked[0].id, "first");
    assert_eq!(ranked[0].duplicates, vec!["second"]);
}
