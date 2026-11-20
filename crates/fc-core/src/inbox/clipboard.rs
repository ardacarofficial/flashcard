//! Clipboard watching filter (docs/06 §7): which copied text is worth a "add as a card?"
//! suggestion. Watching is opt-in (module `clipboard`, off by default); this only decides,
//! it never stores anything. Text that looks like a secret (password, API key, token, card
//! number) is never suggested.

/// Filter settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardFilter {
    /// Fewest characters (after trimming).
    pub min_chars: usize,
    /// Most characters.
    pub max_chars: usize,
}

impl Default for ClipboardFilter {
    fn default() -> Self {
        Self {
            min_chars: 2,
            max_chars: 200,
        }
    }
}

/// Why text was not suggested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardIgnore {
    /// Empty or only whitespace.
    Empty,
    /// Shorter than `min_chars`.
    TooShort,
    /// Longer than `max_chars`.
    TooLong,
    /// More than a few lines (code, documents).
    Multiline,
    /// Looks like a password, key, token or card number.
    LooksSecret,
    /// A bare URL, e-mail address or file path.
    NotText,
}

impl ClipboardIgnore {
    /// Stable reason string.
    pub fn reason(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
            Self::Multiline => "multiline",
            Self::LooksSecret => "secret",
            Self::NotText => "not_text",
        }
    }
}

/// Most lines a suggestion may span.
pub const MAX_LINES: usize = 3;

/// The trimmed text to suggest, or why not.
pub fn check(text: &str, filter: ClipboardFilter) -> Result<String, ClipboardIgnore> {
    let t = text.trim();
    if t.is_empty() {
        return Err(ClipboardIgnore::Empty);
    }
    let chars = t.chars().count();
    if chars < filter.min_chars {
        return Err(ClipboardIgnore::TooShort);
    }
    if chars > filter.max_chars {
        return Err(ClipboardIgnore::TooLong);
    }
    if t.lines().count() > MAX_LINES {
        return Err(ClipboardIgnore::Multiline);
    }
    if is_not_text(t) {
        return Err(ClipboardIgnore::NotText);
    }
    if looks_secret(t) {
        return Err(ClipboardIgnore::LooksSecret);
    }
    Ok(t.to_owned())
}

/// Heuristics for secrets: key prefixes and PEM blocks, JWTs, card numbers (Luhn), and single
/// "words" mixing three or more character classes (typical generated passwords and tokens).
pub fn looks_secret(t: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "sk-",
        "sk_",
        "pk_",
        "ghp_",
        "gho_",
        "github_pat_",
        "xox",
        "AKIA",
        "AIza",
        "glpat-",
        "-----BEGIN",
    ];
    if PREFIXES.iter().any(|p| t.starts_with(p)) {
        return true;
    }
    let single_word = !t.chars().any(char::is_whitespace);
    if single_word && t.split('.').count() == 3 && t.len() > 30 && t.starts_with("eyJ") {
        return true;
    }
    let digits: Vec<u32> = t.chars().filter_map(|c| c.to_digit(10)).collect();
    let card_like = t
        .chars()
        .all(|c| c.is_ascii_digit() || c == ' ' || c == '-');
    if card_like && (13..=19).contains(&digits.len()) && luhn(&digits) {
        return true;
    }
    if single_word && t.chars().count() >= 8 {
        let classes = [
            t.chars().any(|c| c.is_lowercase()),
            t.chars().any(|c| c.is_uppercase()),
            t.chars().any(|c| c.is_ascii_digit()),
            t.chars().any(|c| !c.is_alphanumeric()),
        ];
        if classes.iter().filter(|&&b| b).count() >= 3 {
            return true;
        }
    }
    false
}

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 {
                    x - 9
                } else {
                    x
                }
            } else {
                d
            }
        })
        .sum();
    sum % 10 == 0
}

fn is_not_text(t: &str) -> bool {
    if t.chars().any(char::is_whitespace) {
        return false;
    }
    let lower = t.to_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file:")
        || lower.starts_with("www.")
        || (t.contains('@') && t.contains('.'))
        || t.starts_with('/')
        || t.starts_with("\\\\")
        || (t.len() > 2 && t.as_bytes()[1] == b':' && matches!(t.as_bytes()[2], b'\\' | b'/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(t: &str) -> bool {
        check(t, ClipboardFilter::default()).is_ok()
    }

    #[test]
    fn words_and_sentences_are_suggested() {
        assert!(ok("porsuk"));
        assert!(ok("  The badger dug a hole.  "));
        assert!(ok("Kılıç kalkan"));
        assert!(ok("well-known"));
        assert_eq!(check("  hi ", ClipboardFilter::default()), Ok("hi".into()));
    }

    #[test]
    fn secrets_are_never_suggested() {
        for s in [
            "sk-ant-api03-abcdef",
            "ghp_1234567890abcdef",
            "Tr0ub4dor&3",
            "hunter2!X",
            "4111 1111 1111 1111",
            "4111-1111-1111-1111",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abc",
            "-----BEGIN PRIVATE KEY-----",
        ] {
            assert_eq!(
                check(s, ClipboardFilter::default()),
                Err(ClipboardIgnore::LooksSecret),
                "{s}"
            );
        }
        assert!(!looks_secret("4111 1111 1111 1112"));
    }

    #[test]
    fn other_refusals() {
        let f = ClipboardFilter::default();
        assert_eq!(check("   ", f), Err(ClipboardIgnore::Empty));
        assert_eq!(check("a", f), Err(ClipboardIgnore::TooShort));
        assert_eq!(check(&"a ".repeat(150), f), Err(ClipboardIgnore::TooLong));
        assert_eq!(check("a\nb\nc\nd", f), Err(ClipboardIgnore::Multiline));
        for s in [
            "https://example.com",
            "me@example.com",
            "/etc/passwd",
            "C:\\Users\\x",
        ] {
            assert_eq!(check(s, f), Err(ClipboardIgnore::NotText), "{s}");
        }
    }
}
