//! `tts` module (v0.7): text-to-speech through the registered `TtsProvider`s — built-in ones
//! and plugin-provided ones alike (`Registry::register_tts`). The provider is chosen per call
//! or by the `tts.provider` setting.

use crate::error::{AppError, AppResult};
use crate::handler::QueryCtx;
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use base64::Engine as _;
use fc_api::v1::common::Empty;
use fc_api::v1::methods::*;
use fc_api::v1::tts::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_providers::speech::{SpeechRequest, TtsProvider};
use fc_providers::{Capability, ProviderError, ProviderErrorKind};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

/// Module id.
pub const ID: &str = "tts";

/// Setting: default provider id (empty = the first registered).
pub const PROVIDER: &str = "tts.provider";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.tts.name",
        description_key: "modules.tts.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![SettingDef {
            key: PROVIDER.to_owned(),
            kind: SettingKind::Text { max_len: Some(200) },
            default: serde_json::json!(""),
            label_key: format!("settings.{PROVIDER}.label"),
            description_key: Some(format!("settings.{PROVIDER}.description")),
            advanced: false,
            scopes: vec![SettingScopeKind::App],
        }],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<TtsProviders>(providers);
    reg.query::<TtsVoices>(voices);
    reg.query::<TtsSynthesize>(synthesize);
}

/// Runs a future to completion on this thread (providers are `async` for I/O-bound
/// implementations; the plugin ones finish synchronously).
pub(crate) fn block_on<F: Future>(fut: F) -> F::Output {
    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::park();
    }
}

fn provider_error(provider: &str, e: ProviderError) -> AppError {
    let code = match e.kind {
        ProviderErrorKind::Unavailable => ErrorCode::ProviderUnavailable,
        _ => ErrorCode::ProviderFailed,
    };
    AppError::Api(
        ApiError::new(code, e.message.clone())
            .with_detail("provider", provider)
            .with_detail("kind", format!("{:?}", e.kind)),
    )
}

/// Primary language subtag, lower case (`en-US` → `en`).
fn primary(tag: &str) -> String {
    tag.trim()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// After a failed synthesis: when the provider lists voices and none speaks `language`, the
/// plain "no voice for this language — install one" error (`errors.provider.no_voice`, v1.4.2
/// J4) instead of the provider's own message.
fn no_voice_error(p: &dyn TtsProvider, id: &str, language: &str) -> Option<AppError> {
    let lang = primary(language);
    if matches!(lang.as_str(), "" | "und" | "mul" | "zxx") {
        return None;
    }
    let voices = block_on(p.voices()).ok()?;
    // A provider that lists no voices at all says nothing about languages (as in the audio
    // fill); only the system provider's empty list means "this machine has no voice".
    if voices.is_empty() && id != fc_system_tts::PROVIDER_ID {
        return None;
    }
    if voices.iter().any(|v| primary(&v.language) == lang) {
        return None;
    }
    let mut e = ApiError::new(
        ErrorCode::ProviderUnavailable,
        format!("no installed voice speaks '{language}'"),
    )
    .with_detail("provider", id)
    .with_detail("kind", "no_voice")
    .with_detail("language", language);
    e.message_key = NO_VOICE_KEY.to_owned();
    Some(AppError::Api(e))
}

/// i18n key of the "no voice for this language" sentence.
pub const NO_VOICE_KEY: &str = "errors.provider.no_voice";

fn pick(ctx: &QueryCtx<'_>, requested: Option<String>) -> AppResult<Arc<dyn TtsProvider>> {
    let wanted = match requested {
        Some(p) => Some(p),
        None => ctx
            .tx
            .get_setting(&SettingScope::App, PROVIDER)?
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|s| !s.is_empty()),
    };
    let found = match &wanted {
        Some(id) => ctx.registry.tts_provider(id),
        None => ctx.registry.tts_providers().into_iter().next(),
    };
    found.ok_or_else(|| {
        AppError::Api(
            ApiError::new(ErrorCode::ProviderUnavailable, "no TTS provider")
                .with_detail("provider", wanted.unwrap_or_default()),
        )
    })
}

fn providers(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<TtsProviderList> {
    Ok(provider_list(ctx))
}

/// The registered providers (`tts.providers`).
pub(crate) fn provider_list(ctx: &QueryCtx<'_>) -> TtsProviderList {
    TtsProviderList {
        providers: ctx
            .registry
            .tts_providers()
            .into_iter()
            .map(|p| {
                let info = p.info();
                TtsProviderDto {
                    languages: info
                        .capabilities
                        .iter()
                        .filter_map(|c| match c {
                            Capability::Language(l) => Some(l.clone()),
                            _ => None,
                        })
                        .collect(),
                    id: info.id,
                    name_key: info.name_key,
                    local: info.local,
                }
            })
            .collect(),
    }
}

fn voices(req: ListVoices, ctx: &QueryCtx<'_>) -> AppResult<VoiceList> {
    voice_list(ctx, req.provider)
}

/// Voices of a provider (`tts.voices`).
pub(crate) fn voice_list(ctx: &QueryCtx<'_>, provider: Option<String>) -> AppResult<VoiceList> {
    let p = pick(ctx, provider)?;
    let id = p.info().id;
    let voices = block_on(p.voices()).map_err(|e| provider_error(&id, e))?;
    Ok(VoiceList {
        voices: voices
            .into_iter()
            .map(|v| VoiceDto {
                id: v.id,
                name: v.name,
                language: v.language,
            })
            .collect(),
        provider: id,
    })
}

fn synthesize(req: Synthesize, ctx: &QueryCtx<'_>) -> AppResult<AudioDto> {
    let rate = req.rate.unwrap_or(1.0);
    if !(0.25..=4.0).contains(&rate) || req.text.chars().count() > 5000 {
        return Err(crate::support::invalid("rate must be 0.25–4 and text at most 5000 characters"));
    }
    let p = pick(ctx, req.provider)?;
    let id = p.info().id;
    let asked_voice = req.voice.is_some();
    let language = req.language.clone();
    let clip = block_on(p.synthesize(SpeechRequest {
        text: req.text,
        language: req.language,
        voice: req.voice,
        rate,
    }))
    .map_err(|e| {
        if asked_voice {
            return provider_error(&id, e);
        }
        no_voice_error(&*p, &id, &language).unwrap_or_else(|| provider_error(&id, e))
    })?;
    Ok(AudioDto {
        provider: id,
        mime: clip.mime,
        data_b64: base64::engine::general_purpose::STANDARD.encode(clip.bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_drives_a_pending_future() {
        struct Twice(bool);
        impl Future for Twice {
            type Output = u8;
            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u8> {
                if self.0 {
                    Poll::Ready(7)
                } else {
                    self.0 = true;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
        }
        assert_eq!(block_on(Twice(false)), 7);
    }
}
