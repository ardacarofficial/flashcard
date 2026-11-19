//! Markdown → HTML and HTML sanitizing (docs/03 §16: "card HTML is sanitized; templates cannot
//! run scripts").
//!
//! * [`markdown_to_html`]: CommonMark + tables, strikethrough, task lists and math. Math is
//!   emitted as `<span class="math math-inline|math-display">` and typeset by the frontend
//!   (KaTeX); code blocks keep `class="language-x"` for frontend highlighting.
//! * [`sanitize_html`]: allow-list sanitizer ([ammonia]). The **only** URL schemes that
//!   survive are `media:<sha256>` (images/audio/video from the media store), `note:<uuid>`
//!   (`[[links]]`), `https:`/`http:`/`mailto:` (links only). Classes are limited to the
//!   renderer's own (`fc-*`, `cloze`, `math*`, `language-*`), inline styles to a short list
//!   of layout/typography properties, and every event handler, script, frame, form and
//!   `style`/`link` element is dropped.
//! * [`sanitize_css`]: note type CSS minus anything that loads or executes (`@import`,
//!   `url(…)`, `expression(…)`, `javascript:`, `</style`), with escapes normalised and
//!   brackets balanced so it cannot break out of the frontend's scope block.

use pulldown_cmark::{html, Options, Parser};
use std::borrow::Cow;
use std::collections::HashSet;

/// Renders Markdown (with inline HTML) to unsanitized HTML.
pub fn markdown_to_html(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_MATH);
    let mut out = String::with_capacity(md.len() + md.len() / 2);
    html::push_html(&mut out, Parser::new_ext(md, opts));
    // A single paragraph renders inline (keeps short fields like "dog" free of <p> margins).
    let trimmed = out.trim_end();
    if let Some(inner) = trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
    {
        if !inner.contains("<p>") {
            return inner.to_owned();
        }
    }
    out
}

/// Escapes text for HTML element content and attribute values.
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

const TAGS: &[&str] = &[
    "a",
    "abbr",
    "audio",
    "b",
    "blockquote",
    "br",
    "caption",
    "center",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "font",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "input",
    "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "pre",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "small",
    "source",
    "span",
    "strike",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "u",
    "ul",
    "video",
];

const STYLE_PROPERTIES: &[&str] = &[
    "color",
    "background-color",
    "font-weight",
    "font-style",
    "font-size",
    "text-align",
    "text-decoration",
    "vertical-align",
    "left",
    "top",
    "width",
    "height",
];

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

fn allowed_class(token: &str) -> bool {
    let ident = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'+' | b'#'))
    };
    matches!(token, "cloze" | "math" | "math-inline" | "math-display")
        || token.strip_prefix("fc-").is_some_and(ident)
        || token.strip_prefix("language-").is_some_and(ident)
}

/// Keeps allowed declarations whose values are plain (no functions but colors, no escapes).
fn filter_style(style: &str) -> String {
    let mut out = Vec::new();
    for decl in style.split(';') {
        let Some((prop, value)) = decl.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_ascii_lowercase();
        let value = value.trim();
        let lower = value.to_ascii_lowercase();
        if !STYLE_PROPERTIES.contains(&prop.as_str()) || value.is_empty() {
            continue;
        }
        let plain_chars = lower
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b" %.#-,()".contains(&b));
        let functions_ok = !lower.contains('(')
            || ["rgb(", "rgba(", "hsl(", "hsla("]
                .iter()
                .any(|f| lower.starts_with(f) && lower.matches('(').count() == 1);
        if plain_chars && functions_ok {
            out.push(format!("{prop}:{value}"));
        }
    }
    out.join(";")
}

/// Decides what happens to one attribute value (after ammonia's own allow-lists).
fn filter_attribute<'u>(element: &str, attribute: &str, value: &'u str) -> Option<Cow<'u, str>> {
    match attribute {
        "src" => {
            // Media only: no remote loads, no data:/javascript:.
            let hash = value.trim().strip_prefix("media:")?;
            is_hex64(hash).then(|| Cow::Owned(format!("media:{hash}")))
        }
        "href" => {
            let v = value.trim();
            let lower = v.to_ascii_lowercase();
            if let Some(id) = v.strip_prefix("note:") {
                return is_uuid(id).then(|| Cow::Owned(format!("note:{id}")));
            }
            (lower.starts_with("https://")
                || lower.starts_with("http://")
                || lower.starts_with("mailto:"))
            .then_some(Cow::Borrowed(v))
        }
        "class" => {
            let kept: Vec<&str> = value
                .split_whitespace()
                .filter(|t| allowed_class(t))
                .collect();
            (!kept.is_empty()).then(|| Cow::Owned(kept.join(" ")))
        }
        "style" => {
            let kept = filter_style(value);
            (!kept.is_empty()).then_some(Cow::Owned(kept))
        }
        "type" if element == "input" => (value == "checkbox").then_some(Cow::Borrowed(value)),
        _ => Some(Cow::Borrowed(value)),
    }
}

