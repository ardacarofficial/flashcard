//! Provider presets (v1.3): named starting points the UI's "add provider" gallery is drawn
//! from — Claude, OpenAI, OpenRouter, Gemini, Groq, Mistral, DeepSeek, Ollama, LM Studio and
//! a custom OpenAI-compatible endpoint.
//!
//! A preset is **data, not an engine**: each one reuses one of the four wire protocols
//! ([`ProviderKind`]). A stored provider does not remember which preset it came from; its
//! preset is recognised again from its kind and address ([`detect`]), so editing the address
//! can never leave a stale "this is OpenRouter" flag behind, and old providers (created
//! before presets existed) get the right key rules without a migration.
//!
//! Facts (base URLs, key pages) were checked against the vendors' public documentation in
//! November 2026 where it could be reached; the ones that could not be checked are marked
//! `UNVERIFIED` below. Model ids are only suggestions shown before the real list is fetched
//! from the provider (`list_models`); they are editable data.

use crate::ProviderKind;

/// A named starting point for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderPreset {
    /// Stable id (`openrouter`); the UI's i18n keys are `ai.preset.<id>.*`.
    pub id: &'static str,
    /// Wire protocol.
    pub kind: ProviderKind,
    /// Display name — the vendor's own name, used as the new provider's name (`""` for the
    /// custom preset: the UI names it in the user's language).
    pub name: &'static str,
    /// Base URL a new provider starts with (`""` = the user must type it: custom endpoint).
    pub base_url: &'static str,
    /// Requests need an API key (hosted services). Local servers accept an optional one.
    pub needs_key: bool,
    /// Where the user creates a key (shown as a link), or `None`.
    pub key_url: Option<&'static str>,
    /// Where to get the software (local servers) or read the setup docs, or `None`.
    pub setup_url: Option<&'static str>,
    /// Runs on this computer: nothing leaves it.
    pub local: bool,
    /// Free to use: local, or a hosted service with a free tier / free models.
    pub free: bool,
    /// Model a new provider starts with (`""` = pick one from the fetched list).
    pub default_model: &'static str,
    /// Model ids offered before the list is fetched.
    pub suggested_models: &'static [&'static str],
}

/// Id of the custom OpenAI-compatible preset (also what an unrecognised compatible address
/// is detected as).
pub const CUSTOM: &str = "custom";

