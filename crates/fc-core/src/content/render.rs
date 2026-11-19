//! Rendering note fields and template sides to (unsanitized) HTML. Used by the built-in card
//! type engines; the app sanitizes the final output with [`super::html::sanitize_html`].

use super::cloze::{cloze_answer, render_cloze};
use super::html::{escape_html, markdown_to_html};
use super::refs::{is_media_hash, link_key, render_links, MEDIA_SCHEME};
use super::template::{parse, Resolver, Template, TemplateError};
use crate::note::{strip_html, FieldValues};
use crate::notetype::{FieldKey, FieldKind, NoteType};
use crate::tag::TagName;
use std::cell::RefCell;

/// Resolves a normalized link key ([`link_key`]) to a note id string.
pub type LinkResolver<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Cloze context of a field render: `(active ordinal, reveal)`.
pub type ClozeCtx = Option<(u32, bool)>;

/// Renders one field value to HTML according to its kind.
///
/// * rich text: cloze deletions → `[[links]]` → Markdown;
/// * text / number / tag list: escaped, line breaks kept, `[[links]]` resolved (text only);
/// * link: the value is a link target;
/// * image / audio / video: whitespace-separated `media:<sha256>` references become
///   `<img>` / `<audio controls>` / `<video controls>`; other words are shown as text.
pub fn render_field(
    kind: FieldKind,
    raw: &str,
    cloze: ClozeCtx,
    links: LinkResolver<'_>,
) -> String {
    let (active, reveal) = cloze.map_or((None, false), |(a, r)| (Some(a), r));
    match kind {
        FieldKind::RichText => {
            let text = render_cloze(raw, active, reveal);
            markdown_to_html(&render_links(&text, links))
        }
        FieldKind::Text => {
            // Escape around the links, not inside them (render_links escapes its labels);
            // cloze markers survive escaping and become markup afterwards.
            let linked = render_links_escaped(raw, links);
            render_cloze(&linked, active, reveal).replace('\n', "<br>")
        }
        FieldKind::Number | FieldKind::TagList => escape_html(raw),
        FieldKind::Link => {
            let t = raw.trim();
            if t.is_empty() {
                String::new()
            } else {
                render_links(&format!("[[{}]]", t.replace(['[', ']'], "")), links)
            }
        }
        FieldKind::Image | FieldKind::Audio | FieldKind::Video => raw
            .split_whitespace()
            .map(|token| match token.strip_prefix(MEDIA_SCHEME) {
                Some(hash) if is_media_hash(hash) => match kind {
                    FieldKind::Image => format!("<img src=\"media:{hash}\" alt=\"\">"),
                    FieldKind::Audio => {
                        format!("<audio controls src=\"media:{hash}\"></audio>")
                    }
                    _ => format!("<video controls src=\"media:{hash}\"></video>"),
                },
                _ => escape_html(token),
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn render_links_escaped(text: &str, links: LinkResolver<'_>) -> String {
    // Split on link boundaries by rendering with markers, escaping only the plain parts.
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let Some(len) = rest[start..].find("]]") else {
            break;
        };
        let end = start + len + 2;
        out.push_str(&escape_html(&rest[..start]));
        let rendered = render_links(&rest[start..end], links);
        if rendered == rest[start..end] {
            out.push_str(&escape_html(&rendered));
        } else {
            out.push_str(&rendered);
        }
        rest = &rest[end..];
    }
    out.push_str(&escape_html(rest));
    out
}

/// A `{{tts:…}}` request found while rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsSpan {
    /// BCP-47 language (empty = unspecified).
    pub lang: String,
    /// Plain text to speak.
    pub text: String,
}

/// Inputs of one side render.
pub struct RenderEnv<'a> {
    /// Note type (field kinds, name).
    pub notetype: &'a NoteType,
    /// Note field values.
    pub fields: &'a FieldValues,
    /// Note tags.
    pub tags: &'a [TagName],
    /// Deck path of the card.
    pub deck: &'a str,
    /// Template/card name (`{{Card}}`).
    pub card_name: &'a str,
    /// Active cloze ordinal (cloze cards).
    pub cloze: Option<u32>,
    /// Rendering the answer side.
    pub reveal: bool,
    /// Rendered question HTML (`{{FrontSide}}` on the answer side).
    pub front_side: Option<&'a str>,
    /// `[[link]]` resolution.
    pub links: LinkResolver<'a>,
}

/// Output of one side render.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SideOutput {
    /// HTML (unsanitized).
    pub html: String,
    /// TTS requests in order.
    pub tts: Vec<TtsSpan>,
    /// Expected typed answer (`{{type:field}}`), plain text.
    pub type_answer: Option<String>,
}

