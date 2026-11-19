use super::{
    add_note, card, cid, classic, deck, did, nid, note, put_note, put_update, search_text, tag, Fx,
};
use crate::error::StorageError;
use crate::repo::*;
use fc_core::card::CardState;
use fc_core::notetype::builtin;
use fc_core::paging::PageRequest;
use fc_core::{NoteId, Timestamp};

fn fts(fx: &Fx, query: &str) -> Vec<NoteId> {
    let conn = fx.raw();
    let mut stmt = conn
        .prepare(
            "SELECT n.id FROM notes_fts f JOIN notes n ON n.fts_rowid = f.rowid
             WHERE notes_fts MATCH ?1 AND n.deleted_at IS NULL ORDER BY n.id",
        )
        .unwrap();
    let ids = stmt
        .query_map([query], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|s| s.unwrap().parse().unwrap())
        .collect();
    ids
}

fn fts_rows(fx: &Fx) -> i64 {
    fx.raw()
        .query_row("SELECT count(*) FROM notes_fts", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn notetype_upsert_get_list() {
    let fx = Fx::new();
    let mut nt = classic();
    fx.write(|tx| tx.upsert_notetype(&nt).unwrap());
    fx.read(|r| {
        assert_eq!(r.get_notetype(builtin::CLASSIC_ID).unwrap().unwrap(), nt);
        assert_eq!(r.list_notetypes(false).unwrap().len(), 1);
    });
    nt.name = "Klasik".into();
    nt.version = 2;
    nt.deleted_at = Some(Timestamp(5));
    fx.write(|tx| tx.upsert_notetype(&nt).unwrap());
    fx.read(|r| {
        assert_eq!(r.get_notetype(nt.id).unwrap().unwrap(), nt);
        assert!(r.list_notetypes(false).unwrap().is_empty());
        assert_eq!(r.list_notetypes(true).unwrap(), vec![nt.clone()]);
    });
}

#[test]
fn note_roundtrip_with_tags_in_order() {
    let fx = Fx::seeded();
    let n = note(1, "<b>run</b>", "koşmak", &["verb", "Dil::Fiil", "a1"], 100);
    fx.write(|tx| put_note(tx, &n, &search_text(&n)).unwrap());
    fx.read(|r| {
        let got = r.get_note(nid(1)).unwrap().unwrap();
        assert_eq!(got, n);
        assert_eq!(got.sort_field, "run");
        let names: Vec<_> = got.tags.iter().map(|t| t.as_str()).collect();
        assert_eq!(names, ["verb", "Dil::Fiil", "a1"]);
        assert!(r.get_note(nid(2)).unwrap().is_none());
    });
}

#[test]
fn case_duplicate_tags_are_stored_once_and_reuse_existing_rows() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        put_note(tx, &note(1, "a", "", &["Verb"], 1), "a").unwrap();
        put_note(tx, &note(2, "b", "", &["VERB", "x", "verb"], 2), "b").unwrap();
        put_note(tx, &note(3, "c", "", &["Çay"], 3), "c").unwrap();
        put_note(tx, &note(4, "d", "", &["çay"], 4), "d").unwrap();
    });
    fx.read(|r| {
        let tags = r.list_tags().unwrap();
        let names: Vec<_> = tags.iter().map(|(t, n)| (t.name.as_str(), *n)).collect();
        assert_eq!(names, [("Verb", 2), ("x", 1), ("Çay", 2)]);
        let n2 = r.get_note(nid(2)).unwrap().unwrap();
        let n2_tags: Vec<_> = n2.tags.iter().map(|t| t.as_str()).collect();
        assert_eq!(
            n2_tags,
            ["Verb", "x"],
            "stored spelling, first position wins"
        );
    });
}

