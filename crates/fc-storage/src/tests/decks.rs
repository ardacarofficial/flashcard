use super::{card, cid, deck, did, nid, note, put_note, Fx};
use crate::error::StorageError;
use crate::repo::*;
use crate::store::Store;
use fc_core::card::CardState;
use fc_core::preset::{Preset, PresetConfig};
use fc_core::{PresetId, Timestamp};

#[test]
fn deck_roundtrip_and_listing() {
    let fx = Fx::new();
    let mut a = deck(1, None, "Dil");
    a.description = "**md**".into();
    a.icon = Some("📚".into());
    a.color = Some("#ff0000".into());
    a.position = 2;
    let b = deck(2, Some(1), "İngilizce");
    let mut c = deck(3, None, "Matematik");
    c.position = 1;
    fx.write(|tx| {
        for d in [&a, &b, &c] {
            tx.insert_deck(d).unwrap();
        }
    });
    fx.read(|r| {
        assert_eq!(r.get_deck(did(1)).unwrap().unwrap(), a);
        assert_eq!(r.get_deck(did(2)).unwrap().unwrap().parent_id, Some(did(1)));
        assert!(r.get_deck(did(9)).unwrap().is_none());
        let names: Vec<_> = r
            .list_decks(false)
            .unwrap()
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(
            names,
            ["İngilizce", "Matematik", "Dil"],
            "by position, then name"
        );
    });
}

#[test]
fn find_child_is_unicode_case_insensitive_and_skips_trash() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "Dil")).unwrap();
        tx.insert_deck(&deck(2, Some(1), "Çeviri")).unwrap();
        tx.insert_deck(&deck(3, None, "Çeviri")).unwrap();
    });
    fx.read(|r| {
        assert_eq!(
            r.find_child_deck(Some(did(1)), "çEVİRİ")
                .unwrap()
                .map(|d| d.id),
            None,
            "İ lowercases to i̇ (no Turkish rule, see docs/10 §6)"
        );
        assert_eq!(
            r.find_child_deck(Some(did(1)), "çeviri")
                .unwrap()
                .unwrap()
                .id,
            did(2)
        );
        assert_eq!(
            r.find_child_deck(None, "ÇEVIRI").unwrap().unwrap().id,
            did(3)
        );
        assert_eq!(r.find_child_deck(None, "dil").unwrap().unwrap().id, did(1));
        assert!(r.find_child_deck(None, "nope").unwrap().is_none());
    });
    fx.write(|tx| tx.set_decks_deleted(&[did(3)], Some(Timestamp(5))).unwrap());
    fx.read(|r| assert!(r.find_child_deck(None, "Çeviri").unwrap().is_none()));
}

#[test]
fn move_and_rename_via_update() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, None, "B")).unwrap();
        tx.insert_deck(&deck(3, Some(1), "C")).unwrap();
    });
    fx.write(|tx| {
        let mut c = tx.get_deck(did(3)).unwrap().unwrap();
        c.parent_id = Some(did(2));
        c.name = "C2".into();
        c.updated_at = Timestamp(99);
        tx.update_deck(&c).unwrap();
        assert!(matches!(
            tx.update_deck(&deck(42, None, "X")),
            Err(StorageError::NotFound { entity: "deck", .. })
        ));
    });
    fx.read(|r| {
        let c = r.get_deck(did(3)).unwrap().unwrap();
        assert_eq!(
            (c.parent_id, c.name.as_str(), c.updated_at),
            (Some(did(2)), "C2", Timestamp(99))
        );
        assert_eq!(r.deck_ancestry(did(3)).unwrap(), [did(2), did(3)]);
    });
}

#[test]
fn ancestry_root_first_and_errors() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, Some(1), "B")).unwrap();
        tx.insert_deck(&deck(3, Some(2), "C")).unwrap();
    });
    fx.read(|r| {
        assert_eq!(r.deck_ancestry(did(3)).unwrap(), [did(1), did(2), did(3)]);
        assert_eq!(r.deck_ancestry(did(1)).unwrap(), [did(1)]);
        assert!(matches!(
            r.deck_ancestry(did(9)),
            Err(StorageError::NotFound { entity: "deck", .. })
        ));
    });
    // A cycle written behind the app's back is reported, not looped on.
    fx.raw()
        .execute(
            "UPDATE decks SET parent_id = ?1 WHERE id = ?2",
            [did(3).to_string(), did(1).to_string()],
        )
        .unwrap();
    fx.read(|r| {
        assert!(matches!(
            r.deck_ancestry(did(3)),
            Err(StorageError::Corrupt { .. })
        ))
    });
}

