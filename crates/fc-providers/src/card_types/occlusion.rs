//! `image_occlusion` engine: masks drawn over an image, one card per mask (docs/02 §4).
//!
//! The `masks` field holds JSON written by the mask editor:
//! `{"mode": "hide_all" | "hide_one", "masks": [{"n": 1, "x": 0.1, "y": 0.2, "w": 0.3, "h": 0.1}]}`
//! with coordinates as fractions of the image size. `n` is the mask's stable number (card key
//! `m<n>`); deleting a mask never renumbers the others. `hide_all`: every mask covers its
//! region and the asked one is highlighted; `hide_one`: only the asked mask is shown.

use super::{field, info, render_sides, require_field, tts_of};
use crate::provider::ProviderInfo;
use crate::strategy::{
    CardInput, CardTypeEngine, Interaction, RenderContext, RenderedCard, StrategyError,
    StrategyResult,
};
use fc_core::card::CardSpec;
use fc_core::content::builtin::engine;
use fc_core::content::refs::extract_media;
use fc_core::note::Note;
use fc_core::notetype::{CardTemplate, FieldKey, FieldKind, NoteType};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Which masks the question shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaskMode {
    /// Hide every mask, ask one.
    #[default]
    HideAll,
    /// Hide only the asked mask.
    HideOne,
}

/// One mask rectangle (fractions of the image).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    /// Stable number (card key `m<n>`); `0`/missing = position in the list + 1.
    #[serde(default)]
    pub n: u32,
    /// Left.
    pub x: f64,
    /// Top.
    pub y: f64,
    /// Width.
    pub w: f64,
    /// Height.
    pub h: f64,
}

/// The `masks` field.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Masks {
    /// Mode.
    #[serde(default)]
    pub mode: MaskMode,
    /// Masks.
    #[serde(default)]
    pub masks: Vec<Mask>,
}

/// Parses the `masks` field (empty → no masks). Numbers are filled in for masks without one.
pub fn parse_masks(raw: &str) -> StrategyResult<Masks> {
    if raw.trim().is_empty() {
        return Ok(Masks::default());
    }
    let mut m: Masks =
        serde_json::from_str(raw).map_err(|e| StrategyError(format!("masks: {e}")))?;
    let mut next = m.masks.iter().map(|x| x.n).max().unwrap_or(0);
    for mask in &mut m.masks {
        if mask.n == 0 {
            next += 1;
            mask.n = next;
        }
        mask.x = mask.x.clamp(0.0, 1.0);
        mask.y = mask.y.clamp(0.0, 1.0);
        mask.w = mask.w.clamp(0.0, 1.0 - mask.x);
        mask.h = mask.h.clamp(0.0, 1.0 - mask.y);
    }
    Ok(m)
}

fn key(n: u32) -> String {
    format!("m{n}")
}

fn widget(image: Option<&String>, masks: &Masks, active: u32, reveal: bool) -> String {
    let mut out = String::from("<div class=\"fc-io\">");
    if let Some(hash) = image {
        let _ = write!(out, "<img src=\"media:{hash}\" alt=\"\">");
    }
    for m in &masks.masks {
        let is_active = m.n == active;
        let shown = match (is_active, reveal) {
            (true, true) => false,
            (true, false) => true,
            (false, _) => masks.mode == MaskMode::HideAll,
        };
        if !shown && !is_active {
            continue;
        }
        let class = match (is_active, reveal) {
            (true, true) => "fc-io-mask fc-io-revealed",
            (true, false) => "fc-io-mask fc-io-active",
            _ => "fc-io-mask",
        };
        let _ = write!(
            out,
            "<div class=\"{class}\" style=\"left:{:.2}%;top:{:.2}%;width:{:.2}%;height:{:.2}%\"></div>",
            m.x * 100.0,
            m.y * 100.0,
            m.w * 100.0,
            m.h * 100.0
        );
    }
    out.push_str("</div>");
    out
}

/// Image occlusion cards.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImageOcclusionEngine;

impl CardTypeEngine for ImageOcclusionEngine {
    fn info(&self) -> ProviderInfo {
        info(engine::IMAGE_OCCLUSION)
    }

