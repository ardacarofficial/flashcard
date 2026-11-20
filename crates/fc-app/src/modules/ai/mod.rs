//! `ai` module (v0.8, docs/06 §1): AI providers, per-feature routing, the price table, usage
//! tracking and the AI features (card generation, field filling, explain, hint, answer check,
//! improve, leech rewrite, distractors).
//!
//! **Nothing is sent without a preview.** `ai.preview` builds the exact request — provider,
//! model, system prompt, messages (with the source/notes they contain), token and cost
//! estimate — and a hash of it. `ai.run` must carry that hash: it rebuilds the request and
//! refuses (`CONFLICT_AI_PREVIEW_STALE`) if anything differs, then queues an `ai.request` job
//! with the request itself, so what runs is what was shown. The job calls the provider
//! outside any transaction (with timeout and retries), records each call in `ai_usage`
//! (internal `ai.usage.record`), validates the answer against the feature's schema (one repair
//! attempt) and returns an `AiJobResult`. Applying a result is an ordinary undoable command
//! (`ai.notes.add`, `ai.notes.update`).
//!
//! **Keys** live only in the OS keychain ([`AiServices`]' [`SecretStore`], account
//! `ai.provider.<id>`): `ai.providers.set_key` is not an undo step, so its params never reach
//! the oplog; responses carry only `has_key`; provider errors are scrubbed of the key.
//!
//! **Classifications** (docs/02 §8.1): generation and field filling read the deck's
//! classifications (values given or their defaults), put them in the prompt ("Seviye: B1") and
//! on the generated notes.

mod context;
mod notes;
mod prompts;
mod providers;
mod run;
mod sources;
mod usage;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_v2;
#[cfg(test)]
mod tests_phase3;

use crate::app::App;
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use fc_ai::secrets::{NoSecrets, SecretStore};
use fc_api::v1::ai::AiFeature;
use fc_core::settings::{SettingDef, SettingKind, SettingScopeKind};
use fc_core::AiProviderId;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Module id.
pub const ID: &str = "ai";

/// Job kind of a confirmed AI request.
pub const REQUEST_JOB: &str = "ai.request";
/// Job kind of a model list refresh.
pub const MODELS_JOB: &str = "ai.models";
/// Job kind of a source extraction (PDF pages, web page).
pub const SOURCE_JOB: &str = "ai.source";
/// Worker lane of every AI job: network waits never hold up imports/exports.
pub const LANE: &str = "ai";

/// Setting: monthly budget in USD (`0` = none).
pub const BUDGET: &str = "ai.monthly_budget_usd";
/// Setting: whole-request timeout in seconds.
pub const TIMEOUT: &str = "ai.request_timeout_secs";
/// Setting: retries of a rate-limited/unavailable request.
pub const RETRIES: &str = "ai.max_retries";
/// Setting: let "cards from a web page" reach local-network hosts (default off).
pub const WEB_ALLOW_PRIVATE: &str = "ai.web.allow_private_hosts";

/// Setting key of a feature's prompt template (`ai.prompt.generate_cards`, …).
pub fn prompt_setting(feature: AiFeature) -> String {
    format!("ai.prompt.{}", feature.id())
}

/// Keychain account of a provider's API key.
pub fn key_account(id: AiProviderId) -> String {
    format!("ai.provider.{id}")
}

/// Services the module needs from the host: the keychain (installed by the transport with
/// [`App::set_secret_store`]). Reached through the registry so handlers and jobs share it.
pub struct AiServices {
    secrets: RwLock<Arc<dyn SecretStore>>,
    keychain: AtomicBool,
    /// First retry pause in ms (doubles each retry); tests set 0.
    retry_base_ms: AtomicU64,
}

impl Default for AiServices {
    fn default() -> Self {
        Self {
            secrets: RwLock::new(Arc::new(NoSecrets)),
            keychain: AtomicBool::new(false),
            retry_base_ms: AtomicU64::new(1000),
        }
    }
}

impl AiServices {
    /// The keychain.
    pub fn secrets(&self) -> Arc<dyn SecretStore> {
        self.secrets
            .read()
            .map(|s| Arc::clone(&s))
            .unwrap_or_else(|_| Arc::new(NoSecrets))
    }

    /// Whether a real keychain is installed.
    pub fn has_keychain(&self) -> bool {
        self.keychain.load(Ordering::SeqCst)
    }

