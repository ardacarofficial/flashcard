//! Built-in card type engines (docs/02 §4). Each is a plain [`CardTypeEngine`]; built-in note
//! types combine them with templates (reverse, optional reverse, image quiz and listening
//! cards are templates over `classic`/`typing`, not separate engines).

mod choice;
mod classic;
mod cloze;
mod occlusion;

pub use choice::{MultipleChoiceEngine, DISTRACTOR_POOL_CAPABILITY};
pub use classic::{ClassicEngine, TypingEngine};
pub use cloze::ClozeEngine;
pub use occlusion::{parse_masks, ImageOcclusionEngine, Mask, MaskMode, Masks};

use crate::provider::ProviderInfo;
use crate::strategy::{
    CardInput, CardTypeEngine, RenderContext, StrategyError, StrategyResult, TtsRequest,
};
use fc_core::content::render::{render_template, RenderEnv, SideOutput};
use fc_core::notetype::{FieldKey, FieldKind, NoteType};
use std::sync::Arc;

/// Every built-in engine.
pub fn builtin_engines() -> Vec<Arc<dyn CardTypeEngine>> {
    vec![
        Arc::new(ClassicEngine),
        Arc::new(TypingEngine),
        Arc::new(ClozeEngine),
        Arc::new(MultipleChoiceEngine),
        Arc::new(ImageOcclusionEngine),
    ]
}

/// Info of a built-in, offline engine.
pub(crate) fn info(id: &str) -> ProviderInfo {
    ProviderInfo {
        id: id.to_owned(),
        name_key: format!("cardtypes.engine.{id}"),
        capabilities: Vec::new(),
        settings: Vec::new(),
        secret_keys: Vec::new(),
        local: true,
    }
}

/// Renders both template sides of a card (answer side gets `{{FrontSide}}`).
pub(crate) fn render_sides(
    input: &CardInput<'_>,
    ctx: &RenderContext,
    cloze: Option<u32>,
) -> StrategyResult<(SideOutput, SideOutput)> {
    let links = |key: &str| ctx.links.get(key).map(ToString::to_string);
    let env = RenderEnv {
        notetype: input.notetype,
        fields: &input.note.fields,
        tags: &input.note.tags,
        deck: &ctx.deck_path,
        card_name: &input.template.name,
        cloze,
        reveal: false,
        front_side: None,
        links: &links,
    };
    let err = |e: fc_core::content::template::TemplateError| StrategyError(e.to_string());
    let q = render_template(&input.template.front, &env).map_err(err)?;
    let a = render_template(
        &input.template.back,
        &RenderEnv {
            reveal: true,
            front_side: Some(&q.html),
            ..env
        },
    )
    .map_err(err)?;
    Ok((q, a))
}

/// TTS requests of both sides, question first, without duplicates.
pub(crate) fn tts_of(q: &SideOutput, a: &SideOutput) -> Vec<TtsRequest> {
    let mut out: Vec<TtsRequest> = Vec::new();
    for t in q.tts.iter().chain(&a.tts) {
        let r = TtsRequest {
            lang: t.lang.clone(),
            text: t.text.clone(),
        };
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

/// Requires a field `key` of one of `kinds` in the note type.
pub(crate) fn require_field(nt: &NoteType, key: &str, kinds: &[FieldKind]) -> StrategyResult<()> {
    match nt.field(&FieldKey::new(key)) {
        Some(f) if kinds.is_empty() || kinds.contains(&f.kind) => Ok(()),
        _ => Err(StrategyError(format!(
            "note type needs a field '{key}' of kind {kinds:?}"
        ))),
    }
}

/// Raw value of field `key`.
pub(crate) fn field<'a>(input: &CardInput<'a>, key: &str) -> &'a str {
    input
        .note
        .fields
        .get(&FieldKey::new(key))
        .map_or("", String::as_str)
}

#[cfg(test)]
pub(crate) mod test_support {
    use fc_core::content::builtin;
    use fc_core::i18n::Locale;
    use fc_core::note::Note;
    use fc_core::notetype::{FieldKey, NoteType};
    use fc_core::{DeckId, NoteId, Timestamp};
    use uuid::Uuid;

    pub fn note(nt: &NoteType, pairs: &[(&str, &str)]) -> Note {
        let mut fields = std::collections::BTreeMap::new();
        for f in &nt.fields {
            fields.insert(f.key.clone(), String::new());
        }
        for (k, v) in pairs {
            fields.insert(FieldKey::new(*k), (*v).to_owned());
        }
        Note {
            id: NoteId(Uuid::from_u128(1)),
            notetype_id: nt.id,
            home_deck_id: DeckId(Uuid::from_u128(2)),
            fields,
            sort_field: String::new(),
            checksum: String::new(),
            tags: Vec::new(),
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
            deleted_at: None,
        }
    }

    pub fn builtin_nt(key: &str) -> NoteType {
        builtin::find(key).unwrap().build(Timestamp(1), Locale::En)
    }
}
