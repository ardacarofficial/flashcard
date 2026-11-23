//! Secret redaction for logs, crash reports and exported log bundles.
//!
//! Defence in depth: the app never logs request params, keys or tokens on purpose; this pass
//! catches what slips through (an error message quoting a URL with a key, a panic message, a
//! third-party library line). It masks:
//!
//! * private key blocks (PEM),
//! * credentials in URLs (`https://user:pass@host`) and secret query parameters,
//! * `Authorization`/`Bearer`/`Basic` values and `key = value` pairs whose key names a secret
//!   (`api_key`, `token`, `password`, `secret`, `authorization`, `cookie`, …),
//! * well-known key formats (OpenAI/Anthropic `sk-…`, GitHub `ghp_…`, Slack `xox…`, AWS
//!   `AKIA…`, Google `AIza…`, GitLab `glpat-…`, Stripe `sk_live_…`) and JWTs,
//! * password hashes in PHC form (`$argon2id$…`: the app lock's hash),
//! * long hex strings (≥ 32: token hashes, raw tokens) and long mixed-case alphanumeric runs
//!   (≥ 32 with lower, upper and digits: generated keys),
//! * email addresses and the user name in home-folder paths (`C:\Users\<name>`,
//!   `/home/<name>`, `/Users/<name>`),
//! * any exact secret the caller knows (the app adds the AI provider keys it holds).
//!
//! UUIDs (hyphenated) and ordinary words survive, so logs stay useful. Redaction is
//! idempotent: redacting a redacted text changes nothing.

use regex::{Captures, Regex};

/// What replaces a secret.
pub const MASK: &str = "[REDACTED]";
/// What replaces an email address.
pub const EMAIL_MASK: &str = "[EMAIL]";
/// What replaces the user name in a home-folder path.
pub const USER_MASK: &str = "<user>";
/// Exact secrets shorter than this are not replaced literally (they would hit ordinary words).
pub const MIN_EXACT_SECRET: usize = 6;

/// Result of [`Redactor::redact`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted {
    /// The masked text.
    pub text: String,
    /// How many replacements were made.
    pub count: usize,
}

/// Compiled redaction rules. Build once ([`Redactor::new`]) and reuse; cheap to clone.
#[derive(Debug, Clone)]
pub struct Redactor {
    rules: Vec<Rule>,
    exact: Vec<String>,
}

#[derive(Debug, Clone)]
struct Rule {
    re: Regex,
    kind: RuleKind,
}

#[derive(Debug, Clone, Copy)]
enum RuleKind {
    /// Whole match → [`MASK`].
    Whole,
    /// Keep capture 1 (and capture 2 as a suffix, when the pattern has one), mask the rest.
    KeepPrefix,
    /// Whole match → [`MASK`] when it mixes lower case, upper case and digits.
    Mixed,
    /// Whole match → [`EMAIL_MASK`].
    Email,
    /// Keep capture 1, replace the rest with [`USER_MASK`].
    User,
}

/// Keys whose value is masked. v1.4: `typed_answer(s)` too — what the user typed while
/// studying is user history (kept in `typed_answers`), never a log line.
const SECRET_KEYS: &str = "api[_-]?key|apikey|x-api-key|access[_-]?token|refresh[_-]?token|auth[_-]?token|id[_-]?token|client[_-]?secret|secret[_-]?key|secret|password|passwd|pwd|passphrase|token|authorization|private[_-]?key|session[_-]?id|cookie|signature|typed[_-]?answers?";

impl Default for Redactor {
    fn default() -> Self {
        Self::new()
    }
}

