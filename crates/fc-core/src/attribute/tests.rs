use super::templates::find_template;
use super::*;
use crate::error::{AttributeValueProblem, CoreError};
use crate::i18n::Locale;
use crate::id::{IdGen, V7IdGen};
use crate::rng::SplitMix64;
use crate::time::FixedClock;
use std::sync::Arc;
use uuid::Uuid;

fn ids() -> V7IdGen<SplitMix64> {
    V7IdGen::new(
        Arc::new(FixedClock::new(Timestamp(1_000))),
        SplitMix64::new(1),
    )
}

fn deck(n: u128) -> DeckId {
    DeckId(Uuid::from_u128(n))
}

fn cefr(scope: Option<DeckId>, g: &mut dyn IdGen) -> AttributeDef {
    instantiate_template(
        find_template("cefr").unwrap(),
        scope,
        Locale::Tr,
        0,
        g,
        Timestamp(1),
    )
}

fn plain(key: &str, kind: AttributeKind, scope: Option<DeckId>, g: &mut dyn IdGen) -> AttributeDef {
    AttributeDef {
        id: AttributeDefId::generate(g),
        scope_deck_id: scope,
        key: key.into(),
        name: key.into(),
        kind,
        options: vec![],
        target: AttributeTarget::Note,
        required: false,
        default_value: None,
        position: 0,
        template_id: None,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        deleted_at: None,
    }
}

#[test]
fn all_templates_instantiate_to_valid_defs() {
    let mut g = ids();
    let expected = ["cefr", "yks", "jlpt", "hsk", "school_grade", "difficulty"];
    let got: Vec<_> = builtin_templates().iter().map(|t| t.id).collect();
    assert_eq!(got, expected);
    for t in builtin_templates() {
        for locale in [Locale::En, Locale::Tr] {
            let def = instantiate_template(t, None, locale, 0, &mut g, Timestamp(1));
            validate_attribute_def(&def).unwrap();
            assert_eq!(def.options.len(), t.options.len());
            assert_eq!(def.template_id.as_deref(), Some(t.id));
        }
    }
    let grade = instantiate_template(
        find_template("school_grade").unwrap(),
        None,
        Locale::Tr,
        0,
        &mut g,
        Timestamp(1),
    );
    assert_eq!(grade.options.len(), 12);
    assert_eq!(grade.options[0].label, "1. sınıf");
    let level = cefr(None, &mut g);
    assert_eq!(level.name, "Seviye");
    let labels: Vec<_> = level
        .sorted_options()
        .iter()
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(labels, ["A1", "A2", "B1", "B2", "C1", "C2"]);
}

#[test]
fn ordered_comparison_follows_option_order() {
    let mut g = ids();
    let level = cefr(None, &mut g);
    let a1 = level.option_by_label("a1").unwrap().id;
    let b2 = level.option_by_label("B2").unwrap().id;
    assert_eq!(
        level.compare_options(a1, b2),
        Some(std::cmp::Ordering::Less)
    );
    let mut unordered = level.clone();
    unordered.kind = AttributeKind::Enum;
    assert_eq!(unordered.compare_options(a1, b2), None);
}

