//! Source text for card generation (docs/06 §1.4): text extracted from a PDF (a chosen page
//! range) or from a web page (fetched without running scripts, readable text only).
//!
//! Extraction only reads; nothing here talks to an AI provider. The text becomes the
//! editable source of `ai.preview`, so what is sent is still exactly what the user saw.

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Largest PDF read (bytes).
pub const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024;
/// Largest web page downloaded (bytes).
pub const MAX_PAGE_BYTES: u64 = 4 * 1024 * 1024;
/// Whole-fetch time limit of a web page.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
/// Redirects followed when fetching a web page.
pub const MAX_REDIRECTS: u32 = 5;

/// Why a source could not be read. `reason` is a stable machine-readable tag.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}: {message}")]
pub struct SourceError {
    /// `not_pdf`, `encrypted`, `pages`, `no_text`, `too_large`, `url`, `content_type`,
    /// `http_status`, `unreachable`, `blocked_host`, `timeout`, `io`.
    pub reason: &'static str,
    /// Developer-facing detail.
    pub message: String,
}

impl SourceError {
    fn new(reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }
}

/// Text extracted from a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    /// Title (PDF metadata `Title`, the page's `<title>`).
    pub title: Option<String>,
    /// The readable text.
    pub text: String,
    /// Pages in the PDF (`None` for web pages).
    pub page_count: Option<u32>,
    /// First page read (1-based, inclusive).
    pub from_page: Option<u32>,
    /// Last page read (inclusive).
    pub to_page: Option<u32>,
}

// ------------------------------------------------------------------ PDF

/// Text of pages `from..=to` (1-based; `None` = first/last page) of the PDF in `bytes`.
pub fn pdf_text(
    bytes: &[u8],
    from: Option<u32>,
    to: Option<u32>,
) -> Result<Extracted, SourceError> {
    // The header may follow a little junk (allowed by readers) but must be near the start.
    if !bytes[..bytes.len().min(1024)]
        .windows(4)
        .any(|w| w == b"%PDF")
    {
        return Err(SourceError::new("not_pdf", "the file is not a PDF"));
    }
    let mut doc = lopdf::Document::load_mem(bytes)
        .map_err(|e| SourceError::new("not_pdf", format!("unreadable PDF: {e}")))?;
    if doc.is_encrypted() && doc.decrypt("").is_err() {
        return Err(SourceError::new(
            "encrypted",
            "the PDF is password-protected",
        ));
    }
    let pages = doc.get_pages();
    let count = u32::try_from(pages.len()).unwrap_or(u32::MAX);
    if count == 0 {
        return Err(SourceError::new("no_text", "the PDF has no pages"));
    }
    let from = from.unwrap_or(1);
    let to = to.unwrap_or(count).min(count);
    if from == 0 || from > to {
        return Err(SourceError::new(
            "pages",
            format!("page range {from}–{to} is outside 1–{count}"),
        ));
    }
    let mut text = String::new();
    for page in from..=to {
        // One page at a time: a page lopdf cannot decode is skipped, not fatal.
        if let Ok(t) = doc.extract_text(&[page]) {
            let t = tidy(&t);
            if !t.is_empty() {
                if !text.is_empty() {
                    text.push_str("\n\n");
                }
                text.push_str(&t);
            }
        }
    }
    if text.trim().is_empty() {
        return Err(SourceError::new(
            "no_text",
            "no text found on these pages (a scanned PDF needs OCR)",
        ));
    }
    Ok(Extracted {
        title: pdf_title(&doc),
        text,
        page_count: Some(count),
        from_page: Some(from),
        to_page: Some(to),
    })
}

