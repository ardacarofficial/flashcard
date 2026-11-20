//! Exporting image occlusion notes in Anki's own format (Anki 23.10+): the stock "Image
//! Occlusion" note type (cloze kind, `originalStockKind` 6) whose `Occlusion` field holds
//! one `{{c<n>::image-occlusion:rect:left=…:top=…:width=…:height=…:oi=1}}` per mask.
//! Import is [`super::convert`]'s `occlusion_masks`.

use super::{AnkiNotetype, AnkiTemplate};
use serde_json::Value;
use std::fmt::Write as _;

/// Anki's stock kind number of the image occlusion note type (`originalStockKind`).
pub const STOCK_KIND: i64 = 6;

/// Field names of Anki's note type, in order.
pub const FIELDS: [&str; 5] = ["Occlusion", "Image", "Header", "Back Extra", "Comments"];

const QUESTION: &str = r#"{{#Header}}<div>{{Header}}</div>{{/Header}}
<div style="display: none">{{cloze:Occlusion}}</div>
<div id="err"></div>
<div id="image-occlusion-container">
    {{Image}}
    <canvas id="image-occlusion-canvas"></canvas>
</div>
<script>
try {
    anki.imageOcclusion.setup();
} catch (exc) {
    document.getElementById("err").innerHTML = `Error loading image occlusion. Is your Anki version up to date?<br><br>${exc}`;
}
</script>
"#;

const ANSWER_TAIL: &str = r#"
<div><button id="toggle">Toggle Masks</button></div>
{{#Back Extra}}<div>{{Back Extra}}</div>{{/Back Extra}}
"#;

const CSS: &str = "#image-occlusion-canvas {
    --inactive-shape-color: #ffeba2;
    --active-shape-color: #ff8e8e;
    --inactive-shape-border: 1px #212121;
    --active-shape-border: 1px #212121;
    --highlight-shape-color: #ff8e8e00;
    --highlight-shape-border: 1px #ff8e8e;
}

.card {
    font-family: arial;
    font-size: 20px;
    text-align: center;
    color: black;
    background-color: white;
}
";

/// Anki's image occlusion note type (as Anki 23.10+ creates it) under `name`.
pub fn notetype(id: i64, name: &str) -> AnkiNotetype {
    AnkiNotetype {
        id,
        name: name.to_owned(),
        kind: 1,
        sort_field: 0,
        css: CSS.to_owned(),
        fields: FIELDS.iter().map(|f| (*f).to_owned()).collect(),
        templates: vec![AnkiTemplate {
            name: "Image Occlusion".into(),
            qfmt: QUESTION.to_owned(),
            afmt: format!("{QUESTION}{ANSWER_TAIL}"),
        }],
    }
}

/// Whether `nt` is Anki's image occlusion note type (by structure, as on import).
pub fn is_occlusion(nt: &AnkiNotetype) -> bool {
    nt.kind == 1
        && nt
            .templates
            .iter()
            .any(|t| t.qfmt.contains("image-occlusion"))
}

/// The `Occlusion` field for a v0.3 `masks` value (`{"mode", "masks": [{n,x,y,w,h}]}`):
/// one rectangle per mask, `oi=1` when every mask is hidden (`hide_all`). Masks without a
/// number get the next free one, as the card engine does; unreadable JSON gives `""`.
pub fn shapes(masks: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(masks) else {
        return String::new();
    };
    let hide_all = v.get("mode").and_then(Value::as_str) != Some("hide_one");
    let list = v
        .get("masks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let num = |m: &Value, k: &str| m.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let mut next = list
        .iter()
        .filter_map(|m| m.get("n").and_then(Value::as_u64))
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for m in &list {
        let n = match m.get("n").and_then(Value::as_u64).filter(|n| *n > 0) {
            Some(n) => n,
            None => {
                next += 1;
                next
            }
        };
        let (x, y) = (num(m, "x").clamp(0.0, 1.0), num(m, "y").clamp(0.0, 1.0));
        let (w, h) = (
            num(m, "w").clamp(0.0, 1.0 - x),
            num(m, "h").clamp(0.0, 1.0 - y),
        );
        let _ = write!(
            out,
            "{{{{c{n}::image-occlusion:rect:left={}:top={}:width={}:height={}{}}}}}",
            fraction(x),
            fraction(y),
            fraction(w),
            fraction(h),
            if hide_all { ":oi=1" } else { "" }
        );
    }
    out
}

/// `0.1234` (four decimals, trailing zeros dropped), as Anki's editor writes them.
fn fraction(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() {
        "0".into()
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_become_anki_shapes_and_back() {
        let masks = r#"{"mode":"hide_all","masks":[{"n":2,"x":0.1,"y":0.2,"w":0.3,"h":0.4},{"x":0.5,"y":0.5,"w":0.25,"h":0.125}]}"#;
        let s = shapes(masks);
        assert_eq!(
            s,
            "{{c2::image-occlusion:rect:left=0.1:top=0.2:width=0.3:height=0.4:oi=1}}\
             {{c3::image-occlusion:rect:left=0.5:top=0.5:width=0.25:height=0.125:oi=1}}"
        );
        let back: Value =
            serde_json::from_str(&super::super::convert::occlusion_masks(&s)).unwrap();
        assert_eq!(back["mode"], "hide_all");
        assert_eq!(back["masks"][0]["n"], 2);
        assert_eq!(back["masks"][1]["n"], 3);
        assert_eq!(back["masks"][1]["w"], 0.25);
        let one = shapes(r#"{"mode":"hide_one","masks":[{"n":1,"x":0,"y":0,"w":2,"h":0.5}]}"#);
        assert_eq!(
            one,
            "{{c1::image-occlusion:rect:left=0:top=0:width=1:height=0.5}}"
        );
        assert_eq!(shapes("not json"), "");
        assert!(is_occlusion(&notetype(1, "Image Occlusion")));
    }
}
