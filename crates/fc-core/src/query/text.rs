//! Text matching shared by every query backend: search folding (Turkish-aware), the tokenizer
//! the full-text index emulates, `*` globs and regex over field text.
//!
//! The SQLite store indexes [`fold`]ed text in FTS5 (`unicode61 remove_diacritics 2`) and calls
//! [`field_glob_matches`] / [`regex_matches`] from SQL functions; the in-memory store calls the
//! same functions, so both backends agree on what matches.

use crate::note::strip_html;
use std::collections::BTreeMap;

/// Folds text for searching: lowercase, Turkish dotted/dotless `i` unified (`I ı İ i` → `i`),
/// Latin diacritics removed (`ş → s`, `ç → c`, `ğ → g`, `ö → o`, `ü → u`, `é → e` …).
pub fn fold(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            'I' | 'ı' | 'İ' => out.push('i'),
            _ => {
                for l in c.to_lowercase() {
                    if l == '\u{307}' {
                        // Combining dot above left by lowercasing `İ` elsewhere.
                        continue;
                    }
                    out.push(strip_diacritic(l));
                }
            }
        }
    }
    out
}

fn strip_diacritic(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' | 'ĉ' | 'ċ' => 'c',
        'ď' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' | 'ĝ' | 'ġ' | 'ģ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ñ' | 'ń' | 'ň' | 'ņ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' | 'ø' => 'o',
        'ř' | 'ŕ' => 'r',
        'ş' | 'ś' | 'š' | 'ŝ' | 'ș' => 's',
        'ţ' | 'ť' | 'ț' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ž' | 'ż' => 'z',
        other => other,
    }
}

/// Splits folded text into index tokens: maximal runs of alphanumeric characters (what the
/// FTS5 `unicode61` tokenizer produces for Latin/Turkish text).
pub fn tokens(folded: &str) -> Vec<&str> {
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Whether `text` (raw) contains the token sequence `phrase` (already folded tokens), the last
/// token matched as a prefix when `prefix` is set. Empty `phrase` matches everything.
pub fn phrase_matches(text: &str, phrase: &[String], prefix: bool) -> bool {
    if phrase.is_empty() {
        return true;
    }
    let folded = fold(text);
    let toks = tokens(&folded);
    let n = phrase.len();
    if toks.len() < n {
        return false;
    }
    (0..=toks.len() - n).any(|start| {
        phrase.iter().enumerate().all(|(i, p)| {
            let t = toks[start + i];
            if prefix && i == n - 1 {
                t.starts_with(p.as_str())
            } else {
                t == p
            }
        })
    })
}

/// A `*` wildcard pattern over folded text (`\*` = literal star). Matches the **whole** text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glob {
    /// Literal parts between stars (folded).
    parts: Vec<String>,
    /// Whether the pattern contains a wildcard at all.
    wild: bool,
}

impl Glob {
    /// Compiles a pattern (folded with [`fold`]).
    pub fn new(pattern: &str) -> Self {
        let mut parts = vec![String::new()];
        let mut wild = false;
        let mut chars = pattern.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        if let Some(last) = parts.last_mut() {
                            last.push(n);
                        }
                    }
                }
                '*' => {
                    wild = true;
                    parts.push(String::new());
                }
                c => {
                    if let Some(last) = parts.last_mut() {
                        last.push(c);
                    }
                }
            }
        }
        Self {
            parts: parts.iter().map(|p| fold(p)).collect(),
            wild,
        }
    }

    /// Whether the pattern has a `*`.
    pub fn is_wild(&self) -> bool {
        self.wild
    }

    /// The pattern as stored/transported: parts joined by `*`, literal stars escaped.
    pub fn source(&self) -> String {
        self.parts
            .iter()
            .map(|p| p.replace('\\', "\\\\").replace('*', "\\*"))
            .collect::<Vec<_>>()
            .join("*")
    }

    /// Whether `text` (raw; folded here) matches the whole pattern.
    pub fn matches(&self, text: &str) -> bool {
        self.matches_folded(&fold(text))
    }

    /// Whether already-folded `text` matches.
    pub fn matches_folded(&self, text: &str) -> bool {
        let parts = &self.parts;
        if parts.len() == 1 {
            return text == parts[0];
        }
        let first = &parts[0];
        let last = &parts[parts.len() - 1];
        if !text.starts_with(first.as_str()) {
            return false;
        }
        let mut rest = &text[first.len()..];
        if rest.len() < last.len() || !rest.ends_with(last.as_str()) {
            return false;
        }
        rest = &rest[..rest.len() - last.len()];
        for mid in &parts[1..parts.len() - 1] {
            match rest.find(mid.as_str()) {
                Some(i) => rest = &rest[i + mid.len()..],
                None => return false,
            }
        }
        true
    }
}

/// Index tokens every whole-field match of `glob` must contain, as `(folded token, prefix)`:
/// tokens of the literal parts that start at a token boundary (a token right after a `*` may
/// be the tail of a longer word and is skipped); the last token of a part followed by `*` is a
/// prefix. Used to pre-filter field globs with the full-text index (necessary, not sufficient).
pub fn glob_index_tokens(glob: &Glob) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let n = glob.parts.len();
    for (pi, part) in glob.parts.iter().enumerate() {
        let chars: Vec<char> = part.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if !chars[i].is_alphanumeric() {
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
            let left_bounded = start > 0 || pi == 0;
            let right_bounded = i < chars.len() || pi == n - 1;
            if left_bounded {
                out.push((chars[start..i].iter().collect(), !right_bounded));
            }
        }
    }
    out
}

