//! Phase-3 integration, round 3 (v0.8 AI next to v0.8 MCP, v0.6 statistics and the Phase-2
//! history rules):
//!
//! - AI-generated note batches are ordinary undo steps whose stored inverses (and forward
//!   commands, for redo) are all on the replay allowlist; the ledger/key/job methods are not.
//! - The AI lane runs beside the main lane (I/O and optimizer jobs): neither waits for the
//!   other, and a running AI request can be cancelled.
//! - An MCP client reaches no `ai.*` method (not even reads), not directly and not inside
//!   `batch.execute`; no tool output carries the key.
//! - Level-aware generation writes classification values that statistics break down; AI usage
//!   stays out of the statistics (its own screen only).

use super::tests::{app, claude, claude_tool, generate, levels, result, run, KEY};
use super::*;
use crate::modules::mcp::tools::{call_tool, catalog, method_reachable};
use crate::modules::mcp::MCP_PERMISSIONS;
use crate::testing::TestApp;
use fc_ai::mock::{MockResponse, MockServer};
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::io::{ExportFormat, StartExport};
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::oplog::{Actor, ExecuteBatch, ListOplog};
use fc_api::v1::planning::RunOptimizer;
use fc_api::v1::presets::{AssignPreset, CreatePreset};
use fc_api::v1::stats::{StatsBreakdownRequest, StatsRequest, StatsScope};
use fc_api::{ApiMethod, ErrorCode, InvokeRequest, Visibility};
use fc_core::JobId;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const CLIENT: &str = "claude-code";

fn two_notes() -> MockResponse {
    claude_tool(json!({"notes": [
        {"front": "photosynthesis", "back": "how plants make food from light"},
        {"front": "chlorophyll", "back": "the green pigment of plants"},
    ]}))
}

/// Runs a level-aware generation and adds the drafts; returns the new notes.
fn generate_and_add(t: &TestApp, lv: &super::tests::Levels) -> AiNotesAdded {
    let r = result(&run(t, generate(lv, "Plants turn light into sugar.")));
    let AiOutput::GenerateCards {
        deck_id,
        notetype_id,
        notes,
        attributes,
    } = r.output
    else {
        panic!("{:?}", r.output)
    };
    t.ok::<AiNotesAdd>(AddAiNotes {
        deck_id,
        notetype_id,
        notes,
        attributes,
        tags: vec![],
    })
}

fn replayable(t: &TestApp, qualified: &str) -> bool {
    let name = qualified.strip_prefix("v1.").unwrap_or(qualified);
    t.app.registry().descriptor(name).unwrap().replayable
}

// ------------------------------------------------------------------ undo / replay

#[test]
fn ai_note_batches_are_replayable_undo_steps() {
    let server = MockServer::start(|_| two_notes());
    let t = app();
    claude(&t, &server, Some(KEY));
    let lv = levels(&t);
    let before = crate::testing::suite::snapshot(&t);
    let logged_before = t.ok::<OplogList>(ListOplog::default()).items.len();
    let added = generate_and_add(&t, &lv);
    assert_eq!(added.note_ids.len(), 2);
    let after_add = crate::testing::suite::snapshot(&t);
    t.ok::<AiNotesUpdate>(UpdateAiNote {
        note_id: added.note_ids[0],
        fields: BTreeMap::from([("back".into(), "light → sugar".into())]),
    });
    let after_update = crate::testing::suite::snapshot(&t);

    // Two undo steps (the batch of nested `notes.create`, one `notes.update`); every forward
    // command (redo) and stored inverse (undo) in the history is replayable.
    let log = t.ok::<OplogList>(ListOplog::default());
    let new = &log.items[..log.items.len() - logged_before];
    let mut commands: Vec<_> = new.iter().map(|e| e.command.as_str()).collect();
    commands.sort_unstable();
    assert_eq!(commands, ["v1.notes.create", "v1.notes.create", "v1.notes.update"]);
    let batches: std::collections::BTreeSet<_> = new.iter().map(|e| e.batch_id).collect();
    assert_eq!(batches.len(), 2);
    for e in &log.items {
        if e.inverse.is_empty() {
            continue;
        }
        assert!(replayable(&t, &e.command), "forward {}", e.command);
        for inv in &e.inverse {
            assert!(replayable(&t, &inv.method), "inverse {} of {}", inv.method, e.command);
        }
    }
    // The ledger, the key and job starters never run as stored inverses.
    for name in [
        AiUsageRecord::NAME,
        AiProvidersSetKey::NAME,
        AiRun::NAME,
        AiSourcesExtract::NAME,
        AiProvidersRefreshModels::NAME,
        AiProvidersTest::NAME,
    ] {
        assert!(!replayable(&t, name), "{name}");
    }

    // Undo/redo pass the replay check, step by step and exactly.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(crate::testing::suite::snapshot(&t), after_add);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(crate::testing::suite::snapshot(&t), before);
    t.ok::<OplogRedo>(Empty {});
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(crate::testing::suite::snapshot(&t), after_update);
}

