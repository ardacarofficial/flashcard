//! Review follow-ups of v0.8: keys never follow a provider to a new address, queued requests
//! respect the module switch, `Retry-After` and jitter, streamed output through events, card
//! sources (PDF pages, web pages) and the AI worker lane.

use super::tests::{
    app, claude, claude_text, claude_tool, classic, deck, generate, levels, preview, result,
    run, KEY,
};
use super::*;
use crate::testing::TestApp;
use fc_ai::mock::{sample_pdf, MockResponse, MockServer};
use fc_api::v1::ai::*;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::io::{ExportFormat, StartExport};
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::oplog::Actor;
use fc_api::ErrorCode;
use fc_core::JobId;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn update(p: &AiProviderDto, base_url: &str, keep_key: Option<bool>) -> UpdateAiProvider {
    UpdateAiProvider {
        id: p.id,
        name: p.name.clone(),
        base_url: base_url.into(),
        default_model: p.default_model.clone(),
        models: p.models.clone(),
        is_default: p.is_default,
        keep_key,
    }
}

fn has_key(t: &TestApp) -> bool {
    t.ok::<AiProvidersList>(Empty {}).providers[0].has_key
}

fn run_ai(t: &TestApp, task: &AiTask, hash: String) -> JobDto {
    t.ok::<AiRun>(RunAi {
        task: task.clone(),
        provider_id: None,
        model: None,
        preview_hash: hash,
    })
}

fn job(t: &TestApp, id: JobId) -> JobDto {
    t.ok::<JobsGet>(JobRef { id })
}

// ------------------------------------------------------------------ #1 key and address

#[test]
fn changing_the_address_removes_the_key_unless_confirmed() {
    let a = MockServer::start(|_| claude_text("A"));
    let b = MockServer::start(|_| claude_text("B"));
    let c = MockServer::start(|_| claude_text("C"));
    let t = app();
    let p = claude(&t, &a, Some(KEY));
    assert!(has_key(&t));

    // Name/model edits keep the key.
    let mut same = update(&p, &a.url(), None);
    same.name = "Claude (work)".into();
    assert!(t.ok::<AiProvidersUpdate>(same).has_key);

    // A new address without confirmation: the key is gone.
    let moved = t.ok::<AiProvidersUpdate>(update(&p, &b.url(), None));
    assert!(!moved.has_key && !has_key(&t));
    // `keep_key: false` is the same.
    t.ok::<AiProvidersSetKey>(SetAiProviderKey {
        provider_id: p.id,
        key: Some(KEY.into()),
    });
    assert!(!t.ok::<AiProvidersUpdate>(update(&p, &a.url(), Some(false))).has_key);

    // Confirmed (and the user may write keys): the key stays.
    t.ok::<AiProvidersSetKey>(SetAiProviderKey {
        provider_id: p.id,
        key: Some(KEY.into()),
    });
    assert!(t.ok::<AiProvidersUpdate>(update(&p, &c.url(), Some(true))).has_key);
    // Undo puts the old address back — without the key that was kept for the new one.
    t.ok::<OplogUndo>(Empty {});
    let back = &t.ok::<AiProvidersList>(Empty {}).providers[0];
    assert_eq!(back.base_url, a.url());
    assert!(!back.has_key);
    // Nothing was ever sent.
    assert!(a.requests().is_empty() && b.requests().is_empty() && c.requests().is_empty());
}

