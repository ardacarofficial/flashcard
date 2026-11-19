//! Validation of attribute definitions and values.

use super::{AttributeDef, AttributeKind, AttributeValue, MAX_LABEL_LEN, MAX_TEXT_VALUE_LEN};
use crate::error::{AttributeDefProblem, AttributeValueProblem, CoreError, CoreResult};
use std::collections::HashSet;

/// Maximum length of an attribute key.
pub const MAX_KEY_LEN: usize = 64;

/// Checks a key: `[a-z][a-z0-9_]*`, at most [`MAX_KEY_LEN`] chars.
pub fn validate_attribute_key(key: &str) -> CoreResult<()> {
    let mut chars = key.chars();
    let ok = key.len() <= MAX_KEY_LEN
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(CoreError::InvalidAttributeDef {
            reason: AttributeDefProblem::InvalidKey,
        })
    }
}

/// Derives a key from a display name: `"Sınav Türü"` → `"sinav_turu"`. Turkish letters are
/// transliterated, other non-alphanumerics become `_`. Always returns a valid key.
pub fn slugify_key(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let mapped = match c {
            'ç' | 'Ç' => 'c',
            'ğ' | 'Ğ' => 'g',
            'ı' | 'I' | 'İ' => 'i',
            'ö' | 'Ö' => 'o',
            'ş' | 'Ş' => 's',
            'ü' | 'Ü' => 'u',
            c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
            _ => '_',
        };
        if mapped == '_' && (out.is_empty() || out.ends_with('_')) {
            continue;
        }
        out.push(mapped);
    }
    let mut out = out.trim_end_matches('_').to_owned();
    if out.is_empty() {
        out.push_str("attr");
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'a');
    }
    out.truncate(MAX_KEY_LEN);
    out.trim_end_matches('_').to_owned()
}

/// Trims and collapses whitespace; `None` if empty or longer than [`MAX_LABEL_LEN`].
pub(crate) fn normalize_label(raw: &str) -> Option<String> {
    let s = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!s.is_empty() && s.chars().count() <= MAX_LABEL_LEN).then_some(s)
}

/// Validates a whole definition: key, name, options vs. kind, unique option ids and labels,
/// default value.
pub fn validate_attribute_def(def: &AttributeDef) -> CoreResult<()> {
    let fail = |reason| Err(CoreError::InvalidAttributeDef { reason });
    validate_attribute_key(&def.key)?;
    if normalize_label(&def.name).is_none() {
        return fail(AttributeDefProblem::InvalidName);
    }
    if def.kind.has_options() {
        if def.options.is_empty() {
            return fail(AttributeDefProblem::NoOptions);
        }
    } else if !def.options.is_empty() {
        return fail(AttributeDefProblem::OptionsNotAllowed);
    }
    let mut ids = HashSet::new();
    let mut labels = HashSet::new();
    for o in &def.options {
        if !ids.insert(o.id) {
            return fail(AttributeDefProblem::DuplicateOptionId);
        }
        let Some(label) = normalize_label(&o.label) else {
            return fail(AttributeDefProblem::InvalidOptionLabel);
        };
        if !labels.insert(label.to_lowercase()) {
            return fail(AttributeDefProblem::DuplicateOptionLabel(label));
        }
    }
    if let Some(d) = &def.default_value {
        if validate_attribute_value(def, d).is_err() {
            return fail(AttributeDefProblem::InvalidDefault);
        }
    }
    Ok(())
}

/// Validates `value` against `def` and returns its normalized form (text trimmed,
/// multi-choices sorted by option order).
pub fn validate_attribute_value(
    def: &AttributeDef,
    value: &AttributeValue,
) -> CoreResult<AttributeValue> {
    let fail = |reason| {
        Err(CoreError::InvalidAttributeValue {
            key: def.key.clone(),
            reason,
        })
    };
    let unknown = |id: &crate::id::AttributeOptionId| {
        Err(CoreError::UnknownAttributeOption {
            key: def.key.clone(),
            option: id.to_string(),
        })
    };
    match (def.kind, value) {
        (AttributeKind::OrderedEnum | AttributeKind::Enum, AttributeValue::Choice(id)) => {
            if def.option(*id).is_none() {
                return unknown(id);
            }
            Ok(value.clone())
        }
        (AttributeKind::MultiEnum, AttributeValue::Choices(ids)) => {
            if ids.is_empty() {
                return fail(AttributeValueProblem::BadSelection);
            }
            let mut seen = HashSet::new();
            for id in ids {
                if def.option(*id).is_none() {
                    return unknown(id);
                }
                if !seen.insert(*id) {
                    return fail(AttributeValueProblem::BadSelection);
                }
            }
            let sorted = def
                .sorted_options()
                .into_iter()
                .map(|o| o.id)
                .filter(|id| seen.contains(id))
                .collect();
            Ok(AttributeValue::Choices(sorted))
        }
        (AttributeKind::Number, AttributeValue::Number(n)) => {
            if n.is_finite() {
                Ok(value.clone())
            } else {
                fail(AttributeValueProblem::NotFinite)
            }
        }
        (AttributeKind::Text, AttributeValue::Text(s)) => {
            let t = s.trim();
            if t.is_empty() || t.chars().count() > MAX_TEXT_VALUE_LEN {
                return fail(AttributeValueProblem::BadText);
            }
            Ok(AttributeValue::Text(t.to_owned()))
        }
        (AttributeKind::Date, AttributeValue::Date(s)) => {
            if is_valid_date(s.trim()) {
                Ok(AttributeValue::Date(s.trim().to_owned()))
            } else {
                fail(AttributeValueProblem::BadDate)
            }
        }
        (AttributeKind::Bool, AttributeValue::Bool(_)) => Ok(value.clone()),
        _ => fail(AttributeValueProblem::KindMismatch),
    }
}

/// Strict `YYYY-MM-DD` proleptic Gregorian date check (years 0001–9999).
pub fn is_valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = s.get(r)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (Some(y), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    y >= 1 && (1..=days).contains(&d)
}
