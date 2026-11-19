use super::{card, cid, deck, did, nid, note, Fx};
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
            tx.insert_note(&note(n, &format!("w{n}"), "", &[], 10), "")
                .unwrap();
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