/// Every preset, in gallery order: hosted services (Claude first), then local servers, then
/// the custom endpoint.
pub const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "anthropic",
        kind: ProviderKind::Anthropic,
        name: "Claude",
        // https://docs.claude.com/en/api/overview
        base_url: "https://api.anthropic.com",
        needs_key: true,
        // The Claude Console moved from console.anthropic.com (which redirects) to
        // platform.claude.com.
        key_url: Some("https://platform.claude.com/settings/keys"),
        setup_url: None,
        local: false,
        free: false,
        default_model: "claude-sonnet-5-5",
        suggested_models: &["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"],
    },
    ProviderPreset {
        id: "openai",
        kind: ProviderKind::Openai,
        name: "OpenAI",
        // https://platform.openai.com/docs/api-reference
        base_url: "https://api.openai.com/v1",
        needs_key: true,
        key_url: Some("https://platform.openai.com/api-keys"),
        setup_url: None,
        local: false,
        free: false,
        default_model: "",
        suggested_models: &[],
    },
    ProviderPreset {
        id: "openrouter",
        kind: ProviderKind::OpenaiCompatible,
        name: "OpenRouter",
        // https://openrouter.ai/docs/api-reference/overview — OpenAI-compatible, `GET /models`.
        base_url: "https://openrouter.ai/api/v1",
        needs_key: true,
        key_url: Some("https://openrouter.ai/keys"),
        setup_url: None,
        local: false,
        // Model ids ending in `:free` cost nothing (rate-limited).
        free: true,
        // `openrouter/auto` picks a (paid) model per request; `openrouter/free` picks one of
        // the free models (docs: /docs/guides/routing/routers/free-router).
        default_model: "openrouter/auto",
        suggested_models: &["openrouter/auto", "openrouter/free"],
    },
    ProviderPreset {
        id: "gemini",
        kind: ProviderKind::OpenaiCompatible,
        name: "Google Gemini",
        // Gemini's OpenAI compatibility layer (docs moved from ai.google.dev to
        // aistudio.google.com/docs/openai) — Bearer key, `GET /models` is supported.
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        needs_key: true,
        key_url: Some("https://aistudio.google.com/apikey"),
        setup_url: None,
        local: false,
        // Google AI Studio keys have a free tier (rate-limited).
        free: true,
        default_model: "",
        suggested_models: &[],
    },
    ProviderPreset {
        id: "groq",
        kind: ProviderKind::OpenaiCompatible,
        name: "Groq",
        // https://console.groq.com/docs/openai
        base_url: "https://api.groq.com/openai/v1",
        needs_key: true,
        key_url: Some("https://console.groq.com/keys"),
        setup_url: None,
        local: false,
        free: true,
        default_model: "",
        suggested_models: &[],
    },
    ProviderPreset {
        id: "mistral",
        kind: ProviderKind::OpenaiCompatible,
        name: "Mistral",
        // https://docs.mistral.ai/api/ — `/v1/chat/completions`, `/v1/models`, Bearer key.
        base_url: "https://api.mistral.ai/v1",
        needs_key: true,
        key_url: Some("https://console.mistral.ai/api-keys"),
        setup_url: None,
        local: false,
        // The free plan has no API access (docs: usage limits).
        free: false,
        default_model: "mistral-small-latest",
        suggested_models: &["mistral-small-latest", "mistral-large-latest"],
    },
    ProviderPreset {
        id: "deepseek",
        kind: ProviderKind::OpenaiCompatible,
        name: "DeepSeek",
        // https://api-docs.deepseek.com/ — OpenAI-compatible at the root, Bearer key.
        base_url: "https://api.deepseek.com",
        needs_key: true,
        key_url: Some("https://platform.deepseek.com/api_keys"),
        setup_url: None,
        local: false,
        free: false,
        // Model ids changed in 2026 (UNVERIFIED which are current): pick from the fetched list.
        default_model: "",
        suggested_models: &[],
    },
    ProviderPreset {
        id: "ollama",
        kind: ProviderKind::Ollama,
        name: "Ollama",
        // https://docs.ollama.com/api — `/api/chat`, `/api/tags`, no auth.
        base_url: "http://localhost:11434",
        needs_key: false,
        key_url: None,
        setup_url: Some("https://ollama.com/download"),
        local: true,
        free: true,
        default_model: "",
        suggested_models: &[],
    },
    ProviderPreset {
        id: "lmstudio",
        kind: ProviderKind::OpenaiCompatible,
        name: "LM Studio",
        // https://lmstudio.ai/docs/developer/openai-compat — local server on port 1234 (start
        // it in the Developer tab); auth is off unless turned on in the app (Bearer token).
        base_url: "http://localhost:1234/v1",
        needs_key: false,
        key_url: None,
        setup_url: Some("https://lmstudio.ai/download"),
        local: true,
        free: true,
        default_model: "",
        suggested_models: &[],
    },
    ProviderPreset {
        id: CUSTOM,
        kind: ProviderKind::OpenaiCompatible,
        name: "",
        base_url: "",
        // Decided per address: see [`needs_key`].
        needs_key: false,
        key_url: None,
        setup_url: None,
        local: false,
        free: false,
        default_model: "",
        suggested_models: &[],
    },
];

/// The preset with this id.
pub fn preset(id: &str) -> Option<&'static ProviderPreset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// The preset of a non-compatible kind (one preset per native protocol).
fn native(kind: ProviderKind) -> Option<&'static ProviderPreset> {
    match kind {
        ProviderKind::Anthropic => preset("anthropic"),
        ProviderKind::Openai => preset("openai"),
        ProviderKind::Ollama => preset("ollama"),
        ProviderKind::OpenaiCompatible => None,
    }
}

