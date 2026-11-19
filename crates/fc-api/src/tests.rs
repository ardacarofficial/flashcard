use crate::method::{parse_qualified_name, MethodKind};
use crate::v1;
use std::collections::HashSet;

const MODULES: &[&str] = &["core", "decks", "notetypes", "notes", "tags", "attributes"];

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
        assert!(
            MODULES.contains(&m.module),
            "{}: unknown module {}",
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
