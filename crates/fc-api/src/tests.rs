use crate::method::{parse_qualified_name, MethodKind};
use crate::v1;
use std::collections::HashSet;

#[test]
fn catalog_names_are_unique_and_well_formed() {
    let cat = v1::catalog();
    let mut seen = HashSet::new();
    for m in &cat {
        assert!(seen.insert(m.name), "duplicate method {}", m.name);
        let parts: Vec<_> = m.name.split('.').collect();
        assert!(
            (2..=3).contains(&parts.len()),
            "{}: use area.verb or area.sub.verb",
            m.name
        );
        for p in parts {
            assert!(
                !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}: segments must be snake_case",
                m.name
            );
        }
        // That the module exists is checked against the real registry in fc-app
        // (`modules_are_consistent`), so tracks adding a module touch no
        // list here.
        assert!(
            !m.module.is_empty() && m.module.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{}: module id must be snake_case, got {:?}",
            m.name,
            m.module
        );
        assert!(
            m.permission.ends_with(":read") || m.permission.ends_with(":write"),
            "{}",
            m.name
        );
        if m.kind == MethodKind::Query {
            assert!(!m.undoable, "{}: queries are never undoable", m.name);
            assert!(
                m.permission.ends_with(":read"),
                "{}: queries need :read",
                m.name
            );
        } else {
            assert!(
                m.permission.ends_with(":write"),
                "{}: commands need :write",
                m.name
            );
        }
        assert_eq!(parse_qualified_name(&m.qualified_name()), Some(m.name));
    }
    assert_eq!(parse_qualified_name("decks.create"), None);
    assert_eq!(parse_qualified_name("v2.decks.create"), None);
}

#[test]
fn request_and_response_types_are_named_for_ts() {
    let cfg = ts_rs::Config::new().with_large_int("number");
    for m in v1::catalog() {
        for name in [(m.request_ts)(&cfg), (m.response_ts)(&cfg)] {
            assert!(
                name.chars().all(|c| c.is_ascii_alphanumeric()),
                "{}: request/response must be a named, non-generic type (got {name})",
                m.name
            );
        }
    }
}

#[test]
fn v01_acceptance_methods_exist() {
    let names: HashSet<_> = v1::catalog().iter().map(|m| m.name).collect();
    for required in [
        "decks.create",
        "decks.rename",
        "decks.delete",
        "decks.tree",
        "notes.create",
        "notes.update",
        "notes.delete",
        "notes.list",
        "attributes.define",
        "attributes.define_from_template",
        "attributes.effective",
        "attributes.option.rename",
        "attributes.option.reorder",
        "oplog.undo",
        "oplog.redo",
        "modules.list",
        "settings.schema",
    ] {
        assert!(names.contains(required), "missing {required}");
    }
}

#[test]
fn generated_typescript_is_up_to_date() {
    let dir = crate::tsgen::workspace_root().join(crate::tsgen::DEFAULT_OUT_DIR);
    if let Err(e) = crate::tsgen::check(&dir) {
        panic!("{e}");
    }
}

/// Every error code has a user-facing message in the desktop dictionaries (TR and EN), so a
/// track adding codes is reminded to add its `errors.*` texts to its locale files.
#[test]
fn every_error_code_has_a_desktop_message() {
    use std::collections::BTreeSet;
    fn leaves(v: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, v) in m {
                    let p = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    leaves(v, &p, out);
                }
            }
            _ => {
                out.insert(prefix.to_owned());
            }
        }
    }
    let locales = crate::tsgen::workspace_root().join("apps/desktop/src/lib/i18n/locales");
    for lang in ["en", "tr"] {
        let mut keys = BTreeSet::new();
        for entry in std::fs::read_dir(locales.join(lang)).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            leaves(&serde_json::from_str(&text).unwrap(), "", &mut keys);
        }
        for code in crate::ErrorCode::ALL {
            assert!(
                keys.contains(code.message_key()),
                "locales/{lang}: no text for {} ({code})",
                code.message_key()
            );
        }
    }
}
