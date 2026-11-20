//! Printable HTML (docs/02 §16 "Yazdırma / PDF"): the frontend shows it in a sandboxed frame
//! and uses the system print dialog (which also saves PDF). Text only, HTML-escaped.
//!
//! * [`PrintLayout::Cards`]: paper flash cards. Each sheet of fronts is followed by a sheet of
//!   backs with every row mirrored, so double-sided printing lines them up.
//! * [`PrintLayout::List`]: a two-column table.

use serde::{Deserialize, Serialize};

/// Page layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrintLayout {
    /// Grid of cards.
    Cards {
        /// Cards per row (1–6).
        columns: u32,
        /// Rows per sheet (1–10).
        rows: u32,
    },
    /// Term/definition table.
    List,
}

/// One card's printable text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintItem {
    /// Question side.
    pub front: String,
    /// Answer side.
    pub back: String,
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

const STYLE: &str = "@page{margin:10mm}body{font-family:system-ui,sans-serif;margin:0;color:#000}\
.sheet{display:grid;gap:0;page-break-after:always;break-after:page}\
.card{border:1px dashed #999;display:flex;align-items:center;justify-content:center;\
text-align:center;padding:4mm;overflow:hidden;font-size:14pt}\
table{border-collapse:collapse;width:100%}td{border:1px solid #999;padding:2mm;vertical-align:top}\
h1{font-size:14pt}";

/// Renders `items` as a standalone HTML document titled `title`.
pub fn render(items: &[PrintItem], layout: PrintLayout, title: &str) -> String {
    let mut body = String::new();
    match layout {
        PrintLayout::List => {
            body.push_str(&format!("<h1>{}</h1><table>", esc(title)));
            for it in items {
                body.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td></tr>",
                    esc(&it.front),
                    esc(&it.back)
                ));
            }
            body.push_str("</table>");
        }
        PrintLayout::Cards { columns, rows } => {
            let cols = columns.clamp(1, 6) as usize;
            let rows = rows.clamp(1, 10) as usize;
            let per = cols * rows;
            let grid = format!(
                "grid-template-columns:repeat({cols},1fr);grid-auto-rows:calc((277mm - 1px) / {rows})"
            );
            for chunk in items.chunks(per) {
                body.push_str(&format!("<section class=\"sheet\" style=\"{grid}\">"));
                for it in chunk {
                    body.push_str(&format!("<div class=\"card\">{}</div>", esc(&it.front)));
                }
                body.push_str("</section>");
                body.push_str(&format!("<section class=\"sheet\" style=\"{grid}\">"));
                for row in back_order(chunk.len(), cols) {
                    match row {
                        Some(i) => body.push_str(&format!(
                            "<div class=\"card\">{}</div>",
                            esc(&chunk[i].back)
                        )),
                        None => body.push_str("<div class=\"card\"></div>"),
                    }
                }
                body.push_str("</section>");
            }
        }
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>{STYLE}</style></head><body>{body}</body></html>",
        esc(title)
    )
}

/// Cell order of a backs sheet: each row of `cols` mirrored; `None` = blank cell.
pub fn back_order(n: usize, cols: usize) -> Vec<Option<usize>> {
    let mut out = Vec::new();
    let rows = n.div_ceil(cols.max(1));
    for r in 0..rows {
        for c in (0..cols).rev() {
            let i = r * cols + c;
            out.push((i < n).then_some(i));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_are_mirrored_and_text_escaped() {
        assert_eq!(
            back_order(5, 3),
            vec![Some(2), Some(1), Some(0), None, Some(4), Some(3)]
        );
        let items = vec![PrintItem {
            front: "<b>x</b>".into(),
            back: "y".into(),
        }];
        let html = render(
            &items,
            PrintLayout::Cards {
                columns: 2,
                rows: 2,
            },
            "T",
        );
        assert!(html.contains("&lt;b&gt;x&lt;/b&gt;"));
        assert!(!html.contains("<b>x"));
        assert_eq!(html.matches("class=\"sheet\"").count(), 2);
        let list = render(&items, PrintLayout::List, "T");
        assert!(list.contains("<td>y</td>"));
    }
}
