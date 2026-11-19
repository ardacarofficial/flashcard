//! Canonical (id-based) query text for **stored** queries (saved queries, later custom study
//! and automations): classification keys and option labels are replaced by ids so a stored
//! query keeps meaning the same thing after the user renames a classification or an option,
//! and ordered comparisons (`seviye>=B1`) follow the options' *current* order.
//!
//! * key: `seviye` → `attr_<32 hex>` ([`attr_key`]; a valid query key, resolved by
//!   [`super::resolve`] like a key or name);
//! * value: `B1` → the option id (`0192…-…`; option ids are accepted wherever a label is).
//!
//! Both directions edit the text at the term spans, so everything else (spacing, decks, tags,
//! free text) stays exactly as typed. Terms that do not name exactly one live classification
//! (built-in keys, fields, ambiguous keys across deck scopes) and wildcard values are left as
//! they are. [`render`] turns canonical text back into what the user reads and edits; both
//! functions are total (unparsable text is returned unchanged) and idempotent.

use super::ast::{Expr, Term};
use super::parse::parse_query;
use super::resolve::{name_key, Catalog, BUILTIN_KEYS};
use super::text::{fold, Glob};
use crate::attribute::{AttributeDef, AttributeKind, AttributeOption};
use crate::id::AttributeDefId;

/// Prefix of a canonical classification key.
pub const ATTR_KEY_PREFIX: &str = "attr_";

/// Canonical query key of a classification: `attr_<uuid as 32 hex digits>`.
pub fn attr_key(id: AttributeDefId) -> String {
    format!("{ATTR_KEY_PREFIX}{}", id.0.simple())
}

/// Live definitions a folded key names (by key, display name or canonical key).
pub(crate) fn defs_named<'a>(attributes: &'a [AttributeDef], k: &str) -> Vec<&'a AttributeDef> {
    attributes
        .iter()
        .filter(|d| {
            d.deleted_at.is_none()
                && (fold(&d.key) == k || name_key(&d.name) == k || attr_key(d.id) == k)
        })
        .collect()
}

fn is_choice(def: &AttributeDef) -> bool {
    matches!(
        def.kind,
        AttributeKind::OrderedEnum | AttributeKind::Enum | AttributeKind::MultiEnum
    )
}

/// The option a (non-wildcard) value names: by id or by label (case/accent-insensitive).
fn option_named<'a>(def: &'a AttributeDef, value: &str) -> Option<&'a AttributeOption> {
    let v = value.trim();
    if Glob::new(v).is_wild() {
        return None;
    }
    let by_id = def.options.iter().find(|o| o.id.to_string() == v);
    by_id.or_else(|| {
        let f = fold(v);
        let mut it = def.options.iter().filter(|o| fold(&o.label) == f);
        let first = it.next()?;
        it.next().is_none().then_some(first)
    })
}

fn terms<'e>(e: &'e Expr, out: &mut Vec<&'e Term>) {
    match e {
        Expr::And(v) | Expr::Or(v) => v.iter().for_each(|x| terms(x, out)),
        Expr::Not(x) => terms(x, out),
        Expr::Term(t) => out.push(t),
    }
}

/// Replaces character ranges (`start`, `end`, text) of `src`; ranges must not overlap.
fn splice(src: &str, mut edits: Vec<(u32, u32, String)>) -> String {
    if edits.is_empty() {
        return src.to_owned();
    }
    edits.sort_by_key(|e| e.0);
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut pos = 0usize;
    for (start, end, text) in edits {
        let (s, e) = (start as usize, (end as usize).min(chars.len()));
        if s < pos || s > e {
            continue;
        }
        out.extend(&chars[pos..s]);
        out.push_str(&text);
        pos = e;
    }
    out.extend(&chars[pos..]);
    out
}

/// A value as query text: bare when the lexer reads it back as one word, otherwise quoted.
fn value_text(v: &str) -> String {
    let bare = !v.is_empty()
        && !v
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\\'));
    if bare {
        return v.to_owned();
    }
    let mut s = String::from("\"");
    for c in v.chars() {
        if c == '"' || c == '\\' {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('"');
    s
}

fn edit_terms(
    text: &str,
    f: impl Fn(&Term, &AttributeDef) -> (String, Option<String>),
    cat: &Catalog<'_>,
) -> String {
    let Ok(expr) = parse_query(text) else {
        return text.to_owned();
    };
    let mut ts = Vec::new();
    terms(&expr, &mut ts);
    let mut edits = Vec::new();
    for t in ts {
        let Some(key) = &t.key else { continue };
        let k = fold(key);
        if BUILTIN_KEYS.contains(&k.as_str()) {
            continue;
        }
        let defs = defs_named(cat.attributes, &k);
        let [def] = defs.as_slice() else { continue };
        let (new_key, new_value) = f(t, def);
        edits.push((t.key_span.start, t.key_span.end, new_key));
        if let Some(v) = new_value {
            edits.push((t.value_span.start, t.value_span.end, v));
        }
    }
    splice(text, edits)
}

/// Canonical form of `text` for storage (see the module docs).
pub fn canonicalize(text: &str, cat: &Catalog<'_>) -> String {
    edit_terms(
        text,
        |t, def| {
            let value = is_choice(def)
                .then(|| option_named(def, &t.value))
                .flatten()
                .map(|o| o.id.to_string());
            (attr_key(def.id), value)
        },
        cat,
    )
}

/// Display form of (canonical) `text`: current classification keys and option labels.
pub fn render(text: &str, cat: &Catalog<'_>) -> String {
    edit_terms(
        text,
        |t, def| {
            let key = if def.key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                def.key.clone()
            } else {
                name_key(&def.name)
            };
            let value = is_choice(def)
                .then(|| option_named(def, &t.value))
                .flatten()
                .map(|o| value_text(&o.label));
            (key, value)
        },
        cat,
    )
}
