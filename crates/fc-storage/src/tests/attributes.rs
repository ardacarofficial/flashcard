use super::{cid, deck, did, nid, note, Fx};
use crate::error::StorageError;
use crate::store::Store;
use fc_core::attribute::{
    find_template, instantiate_template, prepare_attribute_values, AttributeDef, AttributeKind,
    AttributeOption, AttributeTarget, AttributeValue, EntityRef,
};
use fc_core::i18n::Locale;
use fc_core::rng::SplitMix64;
use fc_core::time::FixedClock;
use fc_core::{AttributeDefId, AttributeOptionId, DeckId, IdGen, Timestamp, V7IdGen};
use std::sync::Arc;

fn ids() -> V7IdGen<SplitMix64> {
    V7IdGen::new(
        Arc::new(FixedClock::new(Timestamp(1_000))),
        SplitMix64::new(7),
    )
}

fn plain_def(
    ids: &mut dyn IdGen,
    scope: Option<DeckId>,
    key: &str,
    kind: AttributeKind,
    labels: &[&str],
) -> AttributeDef {
    AttributeDef {
        id: AttributeDefId::generate(ids),
        scope_deck_id: scope,
        key: key.into(),
        name: key.to_uppercase(),
        kind,
        options: labels
            .iter()
            .enumerate()
            .map(|(i, l)| AttributeOption {
                id: AttributeOptionId::generate(ids),
                label: (*l).into(),
                color: None,
                order: i as i32,
            })
            .collect(),
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

/// docs/02 §8.1 scenario: "Seviye" (A1–C2) on a parent deck, inherited by a child deck,
/// set on a note, options renamed/reordered — values survive.
#[test]
fn classification_flow_seviye() {
    let fx = Fx::seeded();
    let mut ids = ids();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "İngilizce")).unwrap();
        tx.insert_deck(&deck(2, Some(1), "Kelimeler")).unwrap();
        tx.insert_note(&note(1, "run", "koşmak", &[], 5), "run")
            .unwrap();
    });
    let cefr = find_template("cefr").unwrap();
    let mut seviye =
        instantiate_template(cefr, Some(did(1)), Locale::Tr, 0, &mut ids, Timestamp(2));
    seviye.name = "Seviye".into();
    fx.write(|tx| tx.insert_attribute_def(&seviye).unwrap());

    // Child deck sees it (inherited, depth 1 < ancestry len 2); collection does not.
    let effective = fx.read(|r| r.effective_attribute_defs_for_deck(Some(did(2))).unwrap());
    assert_eq!(effective.len(), 1);
    assert_eq!(effective[0].0, seviye);
    assert_eq!(effective[0].1, 1);
    assert!(fx
        .read(|r| r.effective_attribute_defs_for_deck(None).unwrap())
        .is_empty());

    // Set B1 on the note, the way the app would (core validation first).
    let b1 = seviye.option_by_label("B1").unwrap().id;
    let defs: Vec<_> = effective.iter().map(|(d, _)| d.clone()).collect();
    let eff = fc_core::attribute::effective_attribute_defs(&[did(1), did(2)], &defs);
    let prepared = prepare_attribute_values(
        &eff,
        AttributeTarget::Note,
        &[(seviye.id, Some(AttributeValue::Choice(b1)))],
    )
    .unwrap();
    let entity = EntityRef::Note(nid(1));
    fx.write(|tx| {
        let values: Vec<_> = prepared
            .iter()
            .map(|(a, v)| (*a, Some(v.clone())))
            .collect();
        tx.set_attribute_values(entity, &values).unwrap();
    });

    // Rename B1 → "B1 (Orta)" and reverse the order; the stored value is untouched.
    let mut edited = seviye.clone();
    edited.rename_option(b1, "B1 (Orta)").unwrap();
    let reversed: Vec<_> = edited.sorted_options().iter().rev().map(|o| o.id).collect();
    edited.reorder_options(&reversed).unwrap();
    edited.updated_at = Timestamp(3);
    fx.write(|tx| tx.update_attribute_def(&edited).unwrap());

    fx.read(|r| {
        let def = r.get_attribute_def(seviye.id).unwrap().unwrap();
        assert_eq!(def, edited);
        assert_eq!(def.option(b1).unwrap().label, "B1 (Orta)");
        assert_eq!(def.sorted_options()[0].label, "C2");
        assert_eq!(
            r.get_attribute_values(entity).unwrap(),
            [(seviye.id, AttributeValue::Choice(b1))]
        );
        assert_eq!(r.entities_with_option(seviye.id, b1).unwrap(), [entity]);
        let a1 = def.option_by_label("A1").unwrap().id;
        assert!(r.entities_with_option(seviye.id, a1).unwrap().is_empty());
    });
}