#[test]
fn the_preview_hash_covers_address_kind_and_model() {
    let a = MockServer::start(|_| claude_text("A"));
    let b = MockServer::start(|_| claude_text("B"));
    let t = app();
    let p = claude(&t, &a, Some(KEY));
    let d = deck(&t, "Hash");
    let task = AiTask::Hint {
        card_id: classic(&t, &d, "q", "a").cards[0].id,
    };
    let at_a = preview(&t, task.clone());
    t.ok::<AiProvidersUpdate>(update(&p, &b.url(), Some(true)));
    let at_b = preview(&t, task.clone());
    assert_ne!(at_a.preview_hash, at_b.preview_hash, "address is in the hash");
    assert_eq!(at_b.provider.base_url, b.url());
    // The request previewed for A is refused once the provider points to B.
    let e = t.err::<AiRun>(RunAi {
        task: task.clone(),
        provider_id: None,
        model: None,
        preview_hash: at_a.preview_hash,
    });
    assert_eq!(e.code, ErrorCode::ConflictAiPreviewStale);
    // Another model is another hash.
    let other_model = t.ok::<AiPreviewTask>(PreviewAi {
        task: task.clone(),
        provider_id: None,
        model: Some("claude-haiku-5-5".into()),
    });
    assert_ne!(other_model.preview_hash, at_b.preview_hash);

    // A job queued for B whose provider moves (key kept) before it runs never sends.
    let queued = run_ai(&t, &task, at_b.preview_hash);
    t.ok::<AiProvidersUpdate>(update(&p, &a.url(), Some(true)));
    t.app.run_pending_jobs().unwrap();
    let done = job(&t, queued.id);
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().code, ErrorCode::ConflictAiPreviewStale);
    assert!(a.requests().is_empty() && b.requests().is_empty());
}

// ------------------------------------------------------------------ #2 module switch

#[test]
fn queued_requests_do_not_go_out_after_the_module_is_turned_off() {
    let server = MockServer::start(|_| claude_text("hint"));
    let t = app();
    let p = claude(&t, &server, Some(KEY));
    let d = deck(&t, "Off");
    let task = AiTask::Hint {
        card_id: classic(&t, &d, "q", "a").cards[0].id,
    };
    let pv = preview(&t, task.clone());
    let queued = run_ai(&t, &task, pv.preview_hash);
    let models = t.ok::<AiProvidersRefreshModels>(AiProviderRef { id: p.id });
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: false,
    });
    assert_eq!(t.app.run_pending_jobs().unwrap(), 2);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: true,
    });
    for id in [queued.id, models.id] {
        let done = job(&t, id);
        assert_eq!(done.status, JobStatus::Failed);
        assert_eq!(done.error.unwrap().code, ErrorCode::ConflictModuleDisabled);
    }
    assert!(server.requests().is_empty(), "nothing left the app");
}

// ------------------------------------------------------------------ #5 Retry-After, jitter

#[test]
fn backoff_honours_retry_after_and_jitters() {
    let ms = |d: Duration| d.as_millis() as u64;
    // The server's wait is a floor, plus at most 10 %.
    assert_eq!(ms(backoff(1000, 0, Some(2000), 0.0)), 2000);
    assert_eq!(ms(backoff(1000, 3, Some(2000), 0.999)), 2199);
    // Without it: exponential, half fixed and half random.
    assert_eq!(ms(backoff(1000, 0, None, 0.0)), 500);
    assert_eq!(ms(backoff(1000, 0, None, 0.999)), 999);
    assert_eq!(ms(backoff(1000, 2, None, 0.0)), 2000);
    assert_eq!(ms(backoff(1000, 2, None, 0.5)), 3000);
    assert_eq!(ms(backoff(0, 4, None, 0.7)), 0);
    // The random part varies.
    let s = AiServices::default();
    let pauses: std::collections::BTreeSet<u128> =
        (0..40).map(|_| s.retry_pause(3, None).as_micros()).collect();
    assert!(pauses.len() > 5, "{pauses:?}");
    assert!(pauses.iter().all(|p| (4_000_000..=8_000_000).contains(p)));
}

#[test]
fn a_rate_limited_request_waits_as_long_as_the_provider_asks() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let c = calls.clone();
    let server = MockServer::start(move |_| {
        if c.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            MockResponse::status(429, &json!({"error": {"message": "slow down"}}))
                .header("retry-after", "0.3")
        } else {
            claude_tool(json!({"verdict": "correct", "feedback": "Yes."}))
        }
    });
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Wait");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    let started = Instant::now();
    let r = result(&run(
        &t,
        AiTask::CheckAnswer {
            card_id: card,
            given: "a".into(),
        },
    ));
    // Retry base is 0 in tests: only Retry-After explains the wait.
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert_eq!(r.attempts, 2);

    // A wait longer than a minute is not sat out: the job fails as rate limited.
    let server = MockServer::start(|_| {
        MockResponse::status(429, &json!({"error": {"message": "quota"}})).header("retry-after", "3600")
    });
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Quota");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    let e = run(&t, AiTask::Hint { card_id: card }).error.unwrap();
    assert_eq!((e.code, e.retryable), (ErrorCode::ProviderAiRateLimited, true));
    assert_eq!(server.requests().len(), 1);
}

