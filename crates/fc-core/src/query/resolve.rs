//! Resolution: [`Expr`] + [`Catalog`] → [`Filter`]. Keys are matched case- and
//! accent-insensitively ([`fold`]); names of things (decks, tags, note types) that do not exist
//! simply match nothing, while unknown *keys*, option labels and malformed values are
//! positioned errors.
//!
//! | Key | Meaning |
//! |---|---|
//! | *(none)* | word (token prefix) / `"phrase"` in the indexed note text; inner `*` = glob |
//! | `deck:` `deste:` | deck by full path or leaf name, `*` glob, sub-decks included |
//! | `tag:` `etiket:` | tag (glob), sub-tags included; `tag:none` = untagged |
//! | `notetype:` `note:` | note type name (glob) |
//! | `type:` `card:` | card type engine id / template key |
//! | `is:` | `new learn review relearn due suspended buried flagged` |
//! | `flag:` | `0`/`none`, `1`–`7`, `red orange green blue pink turquoise purple` (TR names too, plus [`Catalog::flag_names`]), `*` |
//! | `stability` `difficulty` `lapses` `reps` | number with `= != < <= > >=` |
//! | `retrievability` | FSRS recall probability now, `0..1` ([`crate::recall`]: the preset's decay from [`Catalog::deck_decay`], whole study days to [`Catalog::day_start`]) |
//! | `added:N` `edited:N` `reviewed:N` | within the last N days — study days when [`Catalog::day_start`] is set (`reviewed:1` = today, `reviewed:2` = today and yesterday), else rolling 24 h periods; `due:N` = due within N days |
//! | `re:` | regex over all fields; `<field>:re:<regex>` for one field |
//! | `nid:` `cid:` | comma-separated ids |
//! | classification key or name | `seviye:B1`, `seviye>=B1` (ordered lists), `sınav:TYT`, `puan>3`, `tarih<2025-01-01`, `x:*` = has a value; canonical `attr_<id>` keys and option ids too ([`super::canonical`]) |
//! | field key or name | whole-field glob: `kelime:app*` |
//!
//! v1.4 language-learning keys (`durum:`, `dil:`, `kelime:`, `karşılaşma`, `nereden:`, `kaynak:`,
//! `ses:`, `görsel:`, `sıklık`, `cefr:`): [`super::lang`]. A classification with the same key
//! wins over them.
//!
//! `k!=v` is `-k:v` (also true when there is no value).

use super::ast::{CmpOp, Expr, Span, Term};
use super::error::{QueryError, QueryErrorKind as K};
use super::filter::{AttrAlt, AttrTest, CardNum, Filter, Pred, TimeCol};
use super::lang::{cefr_ordinal, word_key, MediaFrom, MediaKind, NoteOrigin};
use super::text::{compile_regex, fold, tokens, Glob};
use crate::attribute::{AttributeDef, AttributeKind};
use crate::card::CardState;
use crate::deck::{Deck, PATH_SEPARATOR};
use crate::id::{CardId, DeckId, NoteId};
use crate::notetype::NoteType;
use crate::reading::WordStatus;
use crate::tag::Tag;
use crate::time::{Timestamp, MILLIS_PER_DAY};
use std::collections::HashMap;