impl Redactor {
    /// The built-in rules (module docs).
    pub fn new() -> Self {
        let specs: [(&str, RuleKind); 13] = [
            (
                r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
                RuleKind::Whole,
            ),
            // `untrusted comment: minisign/rsign secret key` blocks: the comment line and the
            // base64 key line after it.
            (
                r"(?i)(untrusted comment:[^\n]*secret key[^\n]*\n)[A-Za-z0-9+/=]+",
                RuleKind::KeepPrefix,
            ),
            (
                r"(?i)(\b[a-z][a-z0-9+.-]*://)[^/\s:@\[\]]+:[^/\s@\[\]]+(@)",
                RuleKind::KeepPrefix,
            ),
            (
                r"(?i)(\b(?:bearer|basic)\s+)[A-Za-z0-9._~+/=-]{6,}",
                RuleKind::KeepPrefix,
            ),
            (
                &format!(
                    r#"(?i)((?:{SECRET_KEYS})["']?\s*[:=]\s*(?:Some\()?)(?:"[^"\n]+"|'[^'\n]+'|[^"'\s,;&}}<>\[\]]+)"#
                ),
                RuleKind::KeepPrefix,
            ),
            (
                r"\b(?:sk-(?:ant-)?[A-Za-z0-9_-]{16,}|sk_(?:live|test)_[A-Za-z0-9]{16,}|rk_(?:live|test)_[A-Za-z0-9]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|glpat-[A-Za-z0-9_-]{20,})",
                RuleKind::Whole,
            ),
            (
                r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
                RuleKind::Whole,
            ),
            // Password hashes in PHC form (the app lock's Argon2id hash, v1.0): slow and salted,
            // but still nothing a bug report needs.
            (r"\$argon2(?:id|i|d)\$[A-Za-z0-9+/=,$-]+", RuleKind::Whole),
            (r"\b[0-9a-fA-F]{32,}\b", RuleKind::Whole),
            (r"[A-Za-z0-9+/_-]{32,}={0,2}", RuleKind::Mixed),
            (
                r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}\b",
                RuleKind::Email,
            ),
            (
                r#"(?i)(\b[a-z]:(?:\\\\|\\|/)Users(?:\\\\|\\|/))[^\\/\t\r\n"'<>:]+"#,
                RuleKind::User,
            ),
            (
                r#"((?:^|[\s"'=(])/(?:home|Users)/)[^/\s"'<>:]+"#,
                RuleKind::User,
            ),
        ];
        let rules = specs
            .iter()
            .filter_map(|(re, kind)| {
                // The patterns are constants (a unit test compiles every one).
                Regex::new(re).ok().map(|re| Rule { re, kind: *kind })
            })
            .collect();
        Self {
            rules,
            exact: Vec::new(),
        }
    }

    /// Number of compiled rules (tests: every built-in pattern compiles).
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Also masks these exact strings (longest first; shorter than [`MIN_EXACT_SECRET`]
    /// characters are ignored).
    pub fn with_secrets<I, S>(mut self, secrets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for s in secrets {
            let s = s.into();
            if s.chars().count() >= MIN_EXACT_SECRET && !self.exact.contains(&s) {
                self.exact.push(s);
            }
        }
        self.exact
            .sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        self
    }

    /// Masks secrets in `text`.
    pub fn redact(&self, text: &str) -> Redacted {
        let mut count = 0usize;
        let mut out = text.to_owned();
        for secret in &self.exact {
            let n = out.matches(secret.as_str()).count();
            if n > 0 {
                count += n;
                out = out.replace(secret.as_str(), MASK);
            }
        }
        for rule in &self.rules {
            let mut hits = 0usize;
            let replaced = rule.re.replace_all(&out, |c: &Captures<'_>| {
                let whole = c.get(0).map_or("", |m| m.as_str());
                let keep = c.get(1).map_or("", |m| m.as_str());
                let (new, hit) = match rule.kind {
                    RuleKind::Whole => (MASK.to_owned(), whole != MASK),
                    RuleKind::KeepPrefix => {
                        let suffix = c.get(2).map_or("", |m| m.as_str());
                        let rest = whole
                            .get(keep.len()..whole.len().saturating_sub(suffix.len()))
                            .unwrap_or_default();
                        (format!("{keep}{MASK}{suffix}"), rest != MASK)
                    }
                    RuleKind::Mixed => {
                        if mixed(whole) {
                            (MASK.to_owned(), true)
                        } else {
                            (whole.to_owned(), false)
                        }
                    }
                    RuleKind::Email => (EMAIL_MASK.to_owned(), true),
                    RuleKind::User => {
                        let rest = &whole[keep.len()..];
                        (format!("{keep}{USER_MASK}"), rest != USER_MASK)
                    }
                };
                if hit {
                    hits += 1;
                }
                new
            });
            if hits > 0 {
                count += hits;
                out = replaced.into_owned();
            }
        }
        Redacted { text: out, count }
    }

    /// [`Redactor::redact`]'s text only.
    pub fn clean(&self, text: &str) -> String {
        self.redact(text).text
    }
}

