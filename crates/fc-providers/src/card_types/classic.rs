//! `classic` (self-graded) and `typing` (typed answer) engines: one card per template, unless
//! the question renders empty (optional reverse).

use super::{info, render_sides, tts_of};
use crate::provider::ProviderInfo;
use crate::strategy::{
    CardInput, CardTypeEngine, Interaction, RenderContext, RenderedCard, StrategyError,
    StrategyResult,
};
use fc_core::card::CardSpec;
use fc_core::content::builtin::engine;
use fc_core::content::render::has_visible_content;
use fc_core::note::Note;
use fc_core::notetype::{CardTemplate, NoteType};

fn generate_if_visible(
    note: &Note,
    notetype: &NoteType,
    template: &CardTemplate,
) -> StrategyResult<Vec<CardSpec>> {
    let input = CardInput {
        note,
        notetype,
        template,
        template_key: &template.key,
    };
    let (q, _) = render_sides(&input, &RenderContext::default(), None)?;
    Ok(if has_visible_content(&q.html) {
        vec![CardSpec {
            template_key: template.key.clone(),
            card_type: template.card_type.clone(),
        }]
    } else {
        Vec::new()
    })
}

/// Front/back, self-graded.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassicEngine;

impl CardTypeEngine for ClassicEngine {
    fn info(&self) -> ProviderInfo {
        info(engine::CLASSIC)
    }

    fn generate_cards(
        &self,
        note: &Note,
        notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>> {
        generate_if_visible(note, notetype, template)
    }

    fn render(&self, input: &CardInput<'_>, ctx: &RenderContext) -> StrategyResult<RenderedCard> {
        let (q, a) = render_sides(input, ctx, None)?;
        Ok(RenderedCard {
            tts: tts_of(&q, &a),
            question: q.html,
            answer: a.html,
            css: input.notetype.css.clone(),
            interaction: Interaction::SelfGrade,
        })
    }
}

/// Typed answer: the template must contain `{{type:field}}`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypingEngine;

impl CardTypeEngine for TypingEngine {
    fn info(&self) -> ProviderInfo {
        info(engine::TYPING)
    }

    fn validate(&self, _notetype: &NoteType, template: &CardTemplate) -> StrategyResult<()> {
        if template.front.contains("{{type:") {
            Ok(())
        } else {
            Err(StrategyError(format!(
                "template {}: typing cards need {{{{type:field}}}} on the front",
                template.key
            )))
        }
    }

    fn generate_cards(
        &self,
        note: &Note,
        notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>> {
        generate_if_visible(note, notetype, template)
    }

    fn render(&self, input: &CardInput<'_>, ctx: &RenderContext) -> StrategyResult<RenderedCard> {
        let (q, a) = render_sides(input, ctx, None)?;
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
    use crate::strategy::{CardResponse, Evaluation};

    #[test]
    fn optional_reverse_generates_only_when_filled() {
        let nt = builtin_nt("optional_reverse");
        let without = note(&nt, &[("front", "dog"), ("back", "köpek")]);
        let with = note(
            &nt,
            &[("front", "dog"), ("back", "köpek"), ("add_reverse", "y")],
        );
        let count = |n: &Note| {
            nt.templates
                .iter()
                .map(|t| ClassicEngine.generate_cards(n, &nt, t).unwrap().len())
                .sum::<usize>()
        };
        assert_eq!(count(&without), 1);
        assert_eq!(count(&with), 2);
    }

    #[test]
    fn reverse_renders_back_as_question() {
        let nt = builtin_nt("classic_reverse");
        let n = note(&nt, &[("front", "dog"), ("back", "köpek")]);
        let input = CardInput {
            note: &n,
            notetype: &nt,
            template: &nt.templates[1],
            template_key: "reverse",
        };
        let r = ClassicEngine
            .render(&input, &RenderContext::default())
            .unwrap();
        assert_eq!(r.question, "köpek");
        assert!(r.answer.ends_with("dog"), "{}", r.answer);
        assert_eq!(r.interaction, Interaction::SelfGrade);
        assert_eq!(
            ClassicEngine.evaluate(&r, &CardResponse::Revealed),
            Evaluation::SelfGraded
        );
    }

    #[test]
    fn typing_exposes_the_expected_answer() {
        let nt = builtin_nt("typing");
        TypingEngine.validate(&nt, &nt.templates[0]).unwrap();
        let mut broken = nt.templates[0].clone();
        broken.front = "{{front}}".into();
        assert!(TypingEngine.validate(&nt, &broken).is_err());
        let n = note(&nt, &[("front", "dog"), ("back", "**köpek**")]);
        let input = CardInput {
            note: &n,
            notetype: &nt,
            template: &nt.templates[0],
            template_key: "forward",
        };
        let r = TypingEngine
            .render(&input, &RenderContext::default())
            .unwrap();
        assert_eq!(
            r.interaction,
            Interaction::Type {
                expected: "köpek".into()
            }
        );
        assert_eq!(
            TypingEngine.evaluate(&r, &CardResponse::Text("kopek".into())),
            Evaluation::CheckText {
                expected: "köpek".into(),
                given: "kopek".into()
            }
        );
    }
}