/// Everything names resolve against (a read snapshot taken by the caller).
#[derive(Debug, Clone, Copy)]
pub struct Catalog<'a> {
    /// Decks (trashed ones are ignored).
    pub decks: &'a [Deck],
    /// Tags.
    pub tags: &'a [Tag],
    /// Note types (trashed ones are ignored).
    pub notetypes: &'a [NoteType],
    /// Classification definitions (trashed ones are ignored).
    pub attributes: &'a [AttributeDef],
    /// User-defined flag names (`name`, flag number 1–7), matched folded in addition to
    /// [`FLAG_NAMES`]. Empty until custom flag names exist (v0.3 `card_flag_defs`).
    pub flag_names: &'a [(String, u32)],
    /// "Now" for relative times (`added:7`, `is:due`, retrievability).
    pub now: Timestamp,
    /// v1.3: start of the current local study day (the app computes it from the user's time
    /// zone and day-start hour). With it, `added:N`/`edited:N`/`reviewed:N` count study days
    /// (`reviewed:1` = since the day began); without it, rolling 24 h periods before `now`.
    pub day_start: Option<Timestamp>,
    /// v1.4: FSRS decay (`w20`) of every deck whose preset's differs from
    /// [`crate::recall::DEFAULT_DECAY`] (personalised weights); other decks use the default.
    /// `retrievability` compares with the same [`crate::recall`] formula card info shows.
    pub deck_decay: &'a [(DeckId, f64)],
    /// v1.4: "known by maturity" threshold of the word index (`words.known_interval_days`)
    /// for `durum:`/`status:`.
    pub known_interval_days: u32,
    /// v1.4: lemma keys of `kelime:`/`word:` values ([`super::lang::word_values`], as typed →
    /// the keys the app's lemmatizers give, `went` → `go`). A value without an entry matches
    /// its own [`super::lang::word_key`] only.
    pub word_keys: &'a [(String, Vec<String>)],
}

/// Built-in flag colors (1-based flag number = bit + 1, Anki's numbering), English and Turkish
/// names (folded).
pub const FLAG_NAMES: &[(&str, u32)] = &[
    ("red", 1),
    ("kirmizi", 1),
    ("orange", 2),
    ("turuncu", 2),
    ("green", 3),
    ("yesil", 3),
    ("blue", 4),
    ("mavi", 4),
    ("pink", 5),
    ("pembe", 5),
    ("turquoise", 6),
    ("turkuaz", 6),
    ("purple", 7),
    ("mor", 7),
];

/// Highest flag number.
pub const MAX_FLAG: u32 = 7;

/// Built-in keys (folded), checked before classifications and fields.
pub const BUILTIN_KEYS: &[&str] = &[
    "deck",
    "deste",
    "tag",
    "etiket",
    "notetype",
    "note",
    "type",
    "card",
    "is",
    "flag",
    "stability",
    "difficulty",
    "lapses",
    "reps",
    "retrievability",
    "added",
    "edited",
    "reviewed",
    "due",
    "re",
    "nid",
    "cid",
];

/// Parses and resolves `text`.
pub fn compile_query(text: &str, cat: &Catalog<'_>) -> Result<Filter, QueryError> {
    resolve(&super::parse::parse_query(text)?, cat)
}

/// Resolves a parsed query.
pub fn resolve(expr: &Expr, cat: &Catalog<'_>) -> Result<Filter, QueryError> {
    let r = Resolver::new(cat);
    r.expr(expr)
}

/// Folded key form of a display name: spaces become `_` (`"Ders adı"` → `ders_adi`).
pub fn name_key(name: &str) -> String {
    fold(name.trim())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

struct Resolver<'a> {
    cat: &'a Catalog<'a>,
    live_decks: Vec<&'a Deck>,
    children: HashMap<DeckId, Vec<DeckId>>,
    paths: HashMap<DeckId, String>,
}

fn err(kind: K, span: Span) -> QueryError {
    QueryError::new(kind, span)
}

fn op_span(t: &Term) -> Span {
    Span {
        start: t.key_span.end,
        end: t.value_span.start.max(t.key_span.end),
    }
}

fn unsupported(t: &Term) -> QueryError {
    err(
        K::UnsupportedOperator {
            key: t.key.clone().unwrap_or_default(),
            op: t.op.as_str().to_owned(),
        },
        op_span(t),
    )
}

/// Errors unless the operator is `:`/`=` (`!=` was rewritten before).
fn need_eq(t: &Term) -> Result<(), QueryError> {
    if t.op.is_equality() {
        Ok(())
    } else {
        Err(unsupported(t))
    }
}

