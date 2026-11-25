//! `classic` (self-graded) and `typing` (typed answer) engines: one card per template, unless
//! the question renders empty (optional reverse).

use super::{info, render_question, render_sides, tts_of};
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
    let q = render_question(&input, &RenderContext::default(), None)?;
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

    /// v1.4.2 K2: the question rendered alone is the question `render_sides` renders, and a
    /// template that does not parse (either side) fails with the same error; so the cards a
    /// note yields are the same as when both sides were rendered.
    #[test]
    fn question_alone_equals_the_question_of_both_sides() {
        let values = [
            "",
            " ",
            "dog",
            "**bold** [[link]]",
            "<b>x</b>",
            "&nbsp;",
            "<!-- c -->",
            "<br>",
            "line\n\nline",
            "{{c1::cloze}}",
            "<img src=\"media:x\">",
            "y",
        ];
        let mut seen = 0;
        let mut empty = 0;
        for key in ["classic", "classic_reverse", "optional_reverse", "typing"] {
            let Some(def) = fc_core::content::builtin::find(key) else {
                continue;
            };
            let mut nt = def.build(fc_core::Timestamp(1), fc_core::i18n::Locale::En);
            // Extra shapes: sections, filters, specials, and sides that do not parse.
            let mut extra = nt.templates[0].clone();
            let first = nt.fields[0].key.to_string();
            let variants = [
                (
                    format!("{{{{#{first}}}}}Q: {{{{text:{first}}}}}{{{{/{first}}}}}"),
                    "{{FrontSide}}<hr>{{Tags}}".to_owned(),
                ),
                (
                    format!("{{{{^{first}}}}}{{{{Deck}}}}{{{{/{first}}}}}{{{{hint:{first}}}}}"),
                    "{{Card}}".to_owned(),
                ),
                (
                    format!("{{{{tts:en:{first}}}}}"),
                    format!("{{{{type:{first}}}}}"),
                ),
                (
                    format!("{{{{{first}}}}}"),
                    "{{#open}}never closed".to_owned(),
                ),
                ("{{".to_owned(), "fine".to_owned()),
                ("{{ }}".to_owned(), "{{/x}}".to_owned()),
            ];
            for (front, back) in variants {
                extra.front = front;
                extra.back = back;
                nt.templates.push(extra.clone());
            }
            for a in values {
                for b in values {
                    let mut pairs: Vec<(&str, &str)> = Vec::new();
                    for (i, f) in nt.fields.iter().enumerate() {
                        pairs.push((f.key.as_str(), if i % 2 == 0 { a } else { b }));
                    }
                    let mut n = note(&nt, &pairs);
                    if a.len() % 2 == 1 {
                        n.tags = vec![fc_core::tag::TagName::parse("verb").unwrap()];
                    }
                    for t in &nt.templates {
                        let input = CardInput {
                            note: &n,
                            notetype: &nt,
                            template: t,
                            template_key: &t.key,
                        };
                        let ctx = RenderContext::default();
                        let both = render_sides(&input, &ctx, None).map(|(q, _)| q);
                        let alone = render_question(&input, &ctx, None);
                        assert_eq!(
                            alone, both,
                            "{key} {:?} / {:?} with {a:?} {b:?}",
                            t.front, t.back
                        );
                        seen += 1;
                        if alone.is_ok_and(|q| !has_visible_content(&q.html)) {
                            empty += 1;
                        }
                    }
                }
            }
        }
        assert!(
            seen > 1_000 && empty > 50,
            "{seen} renders, {empty} empty questions"
        );
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
