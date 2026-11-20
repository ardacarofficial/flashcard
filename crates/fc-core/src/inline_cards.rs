//! Inline card creation (docs/02 §14, RemNote style): in a longer text every `question ::
//! answer` line becomes a card. Pure parsing; the app turns the result into notes.
//!
//! Rules:
//! * The **first** `::` of a line that is outside `{{…}}` (cloze), `[[…]]` (links) and
//!   `` `…` `` (inline code) splits it; both sides are trimmed and must be non-empty.
//! * `\::` is a literal `::` (kept as `::`, never a separator).
//! * List markers (`- `, `* `, `+ `, `• `, `1. `, `1) `) and Markdown headings (`# `) at the start
//!   of a line are dropped; lines inside fenced code blocks (```` ``` ````) are ignored.
//! * At most [`MAX_CARDS`] cards; the rest is reported as truncated.

/// Most cards one text yields.
pub const MAX_CARDS: usize = 500;

/// One parsed card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineCard {
    /// 1-based line number in the text.
    pub line: u32,
    /// Question side.
    pub front: String,
    /// Answer side.
    pub back: String,
}

/// Result of [`parse`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    /// Cards, in text order.
    pub cards: Vec<InlineCard>,
    /// More than [`MAX_CARDS`] lines matched (the rest was dropped).
    pub truncated: bool,
}

fn strip_marker(line: &str) -> &str {
    let t = line.trim_start();
    for m in ["- ", "* ", "+ ", "• ", "# ", "## ", "### "] {
        if let Some(rest) = t.strip_prefix(m) {
            return rest;
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &t[digits..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return r;
        }
    }
    t
}

/// Byte index of the separating `::`, if any.
fn separator(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let (mut braces, mut brackets, mut code) = (0usize, 0usize, false);
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' if b.get(i + 1) == Some(&b':') && b.get(i + 2) == Some(&b':') => {
                i += 3;
                continue;
            }
            b'`' => code = !code,
            b'{' if !code && b.get(i + 1) == Some(&b'{') => {
                braces += 1;
                i += 2;
                continue;
            }
            b'}' if !code && braces > 0 && b.get(i + 1) == Some(&b'}') => {
                braces -= 1;
                i += 2;
                continue;
            }
            b'[' if !code && b.get(i + 1) == Some(&b'[') => {
                brackets += 1;
                i += 2;
                continue;
            }
            b']' if !code && brackets > 0 && b.get(i + 1) == Some(&b']') => {
                brackets -= 1;
                i += 2;
                continue;
            }
            b':' if !code && braces == 0 && brackets == 0 && b.get(i + 1) == Some(&b':') => {
                return Some(i);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn unescape(s: &str) -> String {
    s.trim().replace("\\::", "::")
}

/// Parses `text` into cards.
pub fn parse(text: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut fenced = false;
    for (n, raw) in text.lines().enumerate() {
        if raw.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let line = strip_marker(raw);
        let Some(at) = separator(line) else {
            continue;
        };
        let front = unescape(&line[..at]);
        let back = unescape(&line[at + 2..]);
        if front.is_empty() || back.is_empty() {
            continue;
        }
        if out.cards.len() == MAX_CARDS {
            out.truncated = true;
            break;
        }
        out.cards.push(InlineCard {
            line: u32::try_from(n + 1).unwrap_or(u32::MAX),
            front,
            back,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(text: &str) -> Vec<(u32, String, String)> {
        parse(text)
            .cards
            .into_iter()
            .map(|c| (c.line, c.front, c.back))
            .collect()
    }

    #[test]
    fn question_answer_lines_become_cards() {
        let text = "Hayvanlar\n\
                    - porsuk :: badger\n\
                    1. kedi::cat\n\
                    düz metin satırı\n\
                    # Başkent :: Ankara\n\
                    boş :: \n\
                    :: yalnız cevap\n\
                    {{c1::Ankara}} Türkiye'nin başkentidir\n\
                    Saat 10:30'da :: toplantı\n\
                    [[İngilizce::Fiil]] bağlantısı :: link\n\
                    `a::b` kodu :: kod\n\
                    kaçış \\:: değil :: evet\n\
                    ```\n\
                    gizli :: kod bloğu\n\
                    ```\n\
                    a :: b :: c";
        assert_eq!(
            pairs(text),
            vec![
                (2, "porsuk".into(), "badger".into()),
                (3, "kedi".into(), "cat".into()),
                (5, "Başkent".into(), "Ankara".into()),
                (9, "Saat 10:30'da".into(), "toplantı".into()),
                (10, "[[İngilizce::Fiil]] bağlantısı".into(), "link".into()),
                (11, "`a::b` kodu".into(), "kod".into()),
                (12, "kaçış :: değil".into(), "evet".into()),
                (16, "a".into(), "b :: c".into()),
            ]
        );
    }

    #[test]
    fn many_lines_are_capped() {
        let text: String = (0..MAX_CARDS + 3)
            .map(|i| format!("q{i} :: a{i}\n"))
            .collect();
        let p = parse(&text);
        assert_eq!(p.cards.len(), MAX_CARDS);
        assert!(p.truncated);
        assert!(!parse("q :: a").truncated);
        assert!(parse("").cards.is_empty());
        // CRLF line endings.
        assert_eq!(pairs("a :: b\r\nc :: d\r\n").len(), 2);
    }
}