#[test]
fn soft_delete_and_restore() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, Some(1), "B")).unwrap();
    });
    fx.write(|tx| {
        assert_eq!(
            tx.set_decks_deleted(&[did(1), did(2)], Some(Timestamp(7)))
                .unwrap(),
            2
        );
        assert_eq!(
            tx.set_decks_deleted(&[did(1)], Some(Timestamp(8))).unwrap(),
            0,
            "already trashed: unchanged"
        );
    });
    fx.read(|r| {
        assert!(r.list_decks(false).unwrap().is_empty());
        assert_eq!(r.list_decks(true).unwrap().len(), 2);
        let a = r.get_deck(did(1)).unwrap().unwrap();
        assert_eq!(
            a.deleted_at,
            Some(Timestamp(7)),
            "fetch by id returns trashed rows"
        );
    });
    // A new live deck can take the trashed name; restoring then conflicts.
    fx.write(|tx| tx.insert_deck(&deck(3, None, "A")).unwrap());
    {
        let mut tx = fx.store.write().unwrap();
        assert!(matches!(
            tx.set_decks_deleted(&[did(1)], None),
            Err(StorageError::Constraint { .. })
        ));
    }
    fx.write(|tx| {
        tx.set_decks_deleted(&[did(3)], Some(Timestamp(9))).unwrap();
        assert_eq!(
            tx.set_decks_deleted(&[did(1), did(2), did(9)], None)
                .unwrap(),
            2
        );
        assert_eq!(tx.set_decks_deleted(&[did(1)], None).unwrap(), 0);
    });
    fx.read(|r| assert_eq!(r.list_decks(false).unwrap().len(), 2));
}

#[test]
fn card_counts_per_deck() {
    let fx = Fx::seeded();
    let now = Timestamp(1_000);
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, None, "B")).unwrap();
        for n in 1..=7 {
            put_note(tx, &note(n, &format!("w{n}"), "", &[], 10), "").unwrap();
        }
        // deck 1: new, new(suspended), learning due, review due, review not due, new(buried)
        let mut cards = vec![card(1, 1, 1), card(2, 2, 1), card(3, 3, 1), card(4, 4, 1)];
        cards[1].suspended = true;
        cards[2].scheduling.state = CardState::Learning;
        cards[2].scheduling.due = Some(Timestamp(900));
        cards[3].scheduling.state = CardState::Review;
        cards[3].scheduling.due = Some(Timestamp(1_000));
        let mut later = card(5, 5, 1);
        later.scheduling.state = CardState::Review;
        later.scheduling.due = Some(Timestamp(5_000));
        let mut buried = card(6, 6, 1);
        buried.buried_until = Some(Timestamp(2_000));
        cards.extend([later, buried]);
        // deck 2: one new card whose note is trashed (not counted).
        cards.push(card(7, 7, 2));
        for c in &cards {
            tx.insert_card(c).unwrap();
        }
        tx.set_notes_deleted(&[nid(7)], Some(Timestamp(1))).unwrap();
    });
    fx.read(|r| {
        let counts = r.card_counts_by_deck(now).unwrap();
        assert_eq!(
            counts,
            vec![(
                did(1),
                CardCounts {
                    new: 1,
                    learning: 1,
                    review_due: 1,
                    total: 6
                }
            )]
        );
    });
    // Trashed cards are not counted either.
    fx.write(|tx| tx.set_cards_deleted(&[cid(1)], Some(Timestamp(2))).unwrap());
    fx.read(|r| assert_eq!(r.card_counts_by_deck(now).unwrap()[0].1.total, 5));
}

#[test]
fn presets_upsert_get_list() {
    let fx = Fx::new();
    let mut p = Preset {
        id: PresetId(super::u(1)),
        name: "Default".into(),
        config: PresetConfig::default(),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    };
    p.config
        .extra
        .insert("future".into(), serde_json::json!({"x": 1}));
    fx.write(|tx| tx.upsert_preset(&p).unwrap());
    fx.read(|r| assert_eq!(r.get_preset(p.id).unwrap().unwrap(), p));
    p.name = "Yoğun".into();
    p.config.new_per_day = 50;
    fx.write(|tx| {
        tx.upsert_preset(&p).unwrap();
        let mut d = deck(1, None, "A");
        d.preset_id = Some(p.id);
        tx.insert_deck(&d).unwrap();
    });
    fx.read(|r| {
        assert_eq!(r.list_presets().unwrap(), vec![p.clone()]);
        assert_eq!(r.get_deck(did(1)).unwrap().unwrap().preset_id, Some(p.id));
        assert!(r.get_preset(PresetId(super::u(2))).unwrap().is_none());
    });
}

/// Snapshot of `deleted_at` for decks 1..=5, cards 1..=6, notes 1..=6.
fn trash_state(fx: &Fx) -> Vec<Option<Timestamp>> {
    fx.read(|r| {
        let mut v = Vec::new();
        for n in 1..=5 {
            v.push(r.get_deck(did(n)).unwrap().unwrap().deleted_at);
        }
        for n in 1..=6 {
            v.push(r.get_card(cid(n)).unwrap().unwrap().deleted_at);
        }
        for n in 1..=6 {
            v.push(r.get_note(nid(n)).unwrap().unwrap().deleted_at);
        }
        v
    })
}