/// Field values in `keys` (all fields when `None`), HTML stripped, newline-joined: the text a
/// field glob or a regex is matched against.
pub fn fields_text(fields: &BTreeMap<String, String>, keys: Option<&[String]>) -> String {
    let mut out: Vec<String> = Vec::new();
    for (k, v) in fields {
        if keys.is_none_or(|ks| ks.iter().any(|x| x == k)) {
            out.push(strip_html(v));
        }
    }
    out.join("\n")
}

/// Whether any of the `keys` fields matches `glob` as a whole (HTML stripped, folded).
pub fn field_glob_matches(fields: &BTreeMap<String, String>, keys: &[String], glob: &Glob) -> bool {
    keys.iter()
        .filter_map(|k| fields.get(k))
        .any(|v| glob.matches(&strip_html(v)))
}

/// Compiles a query regex: case-insensitive, `^`/`$` per line (= per field), size-limited.
pub fn compile_regex(pattern: &str) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .multi_line(true)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| e.to_string())
}

/// Whether `re` matches the text of the `keys` fields (all fields when `None`).
pub fn regex_matches(
    fields: &BTreeMap<String, String>,
    keys: Option<&[String]>,
    re: &regex::Regex,
) -> bool {
    re.is_match(&fields_text(fields, keys))
}

/// Find/replace in one field value (raw text, HTML included). `regex`: `find` is a regex and
/// `replace` may use `$1`/`${name}`; otherwise both are literal. Case-insensitive unless
/// `match_case`. Errors with a reason for an empty or invalid pattern.
pub fn replace_text(
    text: &str,
    find: &str,
    replace: &str,
    regex: bool,
    match_case: bool,
) -> Result<String, String> {
    if find.is_empty() {
        return Err("empty pattern".to_owned());
    }
    let pattern = if regex {
        find.to_owned()
    } else {
        regex::escape(find)
    };
    let re = regex::RegexBuilder::new(&pattern)
        .case_insensitive(!match_case)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(if regex {
        re.replace_all(text, replace).into_owned()
    } else {
        re.replace_all(text, regex::NoExpand(replace)).into_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_turkish_and_diacritics() {
        assert_eq!(fold("KILIÇ İstanbul ışık ŞÖĞÜ"), "kilic istanbul isik sogu");
        assert_eq!(fold("Café"), "cafe");
    }

    #[test]
    fn tokenizes_on_non_alphanumerics() {
        assert_eq!(tokens("e-mail, ok!  x1"), vec!["e", "mail", "ok", "x1"]);
    }

    #[test]
    fn phrases_and_prefixes() {
        let p = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert!(phrase_matches(
            "Tam ifade burada",
            &p(&["tam", "ifade"]),
            false
        ));
        assert!(!phrase_matches("ifade tam", &p(&["tam", "ifade"]), false));
        assert!(phrase_matches("Kılıçlar", &p(&["kilic"]), true));
        assert!(!phrase_matches("Kılıçlar", &p(&["kilic"]), false));
        assert!(phrase_matches("anything", &[], false));
    }

    #[test]
    fn globs() {
        assert!(Glob::new("app*").matches("Apple"));
        assert!(!Glob::new("app").matches("Apple"));
        assert!(Glob::new("*ple").matches("apple"));
        assert!(Glob::new("a*p*e").matches("apple"));
        assert!(!Glob::new("a*x*e").matches("apple"));
        assert!(Glob::new("*").matches(""));
        assert!(Glob::new("a\\*b").matches("a*b"));
        assert!(!Glob::new("a\\*b").matches("axb"));
        assert!(Glob::new("İngilizce").matches("ingilizce"));
        assert!(!Glob::new("ab*ba").matches("aba"));
        let g = Glob::new("a\\*b*c");
        assert_eq!(Glob::new(&g.source()), g);
    }

    #[test]
    fn find_and_replace() {
        assert_eq!(
            replace_text("Elma elma", "elma", "armut", false, false).unwrap(),
            "armut armut"
        );
        assert_eq!(
            replace_text("Elma elma", "elma", "armut", false, true).unwrap(),
            "Elma armut"
        );
        assert_eq!(replace_text("a.b", ".", "$1", false, true).unwrap(), "a$1b");
        assert_eq!(
            replace_text("run ran", r"r(u|a)n", "w${1}n", true, true).unwrap(),
            "wun wan"
        );
        assert!(replace_text("x", "", "y", false, false).is_err());
        assert!(replace_text("x", "(", "y", true, false).is_err());
    }

    #[test]
    fn index_tokens_of_globs() {
        let t = |g: &str| glob_index_tokens(&Glob::new(g));
        assert_eq!(t("kılıç*"), vec![("kilic".into(), true)]);
        assert_eq!(
            t("kılıç kalkan"),
            vec![("kilic".into(), false), ("kalkan".into(), false)]
        );
        assert_eq!(t("*an"), vec![]);
        assert_eq!(t("*a b*"), vec![("b".into(), true)]);
        assert_eq!(t("x-y*z"), vec![("x".into(), false), ("y".into(), true)]);
    }

    #[test]
    fn field_text_and_regex() {
        let mut f = BTreeMap::new();
        f.insert("front".to_owned(), "<b>Apple</b>".to_owned());
        f.insert("back".to_owned(), "elma".to_owned());
        let keys = vec!["front".to_owned()];
        assert!(field_glob_matches(&f, &keys, &Glob::new("app*")));
        assert!(!field_glob_matches(&f, &keys, &Glob::new("elma")));
        let re = compile_regex("^ap").unwrap();
        assert!(regex_matches(&f, Some(&keys), &re));
        assert!(compile_regex("(").is_err());
    }
}
