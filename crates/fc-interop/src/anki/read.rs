//! Reading Anki packages (all layouts; see the module table in `anki`).
//!
//! **Hostile collections.** The collection is an SQLite database from an untrusted file, so
//! every name we query must be a real table (a `VIEW` called `notes` with an endless
//! recursive CTE would otherwise hang the worker) without generated columns, and every query
//! runs under a progress handler that aborts it when the caller cancels or
//! [`QUERY_TIME_BUDGET`] runs out.

use super::proto::{self, bytes_field, string_field, varint_field};
use super::schema::DECK_SEPARATOR_18;
use super::*;
use crate::error::{InteropError, InteropResult};
use crate::zipio::ZipIn;
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use std::time::{Duration, Instant};

const FORMAT: &str = "apkg";
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Wall-time budget for all queries against one collection; a query still running after it
/// is aborted (the package is then reported invalid).
pub const QUERY_TIME_BUDGET: Duration = Duration::from_secs(120);
/// SQLite VM instructions between two abort checks.
const PROGRESS_EVERY: std::os::raw::c_int = 10_000;
/// Tables every layout reads.
const BASE_TABLES: [&str; 4] = ["col", "notes", "cards", "revlog"];
/// Extra tables of the latest (schema 18) layout.
const LATEST_TABLES: [&str; 4] = ["notetypes", "fields", "templates", "decks"];

fn sql(e: rusqlite::Error) -> InteropError {
    InteropError::invalid(FORMAT, format!("collection: {e}"))
}

/// Opens an in-memory copy of a collection database.
///
/// Anki 2.1.50+ writes `collection.anki21b` in WAL mode (header bytes 18–19 = 2), which an
/// in-memory database cannot open ("unable to open database file"); the copy is switched to
/// the rollback journal first. Nothing else in the file depends on the journal mode. Its
/// name columns use Anki's own `unicase` collation (case-insensitive), registered here so
/// the tables can be queried at all.
pub(super) fn open_db(bytes: &[u8]) -> InteropResult<Connection> {
    let mut conn = Connection::open_in_memory().map_err(sql)?;
    conn.create_collation("unicase", |a, b| {
        a.to_lowercase()
            .cmp(&b.to_lowercase())
            .then_with(|| a.cmp(b))
    })
    .map_err(sql)?;
    let mut owned;
    let mut bytes = bytes;
    if bytes.len() >= 20 && bytes.starts_with(b"SQLite format 3\0") && bytes[18..20] == [2, 2] {
        owned = bytes.to_vec();
        owned[18] = 1;
        owned[19] = 1;
        bytes = &owned;
    }
    conn.deserialize_read_exact(rusqlite::MAIN_DB, bytes, bytes.len(), true)
        .map_err(sql)?;
    Ok(conn)
}

/// [`open_db`] with the query guard installed: every statement is interrupted once
/// `should_abort` returns `true` or `budget` has passed since opening.
fn open_guarded(
    bytes: &[u8],
    should_abort: impl Fn() -> bool + Send + 'static,
    budget: Duration,
) -> InteropResult<Connection> {
    let mut conn = open_db(bytes)?;
    let deadline = Instant::now().checked_add(budget);
    conn.progress_handler(
        PROGRESS_EVERY,
        Some(move || should_abort() || deadline.is_some_and(|d| Instant::now() >= d)),
    )
    .map_err(sql)?;
    Ok(conn)
}

/// Reads an `.apkg` / `.colpkg` (queries limited to [`QUERY_TIME_BUDGET`]).
pub fn read_package(bytes: &[u8]) -> InteropResult<AnkiCollection> {
    read_package_with_abort(bytes, || false)
}

