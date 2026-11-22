//! Anki ⇄ [`Pack`] mapping.
//!
//! Import ([`to_pack`]):
//! * ids are derived from Anki ids ([`derived_id`]); a note whose guid is one of our UUIDv7
//!   note ids (a package we exported) gets that id back;
//! * Anki's stock note types are recognised by their structure (not their localized names)
//!   and land on the matching v0.3 built-in ([`stock`]): Basic → Classic, Basic (and
//!   reversed card) → Classic + reverse, Basic (optional reversed card) → Optional reverse,
//!   Basic (type in the answer) → Type the answer, Cloze → Cloze, Image Occlusion → Image
//!   occlusion (Anki shapes → v0.3 masks). Other note types are imported as they are: field
//!   names become keys (`Front` → `front`), templates are rewritten to keys;
//! * cloze cards get template key `c<n>`, image occlusion cards `m<n>`;
//! * flags: Anki's flag number `n` (1 red … 7 purple) → bit `n − 1` of `cards.flags` (the
//!   same numbering, `card_flag_defs`);
//! * scheduling: type/queue/due/ivl/reps/lapses/flags → state, due time, stability (FSRS `s`
//!   if present, else the interval), difficulty (FSRS `d`), suspension; cards in filtered
//!   decks go back to their original deck;
//! * review log entries with a rating become `review_log` rows (manual reschedules skipped);
//! * media references are rewritten to v0.3's form ([`crate::media`]): `<img src="media:<h>">`,
//!   `[sound:x]` → `<audio controls src="media:<h>"></audio>`, bare `media:<h>` in image/audio
//!   fields of built-ins. Export reverses it.
//!
//! Export ([`from_pack`]) is the reverse; Anki ids are allocated from creation times.

use super::occlusion;
use super::template::map_field_refs;
use super::*;
use crate::media::{
    extension_for, guess_mime, hash_of, media_tags_to_sound, media_url, references, rewrite,
    sha256_hex, sound_to_audio,
};
use crate::pack::{derived_id, MediaFile, Pack, ReviewEntry};
use fc_core::card::{Card, CardState, Rating, SchedulingState};
use fc_core::content::builtin as builtins;
use fc_core::deck::{normalize_deck_segment, Deck};
use fc_core::i18n::Locale;
use fc_core::note::{note_checksum, sort_field_text, Note};
use fc_core::notetype::{CardTemplate, FieldDef, FieldKey, FieldKind, NoteType, NoteTypeKind};
use fc_core::tag::{dedup_tags, TagName};
use fc_core::{CardId, DeckId, NoteId, NoteTypeId, ReviewLogId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use uuid::Uuid;

const DAY_MS: i64 = 86_400_000;

/// What could not be imported as-is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnkiImportReport {
    /// Notes skipped (unknown note type, no cards).
    pub skipped_notes: usize,
    /// Cards skipped (unknown note or template).
    pub skipped_cards: usize,
    /// Review log rows skipped (manual reschedules, unknown cards).
    pub skipped_reviews: usize,
    /// Tags that are not valid tag names.
    pub skipped_tags: usize,
    /// Media referenced by notes but missing from the package.
    pub missing_media: Vec<String>,
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

fn unique(base: String, fallback: &str, used: &mut HashSet<String>) -> String {
    let base = if base.is_empty() {
        fallback.to_owned()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut n = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}_{n}");
        n += 1;
    }
    candidate
}

fn ms(t: i64, fallback: Timestamp) -> Timestamp {
    if t > 0 {
        Timestamp(t)
    } else {
        fallback
    }
}

fn day_to_ms(crt: i64, day: i64) -> i64 {
    (crt + day * 86_400) * 1000
}

struct ImportedNotetype {
    nt: NoteType,
    /// Target field of each Anki field (`None` = dropped).
    keys: Vec<Option<FieldKey>>,
    cards: CardKeys,
}

/// How an Anki card's `ord` becomes a template key.
enum CardKeys {
    /// `nt.templates[ord]`.
    Templates,
    /// `c<ord + 1>`.
    Cloze,
    /// `m<ord + 1>` (image occlusion; the masks come from the Anki `Occlusion` field).
    Occlusion,
}

impl ImportedNotetype {
    fn card_key(&self, ord: i64) -> Option<(String, String)> {
        let n = ord.checked_add(1).filter(|n| *n > 0)?;
        match self.cards {
            CardKeys::Templates => usize::try_from(ord)
                .ok()
                .and_then(|o| self.nt.templates.get(o))
                .map(|t| (t.key.clone(), t.card_type.clone())),
            CardKeys::Cloze => Some((format!("c{n}"), builtins::engine::CLOZE.to_owned())),
            CardKeys::Occlusion => Some((
                format!("m{n}"),
                builtins::engine::IMAGE_OCCLUSION.to_owned(),
            )),
        }
    }
}

