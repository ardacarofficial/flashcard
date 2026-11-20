//! What a plugin may see (docs/05 §13 "Threat model"). **Every value the host hands a plugin
//! goes through here**, filtered by the *calling* plugin's read permissions for the data
//! domain it holds:
//!
//! | Flow | Filter |
//! |---|---|
//! | `invoke` result | [`exposure`]: a query needs every read permission of its domains (or it is refused); a command's response is cut to ids ([`ids_only`]) without them |
//! | `invoke` error | [`error`]: message and details cut to ids without them; other plugins' messages never pass |
//! | `jobs.*` results | per job, by its kind ([`jobs`]) |
//! | `before:`/`after:` hook input | a hook needs the read permissions of the hooked method's domains (install and run time); the payload is filtered anyway |
//! | `event:` hook input | [`event`]: by the event's domains; without them the payload is cut to ids |
//! | `settings.get` (API) | another plugin's scope is the `plugins` domain (never readable) |
//! | `kv.*`, `settings.*` host functions | the plugin's own data only (no filter needed) |
//! | oplog | `oplog:read` cannot be granted (entries hold every domain) |
//!
//! A *domain* is a permission area (`notes`, `decks`…): readable when `<area>:read` is
//! granted. `plugins`, `io`, `oplog` and the other areas of
//! `fc_plugins::permission::FORBIDDEN_API_AREAS` are never readable. Writes do not imply
//! reads: a plugin holding only `notes:write` learns the ids of what it wrote, not the note
//! text the method echoes back.

use fc_api::v1::events::DomainEvent;
use fc_api::{ErrorCode, MethodKind};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// A data domain: a permission area.
pub(crate) type Domain = &'static str;