#[test]
fn option_editing_keeps_ids_and_renumbers() {
    let mut g = ids();
    let mut level = cefr(None, &mut g);
    let a1 = level.option_by_label("A1").unwrap().id;
    let a2 = level.option_by_label("A2").unwrap().id;

    let old = level.rename_option(a1, "  Başlangıç  ").unwrap();
    assert_eq!(old, "A1");
    assert_eq!(level.option(a1).unwrap().label, "Başlangıç");
    assert!(level.rename_option(a1, "a2").is_err(), "duplicate label");
    assert!(level.rename_option(a1, "   ").is_err());

    let pre = AttributeOption {
        id: AttributeOptionId::generate(&mut g),
        label: "Pre-A1".into(),
        color: None,
        order: 99,
    };
    let pre_id = pre.id;
    level.add_option(pre, None).unwrap();
    assert_eq!(level.sorted_options().last().unwrap().id, pre_id);
    let mid = AttributeOption {
        id: AttributeOptionId::generate(&mut g),
        label: "A1+".into(),
        color: None,
        order: 0,
    };
    let mid_id = mid.id;
    level.add_option(mid, Some(a1)).unwrap();
    assert_eq!(level.sorted_options()[1].id, mid_id);
    assert_eq!(level.sorted_options()[2].id, a2);

    let mut order: Vec<_> = level.sorted_options().iter().map(|o| o.id).collect();
    order.rotate_left(1);
    let previous = level.reorder_options(&order).unwrap();
    assert_eq!(previous[0], a1);
    assert_eq!(level.sorted_options()[0].id, mid_id);
    assert!(level.reorder_options(&order[1..]).is_err());
    let mut dup = order.clone();
    dup[0] = dup[1];
    assert!(level.reorder_options(&dup).is_err());
    level.reorder_options(&previous).unwrap();
    assert_eq!(level.sorted_options()[0].id, a1);

    let (removed, idx) = level.remove_option(mid_id).unwrap();
    assert_eq!((removed.id, idx), (mid_id, 1));
    assert!(level.remove_option(mid_id).is_err());
    assert_eq!(
        level.set_option_color(a1, Some("#ff0000".into())).unwrap(),
        None
    );
    validate_attribute_def(&level).unwrap();

    let mut num = plain("n", AttributeKind::Number, None, &mut g);
    let o = AttributeOption {
        id: AttributeOptionId::generate(&mut g),
        label: "x".into(),
        color: None,
        order: 0,
    };
    assert!(num.add_option(o, None).is_err());
}

#[test]
fn value_validation() {
    let mut g = ids();
    let level = cefr(None, &mut g);
    let b1 = level.option_by_label("B1").unwrap().id;
    let a1 = level.option_by_label("A1").unwrap().id;
    assert!(validate_attribute_value(&level, &AttributeValue::Choice(b1)).is_ok());
    assert!(matches!(
        validate_attribute_value(&level, &AttributeValue::Number(1.0)),
        Err(CoreError::InvalidAttributeValue {
            reason: AttributeValueProblem::KindMismatch,
            ..
        })
    ));
    let foreign = AttributeOptionId::generate(&mut g);
    assert!(matches!(
        validate_attribute_value(&level, &AttributeValue::Choice(foreign)),
        Err(CoreError::UnknownAttributeOption { .. })
    ));

    let mut multi = level.clone();
    multi.kind = AttributeKind::MultiEnum;
    assert_eq!(
        validate_attribute_value(&multi, &AttributeValue::Choices(vec![b1, a1])).unwrap(),
        AttributeValue::Choices(vec![a1, b1]),
        "normalized to option order"
    );
    assert!(validate_attribute_value(&multi, &AttributeValue::Choices(vec![])).is_err());
    assert!(validate_attribute_value(&multi, &AttributeValue::Choices(vec![a1, a1])).is_err());

    let num = plain("n", AttributeKind::Number, None, &mut g);
    assert!(validate_attribute_value(&num, &AttributeValue::Number(f64::NAN)).is_err());
    let text = plain("t", AttributeKind::Text, None, &mut g);
    assert_eq!(
        validate_attribute_value(&text, &AttributeValue::Text("  hi ".into())).unwrap(),
        AttributeValue::Text("hi".into())
    );
    assert!(validate_attribute_value(&text, &AttributeValue::Text("  ".into())).is_err());
    let date = plain("d", AttributeKind::Date, None, &mut g);
    for ok in ["2024-02-29", "2000-02-29", "1999-12-31"] {
        assert!(
            validate_attribute_value(&date, &AttributeValue::Date(ok.into())).is_ok(),
            "{ok}"
        );
    }
    for bad in [
        "2023-02-29",
        "1900-02-29",
        "2024-13-01",
        "2024-1-01",
        "abcd-ef-gh",
        "0000-01-01",
    ] {
        assert!(
            validate_attribute_value(&date, &AttributeValue::Date(bad.into())).is_err(),
            "{bad}"
        );
    }
    let b = plain("b", AttributeKind::Bool, None, &mut g);
    assert!(validate_attribute_value(&b, &AttributeValue::Bool(true)).is_ok());
}