    fn validate(&self, notetype: &NoteType, _template: &CardTemplate) -> StrategyResult<()> {
        require_field(notetype, "image", &[FieldKind::Image])?;
        require_field(notetype, "masks", &[FieldKind::Text])
    }

    fn generate_cards(
        &self,
        note: &Note,
        _notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>> {
        let raw = note
            .fields
            .get(&FieldKey::new("masks"))
            .map_or("", String::as_str);
        let mut numbers: Vec<u32> = parse_masks(raw)?.masks.iter().map(|m| m.n).collect();
        numbers.sort_unstable();
        numbers.dedup();
        Ok(numbers
            .into_iter()
            .map(|n| CardSpec {
                template_key: key(n),
                card_type: template.card_type.clone(),
            })
            .collect())
    }

    fn render(&self, input: &CardInput<'_>, ctx: &RenderContext) -> StrategyResult<RenderedCard> {
        let active: u32 = input
            .template_key
            .strip_prefix('m')
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| StrategyError(format!("not a mask key: {}", input.template_key)))?;
        let masks = parse_masks(field(input, "masks"))?;
        let image = extract_media(field(input, "image")).into_iter().next();
        let (q, a) = render_sides(input, ctx, None)?;
        Ok(RenderedCard {
            tts: tts_of(&q, &a),
            question: format!(
                "{}{}",
                q.html,
                widget(image.as_ref(), &masks, active, false)
            ),
            answer: format!("{}{}", widget(image.as_ref(), &masks, active, true), a.html),
            css: input.notetype.css.clone(),
            interaction: Interaction::SelfGrade,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{builtin_nt, note};
    use super::*;

    const H: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn one_card_per_mask_with_stable_numbers() {
        let nt = builtin_nt("image_occlusion");
        ImageOcclusionEngine
            .validate(&nt, &nt.templates[0])
            .unwrap();
        let masks = r#"{"mode":"hide_all","masks":[{"n":4,"x":0.1,"y":0.1,"w":0.2,"h":0.2},{"x":0.5,"y":0.5,"w":0.9,"h":0.1}]}"#;
        let n = note(&nt, &[("image", &format!("media:{H}")), ("masks", masks)]);
        let specs = ImageOcclusionEngine
            .generate_cards(&n, &nt, &nt.templates[0])
            .unwrap();
        let keys: Vec<_> = specs.iter().map(|s| s.template_key.as_str()).collect();
        assert_eq!(keys, ["m4", "m5"]);

        let input = CardInput {
            note: &n,
            notetype: &nt,
            template: &nt.templates[0],
            template_key: "m5",
        };
        let r = ImageOcclusionEngine
            .render(&input, &RenderContext::default())
            .unwrap();
        assert!(r.question.contains(&format!("src=\"media:{H}\"")));
        assert_eq!(r.question.matches("fc-io-mask").count(), 2);
        assert!(r
            .question
            .contains("fc-io-active\" style=\"left:50.00%;top:50.00%;width:50.00%"));
        assert!(r.answer.contains("fc-io-revealed"));
        assert!(!r.answer.contains("fc-io-active"));
    }

    #[test]
    fn hide_one_and_bad_json() {
        let nt = builtin_nt("image_occlusion");
        let masks = r#"{"mode":"hide_one","masks":[{"n":1,"x":0,"y":0,"w":0.1,"h":0.1},{"n":2,"x":0.2,"y":0,"w":0.1,"h":0.1}]}"#;
        let n = note(&nt, &[("masks", masks)]);
        let input = CardInput {
            note: &n,
            notetype: &nt,
            template: &nt.templates[0],
            template_key: "m1",
        };
        let r = ImageOcclusionEngine
            .render(&input, &RenderContext::default())
            .unwrap();
        assert_eq!(r.question.matches("fc-io-mask").count(), 1);
        let broken = note(&nt, &[("masks", "{nope")]);
        assert!(ImageOcclusionEngine
            .generate_cards(&broken, &nt, &nt.templates[0])
            .is_err());
        assert!(parse_masks("").unwrap().masks.is_empty());
    }
}
