use super::{nid, note, tag, Fx};
use crate::error::StorageError;
use crate::store::Store;
use fc_core::tag::Tag;
use fc_core::{TagId, Timestamp};

fn tag_names(fx: &Fx) -> Vec<(String, u32)> {
    fx.read(|r| {
        r.list_tags()
            .unwrap()
            .into_iter()
            .map(|(t, n)| (t.name.as_str().to_owned(), n))
            .collect()
    })
}

fn note_tags(fx: &Fx, n: u128) -> Vec<String> {
    fx.read(|r| {
        r.get_note(nid(n))
            .unwrap()
            .unwrap()
            .tags
            .iter()
            .map(|t| t.as_str().to_owned())
            .collect()
    })
}

#[test]
fn get_by_name_is_case_insensitive_including_unicode() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_note(&note(1, "a", "", &["Grammar", "Öğrenci::Şube"], 1), "")
            .unwrap()
    });
    fx.read(|r| {
        assert_eq!(
            r.get_tag_by_name(&tag("gRAMMAR"))
                .unwrap()
                .unwrap()
                .name
                .as_str(),
            "Grammar"
        );
        assert_eq!(
            r.get_tag_by_name(&tag("öĞRENCİ::şUBE"))
                .unwrap()
                .map(|t| t.name.as_str().to_owned()),
            None,
            "İ ≠ I under Unicode lowercase (docs/10 §6)"
        );
        assert_eq!(
            r.get_tag_by_name(&tag("öĞRENCI::şUBE"))
                .unwrap()
                .unwrap()
                .name
                .as_str(),
            "Öğrenci::Şube"
        );
        assert!(r.get_tag_by_name(&tag("nope")).unwrap().is_none());
    });
}

#[test]
fn created_tag_ids_are_deterministic_v7() {
    let ids: Vec<TagId> = (0..2)
        .map(|_| {
            let fx = Fx::seeded();
            fx.write(|tx| tx.insert_note(&note(1, "a", "", &["x"], 1234), "").unwrap());
            fx.read(|r| r.get_tag_by_name(&tag("x")).unwrap().unwrap())
        })
        .map(|t| {
            assert_eq!(t.created_at, Timestamp(1234));
            assert_eq!(t.id.0.get_version_num(), 7);
            t.id
        })
        .collect();
    assert_eq!(ids[0], ids[1]);
}

#[test]
fn derived_id_collision_is_salted() {
    let fx = Fx::seeded();
    fx.write(|tx| tx.insert_note(&note(1, "a", "", &["x"], 5), "").unwrap());
    let x = fx.read(|r| r.get_tag_by_name(&tag("x")).unwrap().unwrap());
    // Rename x → y (same id), then create a fresh "x" at the same instant.
    fx.write(|tx| {
        tx.upsert_tag(&Tag {
            name: tag("y"),
            ..x.clone()
        })
        .unwrap();
        tx.insert_note(&note(2, "b", "", &["x"], 5), "").unwrap();
    });
    let new_x = fx.read(|r| r.get_tag_by_name(&tag("x")).unwrap().unwrap());
    assert_ne!(new_x.id, x.id);
    assert_eq!(tag_names(&fx), [("x".to_owned(), 1), ("y".to_owned(), 1)]);
}

#[test]
fn hierarchical_rename_via_upsert() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_note(&note(1, "a", "", &["dil::fiil", "z"], 1), "")
            .unwrap();
        tx.insert_note(&note(2, "b", "", &["dil::fiil::düzensiz"], 2), "")
            .unwrap();
        tx.insert_note(&note(3, "c", "", &["dil::fiiller"], 3), "")
            .unwrap();
    });
    // What the app does for tags.rename dil::fiil → lang::verb.
    let (from, to) = (tag("dil::fiil"), tag("lang::verb"));
    fx.write(|tx| {
        for (t, _) in tx.list_tags().unwrap() {
            if let Some(new_name) = t.name.rebase(&from, &to) {
                tx.upsert_tag(&Tag {
                    name: new_name,
                    updated_at: Timestamp(9),
                    ..t
                })
                .unwrap();
            }
        }
    });
    assert_eq!(note_tags(&fx, 1), ["lang::verb", "z"]);
    assert_eq!(note_tags(&fx, 2), ["lang::verb::düzensiz"]);
    assert_eq!(note_tags(&fx, 3), ["dil::fiiller"]);
    // Renaming onto an existing name (ASCII case-insensitive) is a constraint error → merge.
    let z = fx.read(|r| r.get_tag_by_name(&tag("z")).unwrap().unwrap());
    let mut tx = fx.store.write().unwrap();
    assert!(matches!(
        tx.upsert_tag(&Tag {
            name: tag("LANG::VERB"),
            ..z
        }),
        Err(StorageError::Constraint { .. })
    ));
}

#[test]
fn merge_moves_references_and_deletes_source() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_note(&note(1, "a", "", &["old", "keep"], 1), "")
            .unwrap();
        tx.insert_note(&note(2, "b", "", &["new", "old"], 2), "")
            .unwrap();
        tx.insert_note(&note(3, "c", "", &["new"], 3), "").unwrap();
    });
    let get = |name: &str| fx.read(|r| r.get_tag_by_name(&tag(name)).unwrap().unwrap().id);
    let (old, new) = (get("old"), get("new"));
    fx.write(|tx| {
        assert_eq!(tx.merge_tag(old, new).unwrap(), 2);
        assert!(matches!(
            tx.merge_tag(old, new),
            Err(StorageError::NotFound { entity: "tag", .. })
        ));
        assert_eq!(tx.merge_tag(new, new).unwrap(), 0);
    });
    assert_eq!(note_tags(&fx, 1), ["new", "keep"], "position kept");
    assert_eq!(note_tags(&fx, 2), ["new"], "duplicate collapsed");
    assert_eq!(
        tag_names(&fx),
        [("keep".to_owned(), 1), ("new".to_owned(), 3)]
    );
}

#[test]
fn set_color_and_cleanup() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_note(&note(1, "a", "", &["a", "b"], 1), "")
            .unwrap()
    });
    let a = fx.read(|r| r.get_tag_by_name(&tag("a")).unwrap().unwrap());
    fx.write(|tx| {
        tx.upsert_tag(&Tag {
            color: Some("#00ff00".into()),
            updated_at: Timestamp(2),
            ..a.clone()
        })
        .unwrap();
        // Standalone tag row (e.g. created by tags.merge into a new name).
        tx.upsert_tag(&Tag {
            id: TagId(super::u(0x9999)),
            name: tag("lonely"),
            color: None,
            created_at: Timestamp(3),
            updated_at: Timestamp(3),
        })
        .unwrap();
    });
    fx.read(|r| {
        let a2 = r.get_tag_by_name(&tag("a")).unwrap().unwrap();
        assert_eq!(a2.color.as_deref(), Some("#00ff00"));
        assert_eq!(a2.created_at, a.created_at);
        assert_eq!(r.list_tags().unwrap().len(), 3);
    });
    let mut n = note(1, "a", "", &["a"], 1);
    n.updated_at = Timestamp(4);
    fx.write(|tx| {
        tx.update_note(&n, "").unwrap();
        assert_eq!(tx.delete_unused_tags().unwrap(), 2, "b and lonely");
    });
    assert_eq!(tag_names(&fx), [("a".to_owned(), 1)]);
}