// ------------------------------------------------------------------ worker lanes

fn slow_stream(delay_ms: u64) -> MockResponse {
    let mut chunks = vec![format!(
        "event: message_start\ndata: {}\n\n",
        json!({"type": "message_start", "message": {"model": "claude-sonnet-5-5", "usage": {"input_tokens": 5, "output_tokens": 1}}})
    )];
    for p in ["a", "b", "c"] {
        chunks.push(format!(
            "event: content_block_delta\ndata: {}\n\n",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": p}})
        ));
    }
    chunks.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".into());
    MockResponse::stream("text/event-stream", chunks, delay_ms)
}

fn wait_until(app: &App, id: JobId, done: impl Fn(&JobDto) -> bool) -> (JobDto, Duration) {
    let started = Instant::now();
    loop {
        let job = app.call::<JobsGet>(&JobRef { id }, &Actor::User).unwrap();
        if done(&job) {
            return (job, started.elapsed());
        }
        assert!(started.elapsed() < Duration::from_secs(30), "{job:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn terminal(j: &JobDto) -> bool {
    !matches!(j.status, JobStatus::Queued | JobStatus::Running)
}

#[test]
fn ai_lane_coexists_with_io_and_optimizer_jobs() {
    let server = MockServer::start(|_| slow_stream(500));
    let t = app();
    claude(&t, &server, Some(KEY));
    let lv = levels(&t);
    let note = super::tests::classic(&t, &lv.deck, "q", "a");
    let task = AiTask::Explain {
        card_id: note.cards[0].id,
        question: None,
    };
    let start_ai = |app: &App| {
        let preview = app
            .call::<AiPreviewTask>(
                &PreviewAi {
                    task: task.clone(),
                    provider_id: None,
                    model: None,
                },
                &Actor::User,
            )
            .unwrap();
        app.call::<AiRun>(
            &RunAi {
                task: task.clone(),
                provider_id: None,
                model: None,
                preview_hash: preview.preview_hash,
            },
            &Actor::User,
        )
        .unwrap()
    };
    let preset = t.ok::<PresetsCreate>(CreatePreset {
        name: "Mine".into(),
        config: None,
    });
    t.ok::<PresetsAssign>(AssignPreset {
        deck_ids: vec![lv.deck.id],
        preset_id: Some(preset.id),
    });

    // A slow AI request first, then an export and an optimizer run on the main lane.
    let ai_job = start_ai(&t.app);
    let dir = tempfile::tempdir().unwrap();
    let export = t.ok::<IoExportStart>(StartExport {
        format: ExportFormat::Text,
        path: dir.path().join("out.txt").to_string_lossy().into_owned(),
        deck_id: None,
        include_history: None,
        text_options: None,
    });
    let optimizer = t
        .ok::<OptimizerRun>(RunOptimizer {
            preset_id: preset.id,
            tz_offset_mins: Some(0),
        })
        .job;
    let app = Arc::new(t.app);
    let worker = crate::jobs::spawn_worker(app.clone()).unwrap();
    // main, ai, updates (v1.0).
    assert_eq!(worker.lanes(), 3);
    let (export_job, export_done) = wait_until(&app, export.id, terminal);
    let (opt_job, opt_done) = wait_until(&app, optimizer.id, terminal);
    let (ai, ai_done) = wait_until(&app, ai_job.id, terminal);
    assert_eq!(export_job.status, JobStatus::Succeeded);
    // Two notes' worth of history is too little to train on: it ends, it does not wait.
    assert_eq!(
        opt_job.error.map(|e| e.code),
        Some(ErrorCode::ConflictNotEnoughReviews)
    );
    assert_eq!(ai.status, JobStatus::Succeeded);
    assert!(export_done < ai_done && opt_done < ai_done, "main lane waited for AI");

    // The AI lane is not starved by the main lane either, and a running request cancels.
    let sent_before = server.requests().len();
    let ai_job = start_ai(&app);
    let requests = sent_before + 1;
    let started = Instant::now();
    while server.requests().len() < requests {
        assert!(started.elapsed() < Duration::from_secs(30), "the request never went out");
        std::thread::sleep(Duration::from_millis(5));
    }
    let cancel = |id| {
        app.call::<JobsCancel>(&JobRef { id }, &Actor::User).unwrap();
    };
    let get = |id| app.call::<JobsGet>(&JobRef { id }, &Actor::User).unwrap();
    cancel(ai_job.id);
    let (cancelled, _) = wait_until(&app, ai_job.id, terminal);
    assert_eq!(cancelled.status, JobStatus::Cancelled, "{cancelled:?}");
    assert!(cancelled.result.is_none());
    assert_eq!(server.requests().len(), requests, "no retry after the cancel");
    // A queued one cancels before anything is sent; the lane stays usable.
    drop(worker);
    let queued = start_ai(&app);
    cancel(queued.id);
    app.run_pending_jobs().unwrap();
    assert_eq!(get(queued.id).status, JobStatus::Cancelled);
    assert_eq!(server.requests().len(), requests);
    let after = start_ai(&app);
    app.run_pending_jobs().unwrap();
    assert_eq!(get(after.id).status, JobStatus::Succeeded);
}

// ------------------------------------------------------------------ MCP

#[test]
fn mcp_reaches_no_ai_method_and_no_key() {
    let server = MockServer::start(|_| two_notes());
    let t = app();
    let provider = claude(&t, &server, Some(KEY));
    let lv = levels(&t);
    let added = generate_and_add(&t, &lv);
    let mcp = Actor::Mcp(CLIENT.into());

    // Nothing in the curated surface touches AI.
    assert!(MCP_PERMISSIONS.iter().all(|p| !p.starts_with("ai")));
    for tool in catalog() {
        assert!(tool.methods.iter().all(|m| !m.starts_with("ai.")), "{}", tool.name);
    }

    // Every ai.* method — reads, writes, key, requests — is refused to the MCP actor, even
    // with otherwise valid params, before any params are looked at.
    let ai_methods: Vec<_> = t
        .app
        .registry()
        .methods()
        .filter(|d| d.module == ID && d.visibility == Visibility::Public)
        .map(|d| d.qualified_name())
        .collect();
    assert!(ai_methods.len() > 10, "{ai_methods:?}");
    for name in &ai_methods {
        assert!(!method_reachable(name.strip_prefix("v1.").unwrap()));
        let e = t.app.invoke(name, json!({}), &mcp).unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "{name}: {e:?}");
    }
    let key_call = json!({ "provider_id": provider.id, "key": "sk-stolen" });
    let e = t
        .app
        .invoke("v1.ai.providers.set_key", key_call.clone(), &mcp)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    // Nor inside a batch (MCP's add_notes uses batch.execute).
    for (method, params) in [
        ("v1.ai.providers.set_key", key_call),
        (
            "v1.ai.notes.update",
            json!({ "note_id": added.note_ids[0], "fields": { "back": "x" } }),
        ),
    ] {
        let e = t
            .app
            .call::<BatchExecute>(
                &ExecuteBatch {
                    calls: vec![InvokeRequest {
                        method: method.into(),
                        params,
                    }],
                },
                &mcp,
            )
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "{method}: {e:?}");
    }
    assert!(t.ok::<AiProvidersList>(Empty {}).providers[0].has_key);

    // No read tool says anything about providers or keys.
    let note_id = added.note_ids[0].to_string();
    for (tool, args) in [
        ("list_decks", json!({})),
        ("list_note_types", json!({})),
        ("list_classifications", json!({ "deck": "Dil::İngilizce" })),
        ("search_cards", json!({ "query": "seviye:B1" })),
        ("get_note", json!({ "note_id": note_id })),
    ] {
        let out = call_tool(&t.app, CLIENT, false, tool, args).unwrap().to_string();
        for secret in [KEY, &KEY[KEY.len() - 8..], "has_key", "ai_providers", "api_key"] {
            assert!(!out.contains(secret), "{tool} leaks {secret}: {out}");
        }
    }
}

