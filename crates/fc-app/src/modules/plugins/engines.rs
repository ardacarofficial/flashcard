//! Plugin contributions registered in the [`Registry`] at runtime: card type engines
//! (`CardTypeEngine`), TTS providers (`TtsProvider`) and commands (dynamic methods
//! `plugin.<id>.<command>`). They implement the very traits built-ins implement — plugins are
//! not second-class (docs/05 §1).

use super::host::{plugin_failed, Core, LoadedPlugin, Mode};
use crate::error::AppResult;
use crate::registry::{DynamicHandler, DynamicMethod, Registry};
use crate::App;
use async_trait::async_trait;
use base64::Engine as _;
use fc_api::v1::oplog::Actor;
use fc_api::v1::plugins::PluginCommandResult;
use fc_core::card::CardSpec;
use fc_core::note::Note;
use fc_core::notetype::{CardTemplate, NoteType};
use fc_plugins::abi::ops;
use fc_providers::speech::{AudioClip, SpeechRequest, TtsProvider, Voice};
use fc_providers::strategy::{
    CardInput, CardResponse, CardTypeEngine, Evaluation, Interaction, RenderContext,
    RenderedCard, StrategyError, StrategyResult,
};
use fc_providers::{Capability, ProviderError, ProviderErrorKind, ProviderInfo, ProviderResult};
use serde_json::{json, Value};
use std::sync::Arc;

/// Registers what an active plugin contributes (its permissions decide).
pub(crate) fn register(registry: &Registry, core: &Arc<Core>, p: &Arc<LoadedPlugin>) {
    let id = p.id();
    if p.has("card_types:provide") {
        for ct in &p.manifest.contributes.card_types {
            registry.register_card_type(Arc::new(PluginCardType {
                id: format!("plugin.{id}.{ct}"),
                local: ct.clone(),
                plugin: p.clone(),
                core: core.clone(),
            }));
        }
    }
    if p.has("tts:provide") {
        for t in &p.manifest.contributes.tts {
            registry.register_tts(Arc::new(PluginTts {
                id: format!("plugin.{id}.{t}"),
                local: t.clone(),
                plugin: p.clone(),
                core: core.clone(),
            }));
        }
    }
    if p.has("ui:command") {
        for c in &p.manifest.contributes.commands {
            registry.register_dynamic(
                DynamicMethod {
                    name: format!("plugin.{id}.{c}").into(),
                    module: super::ID.into(),
                    permission: "plugins:write".into(),
                    label_key: format!("plugin.{id}.commands.{c}").into(),
                },
                Arc::new(PluginCommand {
                    plugin_id: id.to_owned(),
                    command: c.clone(),
                }),
            );
        }
    }
}

struct PluginCardType {
    id: String,
    local: String,
    plugin: Arc<LoadedPlugin>,
    core: Arc<Core>,
}

impl PluginCardType {
    fn call<T: serde::de::DeserializeOwned>(&self, op: &str, input: Value) -> StrategyResult<T> {
        let v = self
            .core
            .call_pure(&self.plugin, op, input)
            .map_err(|e| StrategyError(format!("plugin {}: {e}", self.plugin.id())))?;
        serde_json::from_value(v)
            .map_err(|e| StrategyError(format!("plugin {} returned invalid {op}: {e}", self.plugin.id())))
    }

    fn input(&self, note: &Note, notetype: &NoteType, template: &CardTemplate) -> Value {
        json!({
            "card_type": self.local,
            "note": note,
            "notetype": notetype,
            "template": template,
        })
    }
}

