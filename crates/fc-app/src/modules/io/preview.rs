//! `io.import.preview`: separators (detected or given), first rows, suggested column mapping,
//! package contents.

use super::rows::{default_columns, effective_options};
use super::{format_of, interop_err, read_file, text_options, text_options_dto, decode_text};
use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::support::attrs::effective_for_deck;
use crate::support::decks::DeckIndex;
use crate::support::notes::require_notetype;
use fc_api::v1::io::*;
use fc_core::attribute::AttributeTarget;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::notetype::NoteType;
use fc_interop::pack::Pack;
use fc_interop::text::parse_delimited;
use fc_interop::{anki, fcpack, json};

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 200;

fn summary(pack: &Pack) -> PackageSummary {
    let n = |x: usize| u32::try_from(x).unwrap_or(u32::MAX);
    PackageSummary {
        decks: pack
            .decks
            .iter()
            .map(|d| pack.deck_path(d.id).join("::"))
            .collect(),
        notetypes: n(pack.notetypes.len()),
        notes: n(pack.notes.len()),
        cards: n(pack.cards.len()),
        review_logs: n(pack.review_logs.len()),
        media: n(pack.media.len()),
        attribute_defs: n(pack.attribute_defs.len()),
    }
}

/// Header-aware mapping: a first-row cell naming a field (key or name), `tags`/`etiketler`
/// or a classification applicable to the deck maps that column; otherwise fields in order.
fn suggest(
    ctx: &QueryCtx<'_>,
    nt: &NoteType,
    deck: Option<fc_core::DeckId>,
    header: Option<&Vec<String>>,
    columns: usize,
) -> AppResult<Vec<ColumnTarget>> {
    let keys: Vec<String> = nt.fields.iter().map(|f| f.key.0.clone()).collect();
    let Some(header) = header else {
        return Ok(default_columns(&keys, columns));
    };
    let attrs_on = crate::support::module_enabled(
        ctx.tx,
        ctx.registry,
        crate::modules::attributes::ID,
    )?;
    let defs = match deck {
        // Module off: classifications are hidden, never suggested as a column target.
        Some(_) if !attrs_on => Vec::new(),
        Some(d) => {
            let index = DeckIndex::load(ctx.tx)?;
            effective_for_deck(ctx.tx, &index, Some(d))?
        }
        None => Vec::new(),
    };
    let mut matched = 0;
    let mut out: Vec<ColumnTarget> = Vec::new();
    for cell in header.iter().take(columns) {
        let c = cell.trim().to_lowercase();
        let target = if let Some(f) = nt
            .fields
            .iter()
            .find(|f| f.key.0.to_lowercase() == c || f.name.to_lowercase() == c)
        {
            Some(ColumnTarget::Field { key: f.key.0.clone() })
        } else if ["tags", "tag", "etiketler", "etiket"].contains(&c.as_str()) {
            Some(ColumnTarget::Tags)
        } else {
            defs.iter()
                .map(|(d, _)| d)
                .filter(|d| d.target == AttributeTarget::Note)
                .find(|d| d.key.to_lowercase() == c || d.name.to_lowercase() == c)
                .map(|d| ColumnTarget::Attribute { attr_id: d.id })
        };
        if target.is_some() {
            matched += 1;
        }
        out.push(target.unwrap_or(ColumnTarget::Ignore));
    }
    if matched == 0 {
        return Ok(default_columns(&keys, columns));
    }
    out.resize(columns, ColumnTarget::Ignore);
    Ok(out)
}

/// See the module docs.
pub(super) fn preview(req: PreviewImport, ctx: &QueryCtx<'_>) -> AppResult<ImportPreview> {
    let (bytes, path) = match &req.source {
        ImportSource::File { path } => (read_file(path)?, Some(path.as_str())),
        ImportSource::Text { text } => (text.clone().into_bytes(), None),
    };
    let format = format_of(req.format, path);
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let nt = require_notetype(ctx.tx, req.notetype_id.unwrap_or(CLASSIC_ID))?;
    let empty = |format| ImportPreview {
        format,
        text_options: None,
        confidence: None,
        columns: 0,
        rows: Vec::new(),
        total_rows: 0,
        suggested_columns: Vec::new(),
        package: None,
    };
    Ok(match format {
        ImportFormat::Text | ImportFormat::Quizlet => {
            let text = decode_text(&bytes);
            let given = req.text_options.as_ref().map(text_options);
            let (opts, confidence) = effective_options(given, &text, format);
            let records = parse_delimited(&text, &opts).map_err(interop_err)?;
            let columns = records.iter().map(Vec::len).max().unwrap_or(0);
            ImportPreview {
                suggested_columns: suggest(ctx, &nt, req.deck_id, records.first(), columns)?,
                text_options: Some(text_options_dto(&opts)),
                confidence,
                columns: u32::try_from(columns).unwrap_or(u32::MAX),
                total_rows: u32::try_from(records.len()).unwrap_or(u32::MAX),
                rows: records.into_iter().take(limit).collect(),
                ..empty(format)
            }
        }
        ImportFormat::Json => {
            let doc = json::parse(&decode_text(&bytes)).map_err(interop_err)?;
            let rows: Vec<Vec<String>> = doc
                .notes
                .iter()
                .map(|n| {
                    nt.fields
                        .iter()
                        .map(|f| {
                            n.fields
                                .get(&f.key.0)
                                .or_else(|| n.fields.get(&f.name))
                                .cloned()
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .collect();
            ImportPreview {
                columns: u32::try_from(nt.fields.len()).unwrap_or(u32::MAX),
                total_rows: u32::try_from(rows.len()).unwrap_or(u32::MAX),
                rows: rows.into_iter().take(limit).collect(),
                ..empty(format)
            }
        }
        ImportFormat::Anki => {
            let col = anki::read_package(&bytes).map_err(interop_err)?;
            let (pack, _) = anki::to_pack(&col, ctx.now);
            ImportPreview {
                package: Some(summary(&pack)),
                total_rows: u32::try_from(pack.notes.len()).unwrap_or(u32::MAX),
                ..empty(format)
            }
        }
        ImportFormat::Fcpack => {
            let (_, pack) = fcpack::read(&bytes).map_err(interop_err)?;
            ImportPreview {
                package: Some(summary(&pack)),
                total_rows: u32::try_from(pack.notes.len()).unwrap_or(u32::MAX),
                ..empty(format)
            }
        }
    })
}