/// Field key of the image occlusion built-in's mask JSON.
const MASKS: &str = "masks";

/// The v0.3 image occlusion built-in (or a copy of it): its cards come from the mask engine.
fn is_occlusion_builtin(nt: &NoteType) -> bool {
    nt.templates
        .iter()
        .any(|t| t.card_type == builtins::engine::IMAGE_OCCLUSION)
        && nt.field(&FieldKey::new(MASKS)).is_some()
        && nt.field(&FieldKey::new("image")).is_some()
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The v0.3 built-in a stock Anki note type corresponds to: `(built-in, target key per Anki
/// field, card keys)`. Matched on structure (kind, field count, template text), since Anki
/// localizes the names.
fn stock(a: &AnkiNotetype) -> Option<(&'static str, Vec<Option<&'static str>>, CardKeys)> {
    let f = &a.fields;
    let q: Vec<String> = a.templates.iter().map(|t| squash(&t.qfmt)).collect();
    let field = |i: usize| squash(&format!("{{{{{}}}}}", f[i]));
    if a.kind == 1 {
        if q.iter().any(|t| t.contains("image-occlusion")) && f.len() >= 4 {
            let mut keys = vec![Some(MASKS), Some("image"), Some("header"), Some("extra")];
            keys.resize(f.len(), None);
            return Some(("image_occlusion", keys, CardKeys::Occlusion));
        }
        let cloze_of_first = q.len() == 1
            && !f.is_empty()
            && q[0].contains(&squash(&format!("{{{{cloze:{}}}}}", f[0])));
        if cloze_of_first && f.len() <= 2 {
            let keys = [Some("text"), Some("extra")][..f.len()].to_vec();
            return Some(("cloze", keys, CardKeys::Cloze));
        }
        return None;
    }
    if a.kind != 0 {
        return None;
    }
    match (f.len(), q.len()) {
        (2, 1) if q[0] == field(0) => Some((
            builtins::engine::CLASSIC,
            vec![Some("front"), Some("back")],
            CardKeys::Templates,
        )),
        (2, 1) if q[0] == squash(&format!("{}{{{{type:{}}}}}", field(0), f[1])) => Some((
            builtins::engine::TYPING,
            vec![Some("front"), Some("back")],
            CardKeys::Templates,
        )),
        (2, 2) if q[0] == field(0) && q[1] == field(1) => Some((
            "classic_reverse",
            vec![Some("front"), Some("back")],
            CardKeys::Templates,
        )),
        (3, 2)
            if q[0] == field(0)
                && q[1] == squash(&format!("{{{{#{c}}}}}{}{{{{/{c}}}}}", field(1), c = f[2])) =>
        {
            Some((
                "optional_reverse",
                vec![Some("front"), Some("back"), Some("add_reverse")],
                CardKeys::Templates,
            ))
        }
        _ => None,
    }
}

fn import_notetype(a: &AnkiNotetype, now: Timestamp) -> ImportedNotetype {
    if let Some((key, keys, cards)) = stock(a) {
        if let Some(b) = builtins::ALL.iter().find(|b| b.key == key) {
            // The collection has its own copy (seeded); this one only carries the id.
            let nt = b.build(now, Locale::En);
            return ImportedNotetype {
                keys: keys.into_iter().map(|k| k.map(FieldKey::new)).collect(),
                nt,
                cards,
            };
        }
    }
    let mut used = HashSet::new();
    let mut fields = Vec::new();
    let mut keys = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    let field_names: Vec<String> = if a.fields.is_empty() {
        vec!["Text".to_owned()]
    } else {
        a.fields.clone()
    };
    for (i, name) in field_names.iter().enumerate() {
        let key = unique(slug(name), &format!("field_{}", i + 1), &mut used);
        names.insert(name.clone(), key.clone());
        keys.push(Some(FieldKey::new(key.clone())));
        fields.push(FieldDef {
            key: FieldKey::new(key),
            name: if name.trim().is_empty() {
                format!("Field {}", i + 1)
            } else {
                name.clone()
            },
            kind: FieldKind::RichText,
            required: false,
            unique: i == 0,
            searchable: true,
            default_value: None,
            role: None,
        });
    }
    let to_keys = |t: &str| map_field_refs(t, |n| names.get(n).cloned());
    let cloze = a.kind == 1;
    let mut tused = HashSet::new();
    let templates = a
        .templates
        .iter()
        .enumerate()
        .map(|(i, t)| CardTemplate {
            key: if cloze {
                unique("cloze".into(), "cloze", &mut tused)
            } else {
                unique(slug(&t.name), &format!("card_{}", i + 1), &mut tused)
            },
            name: t.name.clone(),
            front: to_keys(&t.qfmt),
            back: to_keys(&t.afmt),
            card_type: if cloze {
                builtins::engine::CLOZE
            } else {
                builtins::engine::CLASSIC
            }
            .into(),
            answer_field: None,
        })
        .collect();
    let created = ms(a.id, now);
    let sort_field = keys
        .get(a.sort_field)
        .or_else(|| keys.first())
        .cloned()
        .flatten()
        .unwrap_or_else(|| FieldKey::new("text"));
    ImportedNotetype {
        nt: NoteType {
            id: NoteTypeId::from_uuid(derived_id(a.id, &format!("notetype:{}:{}", a.id, a.name))),
            name: if a.name.trim().is_empty() {
                "Anki".into()
            } else {
                a.name.clone()
            },
            kind: if cloze {
                NoteTypeKind::Cloze
            } else {
                NoteTypeKind::Standard
            },
            sort_field,
            fields,
            templates,
            css: a.css.clone(),
            version: 1,
            builtin: None,
            created_at: created,
            updated_at: created,
            deleted_at: None,
        },
        keys,
        cards: if cloze {
            CardKeys::Cloze
        } else {
            CardKeys::Templates
        },
    }
}

/// The note type an Anki note type imports as (tests).
#[cfg(test)]
pub(crate) fn import_notetype_for_tests(a: &AnkiNotetype, now: Timestamp) -> NoteType {
    import_notetype(a, now).nt
}

/// Anki image occlusion shapes (`{{cN::image-occlusion:rect:left=.1:top=.2:width=.3:height=.1:oi=1}}`,
/// also `ellipse` with `rx`/`ry` and `polygon` with `points`; fractions of the image) → v0.3
/// mask JSON. A cloze number's shapes become one mask (their bounding box), numbered `N`
/// (card `mN`). `oi=1` ("hide all, guess one") → `hide_all`, else `hide_one`.
pub(crate) fn occlusion_masks(field: &str) -> String {
    use std::collections::BTreeMap;
    let mut boxes: BTreeMap<u32, (f64, f64, f64, f64)> = BTreeMap::new();
    let mut hide_all = false;
    let mut rest = field;
    while let Some(start) = rest.find("{{c") {
        let after = &rest[start + 3..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let body = &after[..end];
        rest = &after[end + 2..];
        let Some((num, shape)) = body.split_once("::image-occlusion:") else {
            continue;
        };
        let Ok(n) = num.parse::<u32>() else {
            continue;
        };
        let mut parts = shape.split(':');
        let kind = parts.next().unwrap_or_default();
        let props: HashMap<&str, &str> = parts.filter_map(|p| p.split_once('=')).collect();
        let num = |k: &str| props.get(k).and_then(|v| v.trim().parse::<f64>().ok());
        hide_all |= props.get("oi").is_some_and(|v| v.trim() == "1");
        let bbox = match kind {
            "rect" => (|| Some((num("left")?, num("top")?, num("width")?, num("height")?)))(),
            // Anki's editor writes radii; a hand-written `width`/`height` is read too.
            "ellipse" => (|| {
                let (w, h) = match (num("rx"), num("ry")) {
                    (Some(rx), Some(ry)) => (2.0 * rx, 2.0 * ry),
                    _ => (num("width")?, num("height")?),
                };
                Some((num("left")?, num("top")?, w, h))
            })(),
            "polygon" => props.get("points").and_then(|pts| {
                let xy: Vec<(f64, f64)> = pts
                    .split_whitespace()
                    .filter_map(|p| {
                        let (x, y) = p.split_once(',')?;
                        Some((x.parse().ok()?, y.parse().ok()?))
                    })
                    .collect();
                let (x0, y0) = xy
                    .iter()
                    .fold((f64::MAX, f64::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
                let (x1, y1) = xy
                    .iter()
                    .fold((f64::MIN, f64::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
                (!xy.is_empty()).then_some((x0, y0, x1 - x0, y1 - y0))
            }),
            _ => None,
        };
        // Only normalized shapes (Anki ≥ 23.10); pixel coordinates cannot be placed.
        let Some((x, y, w, h)) = bbox.filter(|b| {
            [b.0, b.1, b.0 + b.2, b.1 + b.3]
                .iter()
                .all(|v| (0.0..=1.0001).contains(v))
        }) else {
            continue;
        };
        boxes
            .entry(n)
            .and_modify(|b| {
                let (bx1, by1) = ((b.0 + b.2).max(x + w), (b.1 + b.3).max(y + h));
                b.0 = b.0.min(x);
                b.1 = b.1.min(y);
                b.2 = bx1 - b.0;
                b.3 = by1 - b.1;
            })
            .or_insert((x, y, w, h));
    }
    let masks: Vec<serde_json::Value> = boxes
        .into_iter()
        .map(|(n, (x, y, w, h))| {
            // Six decimals: a polygon's bounding box should not read `0.19999999999999998`.
            let r = |v: f64| (v * 1e6).round() / 1e6;
            serde_json::json!({ "n": n, "x": r(x), "y": r(y), "w": r(w), "h": r(h) })
        })
        .collect();
    serde_json::json!({
        "mode": if hide_all { "hide_all" } else { "hide_one" },
        "masks": masks,
    })
    .to_string()
}

fn deck_segments(name: &str) -> Vec<String> {
    let segs: Vec<String> = name
        .split("::")
        .map(|s| normalize_deck_segment(s).unwrap_or_else(|_| "Anki".to_owned()))
        .collect();
    if segs.is_empty() {
        vec!["Default".into()]
    } else {
        segs
    }
}

#[derive(Default, Deserialize)]
struct FsrsData {
    s: Option<f64>,
    d: Option<f64>,
    /// Last review (s), Anki 23.10+.
    lrt: Option<i64>,
}

fn rating(ease: i64) -> Option<Rating> {
    match ease {
        1 => Some(Rating::Again),
        2 => Some(Rating::Hard),
        3 => Some(Rating::Good),
        4 => Some(Rating::Easy),
        _ => None,
    }
}

/// A field value in Anki's markup (media still as `media:<h>`; names come after): bare
/// references of image/audio/video fields become `<img src>` / `[sound:]`, `<audio>`/`<video>`
/// elements of rich text become `[sound:]`.
fn anki_field(kind: FieldKind, value: &str) -> String {
    let refs = || value.split_whitespace().filter(|r| hash_of(r).is_some());
    match kind {
        FieldKind::Image => refs().map(|r| format!("<img src=\"{r}\">")).collect(),
        FieldKind::Audio | FieldKind::Video => refs().map(|r| format!("[sound:{r}]")).collect(),
        _ => media_tags_to_sound(value),
    }
}

/// Converts a read package into a [`Pack`] (see the module docs).
pub fn to_pack(col: &AnkiCollection, now: Timestamp) -> (Pack, AnkiImportReport) {
    let mut report = AnkiImportReport::default();
    let mut pack = Pack::default();

    // Media.
    let mut name_to_hash: HashMap<String, String> = HashMap::new();
    for (name, bytes) in &col.media {
        let hash = sha256_hex(bytes);
        name_to_hash.insert(name.clone(), hash.clone());
        if !pack.media.iter().any(|m| m.hash == hash) {
            pack.media.push(MediaFile {
                hash,
                mime: guess_mime(name).into(),
                name: Some(name.clone()),
                bytes: bytes.clone(),
            });
        }
    }

    // Note types.
    let notetypes: HashMap<i64, ImportedNotetype> = col
        .notetypes
        .iter()
        .map(|a| (a.id, import_notetype(a, now)))
        .collect();

    // Decks actually used by cards (filtered decks resolve to the original deck).
    let anki_decks: HashMap<i64, &AnkiDeck> = col.decks.iter().map(|d| (d.id, d)).collect();
    let home_did = |c: &AnkiCard| if c.odid != 0 { c.odid } else { c.did };
    let did_path = |did: i64| -> Vec<String> {
        anki_decks
            .get(&did)
            .map_or_else(|| vec!["Default".to_owned()], |d| deck_segments(&d.name))
    };
    let mut paths: BTreeSet<Vec<String>> = BTreeSet::new();
    for c in &col.cards {
        let p = did_path(home_did(c));
        for n in 1..=p.len() {
            paths.insert(p[..n].to_vec());
        }
    }
    let by_path: HashMap<String, &AnkiDeck> = col
        .decks
        .iter()
        .map(|d| (deck_segments(&d.name).join("::"), d))
        .collect();
    let mut deck_ids: HashMap<Vec<String>, DeckId> = HashMap::new();
    let mut ordered: Vec<Vec<String>> = paths.into_iter().collect();
    ordered.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    for (i, path) in ordered.iter().enumerate() {
        let full = path.join("::");
        let source = by_path.get(&full);
        let id = DeckId::from_uuid(derived_id(
            source.map_or(0, |d| d.id),
            &format!("deck:{full}"),
        ));
        deck_ids.insert(path.clone(), id);
        let parent_id = (path.len() > 1)
            .then(|| deck_ids.get(&path[..path.len() - 1]).copied())
            .flatten();
        pack.decks.push(Deck {
            id,
            parent_id,
            name: path.last().cloned().unwrap_or_default(),
            description: source.map(|d| d.description.clone()).unwrap_or_default(),
            icon: None,
            color: None,
            preset_id: None,
            position: i64::try_from(i).unwrap_or(0),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        });
    }
    let deck_of = |did: i64| deck_ids.get(&did_path(did)).copied();

    // Last review per card.
    let mut last_review: HashMap<i64, i64> = HashMap::new();
    for r in &col.revlog {
        let e = last_review.entry(r.cid).or_insert(r.id);
        *e = (*e).max(r.id);
    }

    let mut cards_by_note: HashMap<i64, Vec<&AnkiCard>> = HashMap::new();
    for c in &col.cards {
        cards_by_note.entry(c.nid).or_default().push(c);
    }
    let mut used_nts: BTreeSet<i64> = BTreeSet::new();
    let mut card_ids: HashMap<i64, (CardId, CardState)> = HashMap::new();
    let mut missing: BTreeSet<String> = BTreeSet::new();

    for an in &col.notes {
        let Some(imp) = notetypes.get(&an.mid) else {
            report.skipped_notes += 1;
            continue;
        };
        let mut acards: Vec<&AnkiCard> = cards_by_note.get(&an.id).cloned().unwrap_or_default();
        acards.sort_by_key(|c| (c.ord, c.id));
        let Some(home) = acards.first().and_then(|c| deck_of(home_did(c))) else {
            report.skipped_notes += 1;
            continue;
        };
        let note_id = Uuid::parse_str(&an.guid)
            .ok()
            .filter(|u| u.get_version_num() == 7)
            .map(NoteId::from_uuid)
            .unwrap_or_else(|| {
                NoteId::from_uuid(derived_id(an.id, &format!("note:{}:{}", an.id, an.guid)))
            });
        let mut fields = fc_core::note::FieldValues::new();
        for (i, key) in imp.keys.iter().enumerate() {
            let Some(key) = key else {
                continue;
            };
            let raw = an.fields.get(i).cloned().unwrap_or_default();
            if key.as_str() == MASKS && matches!(imp.cards, CardKeys::Occlusion) {
                fields.insert(key.clone(), occlusion_masks(&raw));
                continue;
            }
            let value = rewrite(&raw, |r| {
                if hash_of(r).is_some() {
                    return None;
                }
                let found = name_to_hash
                    .get(r)
                    .or_else(|| name_to_hash.get(&r.replace("&amp;", "&")));
                match found {
                    Some(h) => Some(media_url(h)),
                    None => {
                        if !r.contains("://") && !r.starts_with("data:") {
                            missing.insert(r.to_owned());
                        }
                        None
                    }
                }
            });
            let value = match imp.nt.field(key).map(|f| f.kind) {
                // Image/audio fields of built-ins hold bare `media:<h>` references.
                Some(FieldKind::Image | FieldKind::Audio | FieldKind::Video) => references(&value)
                    .iter()
                    .filter(|r| hash_of(r).is_some())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => sound_to_audio(&value),
            };
            fields.insert(key.clone(), value);
        }
        let mut tags = Vec::new();
        for t in &an.tags {
            match TagName::parse(t) {
                Ok(t) => tags.push(t),
                Err(_) => report.skipped_tags += 1,
            }
        }
        let created = ms(an.id, now);
        let note = Note {
            id: note_id,
            notetype_id: imp.nt.id,
            home_deck_id: home,
            sort_field: sort_field_text(&imp.nt, &fields),
            checksum: note_checksum(&imp.nt, &fields),
            fields,
            tags: dedup_tags(tags),
            created_at: created,
            updated_at: ms(an.modified.saturating_mul(1000), created),
            deleted_at: None,
        };
        used_nts.insert(an.mid);

        for ac in acards {
            let Some((template_key, card_type)) = imp.card_key(ac.ord) else {
                continue;
            };
            let Some(deck) = deck_of(home_did(ac)) else {
                continue;
            };
            let due_raw = if ac.odid != 0 && ac.odue != 0 {
                ac.odue
            } else {
                ac.due
            };
            let state = match ac.ctype {
                1 => CardState::Learning,
                2 => CardState::Review,
                3 => CardState::Relearning,
                _ => CardState::New,
            };
            let due = match state {
                CardState::New => None,
                CardState::Review => Some(Timestamp(day_to_ms(col.crt, due_raw))),
                CardState::Learning | CardState::Relearning => {
                    Some(Timestamp(if due_raw > 1_000_000_000 {
                        due_raw * 1000
                    } else {
                        day_to_ms(col.crt, due_raw)
                    }))
                }
            };
            let fsrs: FsrsData = serde_json::from_str(&ac.data).unwrap_or_default();
            let stability = fsrs.s.or((ac.ivl > 0).then_some(ac.ivl as f64));
            let last = last_review
                .get(&ac.id)
                .map(|t| Timestamp(*t))
                .or_else(|| fsrs.lrt.filter(|t| *t > 0).map(|t| Timestamp(t * 1000)))
                .or_else(|| {
                    (state == CardState::Review && ac.ivl > 0)
                        .then(|| due.map(|d| Timestamp(d.0 - ac.ivl * DAY_MS)))
                        .flatten()
                });
            let id = CardId::from_uuid(derived_id(
                ac.id,
                &format!("card:{}:{}:{}", an.guid, ac.ord, ac.id),
            ));
            card_ids.insert(ac.id, (id, state));
            let created = ms(ac.id, now);
            pack.cards.push(Card {
                id,
                note_id,
                deck_id: deck,
                template_key,
                card_type,
                scheduling: SchedulingState {
                    state,
                    due,
                    stability: if state == CardState::New {
                        None
                    } else {
                        stability
                    },
                    difficulty: fsrs.d,
                    reps: u32::try_from(ac.reps).unwrap_or(0),
                    lapses: u32::try_from(ac.lapses).unwrap_or(0),
                    last_review: last,
                },
                // Anki stores the flag number; `cards.flags` is a bit per flag.
                flags: match ac.flags & 7 {
                    0 => 0,
                    n => 1 << (n - 1),
                },
                suspended: ac.queue == -1,
                // Anki buries (−2 by sibling, −3 by hand) until the next day.
                buried_until: matches!(ac.queue, -2 | -3).then_some(Timestamp(now.0 + DAY_MS)),
                user_note: None,
                created_at: created,
                updated_at: ms(ac.modified.saturating_mul(1000), created),
                deleted_at: None,
            });
        }
        pack.notes.push(note);
    }
    report.skipped_cards = col.cards.len().saturating_sub(card_ids.len());

    for r in &col.revlog {
        let (Some((card, _)), Some(rating), true) =
            (card_ids.get(&r.cid), rating(r.ease), r.rtype != 4)
        else {
            report.skipped_reviews += 1;
            continue;
        };
        let state_before = match r.rtype {
            0 => CardState::Learning,
            2 => CardState::Relearning,
            _ => CardState::Review,
        };
        let state_after = match (rating, state_before) {
            (Rating::Again, CardState::Review | CardState::Relearning) => CardState::Relearning,
            (Rating::Again, _) => CardState::Learning,
            _ if r.ivl > 0 => CardState::Review,
            _ => state_before,
        };
        pack.review_logs.push(ReviewEntry {
            id: ReviewLogId::from_uuid(derived_id(r.id, &format!("rev:{card}:{}", r.id))),
            card_id: *card,
            session_id: None,
            rating,
            state_before,
            state_after,
            scheduled_days: r.ivl.max(0),
            elapsed_days: r.last_ivl.max(0),
            duration_ms: r.time.max(0),
            answer_duration_ms: None,
            hints_used: 0,
            mode: "normal".into(),
            stability_after: None,
            difficulty_after: None,
            created_at: Timestamp(r.id),
        });
    }

    // Several Anki note types can land on the same built-in.
    let mut seen_nts = HashSet::new();
    pack.notetypes = used_nts
        .iter()
        .filter_map(|mid| notetypes.get(mid).map(|i| i.nt.clone()))
        .filter(|nt| seen_nts.insert(nt.id))
        .collect();
    report.missing_media = missing.into_iter().collect();
    (pack, report)
}

// ------------------------------------------------------------------ export

/// Allocates unique Anki ids near the wanted (creation-time) value.
#[derive(Default)]
struct IdAlloc(HashSet<i64>);

impl IdAlloc {
    fn take(&mut self, want: i64) -> i64 {
        let mut v = want.max(1);
        while !self.0.insert(v) {
            v += 1;
        }
        v
    }
}

/// Builds an Anki collection from `pack` (decks by path, note types with templates rewritten
/// to field names, cards with scheduling when `include_scheduling`, media by file name).
pub fn from_pack(pack: &Pack, include_scheduling: bool, now: Timestamp) -> AnkiCollection {
    let now_s = now.0.div_euclid(1000);
    let earliest_due = pack
        .cards
        .iter()
        .filter(|c| include_scheduling && c.scheduling.state != CardState::New)
        .filter_map(|c| c.scheduling.due.map(|d| d.0.div_euclid(1000)))
        .min()
        .unwrap_or(now_s);
    let crt = now_s.min(earliest_due).div_euclid(86_400) * 86_400;

    // Media names.
    let mut used_names: HashSet<String> = HashSet::new();
    let mut hash_to_name: HashMap<String, String> = HashMap::new();
    let mut media = Vec::new();
    for m in &pack.media {
        let base = m
            .name
            .clone()
            .unwrap_or_else(|| format!("{}.{}", m.hash, extension_for(&m.mime)))
            .replace(['/', '\\'], "_");
        let name = if used_names.insert(base.clone()) {
            base
        } else {
            let alt = format!("{}_{}", &m.hash[..8.min(m.hash.len())], base);
            used_names.insert(alt.clone());
            alt
        };
        hash_to_name.insert(m.hash.clone(), name.clone());
        media.push((name, m.bytes.clone()));
    }
    let to_names = |html: &str| {
        rewrite(html, |r| {
            hash_of(r).and_then(|h| hash_to_name.get(h).cloned())
        })
    };

    // Note types.
    let mut nt_ids = IdAlloc::default();
    let mut notetypes = Vec::new();
    let mut mids: HashMap<NoteTypeId, (i64, &NoteType)> = HashMap::new();
    let mut occlusion_nts: HashSet<NoteTypeId> = HashSet::new();
    for nt in &pack.notetypes {
        let mid = nt_ids.take(nt.created_at.0);
        if is_occlusion_builtin(nt) {
            // Anki's own image occlusion note type, so Anki draws and edits the masks.
            notetypes.push(occlusion::notetype(mid, &nt.name));
            occlusion_nts.insert(nt.id);
            mids.insert(nt.id, (mid, nt));
            continue;
        }
        let names: HashMap<String, String> = nt
            .fields
            .iter()
            .map(|f| (f.key.0.clone(), f.name.clone()))
            .collect();
        let to_field_names = |t: &str| map_field_refs(t, |k| names.get(k).cloned());
        let cloze = nt.kind == NoteTypeKind::Cloze;
        let mut templates: Vec<AnkiTemplate> = nt
            .templates
            .iter()
            .map(|t| AnkiTemplate {
                name: t.name.clone(),
                qfmt: to_field_names(&t.front),
                afmt: to_field_names(&t.back),
            })
            .collect();
        if templates.is_empty() {
            let first = nt.fields.first().map_or("Text", |f| f.name.as_str());
            templates.push(AnkiTemplate {
                name: "Cloze".into(),
                qfmt: format!("{{{{cloze:{first}}}}}"),
                afmt: format!("{{{{cloze:{first}}}}}"),
            });
        }
        if cloze {
            templates.truncate(1);
        }
        notetypes.push(AnkiNotetype {
            id: mid,
            name: nt.name.clone(),
            kind: i64::from(cloze),
            sort_field: nt
                .fields
                .iter()
                .position(|f| f.key == nt.sort_field)
                .unwrap_or(0),
            css: nt.css.clone(),
            fields: nt.fields.iter().map(|f| f.name.clone()).collect(),
            templates,
        });
        mids.insert(nt.id, (mid, nt));
    }

    // Decks.
    let mut deck_alloc = IdAlloc::default();
    deck_alloc.take(1);
    let mut decks = vec![AnkiDeck {
        id: 1,
        name: "Default".into(),
        description: String::new(),
        filtered: false,
    }];
    let mut dids: HashMap<DeckId, i64> = HashMap::new();
    for d in &pack.decks {
        let name = pack.deck_path(d.id).join("::");
        if name.to_lowercase() == "default" {
            // Anki's own deck 1; a second "Default" would come back as "Default_".
            dids.insert(d.id, 1);
            decks[0].description = d.description.clone();
            continue;
        }
        let did = deck_alloc.take(d.created_at.0);
        dids.insert(d.id, did);
        decks.push(AnkiDeck {
            id: did,
            name,
            description: d.description.clone(),
            filtered: false,
        });
    }

    // Notes.
    let mut note_alloc = IdAlloc::default();
    let mut nids: HashMap<NoteId, i64> = HashMap::new();
    let mut notes = Vec::new();
    for n in &pack.notes {
        let Some((mid, nt)) = mids.get(&n.notetype_id) else {
            continue;
        };
        let nid = note_alloc.take(n.created_at.0);
        nids.insert(n.id, nid);
        let value = |key: &str, kind: FieldKind| {
            let v = n.fields.get(&FieldKey::new(key)).map_or("", String::as_str);
            to_names(&anki_field(kind, v))
        };
        let fields: Vec<String> = if occlusion_nts.contains(&n.notetype_id) {
            let masks = n
                .fields
                .get(&FieldKey::new(MASKS))
                .map_or("", String::as_str);
            vec![
                occlusion::shapes(masks),
                value("image", FieldKind::Image),
                value("header", FieldKind::RichText),
                value("extra", FieldKind::RichText),
                String::new(),
            ]
        } else {
            nt.fields
                .iter()
                .map(|f| {
                    let v = n.fields.get(&f.key).map_or("", String::as_str);
                    to_names(&anki_field(f.kind, v))
                })
                .collect()
        };
        notes.push(AnkiNote {
            id: nid,
            guid: n.id.to_string(),
            mid: *mid,
            modified: n.updated_at.0.div_euclid(1000),
            tags: n
                .tags
                .iter()
                .map(|t| t.as_str().split_whitespace().collect::<Vec<_>>().join("_"))
                .collect(),
            fields,
        });
    }
    let note_nt: HashMap<NoteId, &NoteType> = pack
        .notes
        .iter()
        .filter_map(|n| mids.get(&n.notetype_id).map(|(_, nt)| (n.id, *nt)))
        .collect();

    // Cards.
    let mut card_alloc = IdAlloc::default();
    let mut cids: HashMap<CardId, i64> = HashMap::new();
    let mut used_ords: HashSet<(i64, i64)> = HashSet::new();
    let mut cards = Vec::new();
    for (pos, c) in pack.cards.iter().enumerate() {
        let (Some(nid), Some(nt)) = (nids.get(&c.note_id), note_nt.get(&c.note_id)) else {
            continue;
        };
        let numbered = |prefix: char| {
            c.template_key
                .strip_prefix(prefix)
                .and_then(|n| n.parse::<i64>().ok())
                .map_or(0, |n| (n - 1).max(0))
        };
        let ord = if occlusion_nts.contains(&nt.id) {
            numbered('m')
        } else if nt.kind == NoteTypeKind::Cloze {
            c.template_key
                .strip_prefix('c')
                .and_then(|n| n.parse::<i64>().ok())
                .map_or(0, |n| (n - 1).max(0))
        } else {
            nt.templates
                .iter()
                .position(|t| t.key == c.template_key)
                .map_or(0, |p| i64::try_from(p).unwrap_or(0))
        };
        // Anki allows one card per (note, ord).
        if !used_ords.insert((*nid, ord)) {
            continue;
        }
        let cid = card_alloc.take(c.created_at.0);
        cids.insert(c.id, cid);
        let s = &c.scheduling;
        let position = i64::try_from(pos).unwrap_or(0) + 1;
        let sched = include_scheduling && s.state != CardState::New;
        // Anki's interval is the scheduled gap (last review → due); stability only stands in
        // when one of them is unknown.
        let interval = || {
            match (s.due, s.last_review) {
                (Some(d), Some(l)) if d.0 > l.0 => {
                    Some((d.0 - l.0 + DAY_MS / 2).div_euclid(DAY_MS))
                }
                _ => None,
            }
            .or_else(|| s.stability.map(|x| x.round() as i64))
            .unwrap_or(1)
            .max(1)
        };
        let due_day = |t: Timestamp| (t.0.div_euclid(1000) - crt).div_euclid(86_400);
        let (ctype, queue, due, ivl, factor) = if !sched {
            (0, 0, position, 0, 0)
        } else {
            let due_ts = s.due.unwrap_or(now);
            match s.state {
                CardState::Review => (2, 2, due_day(due_ts), interval(), 2500),
                CardState::Relearning => (3, 1, due_ts.0.div_euclid(1000), interval(), 2500),
                _ => (1, 1, due_ts.0.div_euclid(1000), 0, 2500),
            }
        };
        // Anki's card data: FSRS memory (`s`, `d`) and the last review time (`lrt`, s), which
        // Anki 23.10+ otherwise fills in on "Check Database".
        let mut data = serde_json::Map::new();
        if let (true, Some(st), Some(d)) = (sched, s.stability, s.difficulty) {
            data.insert("s".into(), st.into());
            data.insert("d".into(), d.into());
        }
        if let (true, Some(l)) = (sched, s.last_review) {
            data.insert("lrt".into(), l.0.div_euclid(1000).into());
        }
        let data = if data.is_empty() {
            String::new()
        } else {
            serde_json::Value::Object(data).to_string()
        };
        cards.push(AnkiCard {
            id: cid,
            nid: *nid,
            did: dids.get(&c.deck_id).copied().unwrap_or(1),
            ord,
            modified: c.updated_at.0.div_euclid(1000),
            ctype,
            queue: if c.suspended {
                -1
            } else if include_scheduling && c.buried_until.is_some_and(|b| b > now) {
                -3
            } else {
                queue
            },
            due,
            ivl,
            factor,
            reps: if include_scheduling {
                i64::from(s.reps)
            } else {
                0
            },
            lapses: if include_scheduling {
                i64::from(s.lapses)
            } else {
                0
            },
            left: 0,
            odue: 0,
            odid: 0,
            // One flag per card in Anki: the lowest one set.
            flags: if c.flags & 0x7f == 0 {
                0
            } else {
                i64::from((c.flags & 0x7f).trailing_zeros() + 1)
            },
            data,
        });
    }

    // Review log.
    let mut rev_alloc = IdAlloc::default();
    let mut revlog = Vec::new();
    if include_scheduling {
        for r in &pack.review_logs {
            let Some(cid) = cids.get(&r.card_id) else {
                continue;
            };
            revlog.push(AnkiRevlog {
                id: rev_alloc.take(r.created_at.0),
                cid: *cid,
                ease: match r.rating {
                    Rating::Again => 1,
                    Rating::Hard => 2,
                    Rating::Good => 3,
                    Rating::Easy => 4,
                },
                ivl: r.scheduled_days,
                last_ivl: r.elapsed_days,
                factor: 2500,
                time: r.duration_ms.clamp(0, i64::from(i32::MAX)),
                rtype: match r.state_before {
                    CardState::New | CardState::Learning => 0,
                    CardState::Review => 1,
                    CardState::Relearning => 2,
                },
            });
        }
    }

    AnkiCollection {
        layout: AnkiLayout::Legacy1,
        crt,
        notetypes,
        decks,
        notes,
        cards,
        revlog,
        media,
    }
}
