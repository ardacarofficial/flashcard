//! Phase-2 integration, round 2 (v0.5 import/export × v0.3 card types × v0.2 study), end to
//! end through the API on a SQLite file (media live next to it):
//!
//! * a CSV import maps a column to the deck's "Seviye" classification and creates the option
//!   it lacks;
//! * an Anki `.apkg` with cloze, image and audio notes, a purple flag and review history
//!   (one review from today) lands on the v0.3 built-in note types; a second package with the
//!   same picture under another name adds no media: the files on disk are deduplicated;
//! * imported cards render through the card type engines (sanitized `media:` image/audio,
//!   cloze); the imported history is `imported` and does not use up today's review limit;
//!   studying a card writes a normal review;
//! * backup → the whole collection folder is deleted → a fresh collection restores the
//!   backup → every table, the media files and the classifications are identical.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::TestApp;
use fc_api::v1::attributes::{DefineAttribute, NewAttributeOption};
use fc_api::v1::cardtypes::CardRef;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::io::*;
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::study::{AnswerCard, StartSession, StudyKind, StudyMode};
use fc_core::attribute::{AttributeKind, AttributeValue, EntityRef};
use fc_core::card::{Card, CardState, Rating};
use fc_core::content::builtin;
use fc_core::content::media::media_hash;
use fc_core::preset::PresetConfig;
use fc_core::CardId;
use fc_interop::anki::fixtures as anki_fx;
use fc_interop::anki::{AnkiCard, AnkiNote, AnkiRevlog};
use fc_storage::repo::{CardFilter, CollectionDump, REVIEW_MODE_IMPORTED};
use fc_storage::SqliteStore;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// `TestApp`'s clock start (2026-01-01T00:00Z).
const TODAY: i64 = 1_767_225_600_000;

fn open(db: &Path) -> TestApp {
    TestApp::new(Arc::new(SqliteStore::open(db).unwrap()))
}

fn finish(t: &TestApp, job: &JobDto) -> JobDto {
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: job.id })
}

fn result<T: DeserializeOwned>(job: &JobDto) -> T {
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    serde_json::from_value(job.result.clone().unwrap()).unwrap()
}

fn request(source: ImportSource, deck: Option<fc_core::DeckId>) -> StartImport {
    StartImport {
        source,
        format: None,
        text_options: None,
        deck_id: deck,
        notetype_id: None,
        columns: None,
        skip_first_row: None,
        duplicates: None,
        create_missing_options: None,
        include_history: None,
    }
}

