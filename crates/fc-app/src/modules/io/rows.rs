//! Row-based imports (text/CSV, Quizlet, JSON): parsed rows before they meet the database,
//! and the text → classification value mapping.

use super::{interop_err, text_options};
use crate::error::AppResult;
use fc_api::v1::io::{ColumnTarget, DuplicatePolicy, ImportFormat, StartImport};
use fc_core::attribute::{AttributeDef, AttributeKind, AttributeValue};
use fc_core::{AttributeDefId, DeckId, NoteTypeId};
use fc_interop::json;
use fc_interop::text::{detect, parse_delimited, TextOptions};

/// How a row names a classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttrRef {
    /// By id (column mapping).
    Id(AttributeDefId),
    /// By key or name (JSON).
    Name(String),
}

/// One incoming note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowSpec {
    /// 1-based source row/note number.
    pub row: u32,
    /// Deck path (JSON); `None` = the import's deck.
    pub deck: Option<String>,
    /// Note type id or name (JSON); `None` = the import's note type.
    pub notetype: Option<String>,
    /// `(field key or name, value)`.
    pub fields: Vec<(String, String)>,
    /// Raw tags.
    pub tags: Vec<String>,
    /// `(classification, raw value)`.
    pub attrs: Vec<(AttrRef, String)>,
}

/// A staged row import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowsImport {
    /// Rows.
    pub rows: Vec<RowSpec>,
    /// Default deck.
    pub deck_id: Option<DeckId>,
    /// Default note type.
    pub notetype_id: NoteTypeId,
    /// Existing notes.
    pub duplicates: DuplicatePolicy,
    /// Create options for unknown labels.
    pub create_missing_options: bool,
}

/// Separators to use: the given ones, else detected (Quizlet keeps two columns).
pub(crate) fn effective_options(
    given: Option<TextOptions>,
    text: &str,
    format: ImportFormat,
) -> (TextOptions, Option<f32>) {
    match given {
        Some(o) => (o, None),
        None => {
            let d = detect(text);
            let mut o = d.options;
            if format == ImportFormat::Quizlet {
                o.max_fields = Some(2);
            }
            (o, Some(d.confidence))
        }
    }
}

/// Default mapping: the note type's fields in order, then nothing.
pub(crate) fn default_columns(field_keys: &[String], columns: usize) -> Vec<ColumnTarget> {
    (0..columns)
        .map(|i| match field_keys.get(i) {
            Some(k) => ColumnTarget::Field { key: k.clone() },
            None => ColumnTarget::Ignore,
        })
        .collect()
}

