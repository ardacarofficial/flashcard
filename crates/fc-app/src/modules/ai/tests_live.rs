//! v1.4 G1: the AI features against a **real** provider (OpenRouter), opt-in.
//!
//! Ignored by default and skipped unless `FC_AI_LIVE=1`: it spends (very little) money and
//! needs the network. Run it with
//!
//! ```text
//! FC_AI_LIVE=1 FC_AI_LIVE_OPENROUTER_KEY=sk-or-v1-… \
//!   cargo test -p fc-app --lib live_openrouter -- --ignored --nocapture
//! ```
//!
//! `FC_AI_LIVE_OPENROUTER_KEY` may be left out where an outbound proxy adds the credential
//! (the development container): a dummy key in OpenRouter's format is sent then.
//! `FC_AI_LIVE_MODEL` picks the model (default `google/gemini-2.5-flash-lite`, cheap and
//! multilingual). One run makes ~6 short requests (well under one US cent).
//!
//! It repeats what a user does, on a real SQLite collection: add OpenRouter from its preset,
//! set the key, test the connection, fetch the model list, then every AI capability once —
//! cards from a text (preview → run → `ai.notes.add`), meaning in context on a Subtitle note
//! (+ `ai.meaning.apply`), `ai.meaning.suggest`, study help (explain, check answer) — and reads
//! each persisted result back, then the usage panel's numbers. See `docs/13-ai-canli-deneme.md`.

use super::tests::{app_over, deck};
use super::*;
use crate::testing::{TempSqlite, TestApp};
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteRef};
use fc_api::v1::settings::SetSetting;
use fc_core::content::builtin;
use fc_core::settings::SettingScope;
use fc_storage::SqliteStore;
use serde_json::json;
use std::sync::Arc;

const DEFAULT_MODEL: &str = "google/gemini-2.5-flash-lite";

fn enabled() -> bool {
    std::env::var("FC_AI_LIVE").is_ok_and(|v| v == "1")
}

fn job(t: &TestApp, job: JobDto) -> JobDto {
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: job.id })
}

fn succeeded(j: &JobDto) -> AiJobResult {
    assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error);
    serde_json::from_value(j.result.clone().unwrap()).unwrap()
}

fn ask(t: &TestApp, task: AiTask) -> AiJobResult {
    let p = t.ok::<AiPreviewTask>(PreviewAi {
        task: task.clone(),
        provider_id: None,
        model: None,
    });
    let started = t.ok::<AiRun>(RunAi {
        task,
        provider_id: None,
        model: None,
        preview_hash: p.preview_hash,
    });
    let r = succeeded(&job(t, started));
    eprintln!(
        "[live] {:?}: model {} · {} in / {} out · ${:?} · attempts {}",
        p.feature, r.model, r.input_tokens, r.output_tokens, r.cost_usd, r.attempts
    );
    r
}

/// Adds OpenRouter the way the "add provider" gallery does: from its preset.
fn add_openrouter(t: &TestApp, model: &str) -> AiProviderDto {
    let list = t.ok::<AiProvidersList>(Empty {});
    let preset = list.presets.iter().find(|p| p.id == "openrouter").expect("preset");
    let p = t.ok::<AiProvidersCreate>(CreateAiProvider {
        kind: preset.kind,
        name: Some(preset.name.clone()),
        base_url: Some(preset.base_url.clone()),
        default_model: Some(model.to_owned()),
        is_default: Some(true),
    });
    assert_eq!((p.preset.as_str(), p.needs_key, p.status), ("openrouter", true, AiProviderStatus::NeedsKey));
    let key = std::env::var("FC_AI_LIVE_OPENROUTER_KEY")
        .unwrap_or_else(|_| format!("sk-or-v1-{}", "0".repeat(64)));
    let p = t.ok::<AiProvidersSetKey>(SetAiProviderKey {
        provider_id: p.id,
        key: Some(key),
    });
    assert!(p.has_key);
    p
}

/// A note as the Subtitle extension makes it: the word, as heard, its dictionary meaning and
/// the line with the word marked.
fn subtitle_note(t: &TestApp, deck: fc_core::DeckId, [word, surface, meaning, sentence]: [&str; 4]) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: builtin::find(builtin::SUBTITLE).unwrap().id,
        deck_id: deck,
        fields: [
            ("word", word),
            ("surface", surface),
            ("meaning", meaning),
            ("sentence", sentence),
            ("translation", ""),
            ("source", "Film · 1:23"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    })
}