fn import(t: &TestApp, req: StartImport) -> ImportReport {
    let job = t.ok::<IoImportStart>(req);
    result(&finish(t, &job))
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

/// Every file under `<collection>/media`: hash file name → bytes.
fn media_files(db: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let root = db.parent().unwrap().join("media");
    for shard in std::fs::read_dir(root).unwrap().flatten() {
        for f in std::fs::read_dir(shard.path()).unwrap().flatten() {
            out.insert(
                f.file_name().to_string_lossy().into_owned(),
                std::fs::read(f.path()).unwrap(),
            );
        }
    }
    out
}

fn all_notes() -> fc_api::v1::notes::ListNotes {
    fc_api::v1::notes::ListNotes {
        deck_id: None,
        include_subdecks: None,
        page: Default::default(),
    }
}

fn all_attributes() -> fc_api::v1::attributes::ListAttributes {
    fc_api::v1::attributes::ListAttributes {
        scope_deck_id: None,
        include_deleted: None,
    }
}

fn dump(t: &TestApp) -> CollectionDump {
    let mut d = t.store.read().unwrap().dump_collection().unwrap();
    // Job rows record the backup/restore jobs themselves; a restore starts a fresh oplog
    // (checked separately).
    d.tables.retain(|t| t.name != "jobs" && t.name != "oplog");
    d
}

fn cards_of_deck(t: &TestApp) -> Vec<Card> {
    t.store
        .read()
        .unwrap()
        .list_cards(&CardFilter::default())
        .unwrap()
}

/// The sample package plus a review of the dog card from today (Anki review row).
fn anki_package() -> Vec<u8> {
    let mut col = anki_fx::sample();
    col.revlog.push(AnkiRevlog {
        id: TODAY - 60_000,
        cid: 1_610_000_000_001,
        ease: 3,
        ivl: 15,
        last_ivl: 4,
        factor: 2500,
        time: 3000,
        rtype: 1,
    });
    fc_interop::anki::write_package(&col, anki_fx::NOW).unwrap()
}

/// Another package: one Basic note whose picture is the sample's cat under another name.
fn second_package() -> Vec<u8> {
    let base = anki_fx::sample();
    let col = fc_interop::anki::AnkiCollection {
        notes: vec![AnkiNote {
            id: 1_630_000_000_000,
            guid: "second-cat".into(),
            mid: anki_fx::BASIC,
            modified: 1_650_000_000,
            tags: vec![],
            fields: vec![
                r#"yine kedi <img src="kopya.jpg">"#.into(),
                "kedi yine".into(),
            ],
        }],
        cards: vec![AnkiCard {
            id: 1_630_000_000_001,
            nid: 1_630_000_000_000,
            did: anki_fx::WORDS,
            ord: 0,
            modified: 1_650_000_000,
            ctype: 0,
            queue: 0,
            due: 1,
            ivl: 0,
            factor: 0,
            reps: 0,
            lapses: 0,
            left: 0,
            odue: 0,
            odid: 0,
            flags: 0,
            data: String::new(),
        }],
        revlog: vec![],
        media: vec![("kopya.jpg".into(), anki_fx::CAT.to_vec())],
        ..base
    };
    fc_interop::anki::write_package(&col, anki_fx::NOW).unwrap()
}

#[test]
fn anki_import_study_backup_wipe_restore_is_identical() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection").join("collection.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let t = open(&db);
    t.app.set_data_dir(dir.path().join("app-data"));

    // --- CSV import into a deck with a "Seviye" classification (created option).
    let lang = t.ok::<DecksCreate>(CreateDeck {
        path: "Lang".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    let seviye = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(lang.id),
        key: Some("seviye".into()),
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: vec![NewAttributeOption {
            label: "A1".into(),
            color: None,
        }],
    });
    let mut csv = request(
        ImportSource::Text {
            text: "ön;arka;seviye\nev;house;A1\nrüya;dream;B2".into(),
        },
        Some(lang.id),
    );
    csv.skip_first_row = Some(true);
    csv.create_missing_options = Some(true);
    csv.columns = Some(vec![
        ColumnTarget::Field {
            key: "front".into(),
        },
        ColumnTarget::Field { key: "back".into() },
        ColumnTarget::Attribute { attr_id: seviye.id },
    ]);
    let r = import(&t, csv);
    assert_eq!((r.added_notes, r.added_options), (2, 1), "{r:?}");
    let def = t.ok::<AttributesGet>(fc_api::v1::attributes::AttributeRef { id: seviye.id });
    let b2 = def.options.iter().find(|o| o.label == "B2").unwrap().id;
    let notes = t.ok::<NotesList>(all_notes()).items;
    let ruya = notes.iter().find(|n| n.sort_field == "rüya").unwrap();
    assert_eq!(
        t.store
            .read()
            .unwrap()
            .get_attribute_values(EntityRef::Note(ruya.id))
            .unwrap(),
        vec![(seviye.id, AttributeValue::Choice(b2))]
    );

    // --- Anki packages: built-in note types, media deduplicated on disk.
    let apkg = write(dir.path(), "deck.apkg", &anki_package());
    let r = import(
        &t,
        request(
            ImportSource::File {
                path: apkg.to_string_lossy().into_owned(),
            },
            None,
        ),
    );
    assert_eq!(
        (r.added_notes, r.added_cards, r.added_media),
        (4, 5, 2),
        "{r:?}"
    );
    assert_eq!((r.added_notetypes, r.added_reviews), (0, 5));
    let again = write(dir.path(), "again.apkg", &second_package());
    let r2 = import(
        &t,
        request(
            ImportSource::File {
                path: again.to_string_lossy().into_owned(),
            },
            None,
        ),
    );
    assert_eq!(
        (r2.added_notes, r2.added_media),
        (1, 0),
        "same picture: no new media"
    );
    let files = media_files(&db);
    let cat = media_hash(anki_fx::CAT);
    let hello = media_hash(anki_fx::HELLO);
    assert_eq!(files.keys().cloned().collect::<Vec<_>>(), {
        let mut v = vec![cat.clone(), hello.clone()];
        v.sort();
        v
    });
    assert_eq!(files[&cat], anki_fx::CAT);
    let cat_media = t.ok::<MediaGet>(fc_api::v1::cardtypes::MediaRef { hash: cat.clone() });
    assert_eq!(cat_media.ref_count, 2, "two notes, one file");
    // The imported notes sit under Lang (path match) and inherit Seviye.
    let decks = t.ok::<DecksList>(Empty {}).decks;
    let words: &DeckDto = decks.iter().find(|d| d.path == "Lang::Words").unwrap();
    assert_eq!(words.parent_id, Some(lang.id));

    // --- Rendering through the v0.3 engines.
    let cards = cards_of_deck(&t);
    let render = |id: CardId| t.ok::<CardsRender>(CardRef { id }).sides;
    let note_front = |c: &Card| {
        t.store
            .read()
            .unwrap()
            .get_note(c.note_id)
            .unwrap()
            .unwrap()
            .fields[&fc_core::notetype::FieldKey::new("front")]
            .clone()
    };
    let classic: Vec<&Card> = cards
        .iter()
        .filter(|c| c.card_type == "classic" && c.template_key == "forward")
        .collect();
    let cat_card = classic
        .iter()
        .find(|c| note_front(c).contains("img") && note_front(c).contains(&cat))
        .unwrap();
    assert!(render(cat_card.id)
        .question
        .contains(&format!("<img src=\"media:{cat}\"")));
    let hello_card = classic
        .iter()
        .find(|c| note_front(c).starts_with("hello"))
        .unwrap();
    let hq = render(hello_card.id).question;
    assert!(
        hq.contains("<audio controls") && hq.contains(&format!("src=\"media:{hello}\"")),
        "{hq}"
    );
    let cloze_card = cards.iter().find(|c| c.template_key == "c1").unwrap();
    let cloze = render(cloze_card.id);
    assert_eq!(cloze.card_type, builtin::engine::CLOZE);
    assert!(cloze.question.contains("cloze") && !cloze.question.contains("Ankara"));
    assert!(cloze.answer.contains("Ankara"));

    // --- Study: imported history (one review from today) does not use up today's limit.
    let dog = cards
        .iter()
        .find(|c| c.scheduling.state == CardState::Review && c.flags == 1 << 6)
        .unwrap();
    let history = t
        .store
        .read()
        .unwrap()
        .review_logs_for_card(dog.id)
        .unwrap();
    assert_eq!(history.len(), 4);
    assert!(history.iter().all(|r| r.mode == REVIEW_MODE_IMPORTED));
    assert!(history.iter().any(|r| r.created_at.0 == TODAY - 60_000));
    let preset = t.ok::<PresetsCreate>(fc_api::v1::presets::CreatePreset {
        name: "Bir tekrar".into(),
        config: Some(PresetConfig {
            reviews_per_day: 1,
            ..PresetConfig::default()
        }),
    });
    t.ok::<PresetsAssign>(fc_api::v1::presets::AssignPreset {
        deck_ids: vec![lang.id],
        preset_id: Some(preset.id),
    });
    let start = || {
        t.ok::<StudyStart>(StartSession {
            kind: StudyKind::Normal,
            mode: Some(StudyMode::Scheduled),
            deck_id: Some(lang.id),
            include_subdecks: None,
            filter: None,
            from_session: None,
            new_limit: None,
            order: None,
            tz_offset_mins: Some(0),
            modes: None,
            scope: None,
        })
    };
    let s = start();
    assert_eq!(
        s.counts.review, 1,
        "the imported review from today is not counted"
    );
    // Study the cards until the dog card comes up, answer it.
    let mut session = s;
    loop {
        let current = session.current.clone().expect("dog card in the session");
        assert!(!current.sides.question.is_empty());
        if current.card_id == dog.id {
            assert_eq!(current.flags, 1 << 6, "purple flag (Anki 7)");
            break;
        }
        session = t.ok::<StudyAnswer>(AnswerCard {
            typed_answer: None,
            session_id: session.id,
            card_id: current.card_id,
            rating: Rating::Good,
            duration_ms: 1_000,
            reveal_ms: Some(400),
        });
    }
    t.ok::<StudyAnswer>(AnswerCard {
        typed_answer: None,
        session_id: session.id,
        card_id: dog.id,
        rating: Rating::Good,
        duration_ms: 1_000,
        reveal_ms: Some(400),
    });
    let history = t
        .store
        .read()
        .unwrap()
        .review_logs_for_card(dog.id)
        .unwrap();
    assert_eq!(history.last().unwrap().mode, "normal");
    t.ok::<StudyEnd>(fc_api::v1::study::SessionRef {
        session_id: session.id,
    });
    assert_eq!(
        start().counts.review,
        0,
        "today's own review uses the limit"
    );

    // --- Backup, delete the whole collection folder, restore into a fresh collection.
    let before = dump(&t);
    let files_before = media_files(&db);
    let defs_before = t.ok::<AttributesList>(all_attributes());
    let backup = dir.path().join("all.fcbackup");
    let job = t.ok::<IoBackupCreate>(CreateBackup {
        path: backup.to_string_lossy().into_owned(),
    });
    let rep: ExportReport = result(&finish(&t, &job));
    assert_eq!(rep.media, 2);
    drop(t);
    std::fs::remove_dir_all(db.parent().unwrap()).unwrap();
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();

    let fresh = open(&db);
    fresh.app.set_data_dir(dir.path().join("app-data"));
    assert!(fresh.ok::<NotesList>(all_notes()).items.is_empty());
    let job = fresh.ok::<IoBackupRestore>(RestoreBackup {
        path: backup.to_string_lossy().into_owned(),
        safety_backup_path: Some(
            dir.path()
                .join("safety.fcbackup")
                .to_string_lossy()
                .into_owned(),
        ),
    });
    result::<RestoreReport>(&finish(&fresh, &job));
    assert_eq!(dump(&fresh), before, "every table is restored exactly");
    crate::modules::io::tests::assert_fresh_history(&fresh);
    assert_eq!(
        media_files(&db),
        files_before,
        "media files restored byte for byte"
    );
    assert_eq!(fresh.ok::<AttributesList>(all_attributes()), defs_before);
    // And the restored collection renders and studies.
    assert!(fresh
        .ok::<CardsRender>(CardRef { id: cat_card.id })
        .sides
        .question
        .contains(&format!("media:{cat}")));
}
