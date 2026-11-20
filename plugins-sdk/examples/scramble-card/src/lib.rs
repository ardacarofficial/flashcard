//! Example: a card type engine (generate / render / evaluate) in a plugin. Uses the note's
//! `front` field as the word and `back` as a hint; letters are shuffled deterministically
//! with the card's seed, the answer is typed.

use fc_plugin_sdk::core::card::CardSpec;
use fc_plugin_sdk::prelude::*;
use fc_plugin_sdk::{Interaction, RenderContext, RenderedCard};

#[derive(Default)]
pub struct Scramble;

/// Deterministic Fisher–Yates with a xorshift seeded by the card.
pub fn scramble(word: &str, seed: u64) -> String {
    let mut chars: Vec<char> = word.chars().collect();
    let mut x = seed | 1;
    for i in (1..chars.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        chars.swap(i, (x % (i as u64 + 1)) as usize);
    }
    let out: String = chars.into_iter().collect();
    if out == word && word.chars().count() > 1 {
        // Never show the word itself: rotate by one.
        let mut c: Vec<char> = word.chars().collect();
        c.rotate_left(1);
        return c.into_iter().collect();
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Plugin for Scramble {
    fn generate_cards(&self, card_type: &str, input: CardTypeInput) -> Result<Vec<CardSpec>> {
        if input.field("front").trim().is_empty() {
            return Ok(vec![]);
        }
        Ok(vec![CardSpec {
            template_key: input.template.key.clone(),
            card_type: format!("plugin.com.example.scramble.{card_type}"),
        }])
    }

    fn render_card(
        &self,
        _card_type: &str,
        input: CardTypeInput,
        ctx: RenderContext,
    ) -> Result<RenderedCard> {
        let word = input.field("front").trim().to_owned();
        let hint = input.field("back").trim().to_owned();
        Ok(RenderedCard {
            question: format!(
                "<div class=\"scramble\">{}</div><div class=\"hint\">{}</div>",
                escape(&scramble(&word, ctx.seed)),
                escape(&hint)
            ),
            answer: format!("<div class=\"scramble\">{}</div>", escape(&word)),
            css: input.notetype.css.clone(),
            interaction: Interaction::Type { expected: word },
            tts: vec![],
        })
    }
}

export_plugin!(Scramble);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scramble_is_a_deterministic_permutation() {
        let s = scramble("kalkan", 42);
        assert_eq!(s, scramble("kalkan", 42));
        assert_ne!(s, "kalkan");
        let (mut a, mut b): (Vec<char>, Vec<char>) =
            (s.chars().collect(), "kalkan".chars().collect());
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }
}
