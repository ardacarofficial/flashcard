//! First-run guide and starter decks (v1.0, docs/02 §32).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::TestApp;
use fc_api::v1::attributes::{GetAttributeValues, ListAttributes};
use fc_api::v1::common::Empty;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::ListNotes;
use fc_api::v1::onboarding::*;
use fc_api::v1::presets::PresetForDeck;
use fc_api::ErrorCode;
use fc_core::attribute::{AttributeValue, EntityRef};
use fc_core::i18n::Locale;
use fc_core::ux::onboarding::StudySubject;
use fc_core::ux::starter;
use fc_core::paging::PageRequest;
use fc_core::DeckId;

fn apply(t: &TestApp, subject: StudySubject, starters: &[&str]) -> OnboardingResult {
    t.ok::<OnboardingApply>(ApplyOnboarding {
        subject,
        deck_name: None,
        starters: starters.iter().map(|s| (*s).to_owned()).collect(),
        locale: Some(Locale::Tr),
    })
}

fn notes_in(t: &TestApp, deck: DeckId) -> Vec<fc_api::v1::notes::NoteListItem> {
    t.ok::<NotesList>(ListNotes {
        deck_id: Some(deck),
        include_subdecks: Some(false),
        page: PageRequest {
            cursor: None,
            limit: Some(500),
        },
    })
    .items
}

fn deck_count(t: &TestApp) -> usize {
    t.ok::<DecksList>(Empty {}).decks.len()
}

/// "Onboarding asks what the user studies and sets up decks with the matching classification
/// template": English → CEFR deck with the language preset, the English starter deck under it
/// with a CEFR level on every note; the whole setup is one undo step.
#[test]
fn english_setup_uses_cefr_and_is_one_undo_step() {
    let t = TestApp::mem();
    assert_eq!(
        t.ok::<OnboardingStatus>(Empty {}),
        OnboardingState {
            show: true,
            completed: false
        }
    );
    let decks_before = deck_count(&t);
    let r = apply(&t, StudySubject::English, &["english_basics", "app_guide"]);
    assert_eq!(r.starter_deck_ids.len(), 2);
    let english = starter::find("english_basics").unwrap();
    let guide = starter::find("app_guide").unwrap();
    assert_eq!(
        r.notes_created as usize,
        english.notes.len() + guide.notes.len()
    );

    // The subject's deck carries the CEFR template; the starter deck sits under it.
    let decks = t.ok::<DecksList>(Empty {}).decks;
    let main = decks.iter().find(|d| d.id == r.deck_id).unwrap();
    assert_eq!(main.name, "İngilizce");
    let sub = decks.iter().find(|d| d.id == r.starter_deck_ids[0]).unwrap();
    assert_eq!(sub.parent_id, Some(r.deck_id));
    let guide_deck = decks.iter().find(|d| d.id == r.starter_deck_ids[1]).unwrap();
    assert_eq!(guide_deck.parent_id, None, "the app guide stays at the top");

    let defs = t
        .ok::<AttributesList>(ListAttributes {
            scope_deck_id: Some(r.deck_id),
            include_deleted: None,
        })
        .defs;
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].template_id.as_deref(), Some("cefr"));
    let starter_defs = t
        .ok::<AttributesList>(ListAttributes {
            scope_deck_id: Some(sub.id),
            include_deleted: None,
        })
        .defs;
    let cefr = &starter_defs[0];
    assert_eq!(cefr.template_id.as_deref(), Some("cefr"));

    // Every starter note has its CEFR level.
    let notes = notes_in(&t, sub.id);
    assert_eq!(notes.len(), english.notes.len());
    for n in &notes {
        let v = t.ok::<AttributesValuesGet>(GetAttributeValues {
            entity: EntityRef::Note(n.id),
        });
        let value = v.values.iter().find(|a| a.attr_id == cefr.id).unwrap();
        assert!(matches!(value.value, Some(AttributeValue::Choice(_))));
    }

    // The language preset (15 new/day) is assigned to the subject's deck.
    let preset = t.ok::<PresetsForDeck>(PresetForDeck { deck_id: r.deck_id });
    assert_eq!(preset.assigned_on, Some(r.deck_id));
    assert_eq!(preset.config.new_per_day, 15);

    // Finished: not shown again.
    assert!(t.ok::<OnboardingStatus>(Empty {}).completed);
    assert!(!t.ok::<OnboardingStatus>(Empty {}).show);

    // One undo removes the whole setup.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(deck_count(&t), decks_before);
    assert!(t
        .ok::<AttributesList>(ListAttributes {
            scope_deck_id: None,
            include_deleted: None
        })
        .defs
        .is_empty());
    // The "finished" bookkeeping is not undone: the guide does not come back.
    assert!(t.ok::<OnboardingStatus>(Empty {}).completed);
}