#[test]
fn deepest_definition_wins_and_trash_is_ignored() {
    let fx = Fx::new();
    let mut ids = ids();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, Some(1), "B")).unwrap();
        tx.insert_deck(&deck(3, None, "Other")).unwrap();
    });
    let global = plain_def(&mut ids, None, "level", AttributeKind::Enum, &["x"]);
    let on_a = plain_def(
        &mut ids,
        Some(did(1)),
        "level",
        AttributeKind::OrderedEnum,
        &["A1"],
    );
    let mut exam = plain_def(
        &mut ids,
        Some(did(2)),
        "exam",
        AttributeKind::Enum,
        &["TYT"],
    );
    exam.position = 1;
    fx.write(|tx| {
        for d in [&global, &on_a, &exam] {
            tx.insert_attribute_def(d).unwrap();
        }
        // Same key in the same scope is refused by the schema.
        let dup = plain_def(&mut ids, Some(did(1)), "level", AttributeKind::Enum, &[]);
        assert!(matches!(
            tx.insert_attribute_def(&dup),
            Err(StorageError::Constraint { .. })
        ));
    });
    let keyed = |deck: Option<DeckId>| -> Vec<(String, AttributeDefId, usize)> {
        fx.read(|r| {
            r.effective_attribute_defs_for_deck(deck)
                .unwrap()
                .into_iter()
                .map(|(d, depth)| (d.key, d.id, depth))
                .collect()
        })
    };
    assert_eq!(
        keyed(Some(did(2))),
        [("level".into(), on_a.id, 1), ("exam".into(), exam.id, 2)]
    );
    assert_eq!(keyed(Some(did(3))), [("level".into(), global.id, 0)]);
    assert_eq!(keyed(None), [("level".into(), global.id, 0)]);

    fx.write(|tx| {
        tx.set_attribute_def_deleted(on_a.id, Some(Timestamp(9)))
            .unwrap();
        assert!(matches!(
            tx.set_attribute_def_deleted(AttributeDefId(super::u(1)), None),
            Err(StorageError::NotFound {
                entity: "attribute",
                ..
            })
        ));
    });
    assert_eq!(keyed(Some(did(2)))[0], ("level".into(), global.id, 0));
    fx.read(|r| {
        assert_eq!(r.list_attribute_defs(false).unwrap().len(), 2);
        assert_eq!(r.list_attribute_defs(true).unwrap().len(), 3);
        assert_eq!(
            r.get_attribute_def(on_a.id).unwrap().unwrap().deleted_at,
            Some(Timestamp(9))
        );
        assert!(matches!(
            r.effective_attribute_defs_for_deck(Some(did(42))),
            Err(StorageError::NotFound { .. })
        ));
    });
    // Restore brings the shadowing back.
    fx.write(|tx| tx.set_attribute_def_deleted(on_a.id, None).unwrap());
    assert_eq!(keyed(Some(did(2)))[0].1, on_a.id);
}