/// [`read_package`] that also stops (with an error) as soon as `should_abort` returns `true`;
/// it is polled while SQLite runs, so a cancelled import does not wait for a slow query.
pub fn read_package_with_abort(
    bytes: &[u8],
    should_abort: impl Fn() -> bool + Send + 'static,
) -> InteropResult<AnkiCollection> {
    let mut zip = ZipIn::open(bytes, FORMAT)?;
    let (layout, db) = if let Some(b) = zip.read("collection.anki21b")? {
        (AnkiLayout::Latest, zip.zstd(&b)?)
    } else if let Some(b) = zip.read("collection.anki21")? {
        (AnkiLayout::Legacy2, b)
    } else if let Some(b) = zip.read("collection.anki2")? {
        (AnkiLayout::Legacy1, b)
    } else {
        return Err(InteropError::invalid(FORMAT, "no collection inside"));
    };
    let conn = open_guarded(&db, should_abort, QUERY_TIME_BUDGET)?;
    require_tables(&conn, &BASE_TABLES)?;
    let crt: i64 = conn
        .query_row("SELECT crt FROM col LIMIT 1", [], |r| r.get(0))
        .optional()
        .map_err(sql)?
        .unwrap_or(0);
    let (notetypes, decks) = if is_plain_table(&conn, "notetypes")? {
        require_tables(&conn, &LATEST_TABLES)?;
        (latest_notetypes(&conn)?, latest_decks(&conn)?)
    } else {
        legacy_meta(&conn)?
    };
    Ok(AnkiCollection {
        layout,
        crt,
        notetypes,
        decks,
        notes: notes(&conn)?,
        cards: cards(&conn)?,
        revlog: revlog(&conn)?,
        media: media(&mut zip, layout)?,
    })
}

/// Whether `name` is an ordinary table: `type = 'table'` in `sqlite_master` *and* in the
/// parsed schema (`pragma_table_list`, which a hand-edited `sqlite_master` row whose SQL
/// says `CREATE VIEW` or `CREATE VIRTUAL TABLE` cannot fake), with no generated columns.
fn is_plain_table(conn: &Connection, name: &str) -> InteropResult<bool> {
    let count = |q: &str| {
        conn.query_row(q, [name], |r| r.get::<_, i64>(0))
            .map_err(sql)
    };
    Ok(
        count("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")? > 0
            && count(
                "SELECT count(*) FROM pragma_table_list WHERE schema = 'main' AND type = 'table' \
                 AND name = ?1",
            )? > 0
            && count("SELECT count(*) FROM pragma_table_xinfo(?1) WHERE hidden <> 0")? == 0,
    )
}

/// Fails unless every name in `names` is a plain table (see [`is_plain_table`]).
fn require_tables(conn: &Connection, names: &[&str]) -> InteropResult<()> {
    for name in names {
        if !is_plain_table(conn, name)? {
            return Err(InteropError::invalid(
                FORMAT,
                format!("collection: {name} is not a plain table"),
            ));
        }
    }
    Ok(())
}