// ------------------------------------------------------------------ #6 session-only keychain

#[test]
fn a_session_only_keychain_is_reported() {
    // Tests install the in-memory store: keys last only as long as the app runs.
    let t = app();
    let l = t.ok::<AiProvidersList>(Empty {});
    assert!(l.keychain && l.keychain_session_only);
    let bare = TestApp::mem();
    let l = bare.ok::<AiProvidersList>(Empty {});
    assert!(!l.keychain && !l.keychain_session_only);
}

// ------------------------------------------------------------------ #3 streaming

fn claude_stream(pieces: &[&str], delay_ms: u64) -> MockResponse {
    let mut chunks = vec![format!(
        "event: message_start\ndata: {}\n\n",
        json!({"type": "message_start", "message": {"model": "claude-sonnet-5-5", "usage": {"input_tokens": 50, "output_tokens": 1}}})
    )];
    for p in pieces {
        chunks.push(format!(
            "event: content_block_delta\ndata: {}\n\n",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": p}})
        ));
    }
    chunks.push(format!(
        "event: message_delta\ndata: {}\n\n",
        json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"output_tokens": 12}})
    ));
    chunks.push("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".into());
    MockResponse::stream("text/event-stream", chunks, delay_ms)
}

fn partials(t: &TestApp, id: JobId) -> Vec<String> {
    t.take_events()
        .into_iter()
        .filter_map(|e| match e.event {
            DomainEvent::AiPartial { job_id, text } if job_id == id => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn explanations_and_hints_stream_through_events() {
    let server = MockServer::start(|r| {
        if r.json()["stream"] == json!(true) {
            claude_stream(&["Light ", "becomes ", "sugar."], 80)
        } else {
            claude_tool(json!({"verdict": "almost", "feedback": "Nearly."}))
        }
    });
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Stream");
    let card = classic(&t, &d, "What do leaves make?", "sugar").cards[0].id;
    for task in [
        AiTask::Explain {
            card_id: card,
            question: None,
        },
        AiTask::Hint { card_id: card },
    ] {
        t.take_events();
        let done = run(&t, task);
        let r = result(&done);
        let text = match r.output {
            AiOutput::Explain { text } | AiOutput::Hint { text } => text,
            other => panic!("{other:?}"),
        };
        assert_eq!(text, "Light becomes sugar.");
        assert_eq!((r.input_tokens, r.output_tokens), (50, 12));
        let seen = partials(&t, done.id);
        // Shown progressively: growing prefixes, ending with the whole answer.
        assert!(seen.len() >= 3, "{seen:?}");
        assert_eq!(seen.first().map(String::as_str), Some("Light "));
        assert_eq!(seen.last().map(String::as_str), Some("Light becomes sugar."));
        assert!(seen.windows(2).all(|w| w[1].starts_with(&w[0])), "{seen:?}");
    }
    // Structured answers are not streamed.
    t.take_events();
    let done = run(
        &t,
        AiTask::CheckAnswer {
            card_id: card,
            given: "sugars".into(),
        },
    );
    assert_eq!(result(&done).output, AiOutput::CheckAnswer { verdict: AiVerdict::Almost, feedback: "Nearly.".into() });
    assert!(partials(&t, done.id).is_empty());
    let streamed: Vec<bool> = server.requests().iter().map(|r| r.json()["stream"] == json!(true)).collect();
    assert_eq!(streamed, [true, true, false]);
}

#[test]
fn a_retried_stream_starts_over() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let c = calls.clone();
    let server = MockServer::start(move |_| {
        if c.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            // Fails mid-stream with a retryable error event.
            MockResponse::stream(
                "text/event-stream",
                vec![
                    format!("data: {}\n\n", json!({"type": "content_block_delta", "delta": {"text": "Wrong start"}})),
                    format!("data: {}\n\n", json!({"type": "error", "error": {"type": "overloaded_error", "message": "busy"}})),
                ],
                0,
            )
        } else {
            claude_stream(&["Right."], 0)
        }
    });
    let t = app();
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Retry");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    t.take_events();
    let done = run(&t, AiTask::Hint { card_id: card });
    assert_eq!(result(&done).output, AiOutput::Hint { text: "Right.".into() });
    assert_eq!(result(&done).attempts, 2);
    let seen = partials(&t, done.id);
    assert_eq!(seen, ["Wrong start", "", "Right.", "Right."]);
}

// ------------------------------------------------------------------ #3 sources

fn extract(t: &TestApp, source: AiSourceInput) -> JobDto {
    let job = t.ok::<AiSourcesExtract>(ExtractAiSource { source });
    assert_eq!(job.kind, SOURCE_JOB);
    t.app.run_pending_jobs().unwrap();
    job_of(t, job.id)
}

fn job_of(t: &TestApp, id: JobId) -> JobDto {
    t.ok::<JobsGet>(JobRef { id })
}

fn text_of(job: &JobDto) -> AiSourceText {
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    serde_json::from_value(job.result.clone().unwrap()).unwrap()
}

#[test]
fn cards_from_a_pdf_page_range() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bio.pdf");
    std::fs::write(
        &path,
        sample_pdf("Biology", &["Mitosis splits cells.", "Leaves make sugar.", "Roots take water."]),
    )
    .unwrap();
    let path = path.to_string_lossy().into_owned();
    let t = app();
    let pdf = |from: Option<u32>, to: Option<u32>| AiSourceInput::Pdf {
        path: path.clone(),
        from_page: from,
        to_page: to,
    };
    let got = text_of(&extract(&t, pdf(Some(2), Some(3))));
    assert_eq!((got.page_count, got.from_page, got.to_page), (Some(3), Some(2), Some(3)));
    assert_eq!(got.title.as_deref(), Some("Biology"));
    assert!(got.text.contains("sugar") && got.text.contains("water") && !got.text.contains("Mitosis"));
    assert!(!got.truncated);

    // The extracted text is an ordinary source: previewed, then sent.
    let server = MockServer::start(|_| claude_tool(json!({"notes": [{"front": "Leaves make?", "back": "sugar"}]})));
    claude(&t, &server, Some(KEY));
    let lv = levels(&t);
    let task = generate(&lv, &got.text);
    let pv = preview(&t, task.clone());
    assert!(pv.messages[0].text.contains("Leaves make sugar."));
    result(&run(&t, task));
    assert!(server.requests()[0].body.contains("Roots take water."));

    // Checked before queueing.
    let e = t.err::<AiSourcesExtract>(ExtractAiSource {
        source: AiSourceInput::Pdf {
            path: "relative.pdf".into(),
            from_page: None,
            to_page: None,
        },
    });
    assert_eq!((e.code, e.details["reason"].as_str()), (ErrorCode::ValidationAiSource, Some("io")));
    let e = t.err::<AiSourcesExtract>(ExtractAiSource { source: pdf(Some(3), Some(1)) });
    assert_eq!(e.details["reason"].as_str(), Some("pages"));
    // Checked by the job.
    let e = extract(&t, pdf(Some(5), None)).error.unwrap();
    assert_eq!((e.code, e.details["reason"].as_str()), (ErrorCode::ValidationAiSource, Some("pages")));
    let missing = dir.path().join("missing.pdf").to_string_lossy().into_owned();
    let e = extract(&t, AiSourceInput::Pdf { path: missing, from_page: None, to_page: None }).error.unwrap();
    assert_eq!(e.details["reason"].as_str(), Some("io"));
}

