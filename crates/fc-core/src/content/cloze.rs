//! Cloze deletions: `{{c1::Ankara}} is the capital` (docs/02 §4). Every distinct number is a
//! separate card (`c1`, `c2`, …); an optional hint follows a second `::`
//! (`{{c1::Ankara::city}}`).

use std::collections::BTreeSet;

/// One parsed deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Deletion<'a> {
    start: usize,
    end: usize,
    ordinal: u32,
    answer: &'a str,
    hint: Option<&'a str>,
}

fn deletions(text: &str) -> Vec<Deletion<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(rel) = text[pos..].find("{{c") {
        let start = pos + rel;
        let after = &text[start + 3..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let rest = &after[digits..];
        let parsed = (digits > 0 && rest.starts_with("::"))
            .then(|| {
                let body_start = start + 3 + digits + 2;
                let close = text[body_start..].find("}}")?;
                let body = &text[body_start..body_start + close];
                let ordinal: u32 = after[..digits].parse().ok().filter(|n| *n > 0)?;
                let (answer, hint) = match body.rfind("::") {
                    Some(i) => (&body[..i], Some(&body[i + 2..])),
                    None => (body, None),
                };
                Some(Deletion {
                    start,
                    end: body_start + close + 2,
                    ordinal,
                    answer,
                    hint,
                })
            })
            .flatten();
        match parsed {
            Some(d) => {
                pos = d.end;
                out.push(d);
            }
            None => pos = start + 3,
        }
    }
    out
}

/// Distinct cloze numbers in `text`, ascending.
pub fn cloze_ordinals(text: &str) -> BTreeSet<u32> {
    deletions(text).into_iter().map(|d| d.ordinal).collect()
}

/// Template key of the card for cloze `ordinal` (`c3`).
pub fn cloze_key(ordinal: u32) -> String {
    format!("c{ordinal}")
}

/// Parses a cloze card key (`c3` → `3`).
pub fn parse_cloze_key(key: &str) -> Option<u32> {
    key.strip_prefix('c')?.parse().ok().filter(|n| *n > 0)
}

/// Replaces deletions for the card of `active` (`None`: no card, show every answer).
///
/// * the active deletion becomes `<span class="cloze">[…]</span>` (or its hint) on the
///   question side, and `<span class="cloze">answer</span>` when `reveal`;
/// * every other deletion shows its answer as plain text.
///
/// Inputs are field markup (Markdown/HTML), outputs too; the caller sanitizes later.
pub fn render_cloze(text: &str, active: Option<u32>, reveal: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for d in deletions(text) {
        out.push_str(&text[pos..d.start]);
        if Some(d.ordinal) == active {
            out.push_str("<span class=\"cloze\">");
            if reveal {
                out.push_str(d.answer);
            } else {
                out.push('[');
                out.push_str(d.hint.map(str::trim).unwrap_or("..."));
                out.push(']');
            }
            out.push_str("</span>");
        } else {
            out.push_str(d.answer);
        }
        pos = d.end;
    }
    out.push_str(&text[pos..]);
    out
}

/// The answer text(s) of deletion `ordinal`, joined with `, ` (for typed cloze answers).
pub fn cloze_answer(text: &str, ordinal: u32) -> String {
    deletions(text)
        .into_iter()
        .filter(|d| d.ordinal == ordinal)
        .map(|d| d.answer)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals_and_keys() {
        let t = "{{c1::Ankara}} is the capital of {{c2::Türkiye::country}}; {{c1::yes}}";
        assert_eq!(
            cloze_ordinals(t).into_iter().collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(cloze_key(2), "c2");
        assert_eq!(parse_cloze_key("c12"), Some(12));
        assert_eq!(parse_cloze_key("c0"), None);
        assert_eq!(parse_cloze_key("forward"), None);
        assert!(cloze_ordinals("{{c::x}} {{c1:x}} {{c0::x}} {{c1::open").is_empty());
    }

    #[test]
    fn renders_question_and_answer() {
        let t = "{{c1::Ankara}} / {{c2::Türkiye::country}}";
        assert_eq!(
            render_cloze(t, Some(1), false),
            "<span class=\"cloze\">[...]</span> / Türkiye"
        );
        assert_eq!(
            render_cloze(t, Some(2), false),
            "Ankara / <span class=\"cloze\">[country]</span>"
        );
        assert_eq!(
            render_cloze(t, Some(2), true),
            "Ankara / <span class=\"cloze\">Türkiye</span>"
        );
        assert_eq!(render_cloze(t, None, false), "Ankara / Türkiye");
        assert_eq!(cloze_answer("{{c1::a}} {{c2::b}} {{c1::c}}", 1), "a, c");
    }
}