fn split_tags(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

/// Text/Quizlet source → rows.
pub(crate) fn from_text(
    req: &StartImport,
    text: &str,
    format: ImportFormat,
    notetype_id: NoteTypeId,
    field_keys: &[String],
) -> AppResult<RowsImport> {
    let given = req.text_options.as_ref().map(text_options);
    let (opts, _) = effective_options(given, text, format);
    let mut records = parse_delimited(text, &opts).map_err(interop_err)?;
    let first_row = if req.skip_first_row.unwrap_or(false) && !records.is_empty() {
        records.remove(0);
        2
    } else {
        1
    };
    let width = records.iter().map(Vec::len).max().unwrap_or(0);
    let columns = match &req.columns {
        Some(c) if !c.is_empty() => c.clone(),
        _ => default_columns(field_keys, width),
    };
    let rows = records
        .into_iter()
        .enumerate()
        .map(|(i, rec)| {
            let mut row = RowSpec {
                row: u32::try_from(i).unwrap_or(u32::MAX).saturating_add(first_row),
                deck: None,
                notetype: None,
                fields: Vec::new(),
                tags: Vec::new(),
                attrs: Vec::new(),
            };
            for (value, target) in rec.into_iter().zip(&columns) {
                match target {
                    ColumnTarget::Ignore => {}
                    ColumnTarget::Field { key } => row.fields.push((key.clone(), value)),
                    ColumnTarget::Tags => row.tags.extend(split_tags(&value)),
                    ColumnTarget::Attribute { attr_id } => {
                        row.attrs.push((AttrRef::Id(*attr_id), value));
                    }
                }
            }
            row
        })
        .collect();
    Ok(RowsImport {
        rows,
        deck_id: req.deck_id,
        notetype_id,
        duplicates: req.duplicates.unwrap_or_default(),
        create_missing_options: req.create_missing_options.unwrap_or(false),
    })
}

/// JSON source → rows.
pub(crate) fn from_json(
    req: &StartImport,
    text: &str,
    notetype_id: NoteTypeId,
) -> AppResult<RowsImport> {
    let doc = json::parse(text).map_err(interop_err)?;
    let rows = doc
        .notes
        .into_iter()
        .enumerate()
        .map(|(i, n)| RowSpec {
            row: u32::try_from(i + 1).unwrap_or(u32::MAX),
            deck: n.deck,
            notetype: n.notetype,
            fields: n.fields.into_iter().collect(),
            tags: n.tags,
            attrs: n
                .attributes
                .iter()
                .filter_map(|(k, v)| json::value_text(v).map(|t| (AttrRef::Name(k.clone()), t)))
                .collect(),
        })
        .collect();
    Ok(RowsImport {
        rows,
        deck_id: req.deck_id,
        notetype_id,
        duplicates: req.duplicates.unwrap_or_default(),
        create_missing_options: req.create_missing_options.unwrap_or(false),
    })
}

/// A raw classification value read against its definition.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ParsedValue {
    /// Nothing given.
    Empty,
    /// Ready to store (still validated by the core on write).
    Value(AttributeValue),
    /// Option labels that do not exist yet.
    MissingLabels(Vec<String>),
    /// Not readable as this kind.
    Invalid,
}

/// Reads `text` as a value of `def`: option labels (case-insensitive; multi-choice split on
/// `|` or `,`), numbers (`,` or `.` decimals), booleans (`true/false`, `1/0`, `yes/no`,
/// `evet/hayır`, `x`), dates and text as given.
pub(crate) fn parse_value(def: &AttributeDef, text: &str) -> ParsedValue {
    let text = text.trim();
    if text.is_empty() {
        return ParsedValue::Empty;
    }
    match def.kind {
        AttributeKind::OrderedEnum | AttributeKind::Enum => match def.option_by_label(text) {
            Some(o) => ParsedValue::Value(AttributeValue::Choice(o.id)),
            None => ParsedValue::MissingLabels(vec![text.to_owned()]),
        },
        AttributeKind::MultiEnum => {
            let labels: Vec<&str> = text
                .split(['|', ','])
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            let missing: Vec<String> = labels
                .iter()
                .filter(|l| def.option_by_label(l).is_none())
                .map(|l| (*l).to_owned())
                .collect();
            if !missing.is_empty() {
                return ParsedValue::MissingLabels(missing);
            }
            let mut ids = Vec::new();
            for l in labels {
                if let Some(o) = def.option_by_label(l) {
                    if !ids.contains(&o.id) {
                        ids.push(o.id);
                    }
                }
            }
            ParsedValue::Value(AttributeValue::Choices(ids))
        }
        AttributeKind::Number => match text.replace(',', ".").parse::<f64>() {
            Ok(n) if n.is_finite() => ParsedValue::Value(AttributeValue::Number(n)),
            _ => ParsedValue::Invalid,
        },
        AttributeKind::Bool => match text.to_lowercase().as_str() {
            "true" | "1" | "yes" | "y" | "evet" | "e" | "x" => {
                ParsedValue::Value(AttributeValue::Bool(true))
            }
            "false" | "0" | "no" | "n" | "hayır" | "hayir" | "h" => {
                ParsedValue::Value(AttributeValue::Bool(false))
            }
            _ => ParsedValue::Invalid,
        },
        AttributeKind::Date => ParsedValue::Value(AttributeValue::Date(text.to_owned())),
        AttributeKind::Text => ParsedValue::Value(AttributeValue::Text(text.to_owned())),
    }
}