/// Reads a PDF file (size-limited) and extracts its text (see [`pdf_text`]).
pub fn pdf_file(
    path: &std::path::Path,
    from: Option<u32>,
    to: Option<u32>,
) -> Result<Extracted, SourceError> {
    let file = std::fs::File::open(path).map_err(|e| SourceError::new("io", e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_PDF_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| SourceError::new("io", e.to_string()))?;
    if bytes.len() as u64 > MAX_PDF_BYTES {
        return Err(SourceError::new(
            "too_large",
            "the PDF is larger than 64 MB",
        ));
    }
    pdf_text(&bytes, from, to)
}

fn pdf_title(doc: &lopdf::Document) -> Option<String> {
    let info = doc.trailer.get(b"Info").ok()?;
    let info = match info {
        lopdf::Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let title = info.as_dict().ok()?.get(b"Title").ok()?;
    let bytes = title.as_str().ok()?;
    let text = if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        bytes.iter().map(|&b| char::from(b)).collect()
    };
    Some(tidy(&text)).filter(|t| !t.is_empty())
}

// ------------------------------------------------------------------ web pages

/// Name resolution for page fetches. Injectable so tests can answer without DNS.
pub trait HostResolver: Send + Sync {
    /// All addresses of `host`.
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<IpAddr>>;
}

/// The operating system's resolver.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemResolver;

impl HostResolver for SystemResolver {
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<IpAddr>> {
        Ok((host, port).to_socket_addrs()?.map(|a| a.ip()).collect())
    }
}

/// Whether `ip` is an address a page fetch must never reach (loopback, private, link-local
/// incl. cloud metadata, CGNAT, unique-local, unspecified, multicast, …). IPv4 embedded in
/// IPv6 (mapped, NAT64) is judged by the IPv4 inside.
pub fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xC0) == 64) // 100.64.0.0/10 CGNAT
                || (o[0] == 198 && (o[1] & 0xFE) == 18) // 198.18.0.0/15 benchmarking
                || o[0] >= 240 // reserved
        }
        IpAddr::V6(v6) => {
            let seg = v6.segments();
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(v4));
            }
            // 64:ff9b::/96 (NAT64) and the deprecated IPv4-compatible ::a.b.c.d.
            if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] || seg[..6] == [0; 6] {
                let v4 = Ipv4Addr::new(
                    (seg[6] >> 8) as u8,
                    seg[6] as u8,
                    (seg[7] >> 8) as u8,
                    seg[7] as u8,
                );
                return is_blocked_ip(IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (seg[0] & 0xFE00) == 0xFC00 // fc00::/7 unique-local
                || (seg[0] & 0xFFC0) == 0xFE80 // fe80::/10 link-local
                || (seg[0] & 0xFFC0) == 0xFEC0 // fec0::/10 site-local (deprecated)
        }
    }
}

/// Connects only to the one address that was vetted: ureq never resolves the name again.
#[derive(Debug)]
struct PinnedResolver(SocketAddr);

impl ureq::unversioned::resolver::Resolver for PinnedResolver {
    fn resolve(
        &self,
        _uri: &ureq::http::Uri,
        _config: &ureq::config::Config,
        _timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
        let mut out = self.empty();
        out.push(self.0);
        Ok(out)
    }
}

/// Splits an http(s) URL into (host without brackets, port, normalized URI).
fn parse_target(url: &str) -> Result<(String, u16, ureq::http::Uri), SourceError> {
    let bad = || SourceError::new("url", "only http(s) addresses can be fetched");
    let valid = (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() <= 2000
        && url.split("://").nth(1).is_some_and(|rest| !rest.is_empty())
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !valid {
        return Err(bad());
    }
    let uri: ureq::http::Uri = url.parse().map_err(|_| bad())?;
    let https = uri.scheme_str() == Some("https");
    let host = uri.host().ok_or_else(bad)?;
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    if host.is_empty() || uri.authority().is_some_and(|a| a.as_str().contains('@')) {
        return Err(bad());
    }
    let port = uri.port_u16().unwrap_or(if https { 443 } else { 80 });
    Ok((host, port, uri))
}

/// Resolves `host` and picks the address to connect to; refuses the hop when any answer is
/// a blocked one (a name that also points inside is not trusted) unless `allow_private`.
fn vet_host(
    host: &str,
    port: u16,
    resolver: &dyn HostResolver,
    allow_private: bool,
) -> Result<SocketAddr, SourceError> {
    let ips: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) => resolver
            .resolve(host, port)
            .map_err(|e| SourceError::new("unreachable", format!("cannot resolve {host}: {e}")))?,
    };
    let Some(first) = ips.first().copied() else {
        return Err(SourceError::new(
            "unreachable",
            format!("cannot resolve {host}"),
        ));
    };
    if !allow_private && ips.iter().any(|ip| is_blocked_ip(*ip)) {
        return Err(SourceError::new(
            "blocked_host",
            "this address points to the local network and cannot be fetched",
        ));
    }
    Ok(SocketAddr::new(first, port))
}