    fn set_secrets(&self, store: Arc<dyn SecretStore>) {
        if let Ok(mut s) = self.secrets.write() {
            *s = store;
            self.keychain.store(true, Ordering::SeqCst);
        }
    }

    /// Whether stored keys outlive this session (see [`SecretStore::persistent`]).
    pub fn keys_persist(&self) -> bool {
        self.secrets().persistent()
    }

    /// Pause before retry `attempt` (0-based): what the provider asked for (`Retry-After`)
    /// plus up to 10 % jitter, else exponential backoff with "equal jitter" (half fixed, half
    /// random), so clients that failed together do not retry together.
    pub(crate) fn retry_pause(&self, attempt: u32, retry_after_ms: Option<u64>) -> Duration {
        backoff(
            self.retry_base_ms.load(Ordering::SeqCst),
            attempt,
            retry_after_ms,
            jitter_unit(),
        )
    }

    #[cfg(test)]
    pub(crate) fn set_retry_base_ms(&self, ms: u64) {
        self.retry_base_ms.store(ms, Ordering::SeqCst);
    }
}

/// See [`AiServices::retry_pause`]; `unit` is a random number in `[0, 1)`.
pub(crate) fn backoff(base_ms: u64, attempt: u32, retry_after_ms: Option<u64>, unit: f64) -> Duration {
    let unit = unit.clamp(0.0, 1.0);
    let ms = match retry_after_ms {
        // The server's wait is a minimum; the jitter only spreads clients out after it.
        Some(after) => after + (after as f64 * 0.1 * unit) as u64,
        None => {
            let full = base_ms.saturating_mul(1 << attempt.min(6));
            full / 2 + ((full / 2) as f64 * unit) as u64
        }
    };
    Duration::from_millis(ms)
}

/// A random number in `[0, 1)` from the standard library's per-instance random hash keys
/// (no extra dependency; jitter needs no quality randomness).
fn jitter_unit() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    (h.finish() >> 11) as f64 / (1u64 << 53) as f64
}

impl App {
    /// Installs the keychain API keys are stored in (desktop/CLI: the OS keychain; tests: an
    /// in-memory store). Until then keys cannot be set (`PROVIDER_AI_KEYCHAIN`).
    pub fn set_secret_store(&self, store: Arc<dyn SecretStore>) {
        if let Some(s) = self.registry().extension::<AiServices>() {
            s.set_secrets(store);
        }
    }

    /// The AI module's services (tests and transports).
    pub fn ai_services(&self) -> Option<Arc<AiServices>> {
        self.registry().extension::<AiServices>()
    }
}

fn number_setting(key: &str, kind: SettingKind, default: serde_json::Value) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind,
        default,
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    let mut settings = vec![
        number_setting(
            BUDGET,
            SettingKind::Float {
                min: Some(0.0),
                max: Some(100_000.0),
            },
            serde_json::json!(0.0),
        ),
        number_setting(
            TIMEOUT,
            SettingKind::Integer {
                min: Some(5),
                max: Some(600),
            },
            serde_json::json!(120),
        ),
        number_setting(
            RETRIES,
            SettingKind::Integer {
                min: Some(0),
                max: Some(5),
            },
            serde_json::json!(2),
        ),
        number_setting(WEB_ALLOW_PRIVATE, SettingKind::Bool, serde_json::json!(false)),
    ];
    // Prompt templates: visible and editable under "advanced" (docs/06 §1.4).
    for feature in AiFeature::ALL {
        let key = prompt_setting(feature);
        settings.push(SettingDef {
            label_key: format!("settings.ai.prompt.{}.label", feature.id()),
            description_key: Some("settings.ai.prompt.description".into()),
            key,
            kind: SettingKind::Text {
                max_len: Some(prompts::MAX_TEMPLATE_CHARS),
            },
            default: serde_json::Value::String(prompts::default_template(feature).to_owned()),
            advanced: true,
            scopes: vec![SettingScopeKind::App],
        });
    }
    ModuleDescriptor {
        id: ID,
        name_key: "modules.ai.name",
        description_key: "modules.ai.description",
        enabled_by_default: true,
        can_disable: true,
        settings,
        ui: vec![main_menu("ai.main", "nav.ai", "sparkles", "/ai", 45)],
    }
}

/// Registers the module, its methods and jobs.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    let services = Arc::new(AiServices::default());
    reg.provide(Arc::clone(&services));
    providers::register(reg, &services);
    usage::register(reg);
    run::register(reg, &services);
    sources::register(reg);
    notes::register(reg);
}
