//! Keyboard shortcut combinations (docs/02 §13): parsing to a canonical form and conflict
//! detection. The UI only binds what the backend resolves.

use std::collections::BTreeMap;

/// Named keys accepted besides single letters/digits/punctuation (canonical spelling).
const NAMED: &[&str] = &[
    "Space",
    "Enter",
    "Escape",
    "Tab",
    "Backspace",
    "Delete",
    "Insert",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
];

/// Canonical form of a combination: modifiers in the order `Ctrl+Alt+Shift+Meta`, then one
/// key (letters upper case, named keys as in [`NAMED`]). Accepts any case, `-`/`+`
/// separators-insensitive aliases (`cmd`/`win` → `Meta`, `esc`, `del`, `up`…). `None` if the
/// text is not exactly one key with optional modifiers.
pub fn normalize_combo(raw: &str) -> Option<String> {
    let parts: Vec<&str> = raw
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    // `Ctrl++` → the key is `+`.
    let parts: Vec<&str> = if raw.trim_end().ends_with("++") || raw.trim() == "+" {
        let mut v = parts;
        v.push("+");
        v
    } else {
        parts
    };
    let (key, mods) = parts.split_last()?;
    let mut flags = [false; 4];
    for m in mods {
        let i = match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => 0,
            "alt" | "option" => 1,
            "shift" => 2,
            "meta" | "cmd" | "command" | "win" | "super" => 3,
            _ => return None,
        };
        if flags[i] {
            return None;
        }
        flags[i] = true;
    }
    let key = canonical_key(key)?;
    let names = ["Ctrl", "Alt", "Shift", "Meta"];
    let mut out: Vec<&str> = names
        .iter()
        .zip(flags)
        .filter(|(_, on)| *on)
        .map(|(n, _)| *n)
        .collect();
    out.push(&key);
    Some(out.join("+"))
}

fn canonical_key(k: &str) -> Option<String> {
    let lower = k.to_ascii_lowercase();
    let alias = match lower.as_str() {
        "esc" => Some("Escape"),
        "del" => Some("Delete"),
        "ins" => Some("Insert"),
        "return" => Some("Enter"),
        "up" => Some("ArrowUp"),
        "down" => Some("ArrowDown"),
        "left" => Some("ArrowLeft"),
        "right" => Some("ArrowRight"),
        "pgup" => Some("PageUp"),
        "pgdn" => Some("PageDown"),
        " " => Some("Space"),
        _ => None,
    };
    if let Some(a) = alias {
        return Some(a.to_owned());
    }
    if let Some(n) = NAMED.iter().find(|n| n.to_ascii_lowercase() == lower) {
        return Some((*n).to_owned());
    }
    let mut chars = k.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_whitespace() && !c.is_control() => {
            Some(c.to_uppercase().collect())
        }
        _ => None,
    }
}

/// Groups of actions bound to the same combination where the contexts overlap. `contexts`
/// gives each action's context; `global` is the context that overlaps every other.
pub fn find_conflicts<'a>(
    bindings: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    global: &str,
) -> Vec<(String, Vec<String>)> {
    let mut by_combo: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (action, context, combo) in bindings {
        by_combo.entry(combo).or_default().push((action, context));
    }
    let mut out = Vec::new();
    for (combo, actions) in by_combo {
        let clash: Vec<String> = actions
            .iter()
            .filter(|(a, c)| {
                actions
                    .iter()
                    .any(|(b, d)| a != b && (c == d || *c == global || *d == global))
            })
            .map(|(a, _)| (*a).to_owned())
            .collect();
        if !clash.is_empty() {
            out.push((combo.to_owned(), clash));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_combos() {
        assert_eq!(normalize_combo("ctrl+k").as_deref(), Some("Ctrl+K"));
        assert_eq!(
            normalize_combo("Shift + ctrl + z").as_deref(),
            Some("Ctrl+Shift+Z")
        );
        assert_eq!(normalize_combo("cmd+Enter").as_deref(), Some("Meta+Enter"));
        assert_eq!(normalize_combo("esc").as_deref(), Some("Escape"));
        assert_eq!(normalize_combo("space").as_deref(), Some("Space"));
        assert_eq!(normalize_combo("1").as_deref(), Some("1"));
        assert_eq!(normalize_combo("ctrl++").as_deref(), Some("Ctrl++"));
        assert_eq!(normalize_combo("f5").as_deref(), Some("F5"));
        assert_eq!(normalize_combo("ctrl+ctrl+k"), None);
        assert_eq!(normalize_combo("ctrl+"), None);
        assert_eq!(normalize_combo("hyper+k"), None);
        assert_eq!(normalize_combo("ab"), None);
        assert_eq!(normalize_combo(""), None);
    }

    #[test]
    fn conflicts_respect_contexts() {
        let c = find_conflicts(
            [
                ("a", "study", "Space"),
                ("b", "browse", "Space"),
                ("c", "global", "Ctrl+K"),
                ("d", "browse", "Ctrl+K"),
                ("e", "study", "1"),
                ("f", "study", "1"),
            ],
            "global",
        );
        assert_eq!(
            c,
            vec![
                ("1".to_owned(), vec!["e".to_owned(), "f".to_owned()]),
                ("Ctrl+K".to_owned(), vec!["c".to_owned(), "d".to_owned()]),
            ]
        );
    }
}
