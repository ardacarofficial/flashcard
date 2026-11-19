//! The card template language (docs/02 §3): a small, logic-less, Mustache-like syntax.
//!
//! | Syntax | Meaning |
//! |---|---|
//! | `{{field}}` | The field's rendered HTML |
//! | `{{text:field}}` | The field's plain text (HTML stripped) |
//! | `{{tts:field}}`, `{{tts:en:field}}` | Text to read aloud (optional BCP-47 language) |
//! | `{{type:field}}` | Typed-answer box; the field's text is the expected answer |
//! | `{{hint:field}}` | Collapsed hint |
//! | `{{cloze:field}}` | Cloze deletions of the field for the current card |
//! | `{{#field}}…{{/field}}` | Shown only if the field is non-empty |
//! | `{{^field}}…{{/field}}` | Shown only if the field is empty |
//! | `{{FrontSide}}`, `{{Deck}}`, `{{Tags}}`, `{{Card}}`, `{{Type}}` | Special values |
//!
//! Parsing and rendering are pure: [`parse`] builds an AST once, [`Template::render`] asks a
//! [`Resolver`] for values. Rendered output is **not** sanitized here; the caller sanitizes the
//! final HTML (see [`super::html::sanitize_html`]).

use std::fmt;

/// Special (non-field) names usable in templates.
pub const SPECIAL_NAMES: &[&str] = &["FrontSide", "Deck", "Tags", "Card", "Type"];

/// Filters understood by the built-in renderer.
pub const KNOWN_FILTERS: &[&str] = &["text", "tts", "type", "hint", "cloze"];

/// A template node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// Literal text/HTML.
    Text(String),
    /// A replacement: `{{filter:arg:name}}` → `filters = ["filter", "arg"]`, `name`.
    Field {
        /// Field (or special) name.
        name: String,
        /// Filter chain, outermost first (`tts:en` → `["tts", "en"]`).
        filters: Vec<String>,
    },
    /// A conditional section.
    Section {
        /// Field name tested for emptiness.
        name: String,
        /// `{{^name}}`: render when empty.
        inverted: bool,
        /// Body.
        body: Vec<Node>,
    },
}

/// A parsed template.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Template {
    /// Top-level nodes.
    pub nodes: Vec<Node>,
}

/// Why a template failed to parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// `{{` without a matching `}}`.
    UnterminatedTag {
        /// Byte offset of the `{{`.
        at: usize,
    },
    /// `{{}}` or a tag with an empty name.
    EmptyTag {
        /// Byte offset.
        at: usize,
    },
    /// `{{#x}}` / `{{^x}}` never closed.
    UnclosedSection {
        /// Section name.
        name: String,
    },
    /// `{{/x}}` without an open `x` (or closing the wrong section).
    UnexpectedClose {
        /// Name in the closing tag.
        name: String,
    },
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedTag { at } => write!(f, "unterminated '{{{{' at byte {at}"),
            Self::EmptyTag { at } => write!(f, "empty tag at byte {at}"),
            Self::UnclosedSection { name } => write!(f, "section '{name}' is never closed"),
            Self::UnexpectedClose { name } => write!(f, "unexpected closing tag '{name}'"),
        }
    }
}

/// Parses a template.
pub fn parse(src: &str) -> Result<Template, TemplateError> {
    // Stack of open sections: (name, inverted, nodes collected so far in the parent).
    let mut stack: Vec<(String, bool, Vec<Node>)> = Vec::new();
    let mut current: Vec<Node> = Vec::new();
    let mut rest = src;
    let mut offset = 0usize;
    while let Some(start) = rest.find("{{") {
        if start > 0 {
            push_text(&mut current, &rest[..start]);
        }
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(TemplateError::UnterminatedTag { at: offset + start });
        };
        let tag = after[..end].trim();
        let at = offset + start;
        let consumed = start + 2 + end + 2;
        if let Some(name) = tag.strip_prefix('#').or_else(|| tag.strip_prefix('^')) {
            let name = name.trim();
            if name.is_empty() {
                return Err(TemplateError::EmptyTag { at });
            }
            let inverted = tag.starts_with('^');
            stack.push((name.to_owned(), inverted, std::mem::take(&mut current)));
        } else if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            match stack.pop() {
                Some((open, inverted, parent)) if open == name => {
                    let body = std::mem::replace(&mut current, parent);
                    current.push(Node::Section {
                        name: open,
                        inverted,
                        body,
                    });
                }
                _ => {
                    return Err(TemplateError::UnexpectedClose {
                        name: name.to_owned(),
                    })
                }
            }
        } else {
            let mut parts: Vec<String> = tag.split(':').map(|p| p.trim().to_owned()).collect();
            let name = parts.pop().unwrap_or_default();
            if name.is_empty() {
                return Err(TemplateError::EmptyTag { at });
            }
            current.push(Node::Field {
                name,
                filters: parts,
            });
        }
        offset += consumed;
        rest = &rest[consumed..];
    }
    if !rest.is_empty() {
        push_text(&mut current, rest);
    }
    if let Some((name, _, _)) = stack.pop() {
        return Err(TemplateError::UnclosedSection { name });
    }
    Ok(Template { nodes: current })
}

