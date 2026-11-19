//! `cloze` engine: one card per cloze number in the fields used by `{{cloze:…}}`.

use super::{info, render_sides, tts_of};
use crate::provider::ProviderInfo;
use crate::strategy::{
    CardInput, CardTypeEngine, Interaction, RenderContext, RenderedCard, StrategyError,
    StrategyResult,
};
use fc_core::card::CardSpec;
use fc_core::content::builtin::engine;
use fc_core::content::cloze::{cloze_key, cloze_ordinals, parse_cloze_key};
use fc_core::content::template::parse;
use fc_core::note::Note;
use fc_core::notetype::{CardTemplate, FieldKey, NoteType};
use std::collections::BTreeSet;

/// Cloze deletion cards.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClozeEngine;

fn cloze_fields(template: &CardTemplate) -> StrategyResult<Vec<String>> {
    let tpl = parse(&template.front).map_err(|e| StrategyError(e.to_string()))?;
    Ok(tpl
        .field_refs()
        .into_iter()
        .filter(|r| r.filters.first().is_some_and(|f| f == "cloze"))
        .map(|r| r.name)
        .collect())
}

impl CardTypeEngine for ClozeEngine {
    fn info(&self) -> ProviderInfo {
        info(engine::CLOZE)
    }

    fn validate(&self, _notetype: &NoteType, template: &CardTemplate) -> StrategyResult<()> {
        if cloze_fields(template)?.is_empty() {
            return Err(StrategyError(format!(
                "template {}: cloze cards need {{{{cloze:field}}}} on the front",
                template.key
            )));
        }
        Ok(())
    }

    fn generate_cards(
        &self,
        note: &Note,
        _notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>> {
        let mut ordinals = BTreeSet::new();
        for name in cloze_fields(template)? {
            if let Some(v) = note.fields.get(&FieldKey::new(name)) {
                ordinals.extend(cloze_ordinals(v));
            }
        }
        Ok(ordinals
            .into_iter()
            .map(|n| CardSpec {
                template_key: cloze_key(n),
                card_type: template.card_type.clone(),
            })
            .collect())
    }

    fn render(&self, input: &CardInput<'_>, ctx: &RenderContext) -> StrategyResult<RenderedCard> {
        let ordinal = parse_cloze_key(input.template_key).ok_or_else(|| {
            StrategyError(format!("not a cloze card key: {}", input.template_key))
        })?;
        let (q, a) = render_sides(input, ctx, Some(ordinal))?;
        let interaction = match q.type_answer.clone() {
            Some(expected) => Interaction::Type { expected },
            None => Interaction::SelfGrade,
        };
        Ok(RenderedCard {
            tts: tts_of(&q, &a),
            question: q.html,
            answer: a.html,
            css: input.notetype.css.clone(),
            interaction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{builtin_nt, note};
    use super::*;

    #[test]
    fn one_card_per_number() {
        let nt = builtin_nt("cloze");
        let n = note(
            &nt,
            &[(
                "text",
                "{{c1::Ankara}} is the capital of {{c3::Türkiye}}; {{c1::x}}",
            )],
        );
        let specs = ClozeEngine
            .generate_cards(&n, &nt, &nt.templates[0])
            .unwrap();
        let keys: Vec<_> = specs.iter().map(|s| s.template_key.as_str()).collect();
        assert_eq!(keys, ["c1", "c3"]);
        assert!(specs.iter().all(|s| s.card_type == "cloze"));

        let input = CardInput {
            note: &n,
            notetype: &nt,
            template: &nt.templates[0],
            template_key: "c3",
        };
        let r = ClozeEngine
            .render(&input, &RenderContext::default())
            .unwrap();
        assert!(r.question.contains("<span class=\"cloze\">[...]</span>"));
        assert!(r.question.starts_with("Ankara"));
        assert!(r.answer.contains("<span class=\"cloze\">Türkiye</span>"));
        let bad = CardInput {
            template_key: "forward",
            ..input
        };
        assert!(ClozeEngine.render(&bad, &RenderContext::default()).is_err());
        ClozeEngine.validate(&nt, &nt.templates[0]).unwrap();
    }
}
