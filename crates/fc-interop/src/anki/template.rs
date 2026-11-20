//! Field references in card templates: `{{Front}}`, `{{#Back}}…{{/Back}}`, `{{^Extra}}`,
//! `{{cloze:Text}}`, `{{type:cloze:Text}}`, `{{hint:Back}}`. Anki refers to fields by
//! **name**, the collection by **key**; import/export translate with [`map_field_refs`].

/// Names that are not fields.
const SPECIAL: &[&str] = &[
    "FrontSide",
    "Tags",
    "Type",
    "Deck",
    "Subdeck",
    "Card",
    "CardFlag",
    "CardID",
];

/// Rewrites every field reference in `template` with `f(name)` (`None` = keep).
pub fn map_field_refs(template: &str, f: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start + 2..].find("}}") else {
            break;
        };
        out.push_str(&rest[..start + 2]);
        let inner = &rest[start + 2..start + 2 + len];
        out.push_str(&map_tag(inner, &f));
        out.push_str("}}");
        rest = &rest[start + 2 + len + 2..];
    }
    out.push_str(rest);
    out
}

fn map_tag(inner: &str, f: &impl Fn(&str) -> Option<String>) -> String {
    let lead_ws = &inner[..inner.len() - inner.trim_start().len()];
    let trail_ws = &inner[inner.trim_end().len()..];
    let body = inner.trim();
    if body.contains("::") {
        // `{{c1::text}}`: a cloze deletion, not a field reference.
        return inner.to_owned();
    }
    let (sigil, body) = match body.chars().next() {
        Some(c @ ('#' | '^' | '/')) => {
            let after = &body[1..];
            let name = after.trim_start();
            (format!("{c}{}", &after[..after.len() - name.len()]), name)
        }
        _ => (String::new(), body),
    };
    let (filters, name) = match body.rfind(':') {
        Some(i) => (&body[..=i], &body[i + 1..]),
        None => ("", body),
    };
    if name.is_empty() || SPECIAL.contains(&name) || is_cloze_number(name) {
        return inner.to_owned();
    }
    match f(name) {
        Some(new) => format!("{lead_ws}{sigil}{filters}{new}{trail_ws}"),
        None => inner.to_owned(),
    }
}

fn is_cloze_number(name: &str) -> bool {
    name.strip_prefix('c')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_field_names_only() {
        let t = "{{Front}} {{#Back}}x{{/Back}} {{^ Extra }} {{cloze:Text}} {{type:cloze:Text}} {{FrontSide}} {{Tags}} {{c1::y}} {{hint:Back}}";
        let out = map_field_refs(t, |n| Some(n.to_lowercase()));
        assert_eq!(
            out,
            "{{front}} {{#back}}x{{/back}} {{^ extra }} {{cloze:text}} {{type:cloze:text}} {{FrontSide}} {{Tags}} {{c1::y}} {{hint:back}}"
        );
        assert_eq!(map_field_refs("{{open", |_| Some("x".into())), "{{open");
        let back = map_field_refs(&out, |n| Some(n.to_uppercase()));
        assert!(back.contains("{{FRONT}}"));
    }
}