fn push_text(nodes: &mut Vec<Node>, text: &str) {
    if let Some(Node::Text(prev)) = nodes.last_mut() {
        prev.push_str(text);
    } else {
        nodes.push(Node::Text(text.to_owned()));
    }
}

/// A field reference found in a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRef {
    /// Name.
    pub name: String,
    /// Filters (empty for sections).
    pub filters: Vec<String>,
    /// Whether the reference is a section (`{{#x}}` / `{{^x}}`).
    pub section: bool,
}

/// Supplies values while rendering.
pub trait Resolver {
    /// Whether `name` has visible content (drives sections).
    fn is_nonempty(&self, name: &str) -> bool;
    /// The rendered replacement for `{{filters:name}}`.
    fn resolve(&self, name: &str, filters: &[String]) -> String;
}

impl Template {
    /// Renders the template.
    pub fn render(&self, r: &dyn Resolver) -> String {
        let mut out = String::new();
        render_nodes(&self.nodes, r, &mut out);
        out
    }

    /// Every field reference in document order (specials included).
    pub fn field_refs(&self) -> Vec<FieldRef> {
        let mut out = Vec::new();
        collect_refs(&self.nodes, &mut out);
        out
    }
}

fn render_nodes(nodes: &[Node], r: &dyn Resolver, out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Field { name, filters } => out.push_str(&r.resolve(name, filters)),
            Node::Section {
                name,
                inverted,
                body,
            } => {
                if r.is_nonempty(name) != *inverted {
                    render_nodes(body, r, out);
                }
            }
        }
    }
}

fn collect_refs(nodes: &[Node], out: &mut Vec<FieldRef>) {
    for node in nodes {
        match node {
            Node::Text(_) => {}
            Node::Field { name, filters } => out.push(FieldRef {
                name: name.clone(),
                filters: filters.clone(),
                section: false,
            }),
            Node::Section { name, body, .. } => {
                out.push(FieldRef {
                    name: name.clone(),
                    filters: Vec::new(),
                    section: true,
                });
                collect_refs(body, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Map(BTreeMap<&'static str, &'static str>);

    impl Resolver for Map {
        fn is_nonempty(&self, name: &str) -> bool {
            self.0.get(name).is_some_and(|v| !v.is_empty())
        }
        fn resolve(&self, name: &str, filters: &[String]) -> String {
            let v = self.0.get(name).copied().unwrap_or("");
            if filters.is_empty() {
                v.to_owned()
            } else {
                format!("[{}|{v}]", filters.join(":"))
            }
        }
    }

    fn map(pairs: &[(&'static str, &'static str)]) -> Map {
        Map(pairs.iter().copied().collect())
    }

    #[test]
    fn renders_fields_filters_and_sections() {
        let t = parse("Q: {{ word }} {{tts:en:word}}{{#ex}}<i>{{ex}}</i>{{/ex}}{{^ex}}-{{/ex}}")
            .unwrap();
        assert_eq!(
            t.render(&map(&[("word", "run"), ("ex", "I run")])),
            "Q: run [tts:en|run]<i>I run</i>"
        );
        assert_eq!(t.render(&map(&[("word", "run")])), "Q: run [tts:en|run]-");
        let refs = t.field_refs();
        assert_eq!(refs.len(), 5);
        assert_eq!(refs[1].filters, vec!["tts".to_owned(), "en".to_owned()]);
        assert!(refs[2].section);
    }

    #[test]
    fn nested_sections() {
        let t = parse("{{#a}}A{{#b}}B{{/b}}{{/a}}").unwrap();
        assert_eq!(t.render(&map(&[("a", "1"), ("b", "1")])), "AB");
        assert_eq!(t.render(&map(&[("a", "1")])), "A");
        assert_eq!(t.render(&map(&[("b", "1")])), "");
    }

    #[test]
    fn plain_text_and_lone_braces() {
        let t = parse("a } b { c").unwrap();
        assert_eq!(t.render(&map(&[])), "a } b { c");
        assert_eq!(parse("").unwrap().nodes, vec![]);
    }

    #[test]
    fn errors() {
        assert_eq!(
            parse("x {{a"),
            Err(TemplateError::UnterminatedTag { at: 2 })
        );
        assert_eq!(parse("{{ }}"), Err(TemplateError::EmptyTag { at: 0 }));
        assert_eq!(parse("{{tts:}}"), Err(TemplateError::EmptyTag { at: 0 }));
        assert_eq!(
            parse("{{#a}}x"),
            Err(TemplateError::UnclosedSection { name: "a".into() })
        );
        assert_eq!(
            parse("{{#a}}{{/b}}"),
            Err(TemplateError::UnexpectedClose { name: "b".into() })
        );
        assert_eq!(
            parse("{{/a}}"),
            Err(TemplateError::UnexpectedClose { name: "a".into() })
        );
        assert!(TemplateError::UnclosedSection { name: "a".into() }
            .to_string()
            .contains("'a'"));
    }
}