/// Joins a `Location` value onto the URL it came from.
fn join_location(base: &ureq::http::Uri, location: &str) -> String {
    let location = location.trim();
    if location.contains("://") {
        return location.to_owned();
    }
    let scheme = base.scheme_str().unwrap_or("http");
    let authority = base.authority().map(|a| a.as_str()).unwrap_or("");
    if let Some(rest) = location.strip_prefix("//") {
        return format!("{scheme}://{rest}");
    }
    if location.starts_with('/') {
        return format!("{scheme}://{authority}{location}");
    }
    let dir = base.path().rsplit_once('/').map_or("", |(d, _)| d);
    format!("{scheme}://{authority}{dir}/{location}")
}

/// How a page fetch may reach the network.
#[derive(Debug, Clone, Copy, Default)]
pub struct FetchPolicy {
    /// Allow loopback / private / link-local hosts (the user opted in explicitly).
    pub allow_private_hosts: bool,
}

/// [`fetch_page_with`] with the OS resolver and the strict policy (no local hosts).
pub fn fetch_page(url: &str) -> Result<Extracted, SourceError> {
    fetch_page_with(url, FetchPolicy::default(), &SystemResolver)
}

/// Fetches `url` (http/https only, no scripts run, size and time limited) and returns its
/// readable text. Every hop (including each redirect, at most [`MAX_REDIRECTS`]) is resolved
/// here, vetted against local-network addresses, and the connection is pinned to the vetted
/// address.
pub fn fetch_page_with(
    url: &str,
    policy: FetchPolicy,
    resolver: &dyn HostResolver,
) -> Result<Extracted, SourceError> {
    fetch_page_dialing(url, policy, resolver, &|addr| addr)
}

/// [`fetch_page_with`] where `dial` maps the vetted address to the one actually connected to
/// (identity in production; tests point a "public" address at a local mock server).
pub(crate) fn fetch_page_dialing(
    url: &str,
    policy: FetchPolicy,
    resolver: &dyn HostResolver,
    dial: &dyn Fn(SocketAddr) -> SocketAddr,
) -> Result<Extracted, SourceError> {
    let deadline = Instant::now() + FETCH_TIMEOUT;
    let mut current = url.trim().to_owned();
    let mut hops = 0;
    let response = loop {
        let (host, port, uri) = parse_target(&current)?;
        let addr = vet_host(&host, port, resolver, policy.allow_private_hosts)?;
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| SourceError::new("timeout", "the page took too long"))?;
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(remaining))
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent("flashcard (reader)")
            .build();
        let agent = ureq::Agent::with_parts(
            config,
            ureq::unversioned::transport::DefaultConnector::default(),
            PinnedResolver(dial(addr)),
        );
        let response = agent
            .get(uri)
            .header("accept", "text/html,application/xhtml+xml,text/plain;q=0.9")
            .call()
            .map_err(|e| match e {
                ureq::Error::Timeout(_) => SourceError::new("timeout", "the page took too long"),
                other => SourceError::new("unreachable", other.to_string()),
            })?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            if let Some(location) = location {
                hops += 1;
                if hops > MAX_REDIRECTS {
                    return Err(SourceError::new("url", "too many redirects"));
                }
                let base = parse_target(&current)?.2;
                current = join_location(&base, &location);
                continue;
            }
        }
        break response;
    };
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(SourceError::new("http_status", format!("HTTP {status}")));
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/html")
        .to_ascii_lowercase();
    let html = content_type.contains("html");
    if !html && !content_type.starts_with("text/plain") {
        return Err(SourceError::new(
            "content_type",
            format!(
                "not a web page ({})",
                content_type.split(';').next().unwrap_or("")
            ),
        ));
    }
    let bytes = response
        .into_body()
        .into_with_config()
        .limit(MAX_PAGE_BYTES)
        .read_to_vec()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => {
                SourceError::new("too_large", "the page is larger than 4 MB")
            }
            ureq::Error::Timeout(_) => SourceError::new("timeout", "the page took too long"),
            other => SourceError::new("unreachable", other.to_string()),
        })?;
    let body = decode(&bytes, &content_type);
    let extracted = if html {
        readable_text(&body)
    } else {
        Extracted {
            title: None,
            text: tidy(&body),
            page_count: None,
            from_page: None,
            to_page: None,
        }
    };
    if extracted.text.is_empty() {
        return Err(SourceError::new("no_text", "no readable text on the page"));
    }
    Ok(extracted)
}