/// Tree: 1 ─ 2 ─ 3, 1 ─ 4 (trashed earlier), 5 elsewhere.
/// Notes (home → cards): n1 (1 → c1@1), n2 (1 → c2@2, c3@5: keeps a live card),
/// n3 (5 → c4@3), n4 (4 → c5@4, trashed earlier with deck 4), n5 (2 → no cards),
/// n6 (5 → c6@5).
fn trash_fixture() -> Fx {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "Root")).unwrap();
        tx.insert_deck(&deck(2, Some(1), "Child")).unwrap();
        tx.insert_deck(&deck(3, Some(2), "Grandchild")).unwrap();
        tx.insert_deck(&deck(4, Some(1), "Old")).unwrap();
        tx.insert_deck(&deck(5, None, "Other")).unwrap();
        for (n, home) in [(1, 1), (2, 1), (3, 5), (4, 4), (5, 2), (6, 5)] {
            tx.insert_note(
                &super::homed(note(n, &format!("w{n}"), "", &[], 1), did(home)),
                "",
            )
            .unwrap();
        }
        for (c, n, d) in [
            (1, 1, 1),
            (2, 2, 2),
            (3, 2, 5),
            (4, 3, 3),
            (5, 4, 4),
            (6, 6, 5),
        ] {
            let mut c = card(c, n, d);
            c.template_key = format!("t{}", c.id);
            tx.insert_card(&c).unwrap();
        }
        // Deck 4 was trashed earlier (with its card and note).
        tx.set_decks_deleted(&[did(4)], Some(Timestamp(50)))
            .unwrap();
        tx.set_cards_deleted(&[cid(5)], Some(Timestamp(50)))
            .unwrap();
        tx.set_notes_deleted(&[nid(4)], Some(Timestamp(50)))
            .unwrap();
    });
    fx
}

#[test]
fn deck_delete_trashes_subtree_cards_and_orphaned_notes_with_one_timestamp() {
    let fx = trash_fixture();
    let before = trash_state(&fx);
    let t = Timestamp(100);
    let set = fx.write(|tx| tx.trash_deck_subtree(did(1), t).unwrap());
    assert_eq!(
        set,
        DeckTrashSet {
            decks: vec![did(1), did(2), did(3)],
            cards: vec![cid(1), cid(2), cid(4)],
            // n2 keeps c3 in deck 5; n3 is homed elsewhere but lost its only card; n5 is a
            // card-less note homed in deck 2; n4 was already trashed.
            notes: vec![nid(1), nid(3), nid(5)],
        }
    );
    let s = Some(t);
    let old = Some(Timestamp(50));
    assert_eq!(
        trash_state(&fx),
        [
            s, s, s, old, None, // decks
            s, s, None, s, old, None, // cards
            s, None, s, old, s, None, // notes
        ]
    );
    // Trashing again is a no-op; unknown decks are NotFound.
    fx.write(|tx| {
        assert!(tx
            .trash_deck_subtree(did(1), Timestamp(200))
            .unwrap()
            .is_empty());
        assert!(matches!(
            tx.trash_deck_subtree(did(99), t),
            Err(StorageError::NotFound { entity: "deck", .. })
        ));
    });

    // Undo: restore exactly the listed ids; redo: trash them again with the same time.
    fx.write(|tx| assert_eq!(tx.set_trash_set_deleted(&set, None).unwrap(), 9));
    assert_eq!(trash_state(&fx), before);
    fx.write(|tx| assert_eq!(tx.set_trash_set_deleted(&set, Some(t)).unwrap(), 9));

    // Trash restore of the deck brings back what went with it, not deck 4's older trash.
    let restored = fx.write(|tx| tx.restore_deck_subtree(did(1)).unwrap());
    assert_eq!(restored, set);
    assert_eq!(trash_state(&fx), before);
    fx.write(|tx| assert!(tx.restore_deck_subtree(did(1)).unwrap().is_empty()));
}

#[test]
fn restoring_a_sub_deck_restores_only_its_part() {
    let fx = trash_fixture();
    let t = Timestamp(100);
    fx.write(|tx| tx.trash_deck_subtree(did(1), t).unwrap());
    let restored = fx.write(|tx| tx.restore_deck_subtree(did(2)).unwrap());
    assert_eq!(
        restored,
        DeckTrashSet {
            decks: vec![did(2), did(3)],
            cards: vec![cid(2), cid(4)],
            notes: vec![nid(3), nid(5)],
        }
    );
    fx.read(|r| {
        assert_eq!(r.get_deck(did(1)).unwrap().unwrap().deleted_at, Some(t));
        assert_eq!(r.get_note(nid(1)).unwrap().unwrap().deleted_at, Some(t));
    });
}
