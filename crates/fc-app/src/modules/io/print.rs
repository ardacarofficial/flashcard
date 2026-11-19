//! `io.print.render`: a deck as printable HTML (text only).

use super::export::collect_pack;
use crate::error::AppResult;
use crate::handler::QueryCtx;
use fc_api::v1::io::{PrintDocument, PrintLayoutDto, RenderPrint};
use fc_core::note::strip_html;
use fc_interop::print::{render as render_html, PrintItem, PrintLayout};

/// Front = the note's sort field; back = its other fields, one per line.
pub(super) fn render(req: RenderPrint, ctx: &QueryCtx<'_>) -> AppResult<PrintDocument> {
    let pack = collect_pack(ctx.tx, Some(req.deck_id), false)?;
    let mut notes = pack.notes.clone();
    notes.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
    let items: Vec<PrintItem> = notes
        .iter()
        .filter_map(|n| {
            let nt = pack.notetypes.iter().find(|t| t.id == n.notetype_id)?;
            let front = strip_html(n.fields.get(&nt.sort_field).map_or("", String::as_str));
            let back = nt
                .fields
                .iter()
                .filter(|f| f.key != nt.sort_field)
                .filter_map(|f| n.fields.get(&f.key))
                .map(|v| strip_html(v))
                .filter(|v| !v.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Some(PrintItem { front, back })
        })
        .collect();
    let layout = match req.layout {
        PrintLayoutDto::Cards { columns, rows } => PrintLayout::Cards { columns, rows },
        PrintLayoutDto::List => PrintLayout::List,
    };
    let title = pack
        .decks
        .iter()
        .find(|d| d.id == req.deck_id)
        .map(|d| d.name.clone())
        .unwrap_or_default();
    Ok(PrintDocument {
        html: render_html(&items, layout, &title),
        cards: u32::try_from(items.len()).unwrap_or(u32::MAX),
    })
}