/// `host[:port]` of an http(s) URL, lowercased (`None` if it is not one).
fn authority(url: &str) -> Option<String> {
    let rest = url
        .trim()
        .strip_prefix("https://")
        .or_else(|| url.trim().strip_prefix("http://"))?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let auth = &rest[..end];
    // Drop credentials (`user:pass@host`).
    let auth = auth.rsplit('@').next().unwrap_or(auth);
    (!auth.is_empty()).then(|| auth.to_ascii_lowercase())
}

/// Host of an http(s) URL, without port or IPv6 brackets.
pub fn host(url: &str) -> Option<String> {
    let auth = authority(url)?;
    if let Some(v6) = auth.strip_prefix('[') {
        return v6.split(']').next().map(str::to_owned);
    }
    Some(auth.split(':').next().unwrap_or_default().to_owned())
}

/// Port of an http(s) URL (the scheme's default when none is written).
fn port(url: &str) -> Option<u16> {
    let auth = authority(url)?;
    let after_host = match auth.strip_prefix('[') {
        Some(v6) => v6
            .split_once(']')
            .map(|(_, r)| r.to_owned())
            .unwrap_or_default(),
        None => auth
            .split_once(':')
            .map(|(_, p)| format!(":{p}"))
            .unwrap_or_default(),
    };
    match after_host.strip_prefix(':') {
        Some(p) => p.parse().ok(),
        None if url.trim().starts_with("https://") => Some(443),
        None => Some(80),
    }
}

/// The address is on this computer (`localhost`, `127.x`, `::1`).
pub fn is_loopback(url: &str) -> bool {
    match host(url) {
        Some(h) => {
            h == "localhost" || h.ends_with(".localhost") || h.starts_with("127.") || h == "::1"
        }
        None => false,
    }
}

/// The address is on this computer or the local network (private IPv4 ranges, `*.local`,
/// `*.lan`, `*.home.arpa`, IPv6 unique-local/link-local): a server the user runs themselves,
/// where a key is optional.
pub fn is_private(url: &str) -> bool {
    if is_loopback(url) {
        return true;
    }
    let Some(h) = host(url) else { return false };
    if let Ok(std::net::IpAddr::V4(ip)) = h.parse::<std::net::IpAddr>() {
        return ip.is_private() || ip.is_link_local();
    }
    if let Ok(std::net::IpAddr::V6(ip)) = h.parse::<std::net::IpAddr>() {
        let first = ip.segments()[0];
        return (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80;
    }
    h.ends_with(".local") || h.ends_with(".lan") || h.ends_with(".home.arpa") || !h.contains('.')
}

/// The preset a provider of `kind` at `base_url` is (custom if the address is unknown).
pub fn detect(kind: ProviderKind, base_url: &str) -> &'static ProviderPreset {
    if let Some(p) = native(kind) {
        return p;
    }
    let found = host(base_url).and_then(|h| {
        PRESETS.iter().find(|p| {
            if p.kind != ProviderKind::OpenaiCompatible || p.id == CUSTOM {
                return false;
            }
            if p.local {
                // Local servers are told apart by their port on this computer.
                is_loopback(base_url) && port(base_url) == port(p.base_url)
            } else {
                host(p.base_url).as_deref() == Some(h.as_str())
            }
        })
    });
    found
        .or_else(|| preset(CUSTOM))
        .unwrap_or(&PRESETS[PRESETS.len() - 1])
}

/// Whether requests to a provider of `kind` at `base_url` need an API key: the preset's rule;
/// for a custom endpoint, hosted addresses need one and the user's own servers (this computer,
/// the local network) do not.
pub fn needs_key(kind: ProviderKind, base_url: &str) -> bool {
    let p = detect(kind, base_url);
    if p.id == CUSTOM {
        !is_private(base_url)
    } else {
        p.needs_key
    }
}
