//! Template validation and key assignment for the note type editor.

use super::template::{parse, KNOWN_FILTERS, SPECIAL_NAMES};
use crate::error::{CoreError, CoreResult};
use crate::notetype::{FieldKey, NoteType, NoteTypeKind};
use std::collections::HashSet;

fn bad(reason: String) -> CoreError {
    CoreError::InvalidNoteType { reason }
}

/// Checks every template of `nt`: both sides parse, references name existing fields (or
/// specials), filters are known, `{{FrontSide}}` only on the back, cloze note types use
/// `{{cloze:…}}` on the front, and every template names a card type engine.
pub fn validate_templates(nt: &NoteType) -> CoreResult<()> {
    for t in &nt.templates {
        if t.card_type.trim().is_empty() {
            return Err(bad(format!("template {}: no card type", t.key)));
        }
        let mut front_has_cloze = false;
        for (side, src) in [("front", &t.front), ("back", &t.back)] {
            let tpl = parse(src).map_err(|e| bad(format!("template {} {side}: {e}", t.key)))?;
            for r in tpl.field_refs() {
                let special = SPECIAL_NAMES.contains(&r.name.as_str());
                if !special && nt.field(&FieldKey::new(r.name.clone())).is_none() {
                    return Err(bad(format!(
                        "template {} {side}: unknown field '{}'",
                        t.key, r.name
                    )));
                }
                if r.name == "FrontSide" && side == "front" {
                    return Err(bad(format!(
                        "template {}: {{{{FrontSide}}}} on the front",
                        t.key
                    )));
                }
                if let Some(first) = r.filters.first() {
                    if !KNOWN_FILTERS.contains(&first.as_str()) {
                        return Err(bad(format!(
                            "template {} {side}: unknown filter '{first}'",
                            t.key
                        )));
                    }
                    if first == "cloze" && side == "front" {
                        front_has_cloze = true;
                    }
                }
            }
        }
        if nt.kind == NoteTypeKind::Cloze && !front_has_cloze {
            return Err(bad(format!(
                "template {}: a cloze note type needs {{{{cloze:field}}}} on the front",
                t.key
            )));
        }
    }
    if nt.kind == NoteTypeKind::Cloze && nt.templates.len() != 1 {
        return Err(bad("a cloze note type has exactly one template".into()));
    }
    Ok(())
}

/// A key derived from a display name: lowercase ASCII letters/digits and `_` (Turkish and
/// other Latin letters folded), never empty.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        let mapped = match c {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' | 'î' | 'ï' | 'í' | 'ì' => 'i',
            'ö' | 'ó' | 'ò' | 'ô' => 'o',
            'ş' => 's',
            'ü' | 'ú' | 'ù' | 'û' => 'u',
            'â' | 'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            c if c.is_ascii_alphanumeric() => c,
            _ => '_',
        };
        if mapped == '_' && (out.is_empty() || out.ends_with('_')) {
            continue;
        }
        out.push(mapped);
    }
    let out = out.trim_end_matches('_').to_owned();
    if out.is_empty() {
        "field".to_owned()
    } else {
        out
    }
}

/// `base`, or `base_2`, `base_3`… — the first one not in `taken`.
pub fn unique_key(base: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|k| !taken.contains(k))
        .unwrap_or_else(|| base.to_owned())
}

/// Gives fields and templates with an empty key a fresh one derived from their name (the
/// editor sends new fields without keys; existing keys are never changed).
pub fn assign_missing_keys(nt: &mut NoteType) {
    let mut taken: HashSet<String> = nt
        .fields
        .iter()
        .filter(|f| !f.key.0.trim().is_empty())
        .map(|f| f.key.0.clone())
        .collect();
    for f in &mut nt.fields {
        if f.key.0.trim().is_empty() {
            let k = unique_key(&slug(&f.name), &taken);
            taken.insert(k.clone());
            f.key = FieldKey::new(k);
        }
    }
    let mut taken: HashSet<String> = nt
        .templates
        .iter()
        .filter(|t| !t.key.trim().is_empty())
        .map(|t| t.key.clone())
        .collect();
    for t in &mut nt.templates {
        if t.key.trim().is_empty() {
            let k = unique_key(&slug(&t.name), &taken);
            taken.insert(k.clone());
            t.key = k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::builtin;
    use crate::i18n::Locale;
    use crate::notetype::{FieldDef, FieldKind};
    use crate::time::Timestamp;

    #[test]
    fn template_errors_are_caught() {
        let base = builtin::classic(Timestamp(1), Locale::En);
        validate_templates(&base).unwrap();
        let check = |front: &str, back: &str| {
            let mut nt = base.clone();
            nt.templates[0].front = front.into();
            nt.templates[0].back = back.into();
            validate_templates(&nt)
        };
        assert!(check("{{nope}}", "").is_err());
        assert!(check("{{FrontSide}}", "").is_err());
        assert!(check("{{bogus:front}}", "").is_err());
        assert!(check("{{#front}}", "").is_err());
        assert!(check(
            "{{front}}",
            "{{FrontSide}} {{Deck}} {{tts:en:back}} {{type:back}}"
        )
        .is_ok());
        let mut no_engine = base.clone();
        no_engine.templates[0].card_type = " ".into();
        assert!(validate_templates(&no_engine).is_err());

        let mut cl = builtin::cloze(Timestamp(1), Locale::En);
        validate_templates(&cl).unwrap();
        cl.templates[0].front = "{{text}}".into();
        assert!(validate_templates(&cl).is_err());
    }

    #[test]
    fn keys_are_assigned_from_names() {
        assert_eq!(slug("Örnek Cümle!"), "ornek_cumle");
        assert_eq!(slug("  ---  "), "field");
        assert_eq!(slug("Çağrı 2"), "cagri_2");
        let mut nt = builtin::classic(Timestamp(1), Locale::En);
        nt.fields.push(FieldDef {
            key: FieldKey::new(""),
            name: "Front".into(),
            kind: FieldKind::Text,
            required: false,
            unique: false,
            searchable: true,
            default_value: None,
        });
        nt.templates.push(nt.templates[0].clone());
        nt.templates[1].key = String::new();
        nt.templates[1].name = "Reverse".into();
        assign_missing_keys(&mut nt);
        assert_eq!(nt.fields[2].key.as_str(), "front_2");
        assert_eq!(nt.templates[1].key, "reverse");
        assert_eq!(nt.fields[0].key.as_str(), "front");
    }
}
