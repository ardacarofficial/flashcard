//! v1.3 migration 1300: field roles, answer fields and labelled backs for the built-in note
//! types of an existing collection. The fixture is every built-in exactly as v1.2 shipped it
//! (both languages), written by the v1.2 builders.

use crate::migrate::{self, MIGRATIONS};
use crate::store::{SqliteStore, Store};
use fc_core::content::builtin::{self, PARTS_CSS};
use fc_core::i18n::Locale;
use fc_core::notetype::{CardTemplate, FieldDef, FieldKey, FieldKind, NoteType, NoteTypeKind};
use fc_core::{NoteTypeId, Timestamp};
use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;
use std::str::FromStr;

const V1_2: &str = include_str!("fixtures/builtin_notetypes_v1_2.json");

/// The v1.2 built-ins of `locale` as stored then.
fn v1_2(locale: Locale) -> Vec<NoteType> {
    let all: Value = serde_json::from_str(V1_2).unwrap();
    let lang = match locale {
        Locale::En => "en",
        Locale::Tr => "tr",
    };
    all[lang]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| NoteType {
            id: NoteTypeId::from_str(n["id"].as_str().unwrap()).unwrap(),
            name: n["name"].as_str().unwrap().to_owned(),
            kind: serde_json::from_value(n["kind"].clone()).unwrap(),
            fields: serde_json::from_value(n["fields"].clone()).unwrap(),
            templates: serde_json::from_value(n["templates"].clone()).unwrap(),
            css: n["css"].as_str().unwrap().to_owned(),
            sort_field: serde_json::from_value(n["sort_field"].clone()).unwrap(),
            version: 1,
            builtin: Some(n["builtin"].as_str().unwrap().to_owned()),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
            deleted_at: None,
        })
        .collect()
}

/// A collection at schema 1299 holding `types`, then opened by this build (1300 applied).
fn migrated(dir: &Path, types: &[NoteType]) -> SqliteStore {
    let path = dir.join("collection.db");
    let upto = MIGRATIONS.iter().position(|m| m.version >= 1300).unwrap();
    {
        let old = SqliteStore::open_with_migrations(&path, &MIGRATIONS[..upto]).unwrap();
        let mut tx = old.write().unwrap();
        for nt in types {
            tx.upsert_notetype(nt).unwrap();
        }
        tx.commit().unwrap();
    }
    SqliteStore::open(&path).unwrap()
}

fn get(store: &SqliteStore, id: NoteTypeId) -> NoteType {
    store.read().unwrap().get_notetype(id).unwrap().unwrap()
}

fn sql_1300() -> &'static str {
    MIGRATIONS.iter().find(|m| m.version == 1300).unwrap().sql
}

#[test]
fn migration_1300_upgrades_untouched_builtins_to_the_v1_3_definitions() {
    for locale in [Locale::En, Locale::Tr] {
        let dir = tempfile::tempdir().unwrap();
        let old = v1_2(locale);
        assert_eq!(old.len(), builtin::ALL.len());
        // The fixture is what v1.2 stored: no roles, no answer fields.
        assert!(old.iter().all(|nt| !nt.has_roles()));
        let store = migrated(dir.path(), &old);
        for b in builtin::ALL {
            let now = get(&store, b.id);
            let want = b.build(Timestamp(1), locale);
            assert_eq!(now.fields, want.fields, "{} fields", b.key);
            assert_eq!(now.templates, want.templates, "{} templates", b.key);
            assert_eq!(now.css, want.css, "{} css", b.key);
            assert_eq!(now.name, want.name);
            assert_eq!(now.sort_field, want.sort_field);
            // Only what changed is a new version.
            let before = old.iter().find(|o| o.id == b.id).unwrap();
            let changed = before.fields != want.fields
                || before.templates != want.templates
                || before.css != want.css;
            assert_eq!(now.version, if changed { 2 } else { 1 }, "{}", b.key);
            assert_eq!(now.updated_at > Timestamp(1), changed, "{}", b.key);
        }
        let subtitle = get(&store, builtin::find(builtin::SUBTITLE).unwrap().id);
        assert_eq!(
            subtitle.templates[0].answer_field,
            Some(FieldKey::new("meaning"))
        );
        assert_eq!(subtitle.css, PARTS_CSS);
    }
}