/// What calling a method exposes to a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exposure {
    /// The result holds data of these domains.
    Reads(&'static [Domain]),
    /// Like `Reads`, but the call itself reveals that data (an oracle: a find/replace count
    /// says what notes contain), so it needs the read permissions to run at all.
    Oracle(&'static [Domain]),
    /// No plugin may call it: the work is not bounded per call (whole-collection scans and
    /// repairs a plugin call could not be stopped in), or it is not classified yet.
    Refused(&'static str),
}

impl Exposure {
    /// Domains of the result (empty for refused methods).
    pub fn domains(self) -> &'static [Domain] {
        match self {
            Self::Reads(d) | Self::Oracle(d) => d,
            Self::Refused(_) => &[],
        }
    }
}

const ATTRIBUTES: &[Domain] = &["attributes"];
const CARDS: &[Domain] = &["cards"];
const CARDS_NOTES: &[Domain] = &["cards", "notes", "notetypes"];
const CARD_INFO: &[Domain] = &["cards", "notes", "study"];
const DECKS: &[Domain] = &["decks"];
const MEDIA: &[Domain] = &["media"];
const MODULES: &[Domain] = &["modules"];
const NOTES: &[Domain] = &["notes"];
const NOTETYPES: &[Domain] = &["notetypes"];
const SEARCH: &[Domain] = &["search"];
const SEARCH_ROWS: &[Domain] = &["search", "cards", "notes"];
const SETTINGS: &[Domain] = &["settings"];
const JOBS: &[Domain] = &["jobs"];
const STUDY: &[Domain] = &["study"];
const STUDY_CARDS: &[Domain] = &["study", "notes"];
const TAGS: &[Domain] = &["tags"];
const TRASH: &[Domain] = &["notes", "decks"];
const TTS: &[Domain] = &["tts"];
// Phase 3 (v0.6 statistics, v0.8 AI and MCP). Statistics take a scope that may be a query
// (an oracle on note contents, like `search.ids`), hence `search` too.
const STATS: &[Domain] = &["stats", "search"];
const STATS_ATTRIBUTES: &[Domain] = &["stats", "search", "attributes"];
const STATS_DECKS: &[Domain] = &["stats", "search", "decks"];
const STATS_ROWS: &[Domain] = &["stats", "search", "cards", "notes"];
const STATS_CARDS: &[Domain] = &["cards", "stats", "search"];
const PLANNING: &[Domain] = &["planning"];
const GOALS: &[Domain] = &["goals"];
const AI: &[Domain] = &["ai"];
/// AI requests and outputs hold note and source text.
const AI_NOTES: &[Domain] = &["ai", "notes"];
/// Never readable by a plugin.
const PLUGINS: &[Domain] = &["plugins"];
const IO: &[Domain] = &["io"];

const UNBOUNDED: &str = "whole-collection work is not available to plugins";
const AI_WRITES: &str = "AI writes are not available to plugins";

/// What calling method `name` (unqualified) exposes. **Every** public method a plugin could
/// be granted is listed (a test walks the catalog); anything else is refused.
pub(crate) fn exposure(name: &str) -> Exposure {
    use Exposure::{Oracle, Reads, Refused};
    match name {
        "attributes.list" | "attributes.effective" | "attributes.templates" | "attributes.get"
        | "attributes.define" | "attributes.define_from_template" | "attributes.update"
        | "attributes.delete" | "attributes.restore" | "attributes.option.add"
        | "attributes.option.rename" | "attributes.option.set_color"
        | "attributes.option.reorder" | "attributes.option.merge" | "attributes.option.delete"
        | "attributes.values.get" | "attributes.values.set" | "bulk.set_attribute" => {
            Reads(ATTRIBUTES)
        }
        "browse.card_info" => Reads(CARD_INFO),
        "browse.trash" => Reads(TRASH),
        "browse.duplicates" | "browse.check_database" | "browse.repair_database"
        | "browse.purge_trash" => Refused(UNBOUNDED),
        "palette.list" | "modules.list" | "commands.list" => Reads(MODULES),
        "shortcuts.list" | "settings.schema" | "settings.get" => Reads(SETTINGS),
        "bulk.move_cards" | "bulk.set_suspended" | "bulk.set_buried" | "bulk.set_flag"
        | "bulk.reset_progress" | "flags.list" | "flags.update" | "cards.set_flag"
        | "cards.set_flags" | "cards.set_user_note" | "cards.meta" => Reads(CARDS),
        "bulk.add_tags" | "bulk.remove_tags" | "bulk.change_notetype" | "notes.create"
        | "notes.update" | "notes.delete" | "notes.restore" | "notes.get" | "notes.list"
        | "links.backlinks" | "links.suggest" => Reads(NOTES),
        "bulk.find_replace" => Oracle(NOTES),
        "notetypes.create" | "notetypes.update" | "notetypes.delete" | "notetypes.restore"
        | "notetypes.usage" | "notetypes.list" | "notetypes.get" | "cardtypes.list" => {
            Reads(NOTETYPES)
        }
        "cards.render" | "cards.preview" => Reads(CARDS_NOTES),
        "media.add" | "media.get" | "media.list" | "media.read" => Reads(MEDIA),
        "jobs.list" | "jobs.get" => Reads(JOBS),
        "decks.tree" | "decks.list" | "decks.get" | "decks.create" | "decks.rename"
        | "decks.move" | "decks.update" | "decks.delete" | "decks.restore" | "presets.list"
        | "presets.get" | "presets.for_deck" | "presets.create" | "presets.update"
        | "presets.clone" | "presets.delete" | "presets.assign" => Reads(DECKS),
        "search.cards" => Reads(SEARCH_ROWS),
        "search.ids" | "search.check" | "saved_queries.list" | "saved_queries.create"
        | "saved_queries.update" | "saved_queries.delete" => Reads(SEARCH),
        "study.start" | "study.session" | "study.active" | "study.answer" | "study.hint"
        | "study.check_answer" | "study.suspend" | "study.bury" | "study.postpone" => {
            Reads(STUDY_CARDS)
        }
        "study.end" | "study.summary" | "study.card_history" => Reads(STUDY),
        "tags.list" | "tags.rename" | "tags.merge" | "tags.set_color" => Reads(TAGS),
        "tts.providers" | "tts.voices" | "tts.synthesize" => Reads(TTS),
        "stats.overview" => Reads(STATS),
        "stats.breakdown" => Reads(STATS_ATTRIBUTES),
        "stats.deck_reports" => Reads(STATS_DECKS),
        "stats.leeches" => Reads(STATS_ROWS),
        // Which cards it changes says which cards are leeches.
        "stats.apply_to_leeches" => Oracle(STATS_CARDS),
        // Whole-history work (cache rebuild, FSRS training): the user's, never a plugin's.
        "stats.refresh_cache" | "optimizer.run" => Refused(UNBOUNDED),
        "optimizer.workload" => Reads(DECKS),
        "planning.exams.list" | "planning.exams.create" | "planning.exams.update"
        | "planning.exams.delete" => Reads(PLANNING),
        "goals.list" | "goals.create" | "goals.update" | "goals.delete" => Reads(GOALS),
        "ai.providers.list" | "ai.routes.list" | "ai.prices.list" | "ai.usage.summary" => Reads(AI),
        "ai.preview" => Reads(AI_NOTES),
        // AI drafts are applied by the user (a plugin writes notes with `notes.*` directly).
        "ai.notes.add" | "ai.notes.update" => Refused(AI_WRITES),
        _ => Refused("not available to plugins"),
    }
}

/// [`exposure`] of one call, by its params where they matter: `settings.get` of another
/// plugin's scope is plugin data; of a deck or preset, deck data too.
pub(crate) fn call_exposure(name: &str, params: &Value, caller: &str) -> Exposure {
    if name == "settings.get" {
        let scope = &params["scope"];
        return match scope["kind"].as_str() {
            Some("plugin") if scope["id"].as_str() != Some(caller) => Exposure::Reads(PLUGINS),
            Some("deck" | "preset") => Exposure::Reads(&["settings", "decks"]),
            _ => Exposure::Reads(SETTINGS),
        };
    }
    exposure(name)
}

/// Whether `granted` may read every one of `domains`.
pub(crate) fn can_read(granted: &BTreeSet<String>, domains: &[Domain]) -> bool {
    domains.iter().all(|d| {
        fc_plugins::permission::api_grantable(&format!("{d}:read"))
            && granted.contains(&format!("{d}:read"))
    })
}

/// The read permissions of `domains` missing from `granted`.
pub(crate) fn missing(granted: &BTreeSet<String>, domains: &[Domain]) -> Vec<String> {
    domains
        .iter()
        .filter(|d| !can_read(granted, &[d]))
        .map(|d| format!("{d}:read"))
        .collect()
}

fn id_key(key: &str) -> bool {
    key == "id" || key == "ids" || key.ends_with("_id") || key.ends_with("_ids")
}

fn id_value(v: &Value) -> bool {
    match v {
        Value::Number(_) | Value::String(_) | Value::Null => true,
        Value::Array(xs) => xs.iter().all(|x| matches!(x, Value::Number(_) | Value::String(_))),
        _ => false,
    }
}

/// `value` cut down to its ids (`id`, `ids`, `*_id`, `*_ids` with scalar values), keeping
/// the structure that leads to them: what a plugin learns about data it may not read.
pub(crate) fn ids_only(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if id_key(k) && id_value(v) {
                    out.insert(k.clone(), v.clone());
                } else if matches!(v, Value::Object(_) | Value::Array(_)) {
                    let inner = ids_only(v);
                    let empty = match &inner {
                        Value::Object(m) => m.is_empty(),
                        Value::Array(a) => a.is_empty(),
                        _ => true,
                    };
                    if !empty {
                        out.insert(k.clone(), inner);
                    }
                }
            }
            Value::Object(out)
        }
        Value::Array(xs) => {
            let kept: Vec<Value> = xs
                .iter()
                .filter(|x| matches!(x, Value::Object(_) | Value::Array(_)))
                .map(ids_only)
                .collect();
            Value::Array(kept)
        }
        _ => Value::Null,
    }
}

