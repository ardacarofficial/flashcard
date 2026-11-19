//! v0.3 content storage: media rows/files (dedup), note references, flags, migration 0200.

use super::*;
use crate::repo::{FlagDef, MediaRecord, NoteRefs};
use fc_core::content::media::media_hash;

fn media(bytes: &[u8], name: &str) -> MediaRecord {
    MediaRecord {
        hash: media_hash(bytes),
        mime: "image/png".into(),
        size: bytes.len() as u64,
        original_name: Some(name.into()),
        ref_count: 0,
        created_at: Timestamp(5),
    }
}

fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn migration_0200_creates_tables_and_default_flags() {
    let fx = Fx::new();
    let conn = fx.raw();
    for t in [
        "note_media",
        "note_link_refs",
        "note_keys",
        "card_flag_defs",
    ] {
        assert!(
            crate::migrate::schema_object_exists(&conn, t).unwrap(),
            "{t}"
        );
    }
    let flags = fx.read(|r| r.list_flag_defs().unwrap());
    assert_eq!(flags.len(), 7);
    assert_eq!(flags[0].color, "red");
    assert!(flags.iter().all(|f| f.name.is_none()));
    // Flag numbers are constrained.
    assert!(conn
        .execute(
            "INSERT INTO card_flag_defs (flag, color, updated_at) VALUES (8, 'x', 0)",
            []
        )
        .is_err());
}

/// docs/04 v0.3 acceptance: the same media added twice is stored as one file.
#[test]
fn same_media_twice_is_one_file_and_one_row() {
    let fx = Fx::new();
    let bytes = b"\x89PNG\r\n\x1a\nfake image";
    let m = media(bytes, "a.png");
    for name in ["a.png", "copy-of-a.png"] {
        fx.write(|tx| {
            tx.write_media_file(&m.hash, bytes).unwrap();
            tx.insert_media(&MediaRecord {
                original_name: Some(name.into()),
                ..m.clone()
            })
            .unwrap();
        });
    }
    let media_dir = fx.dir.path().join("media");
    let files = files_under(&media_dir);
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(
        files[0],
        crate::media::media_file_path(&fx.db_path(), &m.hash).unwrap()
    );
    assert_eq!(std::fs::read(&files[0]).unwrap(), bytes);
    let rows = fx.read(|r| r.list_media().unwrap());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].original_name.as_deref(), Some("a.png"));
    assert_eq!(
        fx.read(|r| r.read_media_file(&m.hash).unwrap()).as_deref(),
        Some(&bytes[..])
    );
    assert!(fx
        .read(|r| r.read_media_file(&media_hash(b"other")).unwrap())
        .is_none());
    assert!(fx.read(|r| r.read_media_file("../x").is_err()));
}

#[test]
fn in_memory_store_keeps_media_in_memory() {
    let store = SqliteStore::open_in_memory().unwrap();
    let bytes = b"abc";
    let h = media_hash(bytes);
    assert!(store.read().unwrap().read_media_file(&h).unwrap().is_none());
    let mut tx = store.write().unwrap();
    tx.write_media_file(&h, bytes).unwrap();
    tx.write_media_file(&h, bytes).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        store
            .read()
            .unwrap()
            .read_media_file(&h)
            .unwrap()
            .as_deref(),
        Some(&bytes[..])
    );
}

#[test]
fn note_refs_drive_ref_counts_links_and_keys() {
    let fx = Fx::seeded();
    let (a, b) = (b"one".as_slice(), b"two".as_slice());
    let (ma, mb) = (media(a, "a"), media(b, "b"));
    fx.write(|tx| {
        tx.insert_media(&ma).unwrap();
        tx.insert_media(&mb).unwrap();
        add_note(tx, &note(1, "Porsuk", "badger", &[], 10), HOME);
        add_note(tx, &note(2, "Kedi", "cat [[porsuk]]", &[], 11), HOME);
        add_note(tx, &note(3, "Köpek", "dog", &[], 12), HOME);
        tx.set_note_refs(
            nid(1),
            &NoteRefs {
                key: "porsuk".into(),
                links: vec![],
                media: vec![ma.hash.clone(), mb.hash.clone()],
            },
        )
        .unwrap();
        tx.set_note_refs(
            nid(2),
            &NoteRefs {
                key: "kedi".into(),
                links: vec![("porsuk".into(), "porsuk".into())],
                media: vec![ma.hash.clone()],
            },
        )
        .unwrap();
        tx.set_note_refs(
            nid(3),
            &NoteRefs {
                key: "köpek".into(),
                links: vec![("porsuk".into(), "p".into())],
                media: vec![],
            },
        )
        .unwrap();
    });
    let rc = |h: &str| fx.read(|r| r.get_media(h).unwrap().unwrap().ref_count);
    assert_eq!((rc(&ma.hash), rc(&mb.hash)), (2, 1));
    assert_eq!(fx.read(|r| r.note_media(nid(1)).unwrap()).len(), 2);
    assert_eq!(
        fx.read(|r| r.notes_linking_to("porsuk").unwrap()),
        vec![nid(2), nid(3)]
    );
    assert_eq!(
        fx.read(|r| r
            .notes_by_link_key(&["porsuk".into(), "köpek".into(), "nope".into()])
            .unwrap()),
        vec![("köpek".into(), nid(3)), ("porsuk".into(), nid(1))]
    );
    assert_eq!(
        fx.read(|r| r.notes_with_key_prefix("kö", 10).unwrap()),
        vec![(nid(3), "Köpek".to_owned())]
    );
    assert_eq!(
        fx.read(|r| r.notes_with_key_prefix("", 2).unwrap()).len(),
        2
    );

    // Replacing refs drops old ones and recounts; unreferenced media can be removed.
    fx.write(|tx| {
        tx.set_note_refs(
            nid(1),
            &NoteRefs {
                key: "porsuk".into(),
                links: vec![],
                media: vec![],
            },
        )
        .unwrap();
        assert!(tx.delete_unreferenced_media(&mb.hash).unwrap());
        assert!(!tx.delete_unreferenced_media(&ma.hash).unwrap());
    });
    assert_eq!(rc(&ma.hash), 1);
    assert!(fx.read(|r| r.get_media(&mb.hash).unwrap()).is_none());

    // Trashed notes neither link nor resolve.
    fx.write(|tx| {
        tx.set_notes_deleted(&[nid(2)], Some(Timestamp(50)))
            .unwrap()
    });
    assert_eq!(
        fx.read(|r| r.notes_linking_to("porsuk").unwrap()),
        vec![nid(3)]
    );
    assert_eq!(
        fx.read(|r| r.note_ids_of_notetype(classic().id, false).unwrap()),
        vec![nid(1), nid(3)]
    );
    assert_eq!(
        fx.read(|r| r.note_ids_of_notetype(classic().id, true).unwrap())
            .len(),
        3
    );
}

#[test]
fn flag_names_are_updatable() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.set_flag_def(&FlagDef {
            flag: 2,
            name: Some("Zor".into()),
            color: "orange".into(),
            updated_at: Timestamp(9),
        })
        .unwrap();
        assert!(tx
            .set_flag_def(&FlagDef {
                flag: 9,
                name: None,
                color: "x".into(),
                updated_at: Timestamp(9),
            })
            .is_err());
    });
    let flags = fx.read(|r| r.list_flag_defs().unwrap());
    assert_eq!(flags[1].name.as_deref(), Some("Zor"));
    assert_eq!(flags[1].updated_at, Timestamp(9));
}