fn parse_number(t: &Term) -> Result<f64, QueryError> {
    let v = t.value.trim().replace(',', ".");
    v.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| {
            err(
                K::InvalidNumber {
                    value: t.value.clone(),
                },
                t.value_span,
            )
        })
}

fn parse_days(t: &Term) -> Result<i64, QueryError> {
    t.value.trim().parse::<i64>().map_err(|_| {
        err(
            K::InvalidNumber {
                value: t.value.clone(),
            },
            t.value_span,
        )
    })
}

/// `YYYY-MM-DD` with plausible month/day.
pub fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if !(digits(0..4) && digits(5..7) && digits(8..10)) {
        return false;
    }
    let m: u32 = s[5..7].parse().unwrap_or(0);
    let d: u32 = s[8..10].parse().unwrap_or(0);
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

fn parse_bool(s: &str) -> Option<bool> {
    match fold(s.trim()).as_str() {
        "yes" | "true" | "1" | "evet" | "dogru" => Some(true),
        "no" | "false" | "0" | "hayir" | "yanlis" => Some(false),
        _ => None,
    }
}

fn norm_op(op: CmpOp) -> CmpOp {
    if op == CmpOp::Colon {
        CmpOp::Eq
    } else {
        op
    }
}

impl<'a> Resolver<'a> {
    fn new(cat: &'a Catalog<'a>) -> Self {
        let live_decks: Vec<&Deck> = cat.decks.iter().filter(|d| !d.is_deleted()).collect();
        let mut children: HashMap<DeckId, Vec<DeckId>> = HashMap::new();
        for d in &live_decks {
            if let Some(p) = d.parent_id {
                children.entry(p).or_default().push(d.id);
            }
        }
        let by_id: HashMap<DeckId, &Deck> = live_decks.iter().map(|d| (d.id, *d)).collect();
        let paths = live_decks
            .iter()
            .map(|d| (d.id, deck_path(&by_id, d)))
            .collect();
        Self {
            cat,
            live_decks,
            children,
            paths,
        }
    }

    /// `retrievability <op> value`: one [`Pred::Recall`] per distinct preset decay, each
    /// limited to its decks; the default decay covers every other deck.
    fn recall(&self, op: CmpOp, value: f64) -> Filter {
        let day_start = self.cat.day_start.unwrap_or(self.cat.now);
        let one = |decay: f64| {
            Filter::Pred(Pred::Recall {
                op,
                value,
                decay,
                day_start,
            })
        };
        let custom: Vec<(DeckId, f64)> = self
            .cat
            .deck_decay
            .iter()
            .copied()
            .filter(|(_, d)| crate::recall::valid_decay(*d))
            .filter(|(_, d)| d.to_bits() != crate::recall::DEFAULT_DECAY.to_bits())
            .collect();
        if custom.is_empty() {
            return one(crate::recall::DEFAULT_DECAY);
        }
        // Group by decay (bit-exact), in first-seen order for a stable filter.
        let mut groups: Vec<(f64, Vec<DeckId>)> = Vec::new();
        for (deck, decay) in &custom {
            match groups
                .iter_mut()
                .find(|(d, _)| d.to_bits() == decay.to_bits())
            {
                Some((_, ids)) => ids.push(*deck),
                None => groups.push((*decay, vec![*deck])),
            }
        }
        let all: Vec<DeckId> = custom.iter().map(|(d, _)| *d).collect();
        let mut alts: Vec<Filter> = groups
            .into_iter()
            .map(|(decay, ids)| Filter::and(vec![Filter::Pred(Pred::Decks(ids)), one(decay)]))
            .collect();
        alts.push(Filter::and(vec![
            Filter::negate(Filter::Pred(Pred::Decks(all))),
            one(crate::recall::DEFAULT_DECAY),
        ]));
        Filter::or(alts)
    }