#[test]
fn migration_1300_leaves_customised_definitions_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut types = v1_2(Locale::En);
    let id_of = |key: &str| builtin::find(key).unwrap().id;
    let find = |types: &[NoteType], key: &str| {
        let id = id_of(key);
        types.iter().position(|n| n.id == id).unwrap()
    };
    // Subtitle: the user edited the back and renamed a field → the template keeps its text
    // and grading; roles still describe the built-in keys.
    let i = find(&types, builtin::SUBTITLE);
    types[i].templates[0].back.push_str("<p>mine</p>");
    types[i].fields[0].name = "Lemma".into();
    let custom_back = types[i].templates[0].back.clone();
    // Vocabulary: `meaning` became plain text (another use) and the CSS is the user's.
    let i = find(&types, "vocabulary");
    types[i].fields[1].kind = FieldKind::Text;
    types[i].css = ".card { color: red }".into();
    // Classic + reverse: the user renamed the reverse card only (names are not compared).
    let i = find(&types, "classic_reverse");
    types[i].templates[1].name = "Back to front".into();
    // A user note type copied from the subtitle card: no `builtin` marker → untouched.
    let mut copy = types[find(&types, builtin::SUBTITLE)].clone();
    copy.id = NoteTypeId::from_str("01900000-0000-7000-8000-0000000000ff").unwrap();
    copy.builtin = None;
    copy.name = "My subtitle copy".into();
    types.push(copy.clone());

    let store = migrated(dir.path(), &types);

    let st = get(&store, id_of(builtin::SUBTITLE));
    assert_eq!(st.templates[0].back, custom_back);
    assert_eq!(st.templates[0].answer_field, None);
    assert_eq!(st.fields[0].name, "Lemma");
    assert!(
        st.fields[0].role.is_some(),
        "roles follow the built-in keys"
    );
    assert_eq!(
        st.css, PARTS_CSS,
        "unused rules, but the CSS was the shipped one"
    );

    let v = get(&store, id_of("vocabulary"));
    assert_eq!(
        v.fields[1].role, None,
        "a field whose kind changed gets no role"
    );
    assert!(v.fields[0].role.is_some());
    assert_eq!(v.css, ".card { color: red }");
    // Its templates were untouched: they get the new backs (they name the same fields).
    assert_eq!(
        v.templates,
        builtin::vocabulary(Timestamp(1), Locale::En).templates
    );

    let r = get(&store, id_of("classic_reverse"));
    assert_eq!(r.templates[1].name, "Back to front");
    assert_eq!(r.templates[1].answer_field, Some(FieldKey::new("front")));

    let mine = get(&store, copy.id);
    assert_eq!(mine, copy);
}

#[test]
fn migration_1300_is_idempotent_and_touches_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let user = NoteType {
        id: NoteTypeId::from_str("01900000-0000-7000-8000-0000000000fe").unwrap(),
        name: "Mine".into(),
        kind: NoteTypeKind::Standard,
        fields: vec![FieldDef {
            key: FieldKey::new("front"),
            name: "Front".into(),
            kind: FieldKind::RichText,
            required: true,
            unique: false,
            searchable: true,
            default_value: None,
            role: None,
        }],
        templates: vec![CardTemplate {
            key: "forward".into(),
            name: "1".into(),
            front: "{{front}}".into(),
            back: "{{FrontSide}}\n<hr class=\"fc-answer\">\n{{back}}".into(),
            card_type: "classic".into(),
            answer_field: None,
        }],
        css: String::new(),
        sort_field: FieldKey::new("front"),
        version: 4,
        builtin: None,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        deleted_at: None,
    };
    let mut types = v1_2(Locale::Tr);
    types.push(user.clone());
    let store = migrated(dir.path(), &types);
    let after: Vec<NoteType> = store.read().unwrap().list_notetypes(true).unwrap();
    assert_eq!(get(&store, user.id), user);
    drop(store);

    // Running the statements again changes nothing (not even versions or times).
    let path = dir.path().join("collection.db");
    let conn = Connection::open(&path).unwrap();
    migrate::configure_connection(&conn).unwrap();
    conn.execute_batch(sql_1300()).unwrap();
    drop(conn);
    let store = SqliteStore::open(&path).unwrap();
    let again: Vec<NoteType> = store.read().unwrap().list_notetypes(true).unwrap();
    assert_eq!(after, again);
}

#[test]
fn migration_1300_on_a_fresh_collection_does_nothing() {
    // A new collection has no note types when 1300 runs (the app seeds v1.3 after): nothing
    // to do, nothing fails.
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(&dir.path().join("collection.db")).unwrap();
    assert!(store
        .read()
        .unwrap()
        .list_notetypes(true)
        .unwrap()
        .is_empty());
}
