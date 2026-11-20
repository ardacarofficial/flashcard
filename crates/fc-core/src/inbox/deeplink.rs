//! `flashcard://` deep links (docs/06 §6).
//!
//! | Link | Meaning |
//! |---|---|
//! | `flashcard://deck/<id>` | open a deck |
//! | `flashcard://note/<id>` | open a note |
//! | `flashcard://study?deck=<id>` / `?query=<query>` | start studying |
//! | `flashcard://add?deck=<id>&front=…&back=…&tags=a,b&notetype=<id>&field.<key>=…&attr.<key>=…` | propose a card (lands in the inbox, never written directly) |
//! | `flashcard://import?url=…` | parsed, but not supported yet |
//!
//! The scheme is case-insensitive, a trailing `/` is ignored (Windows adds one), query values
//! are percent-decoded (`+` = space). Ids are returned as text: the app checks they exist.

use std::collections::BTreeMap;

/// Longest link accepted (bytes). Links come from other programs; anything longer is refused
/// before parsing.
pub const MAX_LINK_BYTES: usize = 8 * 1024;

/// The URL scheme.
pub const SCHEME: &str = "flashcard";

/// A parsed deep link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLink {
    /// Open a deck.
    Deck {
        /// Deck id (unchecked).
        id: String,
    },
    /// Open a note.
    Note {
        /// Note id (unchecked).
        id: String,
    },
    /// Start studying a deck or a query (at least one is set).
    Study {
        /// Deck id (unchecked).
        deck: Option<String>,
        /// Query in the query language.
        query: Option<String>,
    },
    /// Propose a card.
    Add(DeepLinkAdd),
    /// Import from a URL (not supported yet; the app refuses it).
    Import {
        /// Source URL.
        url: String,
    },
}

/// The card a `flashcard://add` link proposes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeepLinkAdd {
    /// Deck id (unchecked).
    pub deck: Option<String>,
    /// Note type id (unchecked).
    pub notetype: Option<String>,
    /// `front` (Classic shorthand).
    pub front: Option<String>,
    /// `back` (Classic shorthand).
    pub back: Option<String>,
    /// `field.<key>=value` pairs.
    pub fields: BTreeMap<String, String>,
    /// `tags=a,b` (and repeated `tag=`), trimmed, empty ones dropped.
    pub tags: Vec<String>,
    /// `attr.<key or name>=<option label>`; repeated keys collect several values.
    pub attributes: BTreeMap<String, Vec<String>>,
}

/// Why a link was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkError {
    /// Not a `flashcard://` link.
    NotFlashcard,
    /// Longer than [`MAX_LINK_BYTES`].
    TooLong,
    /// Unknown target (`flashcard://<target>`).
    UnknownTarget(String),
    /// A required part is missing (`id`, `deck_or_query`, `content`, `url`).
    Missing(&'static str),
    /// Bad percent-encoding or not UTF-8.
    BadEncoding,
}

impl DeepLinkError {
    /// Stable reason string (error `details.reason`).
    pub fn reason(&self) -> &'static str {
        match self {
            Self::NotFlashcard => "scheme",
            Self::TooLong => "too_long",
            Self::UnknownTarget(_) => "target",
            Self::Missing(_) => "missing",
            Self::BadEncoding => "encoding",
        }
    }
}

/// Parses a `flashcard://` link.
pub fn parse(link: &str) -> Result<DeepLink, DeepLinkError> {
    if link.len() > MAX_LINK_BYTES {
        return Err(DeepLinkError::TooLong);
    }
    let link = link.trim();
    let (scheme, rest) = link.split_once("://").ok_or(DeepLinkError::NotFlashcard)?;
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return Err(DeepLinkError::NotFlashcard);
    }
    let rest = rest.split('#').next().unwrap_or_default();
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut segments = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(decode)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter();
    let target = segments.next().unwrap_or_default().to_ascii_lowercase();
    let params = parse_query(query)?;
    let one = |key: &str| {
        params
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    match target.as_str() {
        "deck" | "note" => {
            let id = segments
                .next()
                .or_else(|| one("id"))
                .filter(|s| !s.trim().is_empty())
                .ok_or(DeepLinkError::Missing("id"))?;
            Ok(if target == "deck" {
                DeepLink::Deck { id }
            } else {
                DeepLink::Note { id }
            })
        }
        "study" => {
            let deck = segments.next().or_else(|| one("deck"));
            let query = one("query");
            if deck.is_none() && query.is_none() {
                return Err(DeepLinkError::Missing("deck_or_query"));
            }
            Ok(DeepLink::Study { deck, query })
        }
        "add" => {
            let mut add = DeepLinkAdd {
                deck: one("deck"),
                notetype: one("notetype"),
                ..DeepLinkAdd::default()
            };
            for (k, v) in &params {
                match k.as_str() {
                    "front" => add.front = Some(v.clone()),
                    "back" => add.back = Some(v.clone()),
                    "tags" | "tag" => add.tags.extend(
                        v.split(',')
                            .map(str::trim)
                            .filter(|t| !t.is_empty())
                            .map(str::to_owned),
                    ),
                    _ => {
                        if let Some(key) = k.strip_prefix("field.").filter(|s| !s.is_empty()) {
                            add.fields.insert(key.to_owned(), v.clone());
                        } else if let Some(key) = k.strip_prefix("attr.").filter(|s| !s.is_empty())
                        {
                            add.attributes
                                .entry(key.to_owned())
                                .or_default()
                                .push(v.clone());
                        }
                    }
                }
            }
            let has_content = [&add.front, &add.back]
                .into_iter()
                .flatten()
                .chain(add.fields.values())
                .any(|v| !v.trim().is_empty());
            if !has_content {
                return Err(DeepLinkError::Missing("content"));
            }
            Ok(DeepLink::Add(add))
        }
        "import" => Ok(DeepLink::Import {
            url: one("url").ok_or(DeepLinkError::Missing("url"))?,
        }),
        other => Err(DeepLinkError::UnknownTarget(other.to_owned())),
    }
}