#[test]
fn update_note_replaces_columns_tags_and_fts() {
    let fx = Fx::seeded();
    let n = note(1, "dog", "köpek", &["animal", "a1"], 10);
    fx.write(|tx| put_note(tx, &n, &search_text(&n)).unwrap());
    assert_eq!(fts(&fx, "dog"), [nid(1)]);
    assert_eq!(fts(&fx, "kopek"), [nid(1)], "diacritics folded");

    let mut m = note(1, "cat", "kedi", &["a2", "animal"], 10);
    m.updated_at = Timestamp(20);
    fx.write(|tx| {
        put_update(tx, &m, &search_text(&m)).unwrap();
        assert!(matches!(
            put_update(tx, &note(9, "x", "", &[], 1), "x"),
            Err(StorageError::NotFound { entity: "note", .. })
        ));
    });
    fx.read(|r| assert_eq!(r.get_note(nid(1)).unwrap().unwrap(), m));
    assert!(fts(&fx, "dog").is_empty());
    assert_eq!(fts(&fx, "kedi"), [nid(1)]);
    assert_eq!(fts_rows(&fx), 1, "FTS row updated in place");
    // Unused tag row (a1) is still there until cleanup.
    fx.write(|tx| assert_eq!(tx.delete_unused_tags().unwrap(), 1));
}

#[test]
fn fts_row_is_recreated_if_missing() {
    let fx = Fx::seeded();
    let n = note(1, "dog", "", &[], 10);
    fx.write(|tx| put_note(tx, &n, "dog").unwrap());
    fx.raw().execute("DELETE FROM notes_fts", []).unwrap();
    fx.write(|tx| put_update(tx, &n, "dog").unwrap());
    assert_eq!(fts(&fx, "dog"), [nid(1)]);
    assert_eq!(fts_rows(&fx), 1);
}

#[test]
fn soft_deleted_notes_leave_search_and_come_back() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        add_note(tx, &note(1, "dog", "", &["t"], 10), 1);
        add_note(tx, &note(2, "dogma", "", &["t"], 11), 1);
    });
    fx.write(|tx| {
        assert_eq!(
            tx.set_notes_deleted(&[nid(1)], Some(Timestamp(50)))
                .unwrap(),
            1
        );
    });
    assert!(fts(&fx, "dog").is_empty());
    fx.read(|r| {
        assert_eq!(
            r.get_note(nid(1)).unwrap().unwrap().deleted_at,
            Some(Timestamp(50))
        );
        assert_eq!(r.note_ids_with_tag(&tag("t")).unwrap(), [nid(2)]);
        assert_eq!(
            r.list_tags().unwrap()[0].1,
            1,
            "count excludes trashed notes"
        );
    });
    fx.write(|tx| {
        assert_eq!(tx.set_notes_deleted(&[nid(1)], None).unwrap(), 1);
        assert_eq!(tx.delete_unused_tags().unwrap(), 0);
    });
    assert_eq!(fts(&fx, "dog"), [nid(1)]);
    fx.read(|r| assert_eq!(r.get_note(nid(1)).unwrap().unwrap().tags, [tag("t")]));
}

#[test]
fn checksum_lookup_skips_trash_and_other_types() {
    let fx = Fx::seeded();
    let a = note(1, "<i>Dog</i>", "x", &[], 1);
    let b = note(2, "dog ", "y", &[], 2);
    fx.write(|tx| {
        put_note(tx, &a, "").unwrap();
        put_note(tx, &b, "").unwrap();
    });
    assert_eq!(a.checksum, b.checksum);
    fx.read(|r| {
        assert_eq!(
            r.find_notes_by_checksum(builtin::CLASSIC_ID, &a.checksum)
                .unwrap(),
            [nid(1), nid(2)]
        );
        assert!(r
            .find_notes_by_checksum(fc_core::NoteTypeId(super::u(77)), &a.checksum)
            .unwrap()
            .is_empty());
    });
    fx.write(|tx| tx.set_notes_deleted(&[nid(1)], Some(Timestamp(3))).unwrap());
    fx.read(|r| {
        assert_eq!(
            r.find_notes_by_checksum(builtin::CLASSIC_ID, &a.checksum)
                .unwrap(),
            [nid(2)]
        )
    });
}

