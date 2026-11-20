//! AI providers and features (v0.8). See `v1::ai` for the preview → run → apply flow.

use crate::v1::ai::*;
use crate::v1::common::Empty;
use crate::v1::jobs::JobDto;

methods! {
    // ---- ai: providers ----
    /// Configured providers, the kinds that can be added, and whether a keychain exists.
    AiProvidersList = "ai.providers.list", Query, "ai", "ai:read", Public, undo: false, Empty => AiProviderList;
    /// Adds a provider (Claude, OpenAI, OpenAI-compatible, Ollama).
    AiProvidersCreate = "ai.providers.create", Command, "ai", "ai:write", Public, undo: true, CreateAiProvider => AiProviderDto;
    /// Edits a provider (name, URL, default model, model list, default flag).
    AiProvidersUpdate = "ai.providers.update", Command, "ai", "ai:write", Public, undo: true, UpdateAiProvider => AiProviderDto;
    /// Removes a provider and its key from the keychain (undo brings the provider back without
    /// the key: keys are never part of the undo history).
    AiProvidersDelete = "ai.providers.delete", Command, "ai", "ai:write", Public, undo: true, AiProviderRef => AiProviderDto;
    /// Stores or removes a provider's API key in the OS keychain. Never logged or undoable.
    AiProvidersSetKey = "ai.providers.set_key", Command, "ai", "ai_keys:write", Public, undo: false, SetAiProviderKey => AiProviderDto;
    /// Starts a job that fetches the provider's model list.
    AiProvidersRefreshModels = "ai.providers.refresh_models", Command, "ai", "ai_requests:write", Public, undo: false, AiProviderRef => JobDto;
    /// Internal: writes provider rows back exactly (inverse of provider commands).
    AiProvidersRestore = "ai.providers.restore", Command, "ai", "ai:write", Internal, undo: true, RestoreAiProviders => Empty;
    /// Internal: stores a fetched model list (a cache: it returns no inverse, so it never
    /// becomes an undo step).
    AiProvidersSetModels = "ai.providers.set_models", Command, "ai", "ai:write", Internal, undo: true, SetAiModels => Empty;

    // ---- ai: routes, prices, usage ----
    /// Provider/model per feature.
    AiRoutesList = "ai.routes.list", Query, "ai", "ai:read", Public, undo: false, Empty => AiRouteList;
    /// Sets (or with nulls clears) a feature's provider/model.
    AiRoutesSet = "ai.routes.set", Command, "ai", "ai:write", Public, undo: true, AiRouteDto => AiRouteDto;
    /// The price table used for cost estimates.
    AiPricesList = "ai.prices.list", Query, "ai", "ai:read", Public, undo: false, Empty => AiPriceList;
    /// Adds or changes a model's price.
    AiPricesSet = "ai.prices.set", Command, "ai", "ai:write", Public, undo: true, AiPriceDto => AiPriceDto;
    /// Removes a model's price.
    AiPricesDelete = "ai.prices.delete", Command, "ai", "ai:write", Public, undo: true, AiPriceRef => Empty;
    /// This month / last 7 days, by feature and by model, and the budget.
    AiUsageGet = "ai.usage.summary", Query, "ai", "ai:read", Public, undo: false, Empty => AiUsageSummary;
    /// Internal: records one provider call (a ledger row: no inverse, never an undo step).
    AiUsageRecord = "ai.usage.record", Command, "ai", "ai_requests:write", Internal, undo: true (replay: false), RecordAiUsage => Empty;

    // ---- ai: requests ----
    /// Builds the exact request a task would send — shown to the user before anything leaves.
    AiPreviewTask = "ai.preview", Query, "ai", "ai:read", Public, undo: false, PreviewAi => AiPreview;
    /// Sends a previewed request (same task + its `preview_hash`) as an `ai.request` job.
    AiRun = "ai.run", Command, "ai", "ai_requests:write", Public, undo: false, RunAi => JobDto;
    /// Starts an `ai.source` job that extracts card-generation text from a PDF page range or a
    /// web page. Reads/fetches only; nothing goes to an AI provider.
    AiSourcesExtract = "ai.sources.extract", Command, "ai", "ai_requests:write", Public, undo: false, ExtractAiSource => JobDto;
    /// Adds reviewed AI drafts as notes, with their classification values — one undo step.
    AiNotesAdd = "ai.notes.add", Command, "ai", "notes:write", Public, undo: true, AddAiNotes => AiNotesAdded;
    /// Applies a suggestion to a note: sets the listed fields, keeps the rest — one undo step.
    AiNotesUpdate = "ai.notes.update", Command, "ai", "notes:write", Public, undo: true, UpdateAiNote => crate::v1::notes::NoteDto;
}