/// `value` if `granted` reads all of `domains`, else [`ids_only`].
pub(crate) fn filter(granted: &BTreeSet<String>, domains: &[Domain], value: Value) -> Value {
    if can_read(granted, domains) {
        value
    } else {
        ids_only(&value)
    }
}

/// Domains of a job's message, error and result, by its kind.
fn job_domains(kind: &str) -> &'static [Domain] {
    match kind {
        "browse.purge_trash" => NOTES,
        // Imports, exports, backups, restores: file paths and whole collections. Model
        // downloads: plugin data. Anything new: closed until classified.
        k if k.starts_with("plugins.") => PLUGINS,
        // AI requests and model lists; extracted sources come from files and the web (io).
        "ai.request" | "ai.models" => AI_NOTES,
        "optimizer.run" => &["decks", "stats"],
        _ => IO,
    }
}

/// One job as a plugin sees it: status and progress always; message, error and result only
/// with the read permissions of the job's domain.
fn job(granted: &BTreeSet<String>, mut job: Value) -> Value {
    let kind = job["kind"].as_str().unwrap_or_default().to_owned();
    if !can_read(granted, job_domains(&kind)) {
        if let Value::Object(m) = &mut job {
            for k in ["message", "error", "result"] {
                if let Some(v) = m.get_mut(k) {
                    *v = if v.is_null() { Value::Null } else { ids_only(v) };
                }
            }
        }
    }
    job
}

