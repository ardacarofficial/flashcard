//! `multiple_choice` engine: the `answer` field is right; wrong options come from the
//! `distractors` field (one per line) or, when it is empty, from other notes' answers
//! ([`RenderContext::distractor_pool`]). Order is shuffled deterministically by
//! [`RenderContext::seed`].

use super::{field, info, render_sides, require_field, tts_of};
use crate::provider::{Capability, ProviderInfo};
use crate::strategy::{
    CardInput, CardTypeEngine, Interaction, RenderContext, RenderedCard, StrategyResult,
};
use fc_core::card::CardSpec;
use fc_core::content::builtin::engine;
use fc_core::content::html::escape_html;
use fc_core::note::{strip_html, Note};
use fc_core::notetype::{CardTemplate, NoteType};
use fc_core::rng::{Rng, SplitMix64};

/// Capability asking the app for [`RenderContext::distractor_pool`].
pub const DISTRACTOR_POOL_CAPABILITY: &str = "distractor_pool";

/// Number of wrong options shown.
pub const DISTRACTORS: usize = 3;

/// Multiple choice cards.
#[derive(Debug, Clone, Copy, Default)]
pub struct MultipleChoiceEngine;

fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    for i in (1..items.len()).rev() {
        let j = usize::try_from(rng.next_u64() % (i as u64 + 1)).unwrap_or(0);
        items.swap(i, j);
    }
}

impl CardTypeEngine for MultipleChoiceEngine {
    fn info(&self) -> ProviderInfo {
        let mut i = info(engine::MULTIPLE_CHOICE);
        i.capabilities
            .push(Capability::Other(DISTRACTOR_POOL_CAPABILITY.to_owned()));
        i
    }

    fn validate(&self, notetype: &NoteType, _template: &CardTemplate) -> StrategyResult<()> {
        require_field(notetype, "answer", &[])?;
        require_field(notetype, "distractors", &[])
    }

    fn generate_cards(
        &self,
        _note: &Note,
        _notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>> {
        Ok(vec![CardSpec {
            template_key: template.key.clone(),
            card_type: template.card_type.clone(),
        }])
    }

    fn render(&self, input: &CardInput<'_>, ctx: &RenderContext) -> StrategyResult<RenderedCard> {
        let (q, a) = render_sides(input, ctx, None)?;
        let answer = strip_html(field(input, "answer"));
        let same = |s: &str| s.to_lowercase() == answer.to_lowercase();
        let mut rng = SplitMix64::new(ctx.seed);
        let own: Vec<String> = field(input, "distractors")
            .lines()
            .map(strip_html)
            .filter(|s| !s.is_empty() && !same(s))
            .collect();
        let mut wrong: Vec<String> = Vec::new();
        let mut pool = if own.is_empty() {
            ctx.distractor_pool
                .iter()
                .map(|s| strip_html(s))
                .filter(|s| !s.is_empty() && !same(s))
                .collect()
        } else {
            own
        };
        shuffle(&mut pool, &mut rng);
        for d in pool {
            if wrong.len() == DISTRACTORS {
                break;
            }
            if !wrong.iter().any(|w| w.to_lowercase() == d.to_lowercase()) {
                wrong.push(d);
            }
        }
        let mut options: Vec<(bool, String)> = std::iter::once((true, answer.clone()))
            .chain(wrong.into_iter().map(|w| (false, w)))
            .collect();
        shuffle(&mut options, &mut rng);
        let correct = options.iter().position(|(ok, _)| *ok).unwrap_or(0);
        Ok(RenderedCard {
            tts: tts_of(&q, &a),
            question: q.html,
            answer: a.html,
            css: input.notetype.css.clone(),
            interaction: Interaction::Choice {
                options: options.into_iter().map(|(_, o)| escape_html(&o)).collect(),
                correct,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{builtin_nt, note};
    use super::*;
    use crate::strategy::{CardResponse, Evaluation};

    fn render(n: &Note, nt: &NoteType, ctx: &RenderContext) -> RenderedCard {
        let input = CardInput {
            note: n,
            notetype: nt,
            template: &nt.templates[0],
            template_key: "choice",
        };
        MultipleChoiceEngine.render(&input, ctx).unwrap()
    }

    #[test]
    fn manual_distractors_and_evaluation() {
        let nt = builtin_nt("multiple_choice");
        MultipleChoiceEngine
            .validate(&nt, &nt.templates[0])
            .unwrap();
        let n = note(
            &nt,
            &[
                ("question", "Capital of Türkiye?"),
                ("answer", "Ankara"),
                ("distractors", "İstanbul\nİzmir\n\nankara\nBursa\nAntalya"),
            ],
        );
        let ctx = RenderContext {
            seed: 7,
            ..RenderContext::default()
        };
        let r = render(&n, &nt, &ctx);
        let Interaction::Choice { options, correct } = &r.interaction else {
            panic!("not a choice card")
        };
        assert_eq!(options.len(), 4);
        assert_eq!(options[*correct], "Ankara");
        assert_eq!(options.iter().filter(|o| o.as_str() == "Ankara").count(), 1);
        // Deterministic for a seed.
        assert_eq!(render(&n, &nt, &ctx), r);
        assert_eq!(
            MultipleChoiceEngine.evaluate(&r, &CardResponse::Choice(*correct)),
            Evaluation::Decided { correct: true }
        );
        assert_eq!(
            MultipleChoiceEngine.evaluate(&r, &CardResponse::Choice((*correct + 1) % 4)),
            Evaluation::Decided { correct: false }
        );
    }

    #[test]
    fn automatic_distractors_from_the_pool() {
        let nt = builtin_nt("multiple_choice");
        let n = note(&nt, &[("question", "Q"), ("answer", "<b>cat</b>")]);
        let ctx = RenderContext {
            distractor_pool: vec!["dog".into(), "Cat".into(), "bird".into(), "dog".into()],
            seed: 1,
            ..RenderContext::default()
        };
        let r = render(&n, &nt, &ctx);
        let Interaction::Choice { options, correct } = &r.interaction else {
            panic!("not a choice card")
        };
        let mut sorted = options.clone();
        sorted.sort();
        assert_eq!(sorted, ["bird", "cat", "dog"]);
        assert_eq!(options[*correct], "cat");
    }
}
