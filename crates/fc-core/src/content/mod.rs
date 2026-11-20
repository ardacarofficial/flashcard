//! Card content (v0.3, docs/02 §3–§4, §14): the template language, cloze deletions, Markdown
//! and HTML sanitizing, `[[links]]` and media references, field/side rendering, template
//! validation and the built-in note types. Everything here is pure.

pub mod builtin;
pub mod cloze;
pub mod html;
pub mod media;
pub mod refs;
pub mod render;
pub mod template;
pub mod validate;

pub use html::{markdown_to_html, sanitize_css, sanitize_html};
pub use refs::{extract_links, extract_media, link_key, LinkRef};
pub use validate::validate_templates;