#[test]
fn def_validation() {
    let mut g = ids();
    let mut d = cefr(None, &mut g);
    validate_attribute_def(&d).unwrap();
    d.options[1].label = "a1".into();
    assert!(validate_attribute_def(&d).is_err());

    let mut empty = cefr(None, &mut g);
    empty.options.clear();
    assert!(validate_attribute_def(&empty).is_err());

    let mut text = plain("note", AttributeKind::Text, None, &mut g);
    validate_attribute_def(&text).unwrap();
    text.default_value = Some(AttributeValue::Bool(true));
    assert!(validate_attribute_def(&text).is_err());
    text.default_value = None;
    text.key = "Bad Key".into();
    assert!(validate_attribute_def(&text).is_err());
}

#[test]
fn keys_and_slugs() {
    for ok in ["level", "a", "exam_2", "x1"] {
        validate_attribute_key(ok).unwrap();
    }
    for bad in ["", "1a", "Level", "a-b", "_a", &"a".repeat(65)] {
        assert!(validate_attribute_key(bad).is_err(), "{bad}");
    }
    assert_eq!(slugify_key("Sınav Türü"), "sinav_turu");
    assert_eq!(slugify_key("  İÇĞÖŞÜ  "), "icgosu");
    assert_eq!(slugify_key("2. Dönem"), "a2_donem");
    assert_eq!(slugify_key("!!!"), "attr");
    for s in ["Sınav Türü", "2. Dönem", "!!!", &"ü".repeat(200)] {
        validate_attribute_key(&slugify_key(s)).unwrap();
    }
}

#[test]
fn inheritance_resolution() {
    let mut g = ids();
    // Tree: 1 (İngilizce) > 2 (B1 Kitabı) > 3 (Ünite 1); 9 is unrelated.
    let collection = plain("source", AttributeKind::Text, None, &mut g);
    let level = cefr(Some(deck(1)), &mut g);
    let mut unit = plain("unit", AttributeKind::Number, Some(deck(2)), &mut g);
    unit.position = -1;
    let other = plain("exam", AttributeKind::Text, Some(deck(9)), &mut g);
    let mut deleted = plain("old", AttributeKind::Text, Some(deck(1)), &mut g);
    deleted.deleted_at = Some(Timestamp(5));
    // Sub-deck 3 redefines `level` (shadows the one from deck 1).
    let mut level_override = cefr(Some(deck(3)), &mut g);
    level_override.options.truncate(2);

    let defs = vec![
        collection.clone(),
        level.clone(),
        unit.clone(),
        other,
        deleted,
        level_override.clone(),
    ];

    let keys = |anc: &[DeckId]| -> Vec<(String, usize)> {
        effective_attribute_defs(anc, &defs)
            .iter()
            .map(|e| (e.def.key.clone(), e.depth))
            .collect()
    };
    assert_eq!(keys(&[]), [("source".into(), 0)]);
    assert_eq!(
        keys(&[deck(1)]),
        [("source".into(), 0), ("level".into(), 1)]
    );
    assert_eq!(
        keys(&[deck(1), deck(2)]),
        [
            ("source".into(), 0),
            ("level".into(), 1),
            ("unit".into(), 2)
        ]
    );
    let deepest = effective_attribute_defs(&[deck(1), deck(2), deck(3)], &defs);
    let level_eff = deepest.iter().find(|e| e.def.key == "level").unwrap();
    assert_eq!(level_eff.def.id, level_override.id);
    assert!(!level_eff.is_inherited(3));
    assert!(deepest[0].is_inherited(3));
    assert_eq!(deepest.len(), 3);
}