#[test]
fn note_ids_with_tag_includes_children() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        put_note(tx, &note(1, "a", "", &["dil::fiil"], 1), "").unwrap();
        put_note(tx, &note(2, "b", "", &["Dil::Fiil::düzensiz"], 2), "").unwrap();
        put_note(tx, &note(3, "c", "", &["dil::fiiller"], 3), "").unwrap();
        put_note(tx, &note(4, "d", "", &["dil"], 4), "").unwrap();
    });
    fx.read(|r| {
        assert_eq!(
            r.note_ids_with_tag(&tag("DIL::fiil")).unwrap(),
            [nid(1), nid(2)]
        );
        assert_eq!(r.note_ids_with_tag(&tag("dil")).unwrap().len(), 4);
        assert!(r.note_ids_with_tag(&tag("yok")).unwrap().is_empty());
    });
}

#[test]
fn list_notes_pages_newest_first_with_filters() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, None, "B")).unwrap();
        // Two notes share updated_at = 30 to exercise the id tie-break.
        for (n, at, d) in [(1, 10, 1), (2, 20, 2), (3, 30, 1), (4, 30, 1), (5, 40, 2)] {
            add_note(tx, &note(n, &format!("w{n}"), "", &["t"], at), d);
        }
        put_note(tx, &note(6, "cardless", "", &[], 50), "").unwrap();
    });
    let mut seen = Vec::new();
    let mut page = PageRequest {
        cursor: None,
        limit: Some(2),
    };
    loop {
        let p = fx.read(|r| r.list_notes(&NoteFilter::default(), &page).unwrap());
        assert!(p.items.len() <= 2);
        seen.extend(p.items.iter().map(|row| (row.note.id, row.card_count)));
        match p.next_cursor {
            Some(c) => page.cursor = Some(c),
            None => break,
        }
    }
    assert_eq!(
        seen,
        [
            (nid(6), 0),
            (nid(5), 1),
            (nid(4), 1),
            (nid(3), 1),
            (nid(2), 1),
            (nid(1), 1)
        ]
    );

    let in_deck1 = NoteFilter {
        deck_ids: Some(vec![did(1)]),
        include_deleted: false,
    };
    fx.write(|tx| {
        tx.set_notes_deleted(&[nid(3)], Some(Timestamp(60)))
            .unwrap()
    });
    fx.read(|r| {
        let ids: Vec<_> = r
            .list_notes(&in_deck1, &PageRequest::default())
            .unwrap()
            .items
            .into_iter()
            .map(|row| row.note.id)
            .collect();
        assert_eq!(ids, [nid(4), nid(1)]);
        let with_trash = NoteFilter {
            include_deleted: true,
            ..in_deck1.clone()
        };
        let p = r.list_notes(&with_trash, &PageRequest::default()).unwrap();
        assert_eq!(p.items.len(), 3);
        assert!(p.next_cursor.is_none());
        assert_eq!(p.items[0].note.tags, [tag("t")], "rows carry tags");
        let none = NoteFilter {
            deck_ids: Some(vec![]),
            include_deleted: false,
        };
        assert!(r
            .list_notes(&none, &PageRequest::default())
            .unwrap()
            .items
            .is_empty());
        let bad = PageRequest {
            cursor: Some("garbage".into()),
            limit: None,
        };
        assert!(matches!(
            r.list_notes(&NoteFilter::default(), &bad),
            Err(StorageError::InvalidCursor(_))
        ));
    });
}