/// Bytes → text: UTF-8 unless the content type names a Latin-1 family charset.
fn decode(bytes: &[u8], content_type: &str) -> String {
    let latin1 = [
        "iso-8859-1",
        "latin1",
        "windows-1252",
        "iso-8859-9",
        "windows-1254",
    ]
    .iter()
    .any(|c| content_type.contains(&format!("charset={c}")));
    if latin1 {
        bytes.iter().map(|&b| char::from(b)).collect()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Elements whose content is never part of the readable text.
const SKIP: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "iframe", "head", "nav", "header", "footer",
    "aside", "form", "button", "select", "textarea", "object", "canvas", "menu", "dialog",
];

/// Elements that end a line.
const BLOCK: &[&str] = &[
    "p",
    "div",
    "br",
    "hr",
    "li",
    "tr",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "section",
    "article",
    "main",
    "blockquote",
    "pre",
    "ul",
    "ol",
    "dl",
    "dt",
    "dd",
    "table",
    "figure",
    "figcaption",
    "td",
    "th",
];

/// Elements with no end tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

#[derive(Debug, PartialEq)]
enum Token<'a> {
    Text(&'a str),
    Start(String, bool),
    End(String),
}

/// A forgiving HTML tokenizer: text, start tags (name, self-closing) and end tags. Comments,
/// doctypes and processing instructions are dropped; attribute values may contain `>`; the
/// content of `<script>`/`<style>` is raw text up to its end tag.
fn tokens(html: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0;
    let mut text_start = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &html[i..];
        let next = bytes.get(i + 1).copied().unwrap_or(b' ');
        let is_tag = next.is_ascii_alphabetic() || next == b'/' || next == b'!' || next == b'?';
        if !is_tag {
            i += 1;
            continue;
        }
        if text_start < i {
            out.push(Token::Text(&html[text_start..i]));
        }
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |e| e + 3);
            text_start = i;
            continue;
        }
        if next == b'!' || next == b'?' {
            i += rest.find('>').map_or(rest.len(), |e| e + 1);
            text_start = i;
            continue;
        }
        // Find the tag end, skipping quoted attribute values.
        let mut j = 1;
        let mut quote: Option<u8> = None;
        let rb = rest.as_bytes();
        while j < rb.len() {
            match (quote, rb[j]) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, b'"') | (None, b'\'') => quote = Some(rb[j]),
                (None, b'>') => break,
                _ => {}
            }
            j += 1;
        }
        let inner = &rest[1..j.min(rest.len())];
        i += (j + 1).min(rest.len());
        text_start = i;
        let closing = inner.starts_with('/');
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if closing {
            out.push(Token::End(name));
            continue;
        }
        let self_closing = inner.trim_end().ends_with('/') || VOID.contains(&name.as_str());
        let raw = matches!(name.as_str(), "script" | "style" | "textarea" | "title");
        out.push(Token::Start(name.clone(), self_closing));
        if raw && !self_closing {
            // Raw text up to the matching end tag (case-insensitive).
            let close = format!("</{name}");
            let end = find_ascii_ci(&bytes[i..], close.as_bytes()).map_or(html.len(), |e| i + e);
            if i < end {
                out.push(Token::Text(&html[i..end]));
            }
            i = end;
            text_start = i;
        }
    }
    if text_start < html.len() {
        out.push(Token::Text(&html[text_start..]));
    }
    out
}

/// Position of `needle` (lowercase ASCII) in `hay`, ignoring ASCII case.
fn find_ascii_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&k| hay[k..k + needle.len()].eq_ignore_ascii_case(needle))
}

/// The readable text of an HTML document: the `<article>` (else `<main>`, else `<body>`)
/// without navigation, headers, footers, forms, scripts and styles; block elements become
/// lines, list items `- ` lines; entities are decoded.
pub fn readable_text(html: &str) -> Extracted {
    let toks = tokens(html);
    let mut title = None;
    for (k, t) in toks.iter().enumerate() {
        if *t == Token::Start("title".into(), false) {
            if let Some(Token::Text(s)) = toks.get(k + 1) {
                title = Some(tidy(&decode_entities(s))).filter(|t| !t.is_empty());
            }
            break;
        }
    }
    // Prefer the main content when the page marks it and it has some substance.
    let mut text = String::new();
    for root in ["article", "main", "body"] {
        let t = text_within(&toks, Some(root));
        if t.chars().count() >= 200 || (root == "body" && !t.is_empty()) {
            text = t;
            break;
        }
        if text.is_empty() {
            text = t;
        }
    }
    if text.is_empty() {
        text = text_within(&toks, None);
    }
    Extracted {
        title,
        text,
        page_count: None,
        from_page: None,
        to_page: None,
    }
}