    fn expr(&self, e: &Expr) -> Result<Filter, QueryError> {
        Ok(match e {
            Expr::And(v) => Filter::and(v.iter().map(|x| self.expr(x)).collect::<Result<_, _>>()?),
            Expr::Or(v) => Filter::or(v.iter().map(|x| self.expr(x)).collect::<Result<_, _>>()?),
            Expr::Not(x) => Filter::negate(self.expr(x)?),
            Expr::Term(t) => self.term(t)?,
        })
    }

    fn term(&self, t: &Term) -> Result<Filter, QueryError> {
        let Some(key) = &t.key else {
            return Ok(free_text(t));
        };
        if t.op == CmpOp::Ne {
            let eq = Term {
                op: CmpOp::Eq,
                ..t.clone()
            };
            return Ok(Filter::negate(self.term(&eq)?));
        }
        let k = fold(key);
        let pred = |p: Pred| Ok(Filter::Pred(p));
        match k.as_str() {
            "deck" | "deste" => {
                need_eq(t)?;
                Ok(self.decks(&t.value))
            }
            "tag" | "etiket" => {
                need_eq(t)?;
                Ok(self.tags(&t.value))
            }
            "notetype" | "note" => {
                need_eq(t)?;
                let g = Glob::new(&t.value);
                let ids: Vec<_> = self
                    .cat
                    .notetypes
                    .iter()
                    .filter(|n| n.deleted_at.is_none() && g.matches(&n.name))
                    .map(|n| n.id)
                    .collect();
                Ok(if ids.is_empty() {
                    Filter::Nothing
                } else {
                    Filter::Pred(Pred::NoteTypes(ids))
                })
            }
            "type" => {
                need_eq(t)?;
                pred(Pred::CardType(fold(t.value.trim())))
            }
            "card" => {
                need_eq(t)?;
                pred(Pred::Template(fold(t.value.trim())))
            }
            "is" => {
                need_eq(t)?;
                let p = match fold(t.value.trim()).as_str() {
                    "new" | "yeni" => Pred::State(CardState::New),
                    "learn" | "learning" => Pred::State(CardState::Learning),
                    "review" => Pred::State(CardState::Review),
                    "relearn" | "relearning" => Pred::State(CardState::Relearning),
                    "due" => Pred::DueNow,
                    // v1.3: the app's words for them work too (folded: `yarına` → `yarina`).
                    "suspended" | "aside" | "kenarda" => Pred::Suspended,
                    "buried" | "tomorrow" | "yarina" => Pred::Buried,
                    "flagged" => Pred::Flagged,
                    _ => {
                        return Err(err(
                            K::UnknownState {
                                value: t.value.clone(),
                            },
                            t.value_span,
                        ))
                    }
                };
                pred(p)
            }
            "flag" => {
                need_eq(t)?;
                let v = fold(t.value.trim());
                let n = match v.as_str() {
                    "*" | "any" => return pred(Pred::Flagged),
                    "0" | "none" | "yok" => return Ok(Filter::negate(Filter::Pred(Pred::Flagged))),
                    other => other
                        .parse::<u32>()
                        .ok()
                        .filter(|n| (1..=MAX_FLAG).contains(n))
                        .or_else(|| {
                            FLAG_NAMES
                                .iter()
                                .find(|(name, _)| *name == other)
                                .map(|(_, n)| *n)
                        })
                        .or_else(|| {
                            self.cat
                                .flag_names
                                .iter()
                                .find(|(name, n)| {
                                    fold(name.trim()) == other && (1..=MAX_FLAG).contains(n)
                                })
                                .map(|(_, n)| *n)
                        }),
                };
                match n {
                    Some(n) => pred(Pred::FlagBit(n - 1)),
                    None => Err(err(
                        K::InvalidFlag {
                            value: t.value.clone(),
                        },
                        t.value_span,
                    )),
                }
            }
            "stability" | "difficulty" | "lapses" | "reps" => {
                let prop = match k.as_str() {
                    "stability" => CardNum::Stability,
                    "difficulty" => CardNum::Difficulty,
                    "lapses" => CardNum::Lapses,
                    _ => CardNum::Reps,
                };
                pred(Pred::Num(prop, norm_op(t.op), parse_number(t)?))
            }
            "retrievability" => {
                let value = parse_number(t)?;
                Ok(self.recall(norm_op(t.op), value))
            }
            "added" | "edited" | "reviewed" | "due" => {
                need_eq(t)?;
                let days = parse_days(t)?;
                let now = self.cat.now;
                let back = match self.cat.day_start {
                    // N study days: today's and the N − 1 before it.
                    Some(start) if days >= 1 => {
                        start.plus_millis(-(days - 1).saturating_mul(MILLIS_PER_DAY))
                    }
                    _ => now.plus_millis(-days.saturating_mul(MILLIS_PER_DAY)),
                };
                let (col, op, at) = match k.as_str() {
                    "added" => (TimeCol::CardCreated, CmpOp::Ge, back),
                    "edited" => (TimeCol::NoteEdited, CmpOp::Ge, back),
                    "reviewed" => (TimeCol::LastReview, CmpOp::Ge, back),
                    _ => (TimeCol::Due, CmpOp::Le, now.plus_days(days)),
                };
                pred(Pred::Time(col, op, at))
            }
            "re" => {
                need_eq(t)?;
                regex(t, &t.value, None)
            }
            "nid" => {
                need_eq(t)?;
                Ok(Filter::Pred(Pred::NoteIds(ids::<NoteId>(t)?)))
            }
            "cid" => {
                need_eq(t)?;
                Ok(Filter::Pred(Pred::CardIds(ids::<CardId>(t)?)))
            }
            _ => {
                // v1.4 keys yield to a classification of the same name.
                if super::canonical::defs_named(self.cat.attributes, &k).is_empty() {
                    if let Some(f) = self.lang_term(t, &k)? {
                        return Ok(f);
                    }
                }
                self.attribute_or_field(t, &k)
            }
        }
    }