#[test]
fn every_value_kind_roundtrips() {
    let fx = Fx::seeded();
    let mut ids = ids();
    fx.write(|tx| tx.insert_note(&note(1, "a", "", &[], 1), "").unwrap());
    let multi = plain_def(
        &mut ids,
        None,
        "topics",
        AttributeKind::MultiEnum,
        &["a", "b", "c"],
    );
    let mut num = plain_def(&mut ids, None, "freq", AttributeKind::Number, &[]);
    num.position = 1;
    let mut text = plain_def(&mut ids, None, "src", AttributeKind::Text, &[]);
    text.position = 2;
    let mut date = plain_def(&mut ids, None, "seen", AttributeKind::Date, &[]);
    date.position = 3;
    let mut flag = plain_def(&mut ids, None, "hard", AttributeKind::Bool, &[]);
    flag.position = 4;
    flag.default_value = Some(AttributeValue::Bool(false));
    flag.required = true;
    let mut card_attr = plain_def(&mut ids, None, "side", AttributeKind::Text, &[]);
    card_attr.target = AttributeTarget::Card;
    fx.write(|tx| {
        for d in [&multi, &num, &text, &date, &flag, &card_attr] {
            tx.insert_attribute_def(d).unwrap();
        }
    });
    fx.read(|r| assert_eq!(r.get_attribute_def(flag.id).unwrap().unwrap(), flag));

    let opt = |i: usize| multi.sorted_options()[i].id;
    let note_values = vec![
        (multi.id, AttributeValue::Choices(vec![opt(0), opt(2)])),
        (num.id, AttributeValue::Number(-2.5)),
        (text.id, AttributeValue::Text("Oxford 3000".into())),
        (date.id, AttributeValue::Date("2026-11-19".into())),
        (flag.id, AttributeValue::Bool(true)),
    ];
    let n = EntityRef::Note(nid(1));
    let c = EntityRef::Card(cid(1));
    fx.write(|tx| {
        let set: Vec<_> = note_values
            .iter()
            .map(|(a, v)| (*a, Some(v.clone())))
            .collect();
        tx.set_attribute_values(n, &set).unwrap();
        tx.set_attribute_values(c, &[(card_attr.id, Some(AttributeValue::Text("x".into())))])
            .unwrap();
    });
    fx.read(|r| {
        assert_eq!(r.get_attribute_values(n).unwrap(), note_values);
        assert_eq!(r.get_attribute_values(c).unwrap().len(), 1);
        assert_eq!(r.entities_with_option(multi.id, opt(2)).unwrap(), [n]);
    });
    // value_num is filled for numbers and bools (SQL comparisons in the query language).
    let nums: Vec<Option<f64>> = fx
        .raw()
        .prepare(
            "SELECT value_num FROM attribute_values WHERE entity_kind = 'note' ORDER BY value_num",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(nums.contains(&Some(-2.5)) && nums.contains(&Some(1.0)));

    // Replace one, clear one, leave the rest; multi-select keeps option order on read.
    fx.write(|tx| {
        tx.set_attribute_values(
            n,
            &[
                (
                    multi.id,
                    Some(AttributeValue::Choices(vec![opt(1), opt(0)])),
                ),
                (num.id, None),
            ],
        )
        .unwrap();
    });
    fx.read(|r| {
        let got = r.get_attribute_values(n).unwrap();
        assert_eq!(got.len(), 4);
        assert_eq!(
            got[0],
            (multi.id, AttributeValue::Choices(vec![opt(0), opt(1)]))
        );
        assert!(got.iter().all(|(a, _)| *a != num.id));
    });
    // Values of a trashed definition are kept but hidden.
    fx.write(|tx| {
        tx.set_attribute_def_deleted(text.id, Some(Timestamp(5)))
            .unwrap()
    });
    fx.read(|r| assert_eq!(r.get_attribute_values(n).unwrap().len(), 3));
    fx.write(|tx| tx.set_attribute_def_deleted(text.id, None).unwrap());
    fx.read(|r| assert_eq!(r.get_attribute_values(n).unwrap().len(), 4));
    // Unknown definition id → foreign-key constraint.
    let mut tx = fx.store.write().unwrap();
    assert!(matches!(
        tx.set_attribute_values(
            n,
            &[(
                AttributeDefId(super::u(5)),
                Some(AttributeValue::Bool(true))
            )]
        ),
        Err(StorageError::Constraint { .. })
    ));
}

#[test]
fn update_missing_definition_is_not_found() {
    let fx = Fx::new();
    let def = plain_def(&mut ids(), None, "x", AttributeKind::Text, &[]);
    let mut tx = fx.store.write().unwrap();
    assert!(matches!(
        tx.update_attribute_def(&def),
        Err(StorageError::NotFound {
            entity: "attribute",
            ..
        })
    ));
}
