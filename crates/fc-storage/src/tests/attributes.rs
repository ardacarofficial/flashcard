use super::{cid, deck, did, nid, note, put_note, Fx};
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
        put_note(tx, &note(1, "run", "koşmak", &[], 5), "run").unwrap();
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
    fx.write(|tx| put_note(tx, &note(1, "a", "", &[], 1), "").unwrap());
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

/// Note-level definitions resolve from the note's home deck, card-level ones from the card's
/// deck; shadowing by key happens before filtering by target.
#[test]
fn effective_defs_for_note_use_home_deck_and_for_card_use_card_deck() {
    let fx = Fx::seeded();
    let mut ids = ids();
    let mut on_a = plain_def(&mut ids, Some(did(1)), "level", AttributeKind::Enum, &["x"]);
    on_a.name = "A".into();
    let mut card_b = plain_def(&mut ids, Some(did(2)), "seen", AttributeKind::Bool, &[]);
    card_b.target = AttributeTarget::Card;
    let note_b = plain_def(&mut ids, Some(did(2)), "topic", AttributeKind::Text, &[]);
    let global = plain_def(&mut ids, None, "src", AttributeKind::Text, &[]);
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, None, "B")).unwrap();
        for d in [&on_a, &card_b, &note_b, &global] {
            tx.insert_attribute_def(d).unwrap();
        }
        // Note homed in A, its card lives in B.
        let n = super::homed(note(1, "a", "", &[], 1), did(1));
        tx.insert_note(&n, "a").unwrap();
        tx.insert_card(&super::card(1, 1, 2)).unwrap();
    });
    let keys = |v: Vec<(AttributeDef, usize)>| -> Vec<String> {
        let mut k: Vec<String> = v.into_iter().map(|(d, _)| d.key).collect();
        k.sort();
        k
    };
    fx.read(|r| {
        assert_eq!(
            keys(r.effective_attribute_defs_for_note(nid(1)).unwrap()),
            ["level", "src"]
        );
        assert_eq!(
            keys(r.effective_attribute_defs_for_card(cid(1)).unwrap()),
            ["seen"]
        );
        assert!(matches!(
            r.effective_attribute_defs_for_note(nid(9)),
            Err(StorageError::NotFound { entity: "note", .. })
        ));
        assert!(matches!(
            r.effective_attribute_defs_for_card(cid(9)),
            Err(StorageError::NotFound { entity: "card", .. })
        ));
    });
    // Re-homing the note switches its note-level definitions.
    fx.write(|tx| tx.set_note_home_deck(nid(1), did(2)).unwrap());
    fx.read(|r| {
        assert_eq!(
            keys(r.effective_attribute_defs_for_note(nid(1)).unwrap()),
            ["src", "topic"]
        );
    });
}

fn opt(def: &AttributeDef, label: &str) -> AttributeOptionId {
    def.option_by_label(label).unwrap().id
}

fn labels(fx: &Fx, attr: AttributeDefId) -> Vec<String> {
    fx.read(|r| {
        r.get_attribute_def(attr)
            .unwrap()
            .unwrap()
            .sorted_options()
            .iter()
            .map(|o| format!("{}:{}", o.order, o.label))
            .collect()
    })
}

#[test]
fn insert_option_with_caller_id_at_index() {
    let fx = Fx::seeded();
    let mut ids = ids();
    let def = plain_def(
        &mut ids,
        None,
        "lvl",
        AttributeKind::OrderedEnum,
        &["a", "c"],
    );
    fx.write(|tx| tx.insert_attribute_def(&def).unwrap());
    let b = AttributeOption {
        id: AttributeOptionId(super::u(0xb)),
        label: "b".into(),
        color: Some("#123456".into()),
        order: 99,
    };
    fx.write(|tx| {
        tx.insert_attribute_option(def.id, &b, 1, Timestamp(5))
            .unwrap();
        assert!(matches!(
            tx.insert_attribute_option(def.id, &b, 0, Timestamp(5)),
            Err(StorageError::Constraint { constraint }) if constraint == "attribute_option_id"
        ));
        let mut d = b.clone();
        d.id = AttributeOptionId(super::u(0xd));
        d.label = "d".into();
        // Index past the end appends.
        tx.insert_attribute_option(def.id, &d, 100, Timestamp(6))
            .unwrap();
        assert!(matches!(
            tx.insert_attribute_option(AttributeDefId(super::u(0xdead)), &d, 0, Timestamp(6)),
            Err(StorageError::NotFound { .. })
        ));
    });
    assert_eq!(labels(&fx, def.id), ["0:a", "1:b", "2:c", "3:d"]);
    fx.read(|r| {
        let got = r.get_attribute_def(def.id).unwrap().unwrap();
        assert_eq!(got.updated_at, Timestamp(6));
        assert_eq!(got.option(b.id).unwrap().color.as_deref(), Some("#123456"));
    });
}