    /// v1.4 language-learning keys ([`super::lang`]); `None` = not one of them.
    fn lang_term(&self, t: &Term, k: &str) -> Result<Option<Filter>, QueryError> {
        let value = t.value.trim();
        let v = fold(value);
        let bad = || {
            err(
                K::UnknownOption {
                    key: t.key.clone().unwrap_or_default(),
                    value: t.value.clone(),
                },
                t.value_span,
            )
        };
        let pred = |p: Pred| Ok(Some(Filter::Pred(p)));
        match k {
            "durum" | "status" => {
                need_eq(t)?;
                let s = match v.as_str() {
                    "yeni" | "new" => WordStatus::New,
                    "ogreniyorum" | "learning" | "learn" => WordStatus::Learning,
                    "biliyorum" | "known" => WordStatus::Known,
                    "yoksay" | "yok_say" | "yok say" | "ignored" | "ignore" => WordStatus::Ignored,
                    _ => return Err(bad()),
                };
                pred(Pred::WordStatus {
                    statuses: vec![s],
                    known_days: self.cat.known_interval_days,
                })
            }
            "dil" | "lang" | "language" => {
                need_eq(t)?;
                if matches!(v.as_str(), "yok" | "none") {
                    return pred(Pred::Language(None));
                }
                let l = crate::words::index_language(value);
                if l.is_empty() {
                    return Err(bad());
                }
                pred(Pred::Language(Some(l)))
            }
            "kelime" | "word" => {
                need_eq(t)?;
                if value.starts_with("re:") {
                    return Ok(None);
                }
                let word = if Glob::new(value).is_wild() {
                    Filter::Pred(Pred::WordKeyGlob(Glob::new(value).source()))
                } else {
                    let mut keys = vec![word_key(value)];
                    if let Some((_, more)) = self.cat.word_keys.iter().find(|(w, _)| w == &t.value)
                    {
                        for m in more {
                            if !keys.contains(m) {
                                keys.push(m.clone());
                            }
                        }
                    }
                    keys.retain(|k| !k.is_empty());
                    if keys.is_empty() {
                        return Err(bad());
                    }
                    Filter::Pred(Pred::WordKeys(keys))
                };
                // A field of that name keeps matching as before (`kelime:app*`).
                Ok(Some(Filter::or(vec![word, self.field_or_nothing(t, k)?])))
            }
            "karsilasma" | "seen" => pred(Pred::Encounters(norm_op(t.op), parse_number(t)?)),
            "nereden" | "from" => {
                need_eq(t)?;
                pred(Pred::Origins(vec![
                    NoteOrigin::from_query(&v).ok_or_else(bad)?
                ]))
            }
            "kaynak" | "source" => {
                need_eq(t)?;
                if value.starts_with("re:") {
                    return Ok(None);
                }
                let src = Filter::Pred(Pred::Source(Glob::new(value).source()));
                Ok(Some(Filter::or(vec![src, self.field_or_nothing(t, k)?])))
            }
            "ses" | "audio" | "gorsel" | "image" => {
                need_eq(t)?;
                let kind = if matches!(k, "ses" | "audio") {
                    MediaKind::Audio
                } else {
                    MediaKind::Image
                };
                let from = match v.as_str() {
                    "var" | "yes" | "evet" | "*" | "any" => MediaFrom::Any,
                    "yok" | "none" | "no" | "hayir" => {
                        return Ok(Some(Filter::negate(Filter::Pred(Pred::Media(
                            kind,
                            MediaFrom::Any,
                        )))))
                    }
                    "video" | "altyazi" | "subtitle" | "klip" | "clip" => MediaFrom::Video,
                    "dosya" | "file" => MediaFrom::File,
                    "tts" if kind == MediaKind::Audio => {
                        let ids: Vec<_> = self
                            .cat
                            .notetypes
                            .iter()
                            .filter(|n| n.deleted_at.is_none() && speaks(n))
                            .map(|n| n.id)
                            .collect();
                        return Ok(Some(if ids.is_empty() {
                            Filter::Nothing
                        } else {
                            Filter::Pred(Pred::NoteTypes(ids))
                        }));
                    }
                    _ => return Err(bad()),
                };
                pred(Pred::Media(kind, from))
            }
            "siklik" | "freq" | "frequency" => {
                pred(Pred::FreqRank(norm_op(t.op), parse_number(t)?))
            }
            "cefr" | "level" | "seviye" | "duzey" => {
                let pivot = cefr_ordinal(value).ok_or_else(bad)?;
                let op = norm_op(t.op);
                let bands: Vec<u8> = (1..=6).filter(|b| op.holds(*b, pivot)).collect();
                pred(Pred::Band(bands))
            }
            _ => Ok(None),
        }
    }