/// Sanitizes HTML for display (see the module docs for the allow-list).
pub fn sanitize_html(input: &str) -> String {
    let tags: HashSet<&str> = TAGS.iter().copied().collect();
    let generic: HashSet<&str> = ["class", "title", "lang", "dir", "style"]
        .into_iter()
        .collect();
    let mut builder = ammonia::Builder::empty();
    builder
        .tags(tags)
        .generic_attributes(generic)
        .generic_attribute_prefixes(["data-"].into_iter().collect())
        .add_tag_attributes("a", ["href"])
        .add_tag_attributes("img", ["src", "alt", "width", "height"])
        .add_tag_attributes("audio", ["src", "controls", "loop"])
        .add_tag_attributes("video", ["src", "controls", "loop", "width", "height"])
        .add_tag_attributes("source", ["src", "type"])
        .add_tag_attributes("details", ["open"])
        .add_tag_attributes("ol", ["start"])
        .add_tag_attributes("td", ["colspan", "rowspan", "align"])
        .add_tag_attributes("th", ["colspan", "rowspan", "align"])
        .add_tag_attributes("font", ["color"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .url_schemes(
            ["http", "https", "mailto", "media", "note"]
                .into_iter()
                .collect(),
        )
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"))
        .strip_comments(true)
        .clean_content_tags(
            [
                "script", "style", "template", "noscript", "iframe", "object", "textarea",
            ]
            .into_iter()
            .collect(),
        )
        .attribute_filter(filter_attribute);
    builder.clean(input).to_string()
}

/// Removes everything from note type CSS that could load resources or run code, and repairs its
/// structure so it can be wrapped in a scope block without escaping it.
///
/// 1. Escapes are normalised first ([`normalize_css_escapes`]), so `\\75rl(` cannot slip past
///    the filter as `url(`.
/// 2. `@import`, `url(…)`, `image(…)`, `image-set(…)`, `expression(…)`, `javascript:`,
///    `behavior`/`-moz-binding` and every `<` are removed.
/// 3. Comments are dropped, unterminated strings closed, and `{}`/`()`/`[]` balanced: a stray
///    closer is dropped and missing closers are appended ([`balance_css`]). A single `}` in a
///    note type's CSS therefore can never close the frontend's scope block.
pub fn sanitize_css(css: &str) -> String {
    let mut current = normalize_css_escapes(css);
    // Removing a token (or dropping a stray closer in `balance_css`) can glue its neighbours
    // into a new forbidden token (`ur)l(`), so repeat until nothing changes. Never-stable
    // input (should be impossible) yields nothing rather than something unfiltered.
    for _ in 0..MAX_CSS_PASSES {
        let next = balance_css(&filter_css(&current));
        if next == current {
            return next;
        }
        current = next;
    }
    String::new()
}

const MAX_CSS_PASSES: usize = 16;

/// One filtering pass. Removed tokens are replaced by a space so the text around them cannot
/// fuse into a new forbidden token (`ur<l(` → `ur l(`); [`sanitize_css`] still iterates to a
/// fixpoint as a second line of defence.
fn filter_css(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let lower = css.to_ascii_lowercase();
    let mut i = 0;
    while i < css.len() {
        let rest = &lower[i..];
        if rest.starts_with("@import") {
            // Drop up to and including the next ';' (or the rest).
            i += rest.find(';').map_or(rest.len(), |p| p + 1);
            out.push(' ');
            continue;
        }
        if ["url(", "image(", "image-set(", "expression(", "src("]
            .iter()
            .any(|f| rest.starts_with(f))
        {
            i += rest.find(')').map_or(rest.len(), |p| p + 1);
            out.push_str(" none ");
            continue;
        }
        if rest.starts_with("javascript:") {
            i += "javascript:".len();
            out.push(' ');
            continue;
        }
        if rest.starts_with('<') {
            i += 1;
            out.push(' ');
            continue;
        }
        if rest.starts_with("behavior") || rest.starts_with("-moz-binding") {
            i += rest.find(';').map_or(rest.len(), |p| p + 1);
            out.push(' ');
            continue;
        }
        // Copy one UTF-8 character.
        let ch_len = css[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&css[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// Rewrites every CSS escape into a canonical form: escaped letters and non-ASCII characters
/// become the literal character (so keyword filters see them), everything else becomes
/// `\\<hex> ` (an escaped `}` or `(` stays an identifier character and can never act as
/// syntax). A backslash before a newline (string continuation) or at the end is removed.
fn normalize_css_escapes(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let decoded = match chars.peek().copied() {
            None | Some('\n' | '\r' | '\u{c}') => {
                if chars.next() == Some('\r') && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                continue;
            }
            Some(h) if h.is_ascii_hexdigit() => {
                let mut v: u32 = 0;
                for _ in 0..6 {
                    match chars.peek().and_then(|d| d.to_digit(16)) {
                        Some(d) => {
                            v = v * 16 + d;
                            chars.next();
                        }
                        None => break,
                    }
                }
                match chars.peek().copied() {
                    Some('\r') => {
                        chars.next();
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                    }
                    Some(' ' | '\t' | '\n' | '\u{c}') => {
                        chars.next();
                    }
                    _ => {}
                }
                match char::from_u32(v) {
                    Some(ch) if v != 0 => ch,
                    _ => char::REPLACEMENT_CHARACTER,
                }
            }
            Some(other) => {
                chars.next();
                other
            }
        };
        if decoded.is_ascii_alphabetic() || !decoded.is_ascii() {
            out.push(decoded);
        } else {
            out.push_str(&format!("\\{:x} ", u32::from(decoded)));
        }
    }
    out
}

/// Drops comments, closes unterminated strings and balances brackets (see [`sanitize_css`]).
/// Expects normalised escapes: a backslash is always followed by hex digits and a space.
fn balance_css(css: &str) -> String {
    let mut out = String::with_capacity(css.len() + 8);
    let mut stack: Vec<char> = Vec::new();
    let mut chars = css.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            match c {
                '\n' | '\r' | '\u{c}' => {
                    // A newline ends a CSS string (as a "bad string"); close it properly.
                    out.push(q);
                    out.push(c);
                    quote = None;
                }
                _ => {
                    out.push(c);
                    if c == q {
                        quote = None;
                    }
                }
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                out.push(c);
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for d in chars.by_ref() {
                    if prev == '*' && d == '/' {
                        break;
                    }
                    prev = d;
                }
                out.push(' ');
            }
            '{' | '(' | '[' => {
                stack.push(match c {
                    '{' => '}',
                    '(' => ')',
                    _ => ']',
                });
                out.push(c);
            }
            '}' | ')' | ']' => {
                if stack.last() == Some(&c) {
                    stack.pop();
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    if let Some(q) = quote {
        out.push(q);
    }
    while let Some(close) = stack.pop() {
        out.push(close);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn markdown_basics() {
        assert_eq!(markdown_to_html("**dog**"), "<strong>dog</strong>");
        let table = markdown_to_html("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(table.contains("<table>"), "{table}");
        assert!(markdown_to_html("$x^2$").contains("class=\"math math-inline\""));
        assert!(markdown_to_html("$$\\int x$$").contains("math-display"));
        let code = markdown_to_html("```rust\nfn main() {}\n```");
        assert!(code.contains("class=\"language-rust\""), "{code}");
        assert!(markdown_to_html("a\n\nb").contains("<p>a</p>"));
        assert_eq!(markdown_to_html("~~x~~"), "<del>x</del>");
    }

    #[test]
    fn keeps_renderer_markup() {
        let html = format!(
            "<span class=\"cloze evil\">x</span><img src=\"media:{HASH}\" alt=\"a\">\
             <a class=\"fc-link\" href=\"note:01900000-0000-7000-8000-000000000001\">n</a>\
             <span class=\"math math-inline\">x</span><pre><code class=\"language-rust\">f</code></pre>\
             <div class=\"fc-io-mask\" style=\"left: 10%; top: 5%; position: fixed\"></div>\
             <audio controls src=\"media:{HASH}\"></audio><details class=\"fc-hint\"><summary></summary>h</details>"
        );
        let out = sanitize_html(&html);
        assert!(out.contains("<span class=\"cloze\">x</span>"), "{out}");
        assert!(out.contains(&format!("src=\"media:{HASH}\"")), "{out}");
        assert!(out.contains("href=\"note:01900000-0000-7000-8000-000000000001\""));
        assert!(out.contains("class=\"math math-inline\""));
        assert!(out.contains("class=\"language-rust\""));
        assert!(out.contains("left:10%"), "{out}");
        assert!(!out.contains("position"), "{out}");
        assert!(out.contains("<audio controls=\"\""), "{out}");
        assert!(out.contains("<details class=\"fc-hint\">"));
    }

    /// XSS vectors (docs/04 v0.3 acceptance: "HTML sanitizing tests pass").
    #[test]
    fn xss_vectors_are_neutralized() {
        let vectors = [
            "<script>alert(1)</script>",
            "<SCRIPT SRC=//evil/x.js></SCRIPT>",
            "<img src=x onerror=alert(1)>",
            "<img src=\"javascript:alert(1)\">",
            "<img src=\"media:../../etc/passwd\" onload=alert(1)>",
            "<svg onload=alert(1)><circle/></svg>",
            "<svg><script>alert(1)</script></svg>",
            "<math><mtext><table><mglyph><style><img src=x onerror=alert(1)>",
            "<a href=\"javascript:alert(1)\">x</a>",
            "<a href=\"jAvAsCrIpT:alert(1)\">x</a>",
            "<a href=\"  javascript:alert(1)\">x</a>",
            "<a href=\"java&#x09;script:alert(1)\">x</a>",
            "<a href=\"data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==\">x</a>",
            "<a href=\"vbscript:msgbox(1)\">x</a>",
            "<a href=\"note:javascript:alert(1)\">x</a>",
            "<iframe src=\"https://evil\"></iframe>",
            "<object data=\"x.swf\"></object><embed src=x>",
            "<form action=\"javascript:alert(1)\"><button>go</button></form>",
            "<input onfocus=alert(1) autofocus>",
            "<details open ontoggle=alert(1)>",
            "<body onload=alert(1)>",
            "<div style=\"background:url(javascript:alert(1))\">x</div>",
            "<div style=\"width: expression(alert(1))\">x</div>",
            "<style>@import 'https://evil/x.css';</style>",
            "<link rel=stylesheet href=https://evil/x.css>",
            "<meta http-equiv=\"refresh\" content=\"0;url=javascript:alert(1)\">",
            "<base href=\"javascript:alert(1)//\">",
            "<video><source onerror=\"alert(1)\"></video>",
            "<audio src=x onerror=alert(1)>",
            "<img src=\"https://tracker.example/pixel.gif\">",
            "<a href=\"#\" onclick=\"alert(1)\">x</a>",
            "<noscript><p title=\"</noscript><img src=x onerror=alert(1)>\">",
            "<!--<img src=x onerror=alert(1)>-->",
            "<template><img src=x onerror=alert(1)></template>",
            "<textarea><img src=x onerror=alert(1)></textarea>",
            "\"><img src=x onerror=alert(1)>",
        ];
        for v in vectors {
            let out = sanitize_html(v).to_ascii_lowercase();
            for bad in [
                "<script",
                "onerror",
                "onload",
                "onclick",
                "onfocus",
                "ontoggle",
                "javascript:",
                "vbscript:",
                "data:",
                "<iframe",
                "<object",
                "<embed",
                "<form",
                "<style",
                "<link",
                "<meta",
                "<base",
                "<svg",
                "expression(",
                "url(",
                "https://tracker",
                "https://evil",
                "etc/passwd",
            ] {
                assert!(!out.contains(bad), "vector {v:?} left {bad:?} in {out:?}");
            }
        }
    }

    #[test]
    fn plain_links_survive_with_rel() {
        let out = sanitize_html("<a href=\"https://example.com\">x</a>");
        assert!(out.contains("href=\"https://example.com\""));
        assert!(out.contains("rel=\"noopener noreferrer\""));
        assert_eq!(
            sanitize_html("<a href=\"/relative\">x</a>"),
            "<a rel=\"noopener noreferrer\">x</a>"
        );
    }

    #[test]
    fn css_sanitizer() {
        let css = "@import 'https://x/y.css';\n.card { color: red; background: url(https://evil/p.png); }\n\
                   .x { width: expression(alert(1)); behavior: url(x.htc); } </style><script>";
        let out = sanitize_css(css);
        for bad in [
            "@import",
            "url(",
            "expression(",
            "behavior",
            "</style",
            "<script",
        ] {
            assert!(!out.to_ascii_lowercase().contains(bad), "{bad} in {out}");
        }
        assert!(out.contains("color: red"));
        assert_eq!(sanitize_css(".ç { color: blue }"), ".ç { color: blue }");
    }

    #[test]
    fn css_braces_cannot_escape_scope() {
        // A stray closer is dropped; an unclosed block is closed.
        assert_eq!(
            sanitize_css("} body { color: red }"),
            " body { color: red }"
        );
        assert_eq!(sanitize_css(".a { color: red"), ".a { color: red}");
        assert_eq!(
            sanitize_css(".a { b: rgb( } .c {}"),
            ".a { b: rgb(  .c {})}"
        );
        // Braces in strings and comments do not count; unterminated strings are closed.
        assert_eq!(
            sanitize_css(".a::after { content: \"}\" } /* } */"),
            ".a::after { content: \"}\" }  "
        );
        assert_eq!(
            sanitize_css(".a { content: \"x\n}"),
            ".a { content: \"x\"\n}"
        );
        assert_eq!(sanitize_css(".a { content: \"x"), ".a { content: \"x\"}");
        // Escaped braces stay escaped identifier characters.
        assert_eq!(sanitize_css(".a\\7d { }"), ".a\\7d { }");
        // Whatever goes in, wrapping the result keeps every rule inside the scope.
        for evil in ["}}}} * { x: y }", "{{{", "]) } :root { }", "\"}\n}", "/* }"] {
            let out = sanitize_css(evil);
            let wrapped = format!(".s {{{out}}}");
            assert_eq!(balance_css(&wrapped), wrapped, "{evil:?} -> {out:?}");
        }
    }

    #[test]
    fn css_escapes_are_normalised_before_filtering() {
        for evil in [
            ".a { background: \\75rl(https://evil/x.png) }",
            ".a { background: \\75 rl(https://evil/x.png) }",
            ".a { background: u\\rl(https://evil/x.png) }",
            ".a { background: \\000055RL(https://evil/x.png) }",
            "@\\69mport 'https://evil/x.css';",
            ".a { width: expr\\65ssion(alert(1)) }",
        ] {
            let out = sanitize_css(evil).to_ascii_lowercase();
            assert!(
                !out.contains("evil") && !out.contains("alert"),
                "{evil:?} -> {out:?}"
            );
        }
        // Harmless escapes survive in an equivalent form.
        assert_eq!(
            sanitize_css("q::before { content: \"\\201C\" }"),
            "q::before { content: \"“\" }"
        );
        assert_eq!(sanitize_css(".\\31 0 { }"), ".\\31 0 { }");
        assert_eq!(
            sanitize_css("a { content: \"\\\"\" }"),
            "a { content: \"\\22 \" }"
        );
        // `<` cannot be smuggled in as an escape.
        assert!(!sanitize_css("a { content: \"\\3c /style>\" }").contains('<'));
    }

    fn assert_css_clean(out: &str) {
        let l = out.to_ascii_lowercase();
        for bad in ["url(", "javascript:", "expression(", "@import", "image("] {
            assert!(!l.contains(bad), "{bad} survived in {out:?}");
        }
    }

    #[test]
    fn css_removal_cannot_reassemble_tokens() {
        for payload in [
            "a{background:ur<l(https://e/x)}",
            "a{background:u@import x;rl(https://e/x)}",
            "a{x:javasjavascript:cript:alert(1)}",
            "a{background:ur)l(https://e/x)}",
            "a{background:ur}l(https://e/x)}",
            "a{background:ur<<l(https://e/x)}",
            "a{background:UR<L(https://e/x)}",
        ] {
            assert_css_clean(&sanitize_css(payload));
        }
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        fn fragment() -> impl Strategy<Value = String> {
            prop_oneof![
                prop::sample::select(vec![
                    "url(",
                    "ur",
                    "l(",
                    "u",
                    "rl(",
                    "<",
                    "@import",
                    "@import x;",
                    ";",
                    ")",
                    "(",
                    "}",
                    "{",
                    "]",
                    "[",
                    "javascript:",
                    "javas",
                    "cript:",
                    "expression(",
                    "expr",
                    "ession(",
                    "/*",
                    "*/",
                    "\\75 ",
                    "\\6c ",
                    "\\",
                    "\"",
                    "'",
                    "image(",
                    "src(",
                    "behavior",
                    "-moz-binding",
                    "https://e/x",
                    "fcmedia:abc",
                    " ",
                ])
                .prop_map(String::from),
                "[a-zA-Z(){}<;:@/*\\\\ ]{0,6}",
            ]
        }

        proptest! {
            #[test]
            fn sanitized_css_never_has_forbidden_tokens(
                parts in prop::collection::vec(fragment(), 0..24),
                upper in any::<bool>(),
            ) {
                let mut input = parts.concat();
                if upper {
                    input = input.to_ascii_uppercase();
                }
                assert_css_clean(&sanitize_css(&input));
            }
        }
    }

    #[test]
    fn escape() {
        assert_eq!(
            escape_html("<a href=\"x\">'&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
        );
    }
}