#[test]
fn cards_from_a_web_page() {
    let long = format!("<article>{}</article>", "<p>word word word word word word word word.</p>".repeat(3000));
    let server = MockServer::start(move |r| match r.path.as_str() {
        "/page" => MockResponse::text(
            200,
            "text/html",
            "<html><head><title>Plants</title><script>evil()</script></head><body><nav>Menu</nav>\
             <main><h1>Leaves</h1><p>Leaves make sugar from light.</p></main></body></html>",
        ),
        "/long" => MockResponse::text(200, "text/html", long.clone()),
        _ => MockResponse::text(404, "text/html", ""),
    });
    let t = app();
    let url = |p: &str| AiSourceInput::Url { url: format!("{}{p}", server.url()) };
    let got = text_of(&extract(&t, url("/page")));
    assert_eq!(got.title.as_deref(), Some("Plants"));
    assert_eq!(got.text, "Leaves\n\nLeaves make sugar from light.");
    assert_eq!(got.page_count, None);

    let long = text_of(&extract(&t, url("/long")));
    assert!(long.truncated);
    assert_eq!(long.text.chars().count(), context::MAX_SOURCE_CHARS);

    let e = extract(&t, url("/gone")).error.unwrap();
    assert_eq!((e.code, e.details["reason"].as_str(), e.retryable), (ErrorCode::ProviderAiSource, Some("http_status"), true));
    let e = t.err::<AiSourcesExtract>(ExtractAiSource {
        source: AiSourceInput::Url { url: "file:///etc/passwd".into() },
    });
    assert_eq!((e.code, e.details["reason"].as_str()), (ErrorCode::ValidationAiSource, Some("url")));

    // Turned off while queued: nothing is fetched.
    let before = server.requests().len();
    let queued = t.ok::<AiSourcesExtract>(ExtractAiSource { source: url("/page") });
    t.ok::<ModulesSetEnabled>(SetModuleEnabled { id: ID.into(), enabled: false });
    t.app.run_pending_jobs().unwrap();
    t.ok::<ModulesSetEnabled>(SetModuleEnabled { id: ID.into(), enabled: true });
    assert_eq!(job_of(&t, queued.id).error.unwrap().code, ErrorCode::ConflictModuleDisabled);
    assert_eq!(server.requests().len(), before);
}