// ------------------------------------------------------------------ statistics

#[test]
fn level_aware_generation_reaches_stats_breakdowns_but_usage_does_not() {
    let server = MockServer::start(|_| two_notes());
    let t = app();
    claude(&t, &server, Some(KEY));
    let lv = levels(&t);
    let overview = |t: &TestApp| {
        t.ok::<StatsGetOverview>(StatsRequest {
            scope: StatsScope::default(),
            tz_offset_mins: Some(0),
            days: None,
            forecast_days: Some(14),
            fresh: Some(true),
        })
    };
    let empty = overview(&t);
    // A request alone (usage recorded, nothing added) leaves the statistics unchanged.
    let job = run(&t, generate(&lv, "Plants turn light into sugar."));
    result(&job);
    assert_eq!(t.ok::<AiUsageGet>(Empty {}).month.requests, 1);
    assert_eq!(overview(&t), empty);

    generate_and_add(&t, &lv);
    let b = t.ok::<StatsGetBreakdown>(StatsBreakdownRequest {
        scope: StatsScope::default(),
        attr_id: lv.seviye,
        tz_offset_mins: Some(0),
        days: None,
    });
    assert_eq!(b.name, "Seviye");
    let b1 = b.rows.iter().find(|r| r.option_id == Some(lv.b1)).unwrap();
    assert_eq!(b1.label.as_deref(), Some("B1"));
    assert_eq!(b1.states.new, 2, "{b:?}");
    let others: u32 = b
        .rows
        .iter()
        .filter(|r| r.option_id != Some(lv.b1))
        .map(|r| r.states.new)
        .sum();
    assert_eq!(others, 0, "{b:?}");
    assert_eq!(overview(&t).cards.states.new, 2);
    // Usage (two requests now) is still not part of any statistic.
    assert_eq!(t.ok::<AiUsageGet>(Empty {}).month.requests, 2);
    let json = serde_json::to_string(&overview(&t)).unwrap();
    assert!(!json.contains("cost") && !json.contains("tokens"), "{json}");
}