#[test]
#[ignore = "real provider: FC_AI_LIVE=1 (see the module docs)"]
fn live_openrouter() {
    if !enabled() {
        eprintln!("[live] skipped: set FC_AI_LIVE=1");
        return;
    }
    let model = std::env::var("FC_AI_LIVE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
    let db = TempSqlite::new();
    let t = app_over(Arc::new(SqliteStore::open(&db.path()).unwrap()));

    // 1. Provider: preset, key, connection test, model list.
    let p = add_openrouter(&t, &model);
    let test = succeeded_test(&job(&t, t.ok::<AiProvidersTest>(AiProviderRef { id: p.id })));
    assert_eq!(test.outcome, AiTestOutcome::Ok, "{test:?}");
    let models = job(&t, t.ok::<AiProvidersRefreshModels>(AiProviderRef { id: p.id }));
    assert_eq!(models.status, JobStatus::Succeeded, "{:?}", models.error);
    let stored = t.ok::<AiProvidersList>(Empty {}).providers.remove(0);
    assert!(stored.models.len() > 50, "{} models", stored.models.len());
    assert!(stored.models.contains(&model), "{model} is not in the list");
    assert_eq!(stored.status, AiProviderStatus::Ready);

    // 2a. Cards from a text: preview → run → apply → read back.
    let d = deck(&t, "Live AI");
    let classic = fc_core::notetype::builtin::CLASSIC_ID;
    let r = ask(
        &t,
        AiTask::GenerateCards {
            deck_id: d.id,
            notetype_id: classic,
            source: "To binge-watch means to watch many episodes of a series in a row. \
                     A cliffhanger is an ending that leaves you in suspense."
                .into(),
            count: 2,
            instructions: None,
            images: vec![],
            attributes: vec![],
        },
    );
    let AiOutput::GenerateCards { notes, .. } = r.output else { panic!("{:?}", r.output) };
    assert!(!notes.is_empty() && notes.len() <= 2, "{notes:?}");
    let added = t.ok::<AiNotesAdd>(AddAiNotes {
        deck_id: d.id,
        notetype_id: classic,
        notes: notes.clone(),
        attributes: vec![],
        tags: vec!["ai-live".into()],
    });
    assert_eq!(added.note_ids.len(), notes.len());
    for (id, draft) in added.note_ids.iter().zip(&notes) {
        let n = t.ok::<NotesGet>(NoteRef { id: *id });
        assert_eq!(n.fields.get("front"), draft.fields.get("front"));
        assert!(!n.fields["back"].trim().is_empty());
    }

    // 2b. Meaning in this sentence → apply (field replaced, alternatives accepted) → read back.
    let note = subtitle_note(&t, d.id, ["pick up", "pick up", "kaldırmak", "Can you <b>pick up</b> the kids at five?"]);
    let r = ask(
        &t,
        AiTask::MeaningInContext {
            note_id: Some(note.id),
            sentence: None,
            lemma: None,
            surface: None,
            position: None,
            target_language: None,
            candidates: vec![],
        },
    );
    let AiOutput::MeaningInContext { meaning, also_correct, phrase, original, .. } = r.output else {
        panic!("{:?}", r.output)
    };
    eprintln!("[live] meaning: {meaning} · phrase {phrase} · also {also_correct:?}");
    assert!(!meaning.trim().is_empty());
    assert_eq!(original, "kaldırmak");
    let state = t.ok::<AiMeaningApply>(ApplyAiMeaning {
        note_id: note.id,
        meaning: meaning.clone(),
        phrase,
        expression: false,
        also_correct: also_correct.clone(),
        not_this: vec![],
        confidence: None,
        model: Some(r.model.clone()),
        edited: false,
        accepted: also_correct,
        replace_field: true,
    });
    let back = t.ok::<AiMeaningGet>(AiMeaningRef { note_id: note.id });
    assert_eq!(back, state);
    let m = back.meaning.unwrap();
    assert_eq!((m.status, m.meaning.as_str(), m.original.as_str()), (AiMeaningStatus::Applied, meaning.as_str(), "kaldırmak"));
    assert_eq!(t.ok::<NotesGet>(NoteRef { id: note.id }).fields["meaning"], meaning);

    // 2c. Suggest without a preview (opt-in) on a second note: stored as a suggestion only.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: AUTO_SUGGEST.into(),
        value: json!(true),
    });
    let other = subtitle_note(&t, d.id, ["run", "runs", "koşmak", "She <b>runs</b> a small bakery downtown."]);
    let j = job(&t, t.ok::<AiMeaningSuggest>(AiMeaningRef { note_id: other.id }));
    succeeded(&j);
    let s = t.ok::<AiMeaningGet>(AiMeaningRef { note_id: other.id }).meaning.unwrap();
    assert_eq!(s.status, AiMeaningStatus::Suggested);
    assert_eq!(t.ok::<NotesGet>(NoteRef { id: other.id }).fields["meaning"], "koşmak");
    eprintln!("[live] suggestion for runs: {}", s.meaning);

    // 2d. Study help: explain (streamed text) and check a typed answer.
    let card = t.ok::<NotesGet>(NoteRef { id: added.note_ids[0] }).cards[0].id;
    let r = ask(&t, AiTask::Explain { card_id: card, question: None });
    let AiOutput::Explain { text } = r.output else { panic!() };
    assert!(text.chars().count() > 10, "{text}");
    let r = ask(&t, AiTask::CheckAnswer { card_id: card, given: "something else entirely".into() });
    assert!(matches!(r.output, AiOutput::CheckAnswer { .. }), "{:?}", r.output);

    // 2e. The usage panel: every request counted, with the provider-reported cost.
    let u = t.ok::<AiUsageGet>(Empty {});
    eprintln!("[live] usage: {:?}", u.month);
    assert!(u.month.requests >= 5, "{:?}", u.month);
    assert!(u.month.input_tokens > 0 && u.month.output_tokens > 0);
    assert!(u.month.cost_usd > 0.0, "OpenRouter reports the cost: {:?}", u.month);
    assert!(u.by_feature.iter().any(|g| g.key == "meaning_in_context"));
}

fn succeeded_test(j: &JobDto) -> AiConnectionTest {
    assert_eq!(j.status, JobStatus::Succeeded, "{:?}", j.error);
    serde_json::from_value(j.result.clone().unwrap()).unwrap()
}