    /// The field match of `key:value` when some field has that key or name, else nothing.
    fn field_or_nothing(&self, t: &Term, k: &str) -> Result<Filter, QueryError> {
        match self.attribute_or_field(t, k) {
            Ok(f) => Ok(f),
            Err(e) if matches!(e.kind, K::UnknownKey { .. }) => Ok(Filter::Nothing),
            Err(e) => Err(e),
        }
    }

    fn attribute_or_field(&self, t: &Term, k: &str) -> Result<Filter, QueryError> {
        let defs: Vec<&AttributeDef> = super::canonical::defs_named(self.cat.attributes, k);
        if !defs.is_empty() {
            return attribute(t, &defs);
        }
        let mut keys: Vec<String> = Vec::new();
        for nt in self.cat.notetypes.iter().filter(|n| n.deleted_at.is_none()) {
            for f in &nt.fields {
                if (fold(f.key.as_str()) == k || name_key(&f.name) == k) && !keys.contains(&f.key.0)
                {
                    keys.push(f.key.0.clone());
                }
            }
        }
        if keys.is_empty() {
            return Err(err(
                K::UnknownKey {
                    key: t.key.clone().unwrap_or_default(),
                },
                t.key_span,
            ));
        }
        need_eq(t)?;
        if let Some(pattern) = t.value.strip_prefix("re:") {
            return regex(t, pattern, Some(keys));
        }
        // The full-text index may pre-filter only if every field with such a key is indexed.
        let indexed = self.cat.notetypes.iter().all(|nt| {
            nt.fields
                .iter()
                .all(|f| !keys.contains(&f.key.0) || f.searchable)
        });
        Ok(Filter::Pred(Pred::Field {
            keys,
            glob: Glob::new(&t.value).source(),
            indexed,
        }))
    }