#[test]
fn preparing_values_for_a_note() {
    let mut g = ids();
    let mut level = cefr(Some(deck(1)), &mut g);
    let a2 = level.option_by_label("A2").unwrap().id;
    let b1 = level.option_by_label("B1").unwrap().id;
    level.default_value = Some(AttributeValue::Choice(a2));
    let mut source = plain("source", AttributeKind::Text, None, &mut g);
    source.required = true;
    let mut card_only = plain("hard", AttributeKind::Bool, None, &mut g);
    card_only.target = AttributeTarget::Card;
    card_only.required = true;
    let defs = vec![level.clone(), source.clone(), card_only];
    let eff = effective_attribute_defs(&[deck(1), deck(2)], &defs);

    // Default applied, required text provided, card-level def ignored for notes.
    let out = prepare_attribute_values(
        &eff,
        AttributeTarget::Note,
        &[(source.id, Some(AttributeValue::Text(" Book ".into())))],
    )
    .unwrap();
    assert_eq!(
        out,
        vec![
            (source.id, AttributeValue::Text("Book".into())),
            (level.id, AttributeValue::Choice(a2)),
        ]
    );

    // Explicit value overrides default; explicit None clears it.
    let out = prepare_attribute_values(
        &eff,
        AttributeTarget::Note,
        &[
            (level.id, Some(AttributeValue::Choice(b1))),
            (source.id, Some(AttributeValue::Text("x".into()))),
        ],
    )
    .unwrap();
    assert!(out.contains(&(level.id, AttributeValue::Choice(b1))));
    let out = prepare_attribute_values(
        &eff,
        AttributeTarget::Note,
        &[
            (level.id, None),
            (source.id, Some(AttributeValue::Text("x".into()))),
        ],
    )
    .unwrap();
    assert_eq!(out.len(), 1);

    // Missing required.
    assert!(matches!(
        prepare_attribute_values(&eff, AttributeTarget::Note, &[]),
        Err(CoreError::AttributeRequired { key }) if key == "source"
    ));
    // Not applicable (defined on an unrelated deck / wrong target).
    let stray = plain("stray", AttributeKind::Text, Some(deck(9)), &mut g);
    assert!(matches!(
        prepare_attribute_values(
            &eff,
            AttributeTarget::Note,
            &[(stray.id, Some(AttributeValue::Text("x".into())))]
        ),
        Err(CoreError::AttributeNotApplicable { .. })
    ));
}

#[test]
fn storage_representation_roundtrips() {
    let mut g = ids();
    let level = cefr(None, &mut g);
    let a1 = level.options[0].id;
    let b1 = level.options[2].id;
    let cases = [
        (AttributeKind::OrderedEnum, AttributeValue::Choice(a1)),
        (
            AttributeKind::MultiEnum,
            AttributeValue::Choices(vec![a1, b1]),
        ),
        (AttributeKind::Number, AttributeValue::Number(2.5)),
        (AttributeKind::Text, AttributeValue::Text("Book".into())),
        (
            AttributeKind::Date,
            AttributeValue::Date("2025-06-01".into()),
        ),
        (AttributeKind::Bool, AttributeValue::Bool(true)),
    ];
    for (kind, v) in cases {
        let rows = v.to_stored();
        assert_eq!(
            AttributeValue::from_stored(kind, &rows),
            Some(v.clone()),
            "{kind:?}"
        );
        assert_eq!(AttributeKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(AttributeValue::Choices(vec![a1, b1]).to_stored().len(), 2);
    assert_eq!(
        AttributeValue::Number(3.0).to_stored()[0].value_num,
        Some(3.0)
    );
    assert_eq!(
        AttributeValue::from_stored(AttributeKind::Number, &[]),
        None
    );
    assert_eq!(
        serde_json::to_value(AttributeValue::Choice(a1)).unwrap(),
        serde_json::json!({"type": "choice", "value": a1.to_string()})
    );
}