/// Merge on a multi-select: entities with only `from` move, entities with both drop `from`;
/// restore puts back the option at its index and every value exactly.
#[test]
fn merge_multi_select_and_restore_exactly() {
    let fx = Fx::seeded();
    let mut ids = ids();
    let def = plain_def(
        &mut ids,
        None,
        "topics",
        AttributeKind::MultiEnum,
        &["x", "y", "z"],
    );
    let (x, y, z) = (opt(&def, "x"), opt(&def, "y"), opt(&def, "z"));
    let (n1, n2, n3) = (
        EntityRef::Note(nid(1)),
        EntityRef::Note(nid(2)),
        EntityRef::Note(nid(3)),
    );
    let set = |e, v: Vec<AttributeOptionId>| (e, vec![(def.id, Some(AttributeValue::Choices(v)))]);
    let initial = [
        set(n1, vec![x, y]), // has both: y row dropped
        set(n2, vec![y]),    // only y: moves to z
        set(n3, vec![z]),    // untouched
    ];
    fx.write(|tx| {
        tx.insert_attribute_def(&def).unwrap();
        for n in 1..=3 {
            put_note(tx, &note(n, &format!("w{n}"), "", &[], 1), "").unwrap();
        }
        for (e, v) in &initial {
            tx.set_attribute_values(*e, v).unwrap();
        }
    });
    let snapshot = |fx: &Fx| -> Vec<Vec<(AttributeDefId, AttributeValue)>> {
        fx.read(|r| {
            [n1, n2, n3]
                .iter()
                .map(|e| r.get_attribute_values(*e).unwrap())
                .collect()
        })
    };
    let before = snapshot(&fx);
    let before_labels = labels(&fx, def.id);

    // Merge y into x... from n1's perspective it already has x.
    let removal = fx.write(|tx| {
        tx.remove_attribute_option(def.id, y, Some(x), Timestamp(9))
            .unwrap()
    });
    assert_eq!(removal.index, 1);
    assert_eq!(removal.option.label, "y");
    assert_eq!(removal.replaced_with, Some(x));
    assert_eq!(removal.moved, [n2]);
    assert_eq!(removal.dropped, [n1]);
    assert_eq!(labels(&fx, def.id), ["0:x", "1:z"]);
    assert_eq!(
        snapshot(&fx),
        [
            vec![(def.id, AttributeValue::Choices(vec![x]))],
            vec![(def.id, AttributeValue::Choices(vec![x]))],
            vec![(def.id, AttributeValue::Choices(vec![z]))],
        ]
    );

    fx.write(|tx| {
        tx.restore_attribute_option(def.id, &removal, Timestamp(10))
            .unwrap()
    });
    assert_eq!(labels(&fx, def.id), before_labels);
    assert_eq!(snapshot(&fx), before);
}

#[test]
fn delete_option_without_replacement_and_restore() {
    let fx = Fx::seeded();
    let mut ids = ids();
    let def = plain_def(&mut ids, None, "lvl", AttributeKind::Enum, &["a", "b", "c"]);
    let (a, b) = (opt(&def, "a"), opt(&def, "b"));
    fx.write(|tx| {
        tx.insert_attribute_def(&def).unwrap();
        for n in 1..=2 {
            put_note(tx, &note(n, &format!("w{n}"), "", &[], 1), "").unwrap();
        }
        tx.set_attribute_values(
            EntityRef::Note(nid(1)),
            &[(def.id, Some(AttributeValue::Choice(a)))],
        )
        .unwrap();
        tx.set_attribute_values(
            EntityRef::Note(nid(2)),
            &[(def.id, Some(AttributeValue::Choice(b)))],
        )
        .unwrap();
    });
    let removal = fx.write(|tx| {
        assert!(matches!(
            tx.remove_attribute_option(
                def.id,
                AttributeOptionId(super::u(0x77)),
                None,
                Timestamp(2)
            ),
            Err(StorageError::NotFound {
                entity: "attribute_option",
                ..
            })
        ));
        assert!(matches!(
            tx.remove_attribute_option(def.id, a, Some(a), Timestamp(2)),
            Err(StorageError::NotFound {
                entity: "attribute_option",
                ..
            })
        ));
        tx.remove_attribute_option(def.id, a, None, Timestamp(2))
            .unwrap()
    });
    assert_eq!(removal.index, 0);
    assert!(removal.moved.is_empty());
    assert_eq!(removal.dropped, [EntityRef::Note(nid(1))]);
    fx.read(|r| {
        assert!(r
            .get_attribute_values(EntityRef::Note(nid(1)))
            .unwrap()
            .is_empty());
        assert_eq!(r.entities_with_option(def.id, b).unwrap().len(), 1);
    });
    fx.write(|tx| {
        tx.restore_attribute_option(def.id, &removal, Timestamp(3))
            .unwrap()
    });
    assert_eq!(labels(&fx, def.id), ["0:a", "1:b", "2:c"]);
    fx.read(|r| {
        assert_eq!(
            r.get_attribute_values(EntityRef::Note(nid(1))).unwrap(),
            [(def.id, AttributeValue::Choice(a))]
        );
    });
}

/// Multi-select values come back in option order, also after a reorder.
#[test]
fn multi_select_values_follow_option_order() {
    let fx = Fx::seeded();
    let mut ids = ids();
    let mut def = plain_def(
        &mut ids,
        None,
        "m",
        AttributeKind::MultiEnum,
        &["p", "q", "r"],
    );
    let (p, q, r) = (opt(&def, "p"), opt(&def, "q"), opt(&def, "r"));
    let e = EntityRef::Note(nid(1));
    fx.write(|tx| {
        tx.insert_attribute_def(&def).unwrap();
        put_note(tx, &note(1, "w", "", &[], 1), "").unwrap();
        tx.set_attribute_values(e, &[(def.id, Some(AttributeValue::Choices(vec![r, p, q])))])
            .unwrap();
    });
    let get = |fx: &Fx| fx.read(|rd| rd.get_attribute_values(e).unwrap()[0].1.clone());
    assert_eq!(get(&fx), AttributeValue::Choices(vec![p, q, r]));
    def.reorder_options(&[q, r, p]).unwrap();
    fx.write(|tx| tx.update_attribute_def(&def).unwrap());
    assert_eq!(get(&fx), AttributeValue::Choices(vec![q, r, p]));
}