/// YKS → the exam template (TYT/AYT/YDT) and the exam preset; the TYT starter deck's notes
/// carry their YKS classification.
#[test]
fn yks_setup_uses_the_exam_template() {
    let t = TestApp::mem();
    let r = apply(&t, StudySubject::Yks, &["yks_tyt_math"]);
    let defs = t
        .ok::<AttributesList>(ListAttributes {
            scope_deck_id: Some(r.deck_id),
            include_deleted: None,
        })
        .defs;
    assert_eq!(defs[0].template_id.as_deref(), Some("yks"));
    let preset = t.ok::<PresetsForDeck>(PresetForDeck { deck_id: r.deck_id });
    assert_eq!(preset.config.new_per_day, 30);
    let sub = r.starter_deck_ids[0];
    let sub_def = &t
        .ok::<AttributesList>(ListAttributes {
            scope_deck_id: Some(sub),
            include_deleted: None,
        })
        .defs[0];
    let tyt = sub_def.options.iter().find(|o| o.label == "TYT").unwrap();
    for n in notes_in(&t, sub) {
        let v = t.ok::<AttributesValuesGet>(GetAttributeValues {
            entity: EntityRef::Note(n.id),
        });
        assert_eq!(
            v.values[0].value,
            Some(AttributeValue::Choice(tyt.id)),
            "TYT starter notes are TYT"
        );
    }
}

/// With the classification and preset modules off the guide still works: it only skips what
/// those modules own.
#[test]
fn guide_skips_switched_off_modules() {
    let t = TestApp::mem();
    for id in ["attributes", "presets"] {
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: id.into(),
            enabled: false,
        });
    }
    let r = apply(&t, StudySubject::English, &["english_basics"]);
    assert!(!notes_in(&t, r.starter_deck_ids[0]).is_empty());
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "attributes".into(),
        enabled: true,
    });
    assert!(t
        .ok::<AttributesList>(ListAttributes {
            scope_deck_id: None,
            include_deleted: None
        })
        .defs
        .is_empty());
}

#[test]
fn skip_finishes_and_a_collection_with_cards_never_shows_the_guide() {
    let t = TestApp::mem();
    let s = t.ok::<OnboardingSkip>(Empty {});
    assert!(s.completed && !s.show);

    // An upgraded collection with data: no guide even though it was never finished.
    let t = TestApp::mem();
    t.ok::<StarterInstall>(InstallStarter {
        id: "world_capitals".into(),
        parent_id: None,
        locale: Some(Locale::En),
    });
    let s = t.ok::<OnboardingStatus>(Empty {});
    assert!(!s.show && !s.completed);
}

/// Starter decks: every one installs in both languages, names do not clash, one undo step
/// each, unknown ids are refused.
#[test]
fn starter_decks_install_in_both_languages() {
    let t = TestApp::mem();
    let list = t.ok::<StarterList>(LocaleRequest {
        locale: Some(Locale::En),
    });
    assert_eq!(list.starters.len(), starter::all().len());
    for d in starter::all() {
        for locale in [Locale::En, Locale::Tr] {
            let r = t.ok::<StarterInstall>(InstallStarter {
                id: d.id.into(),
                parent_id: None,
                locale: Some(locale),
            });
            assert_eq!(r.notes_created as usize, d.notes.len());
        }
    }
    // Installing the same deck twice gets a free name.
    let again = t.ok::<StarterInstall>(InstallStarter {
        id: "app_guide".into(),
        parent_id: None,
        locale: Some(Locale::En),
    });
    let decks = t.ok::<DecksList>(Empty {}).decks;
    let name = &decks.iter().find(|d| d.id == again.deck_id).unwrap().name;
    assert!(name.ends_with("(2)"), "{name}");
    let before = deck_count(&t);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(deck_count(&t), before - 1);

    let e = t.err::<StarterInstall>(InstallStarter {
        id: "nope".into(),
        parent_id: None,
        locale: None,
    });
    assert_eq!(e.code, ErrorCode::NotFoundStarterDeck);
}

/// The subject list mirrors the pure plan (templates by subject).
#[test]
fn options_list_every_subject_with_its_template() {
    let t = TestApp::mem();
    let o = t.ok::<OnboardingOptions>(LocaleRequest {
        locale: Some(Locale::Tr),
    });
    assert_eq!(o.subjects.len(), StudySubject::ALL.len());
    let en = o
        .subjects
        .iter()
        .find(|s| s.subject == StudySubject::English)
        .unwrap();
    assert_eq!(en.template_id.as_deref(), Some("cefr"));
    assert_eq!(en.deck_name, "İngilizce");
    let yks = o
        .subjects
        .iter()
        .find(|s| s.subject == StudySubject::Yks)
        .unwrap();
    assert_eq!(yks.template_id.as_deref(), Some("yks"));
}
