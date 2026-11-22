use super::eval::{matches, AttrFact, CardFacts};
use super::*;
use crate::attribute::{
    AttributeDef, AttributeKind, AttributeOption, AttributeTarget, AttributeValue,
};
use crate::card::{Card, CardSpec, CardState};
use crate::deck::Deck;
use crate::id::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, TagId};
use crate::note::Note;
use crate::notetype::builtin;
use crate::tag::{Tag, TagName};
use crate::time::{Timestamp, MILLIS_PER_DAY};
use proptest::prelude::*;
use uuid::Uuid;

const NOW: Timestamp = Timestamp(1_700_000_000_000);

fn u(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

// ------------------------------------------------------------------ parser

fn span_of(q: &str) -> (u32, u32, &'static str) {
    let e = parse_query(q).unwrap_err();
    (e.span.start, e.span.end, e.kind.code())
}

#[test]
fn parses_terms_ops_and_structure() {
    let e =
        parse_query(r#"deck:"Dil::İngilizce" tag:fiil -tag:kolay (a OR b) seviye>=B1"#).unwrap();
    let Expr::And(items) = &e else {
        panic!("{e:?}")
    };
    assert_eq!(items.len(), 5);
    let Expr::Term(t) = &items[0] else { panic!() };
    assert_eq!(t.key.as_deref(), Some("deck"));
    assert_eq!(t.value, "Dil::İngilizce");
    assert!(t.quoted);
    assert!(matches!(&items[2], Expr::Not(_)));
    assert!(matches!(&items[3], Expr::Or(v) if v.len() == 2));
    let Expr::Term(t) = &items[4] else { panic!() };
    assert_eq!(
        (t.key.as_deref(), t.op, t.value.as_str()),
        (Some("seviye"), CmpOp::Ge, "B1")
    );
    assert_eq!(parse_query("  ").unwrap(), Expr::And(vec![]));
    assert_eq!(
        parse_query("a AND b").unwrap().without_spans(),
        parse_query("a b").unwrap().without_spans()
    );
    assert_eq!(
        parse_query("NOT a").unwrap().without_spans(),
        parse_query("-a").unwrap().without_spans()
    );
    // OR binds looser than juxtaposition.
    assert!(
        matches!(parse_query("a b OR c").unwrap(), Expr::Or(v) if matches!(v[0], Expr::And(_)))
    );
    // Unquoted values keep `::` and other operators.
    let Expr::Term(t) = parse_query("deck:Dil::İngilizce").unwrap() else {
        panic!()
    };
    assert_eq!(t.value, "Dil::İngilizce");
    let Expr::Term(t) = parse_query(r#""a \"b\" \\""#).unwrap() else {
        panic!()
    };
    assert_eq!(t.value, r#"a "b" \"#);
}

#[test]
fn errors_are_positioned_in_characters() {
    assert_eq!(span_of(r#"ışık "açık"#), (5, 10, "unclosed_quote"));
    assert_eq!(span_of("a (b c"), (2, 3, "unclosed_paren"));
    assert_eq!(span_of("a b)"), (3, 4, "unmatched_paren"));
    assert_eq!(span_of("a OR"), (4, 4, "expected_term"));
    assert_eq!(span_of("OR a"), (0, 2, "expected_term"));
    assert_eq!(span_of("()"), (1, 2, "expected_term"));
    assert_eq!(span_of("çok deck: x"), (4, 9, "empty_value"));
    assert_eq!(span_of("- a"), (0, 1, "expected_term"));
}

// ------------------------------------------------------------------ fixtures

struct World {
    decks: Vec<Deck>,
    tags: Vec<Tag>,
    notetypes: Vec<crate::notetype::NoteType>,
    attrs: Vec<AttributeDef>,
}

fn deck(id: u128, parent: Option<u128>, name: &str) -> Deck {
    Deck {
        id: DeckId(u(id)),
        parent_id: parent.map(|p| DeckId(u(p))),
        name: name.into(),
        description: String::new(),
        icon: None,
        color: None,
        preset_id: None,
        position: 0,
        created_at: NOW,
        updated_at: NOW,
        deleted_at: None,
    }
}

fn opt(id: u128, label: &str, order: i32) -> AttributeOption {
    AttributeOption {
        id: AttributeOptionId(u(id)),
        label: label.into(),
        color: None,
        order,
    }
}

fn def(
    id: u128,
    key: &str,
    name: &str,
    kind: AttributeKind,
    options: Vec<AttributeOption>,
) -> AttributeDef {
    AttributeDef {
        id: AttributeDefId(u(id)),
        scope_deck_id: None,
        key: key.into(),
        name: name.into(),
        kind,
        options,
        target: AttributeTarget::Note,
        required: false,
        default_value: None,
        position: 0,
        template_id: None,
        created_at: NOW,
        updated_at: NOW,
        deleted_at: None,
    }
}

const LEVEL: u128 = 500;
const EXAM: u128 = 501;
const SUBJECT: u128 = 502;
const SCORE: u128 = 503;
const DATE: u128 = 504;
const HARD: u128 = 505;
const SOURCE: u128 = 506;

fn world() -> World {
    let levels = ["A1", "A2", "B1", "B2", "C1", "C2"]
        .iter()
        .enumerate()
        .map(|(i, l)| opt(600 + i as u128, l, i as i32))
        .collect();
    let mut level = def(LEVEL, "level", "Seviye", AttributeKind::OrderedEnum, levels);
    level.scope_deck_id = Some(DeckId(u(2)));
    World {
        decks: vec![
            deck(1, None, "Dil"),
            deck(2, Some(1), "İngilizce"),
            deck(3, None, "YKS"),
        ],
        tags: vec![
            Tag {
                id: TagId(u(10)),
                name: TagName::parse("fiil").unwrap(),
                color: None,
                created_at: NOW,
                updated_at: NOW,
            },
            Tag {
                id: TagId(u(11)),
                name: TagName::parse("fiil::düzensiz").unwrap(),
                color: None,
                created_at: NOW,
                updated_at: NOW,
            },
        ],
        notetypes: vec![crate::notetype::builtin::classic(
            NOW, "Klasik", "Ön", "Arka",
        )],
        attrs: vec![
            level,
            def(
                EXAM,
                "sinav",
                "Sınav",
                AttributeKind::Enum,
                vec![opt(700, "TYT", 0), opt(701, "AYT", 1)],
            ),
            def(
                SUBJECT,
                "ders",
                "Ders",
                AttributeKind::MultiEnum,
                vec![opt(800, "Fizik", 0), opt(801, "Kimya", 1)],
            ),
            def(SCORE, "puan", "Puan", AttributeKind::Number, vec![]),
            def(DATE, "tarih", "Sınav tarihi", AttributeKind::Date, vec![]),
            def(HARD, "zor", "Zor", AttributeKind::Bool, vec![]),
            def(SOURCE, "kaynak", "Kaynak", AttributeKind::Text, vec![]),
        ],
    }
}

impl World {
    fn cat(&self) -> Catalog<'_> {
        Catalog {
            decks: &self.decks,
            tags: &self.tags,
            notetypes: &self.notetypes,
            attributes: &self.attrs,
            flag_names: &[],
            now: NOW,
            day_start: None,
        }
    }

    fn compile(&self, q: &str) -> Result<Filter, QueryError> {
        compile_query(q, &self.cat())
    }
}

struct Sample {
    card: Card,
    note: Note,
    tags: Vec<TagId>,
    values: Vec<(AttributeDefId, AttributeValue)>,
    text: String,
}

fn sample(n: u128, deck_id: u128, front: &str) -> Sample {
    let note = Note {
        id: NoteId(u(1000 + n)),
        notetype_id: builtin::CLASSIC_ID,
        home_deck_id: DeckId(u(deck_id)),
        fields: [
            (crate::notetype::FieldKey::new("front"), front.to_owned()),
            (crate::notetype::FieldKey::new("back"), String::new()),
        ]
        .into_iter()
        .collect(),
        sort_field: front.into(),
        checksum: String::new(),
        tags: vec![],
        created_at: NOW,
        updated_at: NOW,
        deleted_at: None,
    };
    let card = Card::new(
        CardId(u(2000 + n)),
        note.id,
        DeckId(u(deck_id)),
        CardSpec {
            template_key: "forward".into(),
            card_type: "classic".into(),
        },
        Timestamp(NOW.0 - 10 * MILLIS_PER_DAY),
    );
    Sample {
        card,
        note,
        tags: vec![],
        values: vec![],
        text: front.to_owned(),
    }
}

impl Sample {
    fn hits(&self, f: &Filter) -> bool {
        let stored: Vec<(AttributeDefId, crate::attribute::StoredValue)> = self
            .values
            .iter()
            .flat_map(|(id, v)| v.to_stored().into_iter().map(move |s| (*id, s)))
            .collect();
        let facts: Vec<AttrFact<'_>> = stored
            .iter()
            .map(|(id, s)| AttrFact {
                attr_id: *id,
                target: AttributeTarget::Note,
                value: &s.value,
                value_num: s.value_num,
            })
            .collect();
        matches(
            f,
            &CardFacts {
                card: &self.card,
                note: &self.note,
                tag_ids: &self.tags,
                attrs: &facts,
                search_text: &self.text,
            },
            NOW,
        )
    }
}

fn choice(id: u128) -> AttributeValue {
    AttributeValue::Choice(AttributeOptionId(u(id)))
}

// ------------------------------------------------------------------ resolution + evaluation

#[test]
fn classifications_by_key_or_name_with_order_comparison() {
    let w = world();
    let mut b1 = sample(1, 2, "apple");
    b1.values = vec![(AttributeDefId(u(LEVEL)), choice(602))];
    let mut c1 = sample(2, 2, "pear");
    c1.values = vec![(AttributeDefId(u(LEVEL)), choice(604))];
    let none = sample(3, 2, "fig");

    let q = |s: &str| w.compile(s).unwrap();
    for (query, expect) in [
        ("seviye:B1", [true, false, false]),
        ("level:b1", [true, false, false]),
        ("SEVİYE:B1", [true, false, false]),
        ("seviye>=B1", [true, true, false]),
        ("seviye>B1", [false, true, false]),
        ("seviye<=B2", [true, false, false]),
        ("seviye<A1", [false, false, false]),
        ("seviye!=B1", [false, true, true]),
        ("seviye:*", [true, true, false]),
        ("-seviye:*", [false, false, true]),
        ("seviye:B*", [true, false, false]),
        ("seviye:B1 OR seviye:C1", [true, true, false]),
    ] {
        let f = q(query);
        assert_eq!(
            [b1.hits(&f), c1.hits(&f), none.hits(&f)],
            expect,
            "{query}: {f:?}"
        );
    }
}

#[test]
fn stored_queries_follow_option_renames_and_reorders() {
    use super::canonical::{attr_key, canonicalize, render};
    let mut w = world();
    let mut b1 = sample(1, 2, "apple");
    b1.values = vec![(AttributeDefId(u(LEVEL)), choice(602))];
    let mut c1 = sample(2, 2, "pear");
    c1.values = vec![(AttributeDefId(u(LEVEL)), choice(604))];
    let mut a2 = sample(3, 2, "fig");
    a2.values = vec![(AttributeDefId(u(LEVEL)), choice(601))];
    let hits = |w: &World, q: &str| {
        let f = w.compile(q).unwrap_or_else(|e| panic!("{q}: {e}"));
        [b1.hits(&f), c1.hits(&f), a2.hits(&f)]
    };

    let typed = "seviye>=B1  -sınav:\"TYT\" deck:Dil";
    let stored = canonicalize(typed, &w.cat());
    let level_key = attr_key(AttributeDefId(u(LEVEL)));
    let b1_id = AttributeOptionId(u(602)).to_string();
    assert_eq!(
        stored,
        format!(
            "{level_key}>={b1_id}  -{}:{} deck:Dil",
            attr_key(AttributeDefId(u(EXAM))),
            AttributeOptionId(u(700))
        )
    );
    // Idempotent, executable as is, and rendered back to what was typed (modulo key spelling).
    assert_eq!(canonicalize(&stored, &w.cat()), stored);
    assert_eq!(hits(&w, &stored), hits(&w, typed));
    assert_eq!(hits(&w, &stored), [true, true, false]);
    assert_eq!(render(&stored, &w.cat()), "level>=B1  -sinav:TYT deck:Dil");

    // Rename B1 → "Orta seviye" and the classification key: the stored query still works and
    // renders with the new names (quoted where needed); the old text no longer resolves.
    let level = &mut w.attrs[0];
    level.key = "cefr".into();
    level.options[2].label = "Orta seviye".into();
    assert_eq!(hits(&w, &stored), [true, true, false]);
    assert_eq!(
        render(&stored, &w.cat()),
        "cefr>=\"Orta seviye\"  -sinav:TYT deck:Dil"
    );
    assert!(w.compile("level>=B1").is_err());

    // Reorder: B1 moves below A2 → `>= B1` now also takes A2 (follows the current order).
    let level = &mut w.attrs[0];
    level.options[2].order = 0;
    level.options[0].order = 2;
    level.options[1].order = 1;
    assert_eq!(hits(&w, &stored), [true, true, true]);
    let shown = render(&stored, &w.cat());
    assert_eq!(hits(&w, &shown), hits(&w, &stored));

    // Unparsable text and non-classification terms are untouched.
    assert_eq!(render("seviye:(", &w.cat()), "seviye:(");
    assert_eq!(canonicalize("tag:fiil kilic", &w.cat()), "tag:fiil kilic");
}

#[test]
fn enum_multi_number_date_bool_text_classifications() {
    let w = world();
    let mut s = sample(1, 3, "kuvvet");
    s.values = vec![
        (AttributeDefId(u(EXAM)), choice(700)),
        (
            AttributeDefId(u(SUBJECT)),
            AttributeValue::Choices(vec![AttributeOptionId(u(800)), AttributeOptionId(u(801))]),
        ),
        (AttributeDefId(u(SCORE)), AttributeValue::Number(3.5)),
        (
            AttributeDefId(u(DATE)),
            AttributeValue::Date("2025-06-15".into()),
        ),
        (AttributeDefId(u(HARD)), AttributeValue::Bool(true)),
        (
            AttributeDefId(u(SOURCE)),
            AttributeValue::Text("Palme Yayınları".into()),
        ),
    ];
    for (q, expect) in [
        ("sınav:TYT ders:fizik", true),
        ("sinav:ayt", false),
        ("ders:kimya", true),
        ("puan>3", true),
        ("puan>=3,5", true),
        ("puan<3", false),
        ("puan=3.5", true),
        ("sinav_tarihi<2025-07-01", true),
        ("tarih>2025-07-01", false),
        ("zor:evet", true),
        ("zor:no", false),
        ("kaynak:palme*", true),
        ("kaynak:yayin*", false),
    ] {
        let f = w.compile(q).unwrap();
        assert_eq!(s.hits(&f), expect, "{q}: {f:?}");
    }
}

#[test]
fn classification_errors_are_positioned() {
    let w = world();
    let e = w.compile("deck:x seviye:B7").unwrap_err();
    assert_eq!(
        (e.kind.code(), e.span.start, e.span.end),
        ("unknown_option", 14, 16)
    );
    let e = w.compile("sınav>=TYT").unwrap_err();
    assert_eq!(
        (e.kind.code(), e.span.start, e.span.end),
        ("unsupported_operator", 5, 7)
    );
    let e = w.compile("puan>abc").unwrap_err();
    assert_eq!(e.kind.code(), "invalid_number");
    let e = w.compile("tarih:2025-13-01").unwrap_err();
    assert_eq!(e.kind.code(), "invalid_date");
    let e = w.compile("zor:belki").unwrap_err();
    assert_eq!(e.kind.code(), "invalid_bool");
    let e = w.compile("bilinmeyen:x").unwrap_err();
    assert_eq!(
        (e.kind.code(), e.span.start, e.span.end),
        ("unknown_key", 0, 10)
    );
    assert_eq!(
        w.compile("is:nope").unwrap_err().kind.code(),
        "unknown_state"
    );
    assert_eq!(w.compile("flag:9").unwrap_err().kind.code(), "invalid_flag");
    assert_eq!(
        w.compile(r#"re:"(""#).unwrap_err().kind.code(),
        "invalid_regex"
    );
    assert_eq!(
        w.compile("deck>x").unwrap_err().kind.code(),
        "unsupported_operator"
    );
    assert_eq!(w.compile("nid:zzz").unwrap_err().kind.code(), "invalid_id");
    // Wildcard naming no option matches nothing instead of failing.
    assert_eq!(w.compile("seviye:Z*").unwrap(), Filter::Nothing);
}

#[test]
fn decks_tags_text_and_fields() {
    let w = world();
    let mut en = sample(1, 2, "<b>Apple</b> pie");
    en.tags = vec![TagId(u(11))];
    let mut yks = sample(2, 3, "Kılıç kalkan");
    yks.text = "Kılıç kalkan".into();
    let cases = [
        ("deck:Dil", [true, false]),
        ("deck:İngilizce", [true, false]),
        (r#"deck:"dil::ingilizce""#, [true, false]),
        ("deck:Dil::*", [true, false]),
        ("deck:yok", [false, false]),
        ("deste:yks", [false, true]),
        ("tag:fiil", [true, false]),
        ("tag:fiil::düzensiz", [true, false]),
        ("tag:none", [false, true]),
        ("etiket:fi*", [true, false]),
        ("app", [true, false]),
        ("kilic", [false, true]),
        ("KILIÇ", [false, true]),
        (r#""kılıç kalkan""#, [false, true]),
        (r#""kalkan kılıç""#, [false, false]),
        ("a*le", [true, false]),
        ("front:apple*", [true, false]),
        ("front:apple", [false, false]),
        ("re:^kı", [false, true]),
        ("front:re:pie$", [true, false]),
        ("notetype:classic OR notetype:Klasik", [true, true]),
        ("type:classic card:forward", [true, true]),
        ("-deck:Dil", [false, true]),
        ("deck!=Dil", [false, true]),
    ];
    for (q, expect) in cases {
        let f = w.compile(q).unwrap();
        assert_eq!([en.hits(&f), yks.hits(&f)], expect, "{q}: {f:?}");
    }
}

#[test]
fn scheduling_predicates() {
    let w = world();
    let mut new = sample(1, 2, "a");
    let mut due = sample(2, 2, "b");
    due.card.scheduling.state = CardState::Review;
    due.card.scheduling.due = Some(Timestamp(NOW.0 - 1));
    due.card.scheduling.stability = Some(10.0);
    due.card.scheduling.difficulty = Some(7.5);
    due.card.scheduling.lapses = 5;
    due.card.scheduling.reps = 2;
    due.card.scheduling.last_review = Some(Timestamp(NOW.0 - 10 * MILLIS_PER_DAY));
    due.card.flags = 0b10;
    new.card.suspended = true;
    new.card.buried_until = Some(Timestamp(NOW.0 + 1));
    let cases = [
        ("is:new", [true, false]),
        ("is:review", [false, true]),
        ("is:due", [false, true]),
        ("is:suspended", [true, false]),
        ("is:buried", [true, false]),
        // v1.3 aliases in the app's words (set aside / leave for tomorrow).
        ("is:aside", [true, false]),
        ("is:kenarda", [true, false]),
        ("is:tomorrow", [true, false]),
        ("is:yarına", [true, false]),
        ("is:YARINA", [true, false]),
        ("is:flagged", [false, true]),
        ("flag:orange", [false, true]),
        ("flag:turuncu", [false, true]),
        ("flag:1", [false, false]),
        ("flag:0", [true, false]),
        ("stability>5", [false, true]),
        ("stability<5", [false, false]),
        ("stability!=5", [true, true]),
        ("difficulty>7", [false, true]),
        ("lapses>=5", [false, true]),
        ("reps<3", [true, true]),
        // R(10d, S=10) = (1 + 19/81)^-0.5 ≈ 0.9.
        ("retrievability<0.95", [false, true]),
        ("retrievability<0.85", [false, false]),
        ("retrievability>0", [false, true]),
        ("added:7", [false, false]),
        ("added:11", [true, true]),
        ("reviewed:11", [false, true]),
        ("reviewed:9", [false, false]),
        ("due:0", [false, true]),
        ("edited:1", [true, true]),
    ];
    for (q, expect) in cases {
        let f = w.compile(q).unwrap();
        assert_eq!([new.hits(&f), due.hits(&f)], expect, "{q}: {f:?}");
    }
    let f = w
        .compile(&format!("cid:{} OR nid:{}", new.card.id, due.note.id))
        .unwrap();
    assert!(new.hits(&f) && due.hits(&f));
}

#[test]
fn saved_query_names_are_normalized() {
    assert_eq!(
        normalize_saved_query_name("  Zor   fiillerim "),
        Some("Zor fiillerim".into())
    );
    assert_eq!(normalize_saved_query_name("  "), None);
    assert_eq!(normalize_saved_query_name(&"x".repeat(101)), None);
}

// ------------------------------------------------------------------ property tests

fn arb_word() -> impl Strategy<Value = String> {
    "[a-zçğıöşü0-9*.]{1,8}".prop_filter("keyword", |w| !matches!(w.as_str(), "or" | "and" | "not"))
}

fn arb_key() -> impl Strategy<Value = String> {
    "[a-zçğıöşü_][a-zçğıöşü_0-9]{0,6}"
}

fn arb_op() -> impl Strategy<Value = CmpOp> {
    prop_oneof![
        Just(CmpOp::Colon),
        Just(CmpOp::Eq),
        Just(CmpOp::Ne),
        Just(CmpOp::Lt),
        Just(CmpOp::Le),
        Just(CmpOp::Gt),
        Just(CmpOp::Ge),
    ]
}

fn term(key: Option<String>, op: CmpOp, value: String, quoted: bool) -> Expr {
    Expr::Term(Term {
        key,
        op,
        value,
        quoted,
        span: Span::default(),
        key_span: Span::default(),
        value_span: Span::default(),
    })
}

fn arb_term() -> impl Strategy<Value = Expr> {
    prop_oneof![
        arb_word().prop_map(|w| term(None, CmpOp::Colon, w, false)),
        "\\PC{0,10}".prop_map(|w| term(None, CmpOp::Colon, w, true)),
        (arb_key(), arb_op(), arb_word()).prop_map(|(k, o, v)| term(Some(k), o, v, false)),
        (arb_key(), arb_op(), "\\PC{0,10}").prop_map(|(k, o, v)| term(Some(k), o, v, true)),
    ]
}

fn arb_expr() -> impl Strategy<Value = Expr> {
    arb_term().prop_recursive(4, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 2..4).prop_map(Expr::And),
            prop::collection::vec(inner.clone(), 2..4).prop_map(Expr::Or),
            inner.prop_map(|e| Expr::Not(Box::new(e))),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// print → parse is the identity on syntax trees.
    #[test]
    fn print_parse_roundtrip(e in arb_expr()) {
        let text = e.to_string();
        let parsed = parse_query(&text).map_err(|err| TestCaseError::fail(format!("{text:?}: {err}")))?;
        prop_assert_eq!(parsed.without_spans(), e, "text: {:?}", text);
    }

    /// The parser never panics; errors point inside the input; a successful parse is stable
    /// under print → parse.
    #[test]
    fn parser_is_total_and_positions_are_in_bounds(s in "\\PC{0,40}") {
        let len = u32::try_from(s.chars().count()).unwrap();
        match parse_query(&s) {
            Ok(e) => {
                let again = parse_query(&e.to_string()).unwrap();
                prop_assert_eq!(again.without_spans(), e.without_spans());
            }
            Err(err) => prop_assert!(err.span.start <= err.span.end && err.span.end <= len, "{err}"),
        }
    }

    /// Same, over a query-shaped alphabet that hits the operators and parens often.
    #[test]
    fn query_alphabet_fuzz(s in "[a-c :<>=!()\"\\-*]{0,30}") {
        let len = u32::try_from(s.chars().count()).unwrap();
        if let Err(err) = parse_query(&s) {
            prop_assert!(err.span.end <= len);
        }
    }

    /// Term spans cover exactly the text they were parsed from.
    #[test]
    fn term_spans_slice_the_source(e in arb_term()) {
        let text = e.to_string();
        let Expr::Term(t) = parse_query(&text).unwrap() else { unreachable!() };
        let chars: Vec<char> = text.chars().collect();
        let slice = |s: Span| chars[s.start as usize..s.end as usize].iter().collect::<String>();
        prop_assert_eq!(slice(t.span), text.clone());
        if let Some(k) = &t.key {
            prop_assert_eq!(&slice(t.key_span), k);
        }
    }

    /// `-q` matches exactly the cards `q` does not, and `q OR -q` matches everything.
    #[test]
    fn negation_is_complement(word in "[a-z]{1,4}", text in "[a-z ]{0,20}", lapses in 0u32..8) {
        // A bare `or` / `and` / `not` is an operator, not a word.
        prop_assume!(!["or", "and", "not"].contains(&word.as_str()));
        let w = world();
        let mut s = sample(1, 2, &text);
        s.card.scheduling.lapses = lapses;
        for q in [word.clone(), format!("lapses>{}", lapses % 5), "seviye:*".to_owned(), format!("deck:{word}")] {
            let f = w.compile(&q).unwrap();
            let nf = w.compile(&format!("-({q})")).unwrap();
            let either = w.compile(&format!("({q}) OR -({q})")).unwrap();
            prop_assert_eq!(s.hits(&f), !s.hits(&nf));
            prop_assert!(s.hits(&either));
        }
    }
}

/// v1.3: with the study day's start in the catalog, `reviewed:N` (and `added`/`edited`)
/// count study days — `reviewed:1` = since today began — not rolling 24 h periods.
#[test]
fn relative_days_follow_the_study_day() {
    let w = world();
    let day_start = Timestamp(NOW.0 - 5 * 3_600_000); // the day began 5 h ago
    let cat = Catalog {
        day_start: Some(day_start),
        ..w.cat()
    };
    let mut yesterday_late = sample(1, 2, "a");
    yesterday_late.card.scheduling.state = CardState::Review;
    // 6 h ago: within 24 h, but before today's study day began.
    yesterday_late.card.scheduling.last_review = Some(Timestamp(NOW.0 - 6 * 3_600_000));
    let mut this_morning = sample(2, 2, "b");
    this_morning.card.scheduling.state = CardState::Review;
    this_morning.card.scheduling.last_review = Some(Timestamp(NOW.0 - 4 * 3_600_000));
    for (q, expect) in [("reviewed:1", [false, true]), ("reviewed:2", [true, true])] {
        let f = compile_query(q, &cat).unwrap();
        assert_eq!(
            [yesterday_late.hits(&f), this_morning.hits(&f)],
            expect,
            "{q}"
        );
    }
    // Without a study day: the last 24 hours.
    let f = w.compile("reviewed:1").unwrap();
    assert!(yesterday_late.hits(&f) && this_morning.hits(&f));
}