#[test]
fn cards_roundtrip_filter_and_soft_delete() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, None, "B")).unwrap();
        put_note(tx, &note(1, "a", "", &[], 1), "").unwrap();
        put_note(tx, &note(2, "b", "", &[], 2), "").unwrap();
    });
    let mut c1 = card(1, 1, 1);
    c1.scheduling.state = CardState::Review;
    c1.scheduling.due = Some(Timestamp(500));
    c1.scheduling.stability = Some(3.25);
    c1.scheduling.difficulty = Some(5.5);
    c1.scheduling.reps = 4;
    c1.scheduling.lapses = 1;
    c1.scheduling.last_review = Some(Timestamp(400));
    c1.flags = 0b101;
    c1.suspended = true;
    c1.buried_until = Some(Timestamp(600));
    c1.user_note = Some("hmm".into());
    let c2 = card(2, 2, 2);
    fx.write(|tx| {
        tx.insert_card(&c1).unwrap();
        tx.insert_card(&c2).unwrap();
        let mut dup = card(3, 1, 1);
        dup.template_key = c1.template_key.clone();
        assert!(matches!(
            tx.insert_card(&dup),
            Err(StorageError::Constraint { .. })
        ));
    });
    fx.read(|r| {
        assert_eq!(r.get_card(cid(1)).unwrap().unwrap(), c1);
        let all = r.list_cards(&CardFilter::default()).unwrap();
        assert_eq!(all, vec![c1.clone(), c2.clone()]);
        let by_deck = CardFilter {
            deck_ids: Some(vec![did(2)]),
            ..Default::default()
        };
        assert_eq!(r.list_cards(&by_deck).unwrap(), vec![c2.clone()]);
        let by_note = CardFilter {
            note_ids: Some(vec![nid(1)]),
            deck_ids: Some(vec![did(1), did(2)]),
            include_deleted: false,
        };
        assert_eq!(r.list_cards(&by_note).unwrap(), vec![c1.clone()]);
    });
    let mut moved = c2.clone();
    moved.deck_id = did(1);
    moved.updated_at = Timestamp(70);
    fx.write(|tx| {
        tx.update_card(&moved).unwrap();
        assert!(matches!(
            tx.update_card(&card(9, 1, 1)),
            Err(StorageError::NotFound { entity: "card", .. })
        ));
        assert_eq!(
            tx.set_cards_deleted(&[cid(1)], Some(Timestamp(80)))
                .unwrap(),
            1
        );
    });
    fx.read(|r| {
        assert_eq!(
            r.list_cards(&CardFilter::default()).unwrap(),
            vec![moved.clone()]
        );
        let with_trash = CardFilter {
            include_deleted: true,
            ..Default::default()
        };
        assert_eq!(r.list_cards(&with_trash).unwrap().len(), 2);
    });
    fx.write(|tx| assert_eq!(tx.set_cards_deleted(&[cid(1)], None).unwrap(), 1));
    fx.read(|r| assert_eq!(r.get_card(cid(1)).unwrap().unwrap().deleted_at, None));
}

#[test]
fn home_deck_is_stored_listed_and_changeable() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        tx.insert_deck(&deck(2, None, "B")).unwrap();
        tx.insert_note(&note(1, "a", "", &[], 1), did(1), "a")
            .unwrap();
        // The FK is enforced.
        assert!(matches!(
            tx.insert_note(&note(2, "b", "", &[], 1), did(9), "b"),
            Err(StorageError::Constraint { constraint }) if constraint == "foreign_key"
        ));
    });
    fx.read(|r| {
        assert_eq!(r.note_home_deck(nid(1)).unwrap(), Some(did(1)));
        assert_eq!(r.note_home_deck(nid(2)).unwrap(), None);
        let page = r
            .list_notes(&NoteFilter::default(), &PageRequest::default())
            .unwrap();
        assert_eq!(page.items[0].home_deck_id, did(1));
    });
    fx.write(|tx| {
        // update_note keeps the home deck.
        let mut n = note(1, "a2", "", &[], 1);
        n.updated_at = Timestamp(2);
        tx.update_note(&n, "a2").unwrap();
        assert_eq!(tx.note_home_deck(nid(1)).unwrap(), Some(did(1)));
        tx.set_note_home_deck(nid(1), did(2)).unwrap();
        assert!(matches!(
            tx.set_note_home_deck(nid(9), did(2)),
            Err(StorageError::NotFound { entity: "note", .. })
        ));
    });
    fx.read(|r| assert_eq!(r.note_home_deck(nid(1)).unwrap(), Some(did(2))));
}