struct SideResolver<'a, 'b> {
    env: &'b RenderEnv<'a>,
    tts: RefCell<Vec<TtsSpan>>,
    type_answer: RefCell<Option<String>>,
}

impl SideResolver<'_, '_> {
    fn raw(&self, name: &str) -> Option<(&str, FieldKind)> {
        let key = FieldKey::new(name);
        let def = self.env.notetype.field(&key)?;
        Some((
            self.env.fields.get(&key).map_or("", String::as_str),
            def.kind,
        ))
    }

    fn special(&self, name: &str) -> Option<String> {
        Some(match name {
            "FrontSide" => self.env.front_side.unwrap_or("").to_owned(),
            "Deck" => escape_html(self.env.deck),
            "Tags" => escape_html(
                &self
                    .env
                    .tags
                    .iter()
                    .map(TagName::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            "Card" => escape_html(self.env.card_name),
            "Type" => escape_html(&self.env.notetype.name),
            _ => return None,
        })
    }

    /// Visible text of a field (Markdown rendered for rich text, cloze answers shown).
    fn plain(raw: &str, kind: FieldKind) -> String {
        let text = render_cloze(raw, None, false);
        match kind {
            FieldKind::RichText => strip_html(&markdown_to_html(&text)),
            _ => strip_html(&text),
        }
    }
}

impl Resolver for SideResolver<'_, '_> {
    fn is_nonempty(&self, name: &str) -> bool {
        if let Some(v) = self.special(name) {
            return !v.is_empty();
        }
        self.raw(name)
            .is_some_and(|(raw, _)| !strip_html(raw).is_empty() || raw.contains("<img"))
    }

    fn resolve(&self, name: &str, filters: &[String]) -> String {
        if let Some(v) = self.special(name) {
            return v;
        }
        let Some((raw, kind)) = self.raw(name) else {
            return String::new();
        };
        let env = self.env;
        match filters.first().map(String::as_str) {
            None => render_field(kind, raw, None, env.links),
            Some("text") => escape_html(&Self::plain(raw, kind)),
            Some("cloze") => render_field(kind, raw, env.cloze.map(|c| (c, env.reveal)), env.links),
            Some("tts") => {
                let lang = if filters.len() > 1 {
                    filters[1].clone()
                } else {
                    String::new()
                };
                let text = Self::plain(raw, kind);
                if text.is_empty() {
                    return String::new();
                }
                self.tts.borrow_mut().push(TtsSpan {
                    lang: lang.clone(),
                    text: text.clone(),
                });
                format!(
                    "<span class=\"fc-tts\" data-lang=\"{}\">{}</span>",
                    escape_html(&lang),
                    escape_html(&text)
                )
            }
            Some("type") => {
                let expected = match (filters.get(1).map(String::as_str), env.cloze) {
                    (Some("cloze"), Some(active)) => strip_html(&cloze_answer(raw, active)),
                    _ => Self::plain(raw, kind),
                };
                *self.type_answer.borrow_mut() = Some(expected);
                "<span class=\"fc-type-answer\"></span>".to_owned()
            }
            Some("hint") => {
                let html = render_field(kind, raw, None, env.links);
                if strip_html(&html).is_empty() && !html.contains("<img") {
                    String::new()
                } else {
                    format!("<details class=\"fc-hint\"><summary></summary>{html}</details>")
                }
            }
            Some(_) => render_field(kind, raw, None, env.links),
        }
    }
}

/// Renders a parsed template side.
pub fn render_side(template: &Template, env: &RenderEnv<'_>) -> SideOutput {
    let r = SideResolver {
        env,
        tts: RefCell::new(Vec::new()),
        type_answer: RefCell::new(None),
    };
    let html = template.render(&r);
    SideOutput {
        html,
        tts: r.tts.into_inner(),
        type_answer: r.type_answer.into_inner(),
    }
}

/// Parses and renders a template side.
pub fn render_template(src: &str, env: &RenderEnv<'_>) -> Result<SideOutput, TemplateError> {
    Ok(render_side(&parse(src)?, env))
}

/// Whether rendered HTML has visible content (text, image, audio or video): a card whose
/// question is empty is not generated (this is how "optional reverse" works).
pub fn has_visible_content(html: &str) -> bool {
    !strip_html(html).is_empty()
        || ["<img", "<audio", "<video"]
            .iter()
            .any(|t| html.contains(t))
}

/// Normalized link key of a note: its sort field ([`link_key`]).
pub fn note_link_key(sort_field: &str) -> String {
    link_key(sort_field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notetype::builtin;
    use crate::time::Timestamp;

    const H: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    fn fv(pairs: &[(&str, &str)]) -> FieldValues {
        pairs
            .iter()
            .map(|(k, v)| (FieldKey::new(*k), (*v).to_owned()))
            .collect()
    }

    fn no_links(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn field_kinds() {
        assert_eq!(
            render_field(FieldKind::RichText, "**a** [[B]]", None, &|k| {
                (k == "b").then(|| "ID".into())
            }),
            "<strong>a</strong> <a class=\"fc-link\" href=\"note:ID\">B</a>"
        );
        assert_eq!(
            render_field(FieldKind::Text, "<b>x</b>\n[[y & z]]", None, &no_links),
            "&lt;b&gt;x&lt;/b&gt;<br><span class=\"fc-link fc-link-missing\">y &amp; z</span>"
        );
        assert_eq!(
            render_field(
                FieldKind::Image,
                &format!("media:{H} junk<"),
                None,
                &no_links
            ),
            format!("<img src=\"media:{H}\" alt=\"\"> junk&lt;")
        );
        assert_eq!(
            render_field(FieldKind::Audio, &format!("media:{H}"), None, &no_links),
            format!("<audio controls src=\"media:{H}\"></audio>")
        );
        assert_eq!(
            render_field(FieldKind::Number, "<1>", None, &no_links),
            "&lt;1&gt;"
        );
        assert_eq!(
            render_field(FieldKind::Link, "Kedi", None, &no_links),
            "<span class=\"fc-link fc-link-missing\">Kedi</span>"
        );
    }

    #[test]
    fn sides_with_filters_and_specials() {
        let nt = builtin::classic(Timestamp(1), "Classic", "Front", "Back");
        let fields = fv(&[("front", "dog"), ("back", "*köpek*")]);
        let tags = vec![TagName::parse("animals").unwrap()];
        let env = RenderEnv {
            notetype: &nt,
            fields: &fields,
            tags: &tags,
            deck: "English",
            card_name: "1",
            cloze: None,
            reveal: false,
            front_side: None,
            links: &no_links,
        };
        let q = render_template(
            "{{front}} {{tts:en:front}} {{type:back}} {{hint:back}} {{Deck}}/{{Tags}}/{{Type}}{{^back}}x{{/back}}",
            &env,
        )
        .unwrap();
        assert_eq!(
            q.html,
            "dog <span class=\"fc-tts\" data-lang=\"en\">dog</span> <span class=\"fc-type-answer\"></span> \
             <details class=\"fc-hint\"><summary></summary><em>köpek</em></details> English/animals/Classic"
        );
        assert_eq!(
            q.tts,
            vec![TtsSpan {
                lang: "en".into(),
                text: "dog".into()
            }]
        );
        assert_eq!(q.type_answer.as_deref(), Some("köpek"));

        let back_env = RenderEnv {
            reveal: true,
            front_side: Some(&q.html),
            ..env
        };
        let a = render_template("{{FrontSide}}<hr>{{back}}{{missing}}", &back_env).unwrap();
        assert!(a.html.starts_with("dog <span"));
        assert!(a.html.ends_with("<hr><em>köpek</em>"));
        assert!(render_template("{{#x}}", &back_env).is_err());
    }

    #[test]
    fn cloze_sides() {
        let nt = crate::content::builtin::cloze(Timestamp(1), crate::i18n::Locale::En);
        let fields = fv(&[
            ("text", "{{c1::Ankara}} is in {{c2::Türkiye::country}}"),
            ("extra", ""),
        ]);
        let mk = |reveal| RenderEnv {
            notetype: &nt,
            fields: &fields,
            tags: &[],
            deck: "",
            card_name: "c2",
            cloze: Some(2),
            reveal,
            front_side: None,
            links: &no_links,
        };
        let q = render_template("{{cloze:text}}{{type:cloze:text}}", &mk(false)).unwrap();
        assert_eq!(
            q.html,
            "Ankara is in <span class=\"cloze\">[country]</span><span class=\"fc-type-answer\"></span>"
        );
        assert_eq!(q.type_answer.as_deref(), Some("Türkiye"));
        let a = render_template("{{cloze:text}}", &mk(true)).unwrap();
        assert_eq!(a.html, "Ankara is in <span class=\"cloze\">Türkiye</span>");
        let plain = render_template("{{text}} / {{text:text}}", &mk(false)).unwrap();
        assert_eq!(plain.html, "Ankara is in Türkiye / Ankara is in Türkiye");
    }

    #[test]
    fn visible_content() {
        assert!(!has_visible_content(" <br> <div></div>"));
        assert!(has_visible_content("<img src=\"media:x\">"));
        assert!(has_visible_content("a"));
        assert_eq!(note_link_key("Big  Dog"), "big dog");
    }
}