/// `jobs.get` / `jobs.list` responses, filtered job by job.
pub(crate) fn jobs(granted: &BTreeSet<String>, method: &str, mut response: Value) -> Value {
    match method {
        "jobs.get" => job(granted, response),
        "jobs.list" => {
            if let Some(Value::Array(js)) = response.get_mut("jobs") {
                for j in js.iter_mut() {
                    *j = job(granted, std::mem::take(j));
                }
            }
            response
        }
        _ => response,
    }
}

/// Domains of a domain event's payload.
pub(crate) fn event_domains(event: &DomainEvent) -> &'static [Domain] {
    use DomainEvent as E;
    match event {
        E::DeckCreated { .. }
        | E::DeckUpdated { .. }
        | E::DeckMoved { .. }
        | E::DeckDeleted { .. }
        | E::DeckRestored { .. }
        | E::PresetsChanged => DECKS,
        E::NoteCreated { .. }
        | E::NoteUpdated { .. }
        | E::NotesDeleted { .. }
        | E::NotesRestored { .. }
        | E::Imported { .. }
        | E::CollectionReplaced => NOTES,
        E::TagsChanged => TAGS,
        E::NoteTypeChanged { .. } => NOTETYPES,
        E::CardsUpdated { .. } | E::FlagsChanged | E::CardsChanged { .. } => CARDS,
        E::AttributeDefined { .. }
        | E::AttributeUpdated { .. }
        | E::AttributeDeleted { .. }
        | E::AttributeValuesChanged { .. } => ATTRIBUTES,
        E::SettingChanged { scope, .. } => match scope {
            fc_core::settings::SettingScope::Plugin(_) => PLUGINS,
            fc_core::settings::SettingScope::Deck(_) | fc_core::settings::SettingScope::Preset(_) => {
                &["settings", "decks"]
            }
            _ => SETTINGS,
        },
        E::ModuleToggled { .. } => MODULES,
        E::JobProgress { job } => job_domains(&job.kind),
        E::SavedQueriesChanged => SEARCH,
        // Undo summaries carry the undone step's data (names, texts) of any domain.
        E::Undone { .. } | E::Redone { .. } => &["oplog"],
        E::PluginsChanged { .. } | E::PluginNotice { .. } | E::PluginHooksSkipped { .. } => {
            PLUGINS
        }
        E::StudyCardsChanged { .. } => STUDY,
        // Another process wrote: no payload.
        E::DataChanged => &[],
        E::ExamsChanged => PLANNING,
        E::GoalsChanged => GOALS,
        E::AiConfigChanged => AI,
        E::AiPartial { .. } => AI_NOTES,
    }
}

/// Domains whose events a plugin is not even told about without their read permissions
/// (statistics, planning, goals, AI, MCP — phase 3): elsewhere an unreadable event still
/// arrives cut to ids, these do not arrive at all, so a subscription alone says nothing
/// (not even that an AI request is streaming or a goal changed).
const GATED_EVENT_DOMAINS: &[Domain] = &["stats", "planning", "goals", "ai", "mcp"];

/// Whether an `event:` hook of a plugin holding `granted` is called for `event` at all.
pub(crate) fn event_delivered(granted: &BTreeSet<String>, event: &DomainEvent) -> bool {
    let domains = event_domains(event);
    !domains.iter().any(|d| GATED_EVENT_DOMAINS.contains(d)) || can_read(granted, domains)
}

/// An event as `event:` hook input (`{"type", "data"}`): whole with the read permissions of
/// its domains, else its data cut to ids.
pub(crate) fn event(granted: &BTreeSet<String>, event: &DomainEvent) -> Value {
    let value = serde_json::to_value(event).unwrap_or(Value::Null);
    if can_read(granted, event_domains(event)) {
        return value;
    }
    let mut out = Map::new();
    out.insert("type".into(), value["type"].clone());
    if let Some(data) = value.get("data") {
        out.insert("data".into(), ids_only(data));
    }
    Value::Object(out)
}