fn notes(conn: &Connection) -> InteropResult<Vec<AnkiNote>> {
    let mut st = conn
        .prepare("SELECT id, guid, mid, mod, tags, flds FROM notes ORDER BY id")
        .map_err(sql)?;
    let rows = st
        .query_map([], |r| {
            let tags: String = r.get(4)?;
            let flds: String = r.get(5)?;
            Ok(AnkiNote {
                id: r.get(0)?,
                guid: r.get(1)?,
                mid: r.get(2)?,
                modified: r.get(3)?,
                tags: tags.split_whitespace().map(str::to_owned).collect(),
                fields: flds
                    .split(schema::FIELD_SEPARATOR)
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<_, _>>().map_err(sql)
}

fn cards(conn: &Connection) -> InteropResult<Vec<AnkiCard>> {
    let mut st = conn
        .prepare(
            "SELECT id, nid, did, ord, mod, type, queue, due, ivl, factor, reps, lapses, left,
                    odue, odid, flags, data FROM cards ORDER BY id",
        )
        .map_err(sql)?;
    let rows = st
        .query_map([], |r| {
            Ok(AnkiCard {
                id: r.get(0)?,
                nid: r.get(1)?,
                did: r.get(2)?,
                ord: r.get(3)?,
                modified: r.get(4)?,
                ctype: r.get(5)?,
                queue: r.get(6)?,
                due: r.get(7)?,
                ivl: r.get(8)?,
                factor: r.get(9)?,
                reps: r.get(10)?,
                lapses: r.get(11)?,
                left: r.get(12)?,
                odue: r.get(13)?,
                odid: r.get(14)?,
                flags: r.get(15)?,
                data: r.get::<_, Option<String>>(16)?.unwrap_or_default(),
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<_, _>>().map_err(sql)
}

fn revlog(conn: &Connection) -> InteropResult<Vec<AnkiRevlog>> {
    let mut st = conn
        .prepare("SELECT id, cid, ease, ivl, lastIvl, factor, time, type FROM revlog ORDER BY id")
        .map_err(sql)?;
    let rows = st
        .query_map([], |r| {
            Ok(AnkiRevlog {
                id: r.get(0)?,
                cid: r.get(1)?,
                ease: r.get(2)?,
                ivl: r.get(3)?,
                last_ivl: r.get(4)?,
                factor: r.get(5)?,
                time: r.get(6)?,
                rtype: r.get(7)?,
            })
        })
        .map_err(sql)?;
    rows.collect::<Result<_, _>>().map_err(sql)
}

// ------------------------------------------------------------------ legacy (JSON in col)

fn num(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.parse().ok(),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn by_ord(list: Option<&Vec<Value>>) -> Vec<&Value> {
    let mut items: Vec<&Value> = list.map(|l| l.iter().collect()).unwrap_or_default();
    items.sort_by_key(|v| num(&v["ord"]).unwrap_or(0));
    items
}

fn legacy_meta(conn: &Connection) -> InteropResult<(Vec<AnkiNotetype>, Vec<AnkiDeck>)> {
    let (models, decks): (String, String) = conn
        .query_row("SELECT models, decks FROM col LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(sql)?;
    let models: Value = serde_json::from_str(&models)
        .map_err(|e| InteropError::invalid(FORMAT, format!("models: {e}")))?;
    let decks: Value = serde_json::from_str(&decks)
        .map_err(|e| InteropError::invalid(FORMAT, format!("decks: {e}")))?;
    let mut notetypes = Vec::new();
    for (key, m) in models.as_object().into_iter().flatten() {
        let id = num(&m["id"]).or_else(|| key.parse().ok()).unwrap_or(0);
        notetypes.push(AnkiNotetype {
            id,
            name: text(&m["name"]),
            kind: num(&m["type"]).unwrap_or(0),
            sort_field: usize::try_from(num(&m["sortf"]).unwrap_or(0)).unwrap_or(0),
            css: text(&m["css"]),
            fields: by_ord(m["flds"].as_array())
                .into_iter()
                .map(|f| text(&f["name"]))
                .collect(),
            templates: by_ord(m["tmpls"].as_array())
                .into_iter()
                .map(|t| AnkiTemplate {
                    name: text(&t["name"]),
                    qfmt: text(&t["qfmt"]),
                    afmt: text(&t["afmt"]),
                })
                .collect(),
        });
    }
    notetypes.sort_by_key(|n| n.id);
    let mut out_decks = Vec::new();
    for (key, d) in decks.as_object().into_iter().flatten() {
        out_decks.push(AnkiDeck {
            id: num(&d["id"]).or_else(|| key.parse().ok()).unwrap_or(0),
            name: text(&d["name"]),
            description: text(&d["desc"]),
            filtered: num(&d["dyn"]).unwrap_or(0) != 0,
        });
    }
    out_decks.sort_by_key(|d| d.id);
    Ok((notetypes, out_decks))
}

// ------------------------------------------------------------------ latest (schema 18)

fn latest_notetypes(conn: &Connection) -> InteropResult<Vec<AnkiNotetype>> {
    let mut st = conn
        .prepare("SELECT id, name, config FROM notetypes ORDER BY id")
        .map_err(sql)?;
    let heads: Vec<(i64, String, Vec<u8>)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let mut out = Vec::new();
    for (id, name, config) in heads {
        let cfg = proto::decode(&config)?;
        let mut fst = conn
            .prepare("SELECT name FROM fields WHERE ntid = ?1 ORDER BY ord")
            .map_err(sql)?;
        let fields: Vec<String> = fst
            .query_map([id], |r| r.get(0))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        let mut tst = conn
            .prepare("SELECT name, config FROM templates WHERE ntid = ?1 ORDER BY ord")
            .map_err(sql)?;
        let raw: Vec<(String, Vec<u8>)> = tst
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql)?
            .collect::<Result<_, _>>()
            .map_err(sql)?;
        let mut templates = Vec::new();
        for (tname, tconfig) in raw {
            let t = proto::decode(&tconfig)?;
            templates.push(AnkiTemplate {
                name: tname,
                qfmt: string_field(&t, 1),
                afmt: string_field(&t, 2),
            });
        }
        out.push(AnkiNotetype {
            id,
            name,
            kind: i64::try_from(varint_field(&cfg, 1).unwrap_or(0)).unwrap_or(0),
            sort_field: usize::try_from(varint_field(&cfg, 2).unwrap_or(0)).unwrap_or(0),
            css: string_field(&cfg, 3),
            fields,
            templates,
        });
    }
    Ok(out)
}

fn latest_decks(conn: &Connection) -> InteropResult<Vec<AnkiDeck>> {
    let mut st = conn
        .prepare("SELECT id, name, kind FROM decks ORDER BY id")
        .map_err(sql)?;
    let raw: Vec<(i64, String, Vec<u8>)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    let mut out = Vec::new();
    for (id, name, kind) in raw {
        let k = proto::decode(&kind)?;
        let description = match bytes_field(&k, 1) {
            Some(normal) => string_field(&proto::decode(normal)?, 4),
            None => String::new(),
        };
        out.push(AnkiDeck {
            id,
            name: name.replace(DECK_SEPARATOR_18, "::"),
            description,
            filtered: bytes_field(&k, 2).is_some(),
        });
    }
    Ok(out)
}

// ------------------------------------------------------------------ media

fn media(zip: &mut ZipIn<'_>, layout: AnkiLayout) -> InteropResult<Vec<(String, Vec<u8>)>> {
    let Some(list) = zip.read("media")? else {
        return Ok(Vec::new());
    };
    let compressed = list.starts_with(&ZSTD_MAGIC);
    let entries: Vec<(String, String)> = if compressed || layout == AnkiLayout::Latest {
        let raw = if compressed { zip.zstd(&list)? } else { list };
        let msg = proto::decode(&raw)?;
        let mut out = Vec::new();
        for (i, entry) in proto::repeated_bytes(&msg, 1).into_iter().enumerate() {
            let e = proto::decode(entry)?;
            let zip_name = varint_field(&e, 255).map_or_else(|| i.to_string(), |n| n.to_string());
            out.push((zip_name, string_field(&e, 1)));
        }
        out
    } else {
        let map: serde_json::Map<String, Value> = serde_json::from_slice(&list)
            .map_err(|e| InteropError::invalid(FORMAT, format!("media list: {e}")))?;
        let mut out: Vec<(String, String)> = map.into_iter().map(|(k, v)| (k, text(&v))).collect();
        out.sort_by_key(|(k, _)| k.parse::<u64>().unwrap_or(u64::MAX));
        out
    };
    let mut files = Vec::new();
    for (zip_name, name) in entries {
        if name.is_empty() {
            continue;
        }
        let Some(mut bytes) = zip.read(&zip_name)? else {
            continue;
        };
        if layout == AnkiLayout::Latest && bytes.starts_with(&ZSTD_MAGIC) {
            bytes = zip.zstd(&bytes)?;
        }
        files.push((name, bytes));
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