/// Lower case, upper case and digits all present (generated keys; not paths or words).
fn mixed(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_lowercase())
        && s.chars().any(|c| c.is_ascii_uppercase())
        && s.chars().any(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn r(text: &str) -> String {
        Redactor::new().clean(text)
    }

    #[test]
    fn every_rule_compiles() {
        assert_eq!(Redactor::new().rule_count(), 13);
    }

    #[test]
    fn known_formats_are_masked() {
        let secrets = [
            "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789",
            "sk-proj-abcdefghijklmnopqrstuvwxyz012345",
            "sk_live_abcdefghijklmnop1234",
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "github_pat_11ABCDEFG0123456789_abcdefghijklmnop",
            "xoxb-123456789012-abcdefghijkl",
            "AKIAABCDEFGHIJKLMNOP",
            "AIzaSyA-abcdefghijklmnopqrstuvwxyz012345",
            "glpat-abcdefghijklmnopqrstu",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "Zx9Qw8Er7Ty6Ui5Op4As3Df2Gh1Jk0LmNbVcXz",
        ];
        for s in secrets {
            let line = format!("request failed with {s} (retrying)");
            let out = r(&line);
            assert!(!out.contains(s), "{s} → {out}");
            assert!(out.contains(MASK), "{out}");
            assert!(out.starts_with("request failed with "), "{out}");
        }
    }

    #[test]
    fn key_value_pairs_headers_and_urls_are_masked() {
        let cases = [
            ("api_key=hunter2hunter2", "api_key=[REDACTED]"),
            (
                r#"{"api_key": "abc123secret"}"#,
                r#"{"api_key": [REDACTED]}"#,
            ),
            ("X-Api-Key: plainvalue99", "X-Api-Key: [REDACTED]"),
            ("password = 'correct horse'", "password = [REDACTED]"),
            (
                "Authorization: Bearer abcdef.ghijk",
                "Authorization: [REDACTED] [REDACTED]",
            ),
            ("token=t0k3n&x=1", "token=[REDACTED]&x=1"),
            // v1.4: typed answers (JSON params and Rust `Debug` output).
            (
                r#"{"rating":"again","typed_answer":"the cat sat"}"#,
                r#"{"rating":"again","typed_answer":[REDACTED]}"#,
            ),
            (
                r#"AnswerCard { typed_answer: Some("kedi") }"#,
                r#"AnswerCard { typed_answer: Some([REDACTED]) }"#,
            ),
            (
                "typed_answers_recorded: true",
                "typed_answers_recorded: true",
            ),
            (
                "GET https://bob:s3cret@example.com/v1",
                "GET https://[REDACTED]@example.com/v1",
            ),
            (
                "https://api.example.com/v1?key=abc&api_key=zzzzzz",
                "https://api.example.com/v1?key=abc&api_key=[REDACTED]",
            ),
        ];
        for (input, want) in cases {
            assert_eq!(r(input), want, "{input}");
        }
    }

    #[test]
    fn pem_and_minisign_secret_keys_are_masked() {
        let pem = "x\n-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg\nkqhkiG9w0B\n-----END PRIVATE KEY-----\ny";
        assert_eq!(r(pem), "x\n[REDACTED]\ny");
        let mini = "untrusted comment: rsign encrypted secret key\nRWRTY0IyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==\n";
        let out = r(mini);
        assert!(!out.contains("RWRTY0Iy"), "{out}");
    }

    #[test]
    fn password_hashes_are_masked() {
        let h = "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2hoYXNo";
        let out = r(&format!("lock.profile.p1 = {h} done"));
        assert!(
            !out.contains("c2FsdHNhbHQ") && !out.contains("aGFzaGhhc2g"),
            "{out}"
        );
        assert!(out.ends_with(" done"), "{out}");
    }

    #[test]
    fn personal_data_is_masked() {
        assert_eq!(r("from someone.name+tag@example.co.uk"), "from [EMAIL]");
        assert_eq!(
            r(r"C:\Users\Arda Car\AppData\Roaming\app.flashcard.desktop\collection.db"),
            r"C:\Users\<user>\AppData\Roaming\app.flashcard.desktop\collection.db"
        );
        assert_eq!(
            r(r#"{"path":"C:\\Users\\arda\\Desktop\\a.csv"}"#),
            r#"{"path":"C:\\Users\\<user>\\Desktop\\a.csv"}"#
        );
        assert_eq!(
            r("open /home/arda/.local/share/x"),
            "open /home/<user>/.local/share/x"
        );
        assert_eq!(r("/Users/arda/Library"), "/Users/<user>/Library");
    }

    #[test]
    fn useful_text_survives() {
        for keep in [
            "INFO fc_app::modules::study: session 0190a3b2-7c4d-7e8f-9a0b-1c2d3e4f5a6b started",
            "invoke decks.create failed: VALIDATION_INVALID_PARAMS (12 ms)",
            "crates/fc-app/src/modules/maintenance/crash.rs:10:5",
            "input_tokens: 512, output_tokens: 128",
            "panicked at 'index out of bounds: the len is 3 but the index is 7'",
        ] {
            let out = Redactor::new().redact(keep);
            assert_eq!(out.text, keep);
            assert_eq!(out.count, 0);
        }
    }

    #[test]
    fn exact_secrets_are_masked_even_without_a_known_shape() {
        let red = Redactor::new().with_secrets(["plain words key", "abc"]);
        let out = red.redact("the key is plain words key, and abc stays");
        assert_eq!(out.text, "the key is [REDACTED], and abc stays");
        assert_eq!(out.count, 1);
    }

    fn secret() -> impl Strategy<Value = String> {
        prop_oneof![
            "[A-Za-z0-9]{24,40}".prop_map(|s| format!("sk-ant-api03-{s}")),
            "[A-Za-z0-9]{24,40}".prop_map(|s| format!("sk-{s}")),
            "[A-Za-z0-9]{36}".prop_map(|s| format!("ghp_{s}")),
            "[0-9a-f]{32,64}",
            "[A-Za-z0-9_-]{10,20}".prop_map(|s| format!("eyJ{s}.eyJ{s}.{s}x")),
            // Anything after a secret-named key.
            "[A-Za-z0-9!#$%()*+./:?@^_`|~-]{4,30}".prop_map(|s| format!("api_key={s}")),
            "[A-Za-z0-9._~+/-]{8,30}".prop_map(|s| format!("Bearer {s}")),
        ]
    }

    proptest! {
        /// A secret embedded anywhere in ordinary text never survives, and redaction is
        /// idempotent.
        #[test]
        fn secrets_never_survive(
            before in "[a-z ,.:()]{0,30}",
            after in "[a-z ,.:()]{0,30}",
            s in secret(),
        ) {
            let line = format!("{before} {s} {after}");
            let red = Redactor::new();
            let once = red.redact(&line);
            let value = s.split_once('=').map_or(s.as_str(), |(_, v)| v);
            let value = value.strip_prefix("Bearer ").unwrap_or(value);
            prop_assert!(!once.text.contains(value), "{} → {}", line, once.text);
            prop_assert!(once.count >= 1);
            let twice = red.redact(&once.text);
            prop_assert_eq!(&twice.text, &once.text);
            prop_assert_eq!(twice.count, 0);
        }

        /// Text without anything secret-shaped is returned unchanged.
        #[test]
        fn plain_text_is_untouched(text in "[a-z ,.:()0-9]{0,80}") {
            let out = Redactor::new().redact(&text);
            prop_assert_eq!(out.text, text);
        }
    }
}
