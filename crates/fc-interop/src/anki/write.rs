//! Writing legacy (schema 11) packages: `collection.anki2` + JSON media list + numbered media
//! files. Imports in every Anki version since 2.0.

use super::schema::{FIELD_SEPARATOR, SCHEMA11};
use super::*;
use crate::error::{InteropError, InteropResult};
use crate::zipio::ZipOut;
use fc_core::note::strip_html;
use fc_core::Timestamp;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};

const FORMAT: &str = "apkg";

fn sql(e: rusqlite::Error) -> InteropError {
    InteropError::invalid(FORMAT, format!("collection: {e}"))
}

/// Anki's duplicate checksum: the first 8 hex digits of SHA-1 over the stripped first field.
pub fn field_checksum(first_field: &str) -> i64 {
    let digest = Sha1::digest(strip_html(first_field).as_bytes());
    i64::from(u32::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3],
    ]))
}

fn models_json(col: &AnkiCollection, now_s: i64) -> Value {
    let mut models = Map::new();
    for nt in &col.notetypes {
        let flds: Vec<Value> = nt
            .fields
            .iter()
            .enumerate()
            .map(|(i, name)| {
                json!({
                    "name": name, "ord": i, "sticky": false, "rtl": false,
                    "font": "Arial", "size": 20, "media": [], "description": ""
                })
            })
            .collect();
        let tmpls: Vec<Value> = nt
            .templates
            .iter()
            .enumerate()
            .map(|(i, t)| {
                json!({
                    "name": t.name, "ord": i, "qfmt": t.qfmt, "afmt": t.afmt,
                    "bqfmt": "", "bafmt": "", "did": null, "bfont": "", "bsize": 0
                })
            })
            .collect();
        let req: Vec<Value> = (0..nt.templates.len())
            .map(|i| json!([i, "any", [0]]))
            .collect();
        models.insert(
            nt.id.to_string(),
            json!({
                "id": nt.id, "name": nt.name, "type": nt.kind, "mod": now_s, "usn": 0,
                "sortf": nt.sort_field, "did": 1, "tmpls": tmpls, "flds": flds, "css": nt.css,
                "latexPre": "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage[utf8]{inputenc}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n",
                "latexPost": "\\end{document}", "latexsvg": false, "req": req, "tags": [],
                "vers": []
            }),
        );
    }
    Value::Object(models)
}

fn decks_json(col: &AnkiCollection, now_s: i64) -> Value {
    let mut decks = Map::new();
    for d in &col.decks {
        decks.insert(
            d.id.to_string(),
            json!({
                "id": d.id, "name": d.name, "mod": now_s, "usn": 0, "desc": d.description,
                "dyn": i64::from(d.filtered), "conf": 1, "collapsed": false,
                "browserCollapsed": false, "extendNew": 0, "extendRev": 0,
                "newToday": [0, 0], "revToday": [0, 0], "lrnToday": [0, 0],
                "timeToday": [0, 0]
            }),
        );
    }
    Value::Object(decks)
}

fn dconf_json(now_s: i64) -> Value {
    json!({ "1": {
        "id": 1, "name": "Default", "mod": now_s, "usn": 0, "maxTaken": 60, "autoplay": true,
        "timer": 0, "replayq": true, "dyn": false,
        "new": { "delays": [1.0, 10.0], "ints": [1, 4, 0], "initialFactor": 2500,
                 "order": 1, "perDay": 20, "bury": false },
        "lapse": { "delays": [10.0], "mult": 0.0, "minInt": 1, "leechFails": 8,
                   "leechAction": 1 },
        "rev": { "perDay": 200, "ease4": 1.3, "ivlFct": 1.0, "maxIvl": 36500,
                 "hardFactor": 1.2, "bury": false }
    }})
}

fn conf_json(col: &AnkiCollection) -> Value {
    json!({
        "curDeck": 1, "activeDecks": [1], "nextPos": col.cards.len() + 1,
        "sortType": "noteFld", "sortBackwards": false, "addToCur": true, "schedVer": 2,
        "newSpread": 0, "collapseTime": 1200, "timeLim": 0, "estTimes": true,
        "dueCounts": true, "curModel": col.notetypes.first().map_or(0, |n| n.id),
        "dayLearnFirst": false
    })
}

/// Builds the schema-11 collection database and returns its bytes.
pub fn collection_bytes(col: &AnkiCollection, now: Timestamp) -> InteropResult<Vec<u8>> {
    let now_s = now.0.div_euclid(1000);
    let mut conn = Connection::open_in_memory().map_err(sql)?;
    conn.execute_batch(SCHEMA11).map_err(sql)?;
    let tx = conn.transaction().map_err(sql)?;
    tx.execute(
        "INSERT INTO col VALUES (1, ?1, ?2, ?2, 11, 0, 0, 0, ?3, ?4, ?5, ?6, '{}')",
        params![
            col.crt,
            now.0,
            conf_json(col).to_string(),
            models_json(col, now_s).to_string(),
            decks_json(col, now_s).to_string(),
            dconf_json(now_s).to_string(),
        ],
    )
    .map_err(sql)?;
    let sort_of: std::collections::HashMap<i64, usize> =
        col.notetypes.iter().map(|n| (n.id, n.sort_field)).collect();
    for n in &col.notes {
        let sort = sort_of.get(&n.mid).copied().unwrap_or(0);
        let sfld = strip_html(n.fields.get(sort).map_or("", String::as_str));
        let csum = field_checksum(n.fields.first().map_or("", String::as_str));
        let tags = if n.tags.is_empty() {
            String::new()
        } else {
            format!(" {} ", n.tags.join(" "))
        };
        let flds = n.fields.join(&FIELD_SEPARATOR.to_string());
        tx.execute(
            "INSERT INTO notes VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, 0, '')",
            params![n.id, n.guid, n.mid, n.modified, tags, flds, sfld, csum],
        )
        .map_err(sql)?;
    }
    for c in &col.cards {
        tx.execute(
            "INSERT INTO cards VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                                       ?14, ?15, ?16, ?17)",
            params![
                c.id, c.nid, c.did, c.ord, c.modified, c.ctype, c.queue, c.due, c.ivl, c.factor,
                c.reps, c.lapses, c.left, c.odue, c.odid, c.flags, c.data
            ],
        )
        .map_err(sql)?;
    }
    for r in &col.revlog {
        tx.execute(
            "INSERT INTO revlog VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![r.id, r.cid, r.ease, r.ivl, r.last_ivl, r.factor, r.time, r.rtype],
        )
        .map_err(sql)?;
    }
    tx.commit().map_err(sql)?;
    let data = conn.serialize(rusqlite::MAIN_DB).map_err(sql)?;
    Ok(data.to_vec())
}

/// Writes `col` as a legacy `.apkg`/`.colpkg` (the two differ only in what they contain).
pub fn write_package(col: &AnkiCollection, now: Timestamp) -> InteropResult<Vec<u8>> {
    let db = collection_bytes(col, now)?;
    let mut zip = ZipOut::new(FORMAT);
    zip.add("collection.anki2", &db, true)?;
    let mut list = Map::new();
    for (i, (name, _)) in col.media.iter().enumerate() {
        list.insert(i.to_string(), Value::String(name.clone()));
    }
    zip.add("media", Value::Object(list).to_string().as_bytes(), true)?;
    for (i, (_, bytes)) in col.media.iter().enumerate() {
        zip.add(&i.to_string(), bytes, true)?;
    }
    zip.finish()
}