/// Detail keys that describe the call, not the data (kept on a redacted error).
const STRUCTURAL_DETAILS: &[&str] = &[
    "method", "permission", "function", "field", "module", "limit", "max", "max_bytes",
    "missing", "provider", "reason", "kind", "code",
];

/// An error of method `method` as plugin `caller` sees it. Without the read permissions of
/// the method's domains: the message is the code alone and the details keep only ids and
/// keys that describe the call. Errors raised by *other* plugins (a `before:` hook's
/// rejection or failure) keep only the plugin id.
pub(crate) fn error(
    granted: &BTreeSet<String>,
    caller: &str,
    domains: &[Domain],
    mut e: fc_api::ApiError,
) -> fc_api::ApiError {
    let other_plugin = matches!(
        e.code,
        ErrorCode::ConflictPluginRejected | ErrorCode::ProviderPluginFailed
    ) && e.details.get("plugin_id").and_then(Value::as_str) != Some(caller);
    if other_plugin {
        e.message = super::host::code_str(e.code);
        e.details.retain(|k, _| k == "plugin_id");
        return e;
    }
    if can_read(granted, domains) {
        return e;
    }
    e.message = super::host::code_str(e.code);
    let details = Value::Object(std::mem::take(&mut e.details));
    let mut kept: Map<String, Value> = match ids_only(&details) {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    for k in STRUCTURAL_DETAILS {
        if let Some(v) = details.get(*k) {
            kept.insert((*k).to_owned(), v.clone());
        }
    }
    e.details = kept;
    e
}

/// Whether a plugin may be shown the result of `kind` methods without running them first:
/// queries are refused up front when unreadable (nothing to cut), commands run and their
/// response is cut.
pub(crate) fn refuse_upfront(kind: MethodKind, exposure: Exposure) -> bool {
    kind == MethodKind::Query || matches!(exposure, Exposure::Oracle(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_api::v1::common::{AffectedCount, EntityRef, I18nMessage};
    use fc_api::v1::plugins::{LogLevel, PluginNotice, UiText};
    use fc_core::settings::SettingScope;
    use serde_json::json;

    fn grants(g: &[&str]) -> BTreeSet<String> {
        g.iter().map(|s| (*s).to_owned()).collect()
    }

    fn text(v: &Value) -> String {
        v.to_string()
    }

    #[test]
    fn ids_only_keeps_ids_and_their_path() {
        let v = json!({
            "id": 7, "name": "Secret", "deck": { "id": 3, "path": "Private::Deck" },
            "note_ids": [1, 2], "fields": { "front": "kılıç" }, "items": [{ "id": 1, "sort_field": "x" }, 5],
            "count": 3,
        });
        assert_eq!(
            ids_only(&v),
            json!({ "id": 7, "deck": { "id": 3 }, "note_ids": [1, 2], "items": [{ "id": 1 }] })
        );
        assert_eq!(ids_only(&json!("text")), Value::Null);
    }

    /// Every public method a plugin could be granted is classified; the forbidden areas are
    /// never readable.
    #[test]
    fn every_grantable_method_is_classified() {
        for d in fc_api::v1::methods::catalog() {
            if d.visibility != fc_api::Visibility::Public
                || !fc_plugins::permission::api_grantable(d.permission)
            {
                continue;
            }
            let e = exposure(d.name);
            assert_ne!(e, Exposure::Refused("not available to plugins"), "{} is not classified", d.name);
            if let Exposure::Reads(ds) | Exposure::Oracle(ds) = e {
                let area = d.permission.split_once(':').map_or("", |(a, _)| a);
                assert!(ds.contains(&area), "{}: its own area {area} is one of its domains", d.name);
            }
        }
        for area in fc_plugins::permission::FORBIDDEN_API_AREAS.iter().chain(&["oplog"]) {
            assert!(!can_read(&grants(&[&format!("{area}:read")]), &[area]), "{area}");
        }
    }

    /// One check per domain: with its read permission the value passes whole; without it
    /// (even holding the write permission) only ids pass.
    fn domain_case(domain: &str, domains: &[Domain], value: Value, secret: &str) {
        let read = format!("{domain}:read");
        let write = format!("{domain}:write");
        let all: Vec<String> = domains.iter().map(|d| format!("{d}:read")).collect();
        let all: Vec<&str> = all.iter().map(String::as_str).collect();
        assert!(text(&filter(&grants(&all), domains, value.clone())).contains(secret), "{domain}");
        let without: Vec<&str> = all.iter().copied().filter(|g| *g != read).collect();
        let cut = filter(&grants(&[without.as_slice(), &[write.as_str()]].concat()), domains, value);
        assert!(!text(&cut).contains(secret), "{domain}: {cut}");
    }

    #[test]
    fn domain_notes() {
        let d = exposure("notes.update").domains();
        domain_case("notes", d, json!({ "id": 1, "fields": { "front": "SECRET" } }), "SECRET");
        // Cross-domain results need every domain: search rows hold note text.
        let d = exposure("search.cards").domains();
        domain_case("notes", d, json!({ "items": [{ "card_id": 1, "question": "SECRET" }] }), "SECRET");
    }

    #[test]
    fn domain_cards() {
        let d = exposure("cards.meta").domains();
        domain_case("cards", d, json!({ "cards": [{ "id": 1, "user_note": "SECRET" }] }), "SECRET");
    }

    #[test]
    fn domain_decks() {
        let d = exposure("decks.create").domains();
        domain_case("decks", d, json!({ "id": 1, "path": "SECRET" }), "SECRET");
    }

    #[test]
    fn domain_notetypes() {
        let d = exposure("notetypes.get").domains();
        domain_case("notetypes", d, json!({ "id": 1, "name": "SECRET" }), "SECRET");
    }

    #[test]
    fn domain_attributes() {
        let d = exposure("attributes.values.set").domains();
        domain_case("attributes", d, json!({ "values": [{ "def_id": 1, "value": "SECRET" }] }), "SECRET");
    }

    #[test]
    fn domain_tags() {
        let d = exposure("tags.rename").domains();
        domain_case("tags", d, json!({ "tags": ["SECRET"] }), "SECRET");
    }

    #[test]
    fn domain_media() {
        let d = exposure("media.add").domains();
        domain_case("media", d, json!({ "id": "m1", "file_name": "SECRET.png" }), "SECRET");
    }

    #[test]
    fn domain_search() {
        let d = exposure("saved_queries.create").domains();
        domain_case("search", d, json!({ "id": 1, "query": "SECRET" }), "SECRET");
    }

    #[test]
    fn domain_study() {
        let d = exposure("study.answer").domains();
        domain_case("study", d, json!({ "id": 1, "current": { "card_id": 2, "sides": "SECRET" } }), "SECRET");
        let d = exposure("study.card_history").domains();
        domain_case("study", d, json!({ "reviews": [{ "id": 1, "rating": "SECRET" }] }), "SECRET");
    }

    #[test]
    fn domain_modules() {
        let d = exposure("commands.list").domains();
        domain_case("modules", d, json!({ "commands": [{ "id": "x", "label": "SECRET" }] }), "SECRET");
    }

    #[test]
    fn domain_settings() {
        let d = exposure("settings.get").domains();
        domain_case("settings", d, json!({ "values": [{ "key": "SECRET" }] }), "SECRET");
        // Another plugin's settings are plugin data: never readable, whatever is granted.
        let other = json!({ "scope": { "kind": "plugin", "id": "com.other" } });
        let own = json!({ "scope": { "kind": "plugin", "id": "com.me" } });
        let g = grants(&["settings:read"]);
        assert!(!can_read(&g, call_exposure("settings.get", &other, "com.me").domains()));
        assert!(can_read(&g, call_exposure("settings.get", &own, "com.me").domains()));
    }

    #[test]
    fn domain_tts() {
        let d = exposure("tts.voices").domains();
        domain_case("tts", d, json!({ "voices": [{ "id": "v", "name": "SECRET" }] }), "SECRET");
    }

    #[test]
    fn domain_jobs() {
        let g = grants(&["jobs:read", "notes:read"]);
        let export = json!({
            "id": 1, "kind": "export.json", "status": "succeeded",
            "result": { "path": "/home/me/SECRET.json" }, "message": { "key": "SECRET" },
        });
        let seen = jobs(&g, "jobs.get", export.clone());
        assert!(!text(&seen).contains("SECRET"), "{seen}");
        assert_eq!(seen["status"], "succeeded");
        let list = jobs(&g, "jobs.list", json!({ "jobs": [export] }));
        assert!(!text(&list).contains("SECRET"), "{list}");
        let purge = json!({ "id": 2, "kind": "browse.purge_trash", "result": { "note": "SECRET" } });
        assert!(text(&jobs(&g, "jobs.get", purge)).contains("SECRET"));
    }

    #[test]
    fn domain_events() {
        const DECK: &str = "00000000-0000-4000-8000-000000000005";
        let deck: fc_api::v1::decks::DeckDto = serde_json::from_value(json!({
            "id": DECK, "parent_id": null, "name": "SECRET", "path": "SECRET",
            "description": "", "icon": null, "color": null, "position": 0,
            "preset_id": null, "created_at": 0, "updated_at": 0,
        }))
        .unwrap_or_else(|e| panic!("DeckDto fixture: {e}"));
        let created = DomainEvent::DeckCreated { deck };
        let seen = event(&grants(&["decks:write"]), &created);
        assert_eq!(seen["type"], "deck_created");
        assert_eq!(seen["data"]["deck"]["id"], DECK);
        assert!(!text(&seen).contains("SECRET"), "{seen}");
        assert!(text(&event(&grants(&["decks:read"]), &created)).contains("SECRET"));
        // Plugin data, undo summaries and io never pass, whatever is granted.
        let all = grants(&["decks:read", "notes:read", "settings:read", "jobs:read"]);
        for e in [
            DomainEvent::PluginNotice {
                notice: PluginNotice {
                    plugin_id: "com.other".into(),
                    level: LogLevel::Info,
                    text: UiText::Text { text: "SECRET".into() },
                },
            },
            DomainEvent::Undone {
                reverted_batch_id: DECK.parse().unwrap(),
                summary: I18nMessage::key("x").param("name", "SECRET"),
            },
            DomainEvent::SettingChanged {
                scope: SettingScope::Plugin("com.other".into()),
                key: "SECRET".into(),
            },
        ] {
            assert!(!text(&event(&all, &e)).contains("SECRET"), "{e:?}");
        }
        let deleted = DomainEvent::DeckDeleted {
            id: DECK.parse().unwrap(),
            affected: AffectedCount::default(),
        };
        assert_eq!(event(&grants(&[]), &deleted)["data"]["id"], DECK);
        let values = DomainEvent::AttributeValuesChanged { entities: Vec::<EntityRef>::new() };
        assert_eq!(event(&grants(&[]), &values)["type"], "attribute_values_changed");
    }

    // ---- phase 3 (round 4): one check per new domain ----

    #[test]
    fn domain_stats() {
        let d = exposure("stats.overview").domains();
        domain_case("stats", d, json!({ "totals": { "reviews": 3 }, "heatmap": ["SECRET"] }), "SECRET");
        // A scope query is an oracle on note contents: it needs `search:read` as well.
        domain_case("search", d, json!({ "scope": { "query": "SECRET" } }), "SECRET");
        let d = exposure("stats.leeches").domains();
        domain_case("notes", d, json!({ "items": [{ "card_id": 1, "sort_field": "SECRET" }] }), "SECRET");
        let d = exposure("stats.breakdown").domains();
        domain_case("attributes", d, json!({ "rows": [{ "option_id": 1, "label": "SECRET" }] }), "SECRET");
        // Whole-history work is the user's.
        for m in ["stats.refresh_cache", "optimizer.run"] {
            assert!(matches!(exposure(m), Exposure::Refused(_)), "{m}");
        }
        assert!(matches!(exposure("stats.apply_to_leeches"), Exposure::Oracle(_)));
        assert!(!fc_plugins::permission::api_grantable("stats:write"));
        assert_eq!(job_domains("optimizer.run"), &["decks", "stats"]);
    }

    #[test]
    fn domain_planning() {
        let d = exposure("planning.exams.create").domains();
        domain_case("planning", d, json!({ "id": 1, "name": "SECRET", "query": "SECRET" }), "SECRET");
        assert_eq!(event_domains(&DomainEvent::ExamsChanged), PLANNING);
        assert!(!event_delivered(&grants(&["planning:write"]), &DomainEvent::ExamsChanged));
        assert!(event_delivered(&grants(&["planning:read"]), &DomainEvent::ExamsChanged));
    }

    #[test]
    fn domain_goals() {
        let d = exposure("goals.update").domains();
        domain_case("goals", d, json!({ "id": 1, "query": "SECRET", "progress": { "label": "SECRET" } }), "SECRET");
        assert!(!event_delivered(&grants(&["goals:write"]), &DomainEvent::GoalsChanged));
        assert!(event_delivered(&grants(&["goals:read"]), &DomainEvent::GoalsChanged));
    }

    #[test]
    fn domain_ai() {
        let d = exposure("ai.providers.list").domains();
        domain_case("ai", d, json!({ "providers": [{ "id": "p", "base_url": "https://SECRET" }] }), "SECRET");
        // Previews hold note text: `notes:read` too.
        let d = exposure("ai.preview").domains();
        domain_case("notes", d, json!({ "request": { "messages": ["SECRET"] } }), "SECRET");
        for m in ["ai.notes.add", "ai.notes.update"] {
            assert!(matches!(exposure(m), Exposure::Refused(_)), "{m}");
        }
        for p in ["ai:write", "ai_keys:write", "ai_requests:write"] {
            assert!(!fc_plugins::permission::api_grantable(p), "{p}");
        }
        // Streamed output: not delivered at all without `ai:read` + `notes:read`; whole with them.
        let partial = DomainEvent::AiPartial { job_id: "00000000-0000-4000-8000-000000000009".parse().unwrap(), text: "SECRET".into() };
        assert!(!event_delivered(&grants(&["ai:read"]), &partial));
        let g = grants(&["ai:read", "notes:read"]);
        assert!(event_delivered(&g, &partial) && text(&event(&g, &partial)).contains("SECRET"));
        assert!(!event_delivered(&grants(&[]), &DomainEvent::AiConfigChanged));
        // AI job results (drafts, model lists) by kind; extracted sources are io (never).
        let job = json!({ "id": 1, "kind": "ai.request", "result": { "text": "SECRET" } });
        assert!(!text(&jobs(&grants(&["jobs:read", "ai:read"]), "jobs.get", job.clone())).contains("SECRET"));
        assert!(text(&jobs(&g, "jobs.get", job)).contains("SECRET"));
        let source = json!({ "id": 2, "kind": "ai.source", "result": { "text": "SECRET" } });
        assert!(!text(&jobs(&g, "jobs.get", source)).contains("SECRET"));
    }

    #[test]
    fn domain_mcp() {
        // External program registrations are never readable, whatever is granted.
        assert!(!can_read(&grants(&["mcp:read"]), &["mcp"]));
        let cut = filter(&grants(&["mcp:read", "mcp:write"]), &["mcp"], json!({ "id": 1, "command": "SECRET" }));
        assert!(!text(&cut).contains("SECRET"), "{cut}");
        for m in ["mcp.tools.list", "mcp.servers.list", "mcp.servers.add", "mcp.servers.update", "mcp.servers.remove"] {
            assert!(matches!(exposure(m), Exposure::Refused(_)), "{m}");
        }
        assert!(GATED_EVENT_DOMAINS.contains(&"mcp"));
        // Another process's write (an MCP client) carries no payload: delivered to anyone.
        assert!(event_delivered(&grants(&[]), &DomainEvent::DataChanged));
    }

    #[test]
    fn errors_are_cut_without_read() {
        let e = fc_api::ApiError::new(ErrorCode::ConflictDeckExists, "deck \"SECRET\" exists")
            .with_detail("path", "SECRET")
            .with_detail("deck_id", "d4")
            .with_detail("field", "path");
        let cut = error(&grants(&["decks:write"]), "com.me", DECKS, e.clone());
        assert!(!cut.message.contains("SECRET") && !text(&Value::Object(cut.details.clone())).contains("SECRET"));
        assert_eq!(cut.details["deck_id"], "d4");
        assert_eq!(cut.details["field"], "path");
        assert_eq!(error(&grants(&["decks:read"]), "com.me", DECKS, e.clone()), e);
        // Another plugin's rejection: only its id, whatever is granted.
        let r = fc_api::ApiError::new(ErrorCode::ConflictPluginRejected, "SECRET")
            .with_detail("plugin_id", "com.other")
            .with_detail("message", "SECRET");
        let cut = error(&grants(&["decks:read"]), "com.me", DECKS, r);
        assert!(!cut.message.contains("SECRET"));
        assert_eq!(Value::Object(cut.details), json!({ "plugin_id": "com.other" }));
    }
}