    fn decks(&self, value: &str) -> Filter {
        let g = Glob::new(value.trim());
        let leaf_too = !value.contains(PATH_SEPARATOR);
        let mut out: Vec<DeckId> = Vec::new();
        let mut stack: Vec<DeckId> = self
            .live_decks
            .iter()
            .filter(|d| {
                self.paths.get(&d.id).is_some_and(|p| g.matches(p))
                    || (leaf_too && g.matches(&d.name))
            })
            .map(|d| d.id)
            .collect();
        while let Some(id) = stack.pop() {
            if out.contains(&id) {
                continue;
            }
            out.push(id);
            if let Some(kids) = self.children.get(&id) {
                stack.extend(kids.iter().copied());
            }
        }
        if out.is_empty() {
            Filter::Nothing
        } else {
            out.sort();
            Filter::Pred(Pred::Decks(out))
        }
    }

    fn tags(&self, value: &str) -> Filter {
        if fold(value.trim()) == "none" {
            return Filter::Pred(Pred::Untagged);
        }
        let g = Glob::new(value.trim());
        let ids: Vec<_> = self
            .cat
            .tags
            .iter()
            .filter(|tag| {
                let segs: Vec<&str> = tag.name.segments().collect();
                (1..=segs.len()).any(|i| g.matches(&segs[..i].join(PATH_SEPARATOR)))
            })
            .map(|tag| tag.id)
            .collect();
        if ids.is_empty() {
            Filter::Nothing
        } else {
            Filter::Pred(Pred::Tags(ids))
        }
    }
}

/// Whether a note type speaks its cards (`{{tts:…}}` in a template).
fn speaks(nt: &NoteType) -> bool {
    nt.templates
        .iter()
        .any(|t| t.front.contains("tts:") || t.back.contains("tts:"))
}

/// Full `A::B` path of a live deck (stops at a missing or trashed ancestor).
fn deck_path(by_id: &HashMap<DeckId, &Deck>, d: &Deck) -> String {
    let mut segs = vec![d.name.as_str()];
    let mut cur = d.parent_id;
    while let Some(p) = cur.and_then(|p| by_id.get(&p)) {
        if segs.len() > 256 {
            break;
        }
        segs.push(p.name.as_str());
        cur = p.parent_id;
    }
    segs.reverse();
    segs.join(PATH_SEPARATOR)
}

fn free_text(t: &Term) -> Filter {
    if t.quoted {
        let folded = fold(&t.value);
        let toks: Vec<String> = tokens(&folded).into_iter().map(str::to_owned).collect();
        if toks.is_empty() {
            return Filter::All;
        }
        return Filter::Pred(Pred::Text {
            tokens: toks,
            prefix: false,
        });
    }
    let base = t.value.trim_end_matches('*');
    if base.contains('*') {
        return Filter::Pred(Pred::TextGlob(
            Glob::new(&format!("*{}*", t.value)).source(),
        ));
    }
    let folded = fold(base);
    let toks: Vec<String> = tokens(&folded).into_iter().map(str::to_owned).collect();
    if toks.is_empty() {
        return Filter::All;
    }
    Filter::Pred(Pred::Text {
        tokens: toks,
        prefix: true,
    })
}