/// Text inside the first `root` element (every element when `None`).
fn text_within(toks: &[Token<'_>], root: Option<&str>) -> String {
    let mut out = String::new();
    let mut inside = root.is_none();
    let mut root_depth = 0usize;
    let mut skip_stack: Vec<String> = Vec::new();
    for t in toks {
        match t {
            Token::Start(name, self_closing) => {
                if !inside {
                    if Some(name.as_str()) == root && !self_closing {
                        inside = true;
                        root_depth = 1;
                    }
                    continue;
                }
                if Some(name.as_str()) == root && !self_closing {
                    root_depth += 1;
                }
                if !skip_stack.is_empty() {
                    if !self_closing && SKIP.contains(&name.as_str()) {
                        skip_stack.push(name.clone());
                    }
                    continue;
                }
                if SKIP.contains(&name.as_str()) && !self_closing {
                    skip_stack.push(name.clone());
                    continue;
                }
                if name == "li" {
                    out.push_str("\n- ");
                } else if BLOCK.contains(&name.as_str()) {
                    out.push('\n');
                } else if name == "td" || name == "th" {
                    out.push(' ');
                }
            }
            Token::End(name) => {
                if !inside {
                    continue;
                }
                if let Some(pos) = skip_stack.iter().rposition(|s| s == name) {
                    skip_stack.truncate(pos);
                    continue;
                }
                if !skip_stack.is_empty() {
                    continue;
                }
                if Some(name.as_str()) == root {
                    root_depth = root_depth.saturating_sub(1);
                    if root_depth == 0 {
                        break;
                    }
                }
                // `</li>` adds nothing: the next item starts its own line.
                if BLOCK.contains(&name.as_str()) && name != "li" {
                    out.push('\n');
                }
            }
            Token::Text(s) => {
                if inside && skip_stack.is_empty() {
                    // Source line breaks are spaces, as when the page is shown.
                    let text = decode_entities(s);
                    let mut space = false;
                    for c in text.chars() {
                        if c.is_whitespace() {
                            space = true;
                        } else {
                            if space {
                                out.push(' ');
                                space = false;
                            }
                            out.push(c);
                        }
                    }
                    if space {
                        out.push(' ');
                    }
                }
            }
        }
    }
    if root.is_some() && !inside {
        return String::new();
    }
    tidy(&out)
}

/// Decodes HTML entities (the common named ones and numeric references).
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[..rest.len().min(12)].find(';');
        let decoded = end.and_then(|e| {
            let body = &rest[1..e];
            let c = if let Some(num) = body.strip_prefix('#') {
                let n = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse().ok(),
                };
                n.and_then(char::from_u32)
            } else {
                Some(match body {
                    "amp" => '&',
                    "lt" => '<',
                    "gt" => '>',
                    "quot" => '"',
                    "apos" => '\'',
                    "nbsp" => ' ',
                    "ndash" => '–',
                    "mdash" => '—',
                    "hellip" => '…',
                    "lsquo" => '‘',
                    "rsquo" => '’',
                    "ldquo" => '“',
                    "rdquo" => '”',
                    "laquo" => '«',
                    "raquo" => '»',
                    "copy" => '©',
                    "reg" => '®',
                    "deg" => '°',
                    "middot" => '·',
                    "bull" => '•',
                    "times" => '×',
                    "euro" => '€',
                    _ => return None,
                })
            };
            c.map(|c| (c, e + 1))
        });
        match decoded {
            Some((c, used)) => {
                out.push(c);
                rest = &rest[used..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapses runs of spaces inside lines and keeps at most one blank line in a row.
fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() || line == "-" {
            blank += 1;
            if blank > 1 || out.is_empty() {
                continue;
            }
            out.push('\n');
            continue;
        }
        blank = 0;
        out.push_str(&line);
        out.push('\n');
    }
    out.trim().to_owned()
}