// ------------------------------------------------------------------ #7 worker lanes

#[test]
fn ai_jobs_run_on_their_own_lane() {
    let t = app();
    assert_eq!(t.app.registry().lanes(), ["main", LANE]);
    let (ai_kinds, only) = t.app.registry().lane_kinds(LANE);
    assert!(only);
    let mut ai_kinds = ai_kinds;
    ai_kinds.sort_unstable();
    assert_eq!(ai_kinds, [MODELS_JOB, REQUEST_JOB, SOURCE_JOB]);

    // A slow AI request (≈1.5 s of streaming) does not hold up an export queued after it.
    let server = MockServer::start(|_| claude_stream(&["a", "b", "c"], 500));
    claude(&t, &server, Some(KEY));
    let d = deck(&t, "Lane");
    let card = classic(&t, &d, "q", "a").cards[0].id;
    let task = AiTask::Explain {
        card_id: card,
        question: None,
    };
    let pv = preview(&t, task.clone());
    let ai_job = run_ai(&t, &task, pv.preview_hash);
    let dir = tempfile::tempdir().unwrap();
    let export = t.ok::<IoExportStart>(StartExport {
        format: ExportFormat::Text,
        path: dir.path().join("out.txt").to_string_lossy().into_owned(),
        deck_id: None,
        include_history: None,
        text_options: None,
    });
    let app = Arc::new(t.app);
    let worker = crate::jobs::spawn_worker(app.clone()).unwrap();
    assert_eq!(worker.lanes(), 2);
    let status = |id| {
        app.call::<JobsGet>(&JobRef { id }, &Actor::User)
            .unwrap()
            .status
    };
    let started = Instant::now();
    let mut export_done = None;
    let mut ai_done = None;
    while started.elapsed() < Duration::from_secs(20) && (export_done.is_none() || ai_done.is_none()) {
        if export_done.is_none() && status(export.id) == JobStatus::Succeeded {
            export_done = Some(started.elapsed());
        }
        if ai_done.is_none() && status(ai_job.id) == JobStatus::Succeeded {
            ai_done = Some(started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(worker);
    let (export_done, ai_done) = (export_done.unwrap(), ai_done.unwrap());
    assert!(export_done < ai_done, "export {export_done:?} waited for the AI job {ai_done:?}");
    assert!(dir.path().join("out.txt").exists());
}