fn regex(t: &Term, pattern: &str, keys: Option<Vec<String>>) -> Result<Filter, QueryError> {
    compile_regex(pattern).map_err(|message| err(K::InvalidRegex { message }, t.value_span))?;
    Ok(Filter::Pred(Pred::Regex {
        keys,
        pattern: pattern.to_owned(),
    }))
}

fn ids<T: std::str::FromStr>(t: &Term) -> Result<Vec<T>, QueryError> {
    t.value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<T>().map_err(|_| {
                err(
                    K::InvalidId {
                        value: s.to_owned(),
                    },
                    t.value_span,
                )
            })
        })
        .collect()
}

/// Test of one definition. `Ok(None)` = the value names no option of this definition.
fn attr_test(def: &AttributeDef, t: &Term) -> Result<Option<AttrTest>, QueryError> {
    let op = norm_op(t.op);
    let value = t.value.trim();
    if op == CmpOp::Eq && value == "*" && !t.quoted {
        return Ok(Some(AttrTest::Any));
    }
    match def.kind {
        AttributeKind::OrderedEnum | AttributeKind::Enum | AttributeKind::MultiEnum => {
            if op == CmpOp::Eq {
                let g = Glob::new(value);
                let ids: Vec<_> = def
                    .sorted_options()
                    .into_iter()
                    .filter(|o| g.matches(&o.label) || o.id.to_string() == value)
                    .map(|o| o.id)
                    .collect();
                return Ok((!ids.is_empty()).then_some(AttrTest::Options(ids)));
            }
            if def.kind != AttributeKind::OrderedEnum {
                return Err(unsupported(t));
            }
            let wanted = fold(value);
            let Some(pivot) = def
                .options
                .iter()
                .find(|o| fold(&o.label) == wanted || o.id.to_string() == value)
            else {
                return Ok(None);
            };
            let ids: Vec<_> = def
                .sorted_options()
                .into_iter()
                .filter(|o| op.holds(o.order, pivot.order))
                .map(|o| o.id)
                .collect();
            Ok(Some(AttrTest::Options(ids)))
        }
        AttributeKind::Number => Ok(Some(AttrTest::Number(op, parse_number(t)?))),
        AttributeKind::Date => {
            if !is_iso_date(value) {
                return Err(err(
                    K::InvalidDate {
                        value: t.value.clone(),
                    },
                    t.value_span,
                ));
            }
            Ok(Some(AttrTest::Date(op, value.to_owned())))
        }
        AttributeKind::Text => {
            if op != CmpOp::Eq {
                return Err(unsupported(t));
            }
            Ok(Some(AttrTest::Text(Glob::new(value).source())))
        }
        AttributeKind::Bool => {
            if op != CmpOp::Eq {
                return Err(unsupported(t));
            }
            match parse_bool(value) {
                Some(b) => Ok(Some(AttrTest::Bool(b))),
                None => Err(err(
                    K::InvalidBool {
                        value: t.value.clone(),
                    },
                    t.value_span,
                )),
            }
        }
    }
}

fn attribute(t: &Term, defs: &[&AttributeDef]) -> Result<Filter, QueryError> {
    let mut alts = Vec::new();
    let mut first_err = None;
    for def in defs {
        match attr_test(def, t) {
            Ok(Some(test)) => alts.push(AttrAlt {
                attr_id: def.id,
                target: def.target,
                test,
            }),
            Ok(None) => {}
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    if !alts.is_empty() {
        return Ok(Filter::Pred(Pred::Attr(alts)));
    }
    if let Some(e) = first_err {
        return Err(e);
    }
    if Glob::new(t.value.trim()).is_wild() {
        return Ok(Filter::Nothing);
    }
    Err(err(
        K::UnknownOption {
            key: t.key.clone().unwrap_or_default(),
            value: t.value.clone(),
        },
        t.value_span,
    ))
}