impl CardTypeEngine for PluginCardType {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.clone(),
            name_key: format!("plugin.{}.card_types.{}", self.plugin.id(), self.local),
            capabilities: vec![Capability::Other("plugin".into())],
            settings: vec![],
            secret_keys: vec![],
            local: true,
        }
    }

    fn generate_cards(
        &self,
        note: &Note,
        notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>> {
        let specs: Vec<CardSpec> =
            self.call(ops::CARD_TYPE_GENERATE, self.input(note, notetype, template))?;
        // The card type is always this engine, whatever the plugin wrote.
        Ok(specs
            .into_iter()
            .map(|s| CardSpec {
                template_key: s.template_key,
                card_type: self.id.clone(),
            })
            .collect())
    }

    fn render(&self, input: &CardInput<'_>, ctx: &RenderContext) -> StrategyResult<RenderedCard> {
        let mut v = self.input(input.note, input.notetype, input.template);
        v["template_key"] = json!(input.template_key);
        v["ctx"] = serde_json::to_value(ctx).map_err(|e| StrategyError(e.to_string()))?;
        self.call(ops::CARD_TYPE_RENDER, v)
    }

    fn evaluate(&self, rendered: &RenderedCard, response: &CardResponse) -> Evaluation {
        let input = json!({ "card_type": self.local, "rendered": rendered, "response": response });
        if let Ok(Some(e)) = self.call::<Option<Evaluation>>(ops::CARD_TYPE_EVALUATE, input) {
            return e;
        }
        match (&rendered.interaction, response) {
            (Interaction::Choice { correct, .. }, CardResponse::Choice(i)) => Evaluation::Decided {
                correct: i == correct,
            },
            (Interaction::Type { expected }, CardResponse::Text(given)) => Evaluation::CheckText {
                expected: expected.clone(),
                given: given.clone(),
            },
            _ => Evaluation::SelfGraded,
        }
    }
}

struct PluginTts {
    id: String,
    local: String,
    plugin: Arc<LoadedPlugin>,
    core: Arc<Core>,
}

impl PluginTts {
    fn call(&self, op: &str, input: Value) -> ProviderResult<Value> {
        self.core.call_pure(&self.plugin, op, input).map_err(|e| {
            let kind = if e.kind.is_fault() {
                ProviderErrorKind::Failed
            } else {
                ProviderErrorKind::InvalidRequest
            };
            ProviderError::new(kind, format!("plugin {}: {e}", self.plugin.id()))
        })
    }
}

#[async_trait]
impl TtsProvider for PluginTts {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.clone(),
            name_key: format!("plugin.{}.tts.{}", self.plugin.id(), self.local),
            capabilities: vec![Capability::Other("plugin".into())],
            settings: vec![],
            secret_keys: vec![],
            local: true,
        }
    }

    async fn voices(&self) -> ProviderResult<Vec<Voice>> {
        let v = self.call(ops::TTS_VOICES, json!({ "provider": self.local }))?;
        serde_json::from_value(v)
            .map_err(|e| ProviderError::new(ProviderErrorKind::Failed, e.to_string()))
    }

    async fn synthesize(&self, request: SpeechRequest) -> ProviderResult<AudioClip> {
        #[derive(serde::Deserialize)]
        struct Out {
            mime: String,
            data_b64: String,
        }
        let v = self.call(
            ops::TTS_SYNTHESIZE,
            json!({ "provider": self.local, "request": request }),
        )?;
        let out: Out = serde_json::from_value(v)
            .map_err(|e| ProviderError::new(ProviderErrorKind::Failed, e.to_string()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(out.data_b64)
            .map_err(|e| ProviderError::new(ProviderErrorKind::Failed, e.to_string()))?;
        Ok(AudioClip {
            mime: out.mime,
            bytes,
        })
    }
}

struct PluginCommand {
    plugin_id: String,
    command: String,
}

impl DynamicHandler for PluginCommand {
    fn invoke(&self, params: Value, app: &App, _actor: &Actor) -> AppResult<Value> {
        let r = run_command(app, &self.plugin_id, &self.command, params)?;
        serde_json::to_value(r)
            .map_err(|e| crate::AppError::api(fc_api::ErrorCode::Internal, e.to_string()))
    }
}

/// Runs a contributed command (`plugins.run` and `v1.plugin.<id>.<command>`).
pub(crate) fn run_command(
    app: &App,
    plugin_id: &str,
    command: &str,
    params: Value,
) -> AppResult<PluginCommandResult> {
    let host = app.plugins();
    let plugin = super::require_active(host, plugin_id)?;
    if !plugin.manifest.contributes.commands.iter().any(|c| c == command) {
        return Err(super::no_contribution(plugin_id, command));
    }
    if !plugin.has("ui:command") {
        return Err(super::inactive(plugin_id));
    }
    let input = json!({ "command": command, "params": params });
    let (r, notices) = host.run_op(app, &plugin, ops::COMMAND, input, Mode::Operation);
    let result = r.map_err(|e| plugin_failed(plugin_id, &e))?;
    Ok(PluginCommandResult { result, notices })
}