/// Finds the first `flashcard://` link among process arguments (how the OS hands a link to a
/// freshly started app, and how a second instance forwards its arguments).
pub fn find_in_args<S: AsRef<str>>(args: &[S]) -> Option<String> {
    args.iter()
        .map(|a| a.as_ref().trim().trim_matches('"'))
        .find(|a| {
            a.split_once("://")
                .is_some_and(|(s, _)| s.eq_ignore_ascii_case(SCHEME))
        })
        .map(str::to_owned)
}

fn parse_query(query: &str) -> Result<Vec<(String, String)>, DeepLinkError> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            Ok((decode(&k.replace('+', " "))?, decode(&v.replace('+', " "))?))
        })
        .collect()
}

/// Percent-decodes `s` (UTF-8).
fn decode(s: &str) -> Result<String, DeepLinkError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).ok_or(DeepLinkError::BadEncoding)?;
            let hex = std::str::from_utf8(hex).map_err(|_| DeepLinkError::BadEncoding)?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| DeepLinkError::BadEncoding)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| DeepLinkError::BadEncoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_links() {
        assert_eq!(
            parse("flashcard://deck/0193abc"),
            Ok(DeepLink::Deck {
                id: "0193abc".into()
            })
        );
        assert_eq!(
            parse("FLASHCARD://note/n1/"),
            Ok(DeepLink::Note { id: "n1".into() })
        );
        assert_eq!(
            parse("flashcard://study?deck=d1"),
            Ok(DeepLink::Study {
                deck: Some("d1".into()),
                query: None
            })
        );
        assert_eq!(
            parse("flashcard://study?query=seviye%3AB1+is%3Adue"),
            Ok(DeepLink::Study {
                deck: None,
                query: Some("seviye:B1 is:due".into())
            })
        );
    }

    #[test]
    fn add_link_collects_fields_tags_and_attributes() {
        let link =
            "flashcard://add?deck=d1&front=K%C4%B1l%C4%B1%C3%A7&back=sword&tags=web,%20tr,&tag=x\
                    &field.example=a+b&attr.Seviye=B1&attr.S%C4%B1nav=YDS&attr.S%C4%B1nav=TOEFL";
        let Ok(DeepLink::Add(add)) = parse(link) else {
            panic!("not an add link");
        };
        assert_eq!(add.deck.as_deref(), Some("d1"));
        assert_eq!(add.front.as_deref(), Some("Kılıç"));
        assert_eq!(add.back.as_deref(), Some("sword"));
        assert_eq!(add.tags, ["web", "tr", "x"]);
        assert_eq!(add.fields["example"], "a b");
        assert_eq!(add.attributes["Seviye"], ["B1"]);
        assert_eq!(add.attributes["Sınav"], ["YDS", "TOEFL"]);
    }

    #[test]
    fn refusals() {
        assert_eq!(parse("https://deck/1"), Err(DeepLinkError::NotFlashcard));
        assert_eq!(parse("flashcard:deck/1"), Err(DeepLinkError::NotFlashcard));
        assert_eq!(parse("flashcard://deck"), Err(DeepLinkError::Missing("id")));
        assert_eq!(
            parse("flashcard://study"),
            Err(DeepLinkError::Missing("deck_or_query"))
        );
        assert_eq!(
            parse("flashcard://add?deck=d&front=%20"),
            Err(DeepLinkError::Missing("content"))
        );
        assert_eq!(
            parse("flashcard://add?front=%ZZ"),
            Err(DeepLinkError::BadEncoding)
        );
        assert_eq!(
            parse("flashcard://add?front=%C3"),
            Err(DeepLinkError::BadEncoding)
        );
        assert_eq!(
            parse("flashcard://wipe?all=1"),
            Err(DeepLinkError::UnknownTarget("wipe".into()))
        );
        let long = format!("flashcard://add?front={}", "a".repeat(MAX_LINK_BYTES));
        assert_eq!(parse(&long), Err(DeepLinkError::TooLong));
        assert_eq!(
            parse("flashcard://import?url=https%3A%2F%2Fx"),
            Ok(DeepLink::Import {
                url: "https://x".into()
            })
        );
    }

    #[test]
    fn links_are_found_in_process_arguments() {
        let cold = ["C:\\App\\flashcard.exe", "\"flashcard://deck/d1\""];
        assert_eq!(find_in_args(&cold).as_deref(), Some("flashcard://deck/d1"));
        let second = ["flashcard.exe", "--flag", "Flashcard://study?deck=d"];
        assert_eq!(
            find_in_args(&second).as_deref(),
            Some("Flashcard://study?deck=d")
        );
        assert_eq!(find_in_args(&["flashcard.exe", "https://x"]), None);
    }
}
