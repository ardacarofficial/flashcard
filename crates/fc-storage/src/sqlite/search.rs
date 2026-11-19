//! Query language backend: [`Filter`] → SQL (with FTS5 for free text), the SQL functions the
//! compiled SQL calls, saved queries and the maintenance reports.
//!
//! Every compiled predicate is wrapped in `coalesce(…, 0)` so it is never `NULL`: SQL's
//! three-valued logic would otherwise make `NOT p` disagree with the in-memory reference
//! evaluator (`fc_core::query::eval`) on cards with missing values.

use super::{all, exec, id, json_list, one, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{
    CardHit, CardSearch, CardSort, IntegrityReport, ReviewEntry, SearchPage, SearchRead,
    SearchWrite,
};
use fc_core::attribute::AttributeTarget;
use fc_core::card::Card;
use fc_core::note::{strip_html, Note};
use fc_core::query::eval::sql_op;
use fc_core::query::filter::{AttrTest, CardNum, Filter, Pred, TimeCol};
use fc_core::query::text::{
    compile_regex, field_glob_matches, fields_text, fold, glob_index_tokens, regex_matches, Glob,
};
use fc_core::query::SavedQuery;
use fc_core::tag::TagName;
use fc_core::{CardId, NoteId, NoteTypeId, SavedQueryId, Timestamp};
use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection};
use std::collections::{BTreeMap, HashMap};

// ------------------------------------------------------------------ SQL functions

fn fields_arg(ctx: &Context<'_>, i: usize) -> rusqlite::Result<BTreeMap<String, String>> {
    let text: String = ctx.get(i)?;
    serde_json::from_str(&text).map_err(|e| rusqlite::Error::UserFunctionError(e.into()))
}

fn keys_arg(ctx: &Context<'_>, i: usize) -> rusqlite::Result<Option<Vec<String>>> {
    match ctx.get::<Option<String>>(i)? {
        None => Ok(None),
        Some(t) => serde_json::from_str(&t)
            .map(Some)
            .map_err(|e| rusqlite::Error::UserFunctionError(e.into())),
    }
}

/// Registers the functions compiled queries use (every connection, readers included):
///
/// * `fc_fold(text)` — [`fold`];
/// * `fc_glob(glob, text)` — whole-text glob;
/// * `fc_text_glob(fields_json, glob)` — glob over all fields' text;
/// * `fc_field_glob(fields_json, keys_json, glob)` — some field matches as a whole;
/// * `fc_html_glob(glob, html)` — stripped HTML matches as a whole (one extracted field);
/// * `fc_regex(fields_json, keys_json | NULL, pattern)` — case-insensitive regex.
pub(crate) fn register_functions(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("fc_fold", 1, flags, |ctx| {
        Ok(ctx.get::<Option<String>>(0)?.map(|s| fold(&s)))
    })?;
    conn.create_scalar_function("fc_glob", 2, flags, |ctx| {
        let g = ctx.get_or_create_aux(0, |v| -> Result<Glob, rusqlite::types::FromSqlError> {
            Ok(Glob::new(v.as_str()?))
        })?;
        let text: Option<String> = ctx.get(1)?;
        Ok(text.is_some_and(|t| g.matches(&t)))
    })?;
    conn.create_scalar_function("fc_html_glob", 2, flags, |ctx| {
        let g = ctx.get_or_create_aux(0, |v| -> Result<Glob, rusqlite::types::FromSqlError> {
            Ok(Glob::new(v.as_str()?))
        })?;
        let html: Option<String> = ctx.get(1)?;
        Ok(html.is_some_and(|h| g.matches(&strip_html(&h))))
    })?;
    conn.create_scalar_function("fc_text_glob", 2, flags, |ctx| {
        let g = ctx.get_or_create_aux(1, |v| -> Result<Glob, rusqlite::types::FromSqlError> {
            Ok(Glob::new(v.as_str()?))
        })?;
        Ok(g.matches(&fields_text(&fields_arg(ctx, 0)?, None)))
    })?;
    conn.create_scalar_function("fc_field_glob", 3, flags, |ctx| {
        let g = ctx.get_or_create_aux(2, |v| -> Result<Glob, rusqlite::types::FromSqlError> {
            Ok(Glob::new(v.as_str()?))
        })?;
        let keys = keys_arg(ctx, 1)?.unwrap_or_default();
        Ok(field_glob_matches(&fields_arg(ctx, 0)?, &keys, &g))
    })?;
    conn.create_scalar_function("fc_regex", 3, flags, |ctx| {
        let re = ctx.get_or_create_aux(2, |v| -> Result<regex::Regex, String> {
            compile_regex(v.as_str().map_err(|e| e.to_string())?)
        })?;
        let keys = keys_arg(ctx, 1)?;
        Ok(regex_matches(&fields_arg(ctx, 0)?, keys.as_deref(), &re))
    })?;
    Ok(())
}

/// Text written to `notes_fts`: folded so dotless `ı` and friends match their plain forms.
pub(crate) fn fts_text(text: &str) -> String {
    fold(text)
}

// ------------------------------------------------------------------ compiler

/// SQL text plus positional parameters.
struct Sql {
    text: String,
    params: Vec<SqlValue>,
}

impl Sql {
    fn param(&mut self, v: impl Into<SqlValue>) -> String {
        self.params.push(v.into());
        format!("?{}", self.params.len())
    }

    /// `(?a, ?b, …)` for short lists (the planner sees the values), a `json_each` subquery for
    /// long ones (SQLite caps the number of parameters).
    fn list<T: std::fmt::Display>(&mut self, items: &[T]) -> String {
        if items.is_empty() {
            return "(NULL)".into();
        }
        if items.len() > 64 {
            return format!("(SELECT value FROM json_each({}))", self.param(json(items)));
        }
        let ps: Vec<String> = items.iter().map(|i| self.param(i.to_string())).collect();
        format!("({})", ps.join(", "))
    }
}

fn json<T: std::fmt::Display>(items: &[T]) -> SqlValue {
    SqlValue::Text(json_list(items))
}

/// FTS5 query for a phrase on the `content` column.
fn fts_phrase(tokens: &[String], prefix: bool) -> String {
    let phrase = tokens.join(" ").replace('"', "\"\"");
    format!("content : \"{phrase}\"{}", if prefix { " *" } else { "" })
}

/// `negated`: whether `f` sits under a `NOT`. Only there must a predicate be made non-`NULL`
/// (`coalesce`); in positive position `NULL` already rejects the row, and leaving the bare
/// predicate lets the planner drive the query from an index (`n.id IN (…)`).
fn compile(f: &Filter, now: Timestamp, negated: bool, out: &mut Sql) -> String {
    match f {
        Filter::All => "1".into(),
        Filter::Nothing => "0".into(),
        Filter::And(v) | Filter::Or(v) => {
            if v.is_empty() {
                return if matches!(f, Filter::And(_)) {
                    "1"
                } else {
                    "0"
                }
                .into();
            }
            let sep = if matches!(f, Filter::And(_)) {
                " AND "
            } else {
                " OR "
            };
            let parts: Vec<String> = v.iter().map(|x| compile(x, now, negated, out)).collect();
            format!("({})", parts.join(sep))
        }
        Filter::Not(x) => format!("(NOT {})", compile(x, now, true, out)),
        Filter::Pred(p) if negated => format!("coalesce(({}), 0)", pred(p, now, out)),
        Filter::Pred(p) => format!("({})", pred(p, now, out)),
    }
}

fn pred(p: &Pred, now: Timestamp, out: &mut Sql) -> String {
    match p {
        Pred::Text { tokens, prefix } => {
            let q = out.param(fts_phrase(tokens, *prefix));
            format!("n.fts_rowid IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH {q})")
        }
        Pred::TextGlob(g) => format!("fc_text_glob(n.fields, {})", out.param(g.clone())),
        Pred::Field {
            keys,
            glob,
            indexed,
        } => {
            // Narrow by the full-text index when possible, then check the field exactly.
            // SQLite's JSON extraction is much cheaper than decoding every note in Rust.
            let toks = if *indexed {
                glob_index_tokens(&Glob::new(glob))
            } else {
                Vec::new()
            };
            let prefilter = if toks.is_empty() {
                String::new()
            } else {
                let q = toks
                    .iter()
                    .map(|(t, prefix)| format!("\"{t}\"{}", if *prefix { " *" } else { "" }))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                format!(
                    "n.fts_rowid IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH {}) AND ",
                    out.param(format!("content : ({q})"))
                )
            };
            let g = out.param(glob.clone());
            let parts: Vec<String> = keys
                .iter()
                .map(|k| {
                    let path = out.param(format!("$.\"{}\"", k.replace('"', "\\\"")));
                    format!("fc_html_glob({g}, json_extract(n.fields, {path}))")
                })
                .collect();
            if parts.is_empty() {
                "0".into()
            } else {
                format!("{prefilter}({})", parts.join(" OR "))
            }
        }
        Pred::Regex { keys, pattern } => {
            let k = match keys {
                Some(keys) => out.param(json(keys)),
                None => "NULL".into(),
            };
            let r = out.param(pattern.clone());
            format!("fc_regex(n.fields, {k}, {r})")
        }
        Pred::Decks(ids) => format!("c.deck_id IN {}", out.list(ids)),
        Pred::Tags(ids) => format!(
            "n.id IN (SELECT note_id FROM note_tags WHERE tag_id IN {})",
            out.list(ids)
        ),
        Pred::Untagged => "NOT EXISTS (SELECT 1 FROM note_tags WHERE note_id = n.id)".into(),
        Pred::NoteTypes(ids) => format!("n.notetype_id IN {}", out.list(ids)),
        Pred::CardType(t) => format!("fc_fold(c.card_type) = {}", out.param(t.clone())),
        Pred::Template(t) => format!("fc_fold(c.template_key) = {}", out.param(t.clone())),
        Pred::State(s) => format!("c.state = {}", out.param(s.as_str().to_owned())),
        Pred::DueNow => {
            let n = out.param(now.0);
            format!(
                "c.state <> 'new' AND c.due IS NOT NULL AND c.due <= {n} AND c.suspended = 0 \
                 AND (c.buried_until IS NULL OR c.buried_until <= {n})"
            )
        }
        Pred::Suspended => "c.suspended = 1".into(),
        Pred::Buried => format!("c.buried_until > {}", out.param(now.0)),
        Pred::Flagged => "c.flags <> 0".into(),
        Pred::FlagBit(b) => {
            let mask = 1i64.checked_shl(*b).unwrap_or(0);
            format!("(c.flags & {}) <> 0", out.param(mask))
        }
        Pred::Num(prop, op, v) => {
            let col = match prop {
                CardNum::Stability => "c.stability",
                CardNum::Difficulty => "c.difficulty",
                CardNum::Lapses => "c.lapses",
                CardNum::Reps => "c.reps",
            };
            format!("{col} {} {}", sql_op(*op), out.param(*v))
        }
        Pred::Elapsed(op, th) => {
            let n = out.param(now.0);
            let t = out.param(*th);
            format!(
                "c.stability > 0 AND c.last_review IS NOT NULL AND \
                 ((({n} - c.last_review) / 86400000.0) / c.stability) {} {t}",
                sql_op(*op)
            )
        }
        Pred::Time(col, op, at) => {
            let col = match col {
                TimeCol::CardCreated => "c.created_at",
                TimeCol::NoteEdited => "n.updated_at",
                TimeCol::LastReview => "c.last_review",
                TimeCol::Due => "(CASE WHEN c.state <> 'new' THEN c.due END)",
            };
            format!("{col} {} {}", sql_op(*op), out.param(at.0))
        }
        Pred::Attr(alts) => {
            let parts: Vec<String> = alts
                .iter()
                .map(|alt| {
                    let (entity, kind) = match alt.target {
                        AttributeTarget::Note => ("n.id", "note"),
                        AttributeTarget::Card => ("c.id", "card"),
                    };
                    let a = out.param(alt.attr_id.to_string());
                    let test = match &alt.test {
                        AttrTest::Any => String::new(),
                        AttrTest::Options(ids) => format!(" AND value IN {}", out.list(ids)),
                        AttrTest::Number(op, v) => {
                            format!(" AND value_num {} {}", sql_op(*op), out.param(*v))
                        }
                        AttrTest::Text(g) => {
                            format!(" AND fc_glob({}, value)", out.param(g.clone()))
                        }
                        AttrTest::Date(op, d) => {
                            format!(" AND value {} {}", sql_op(*op), out.param(d.clone()))
                        }
                        AttrTest::Bool(b) => format!(
                            " AND value = {}",
                            out.param(if *b { "true" } else { "false" }.to_owned())
                        ),
                    };
                    format!(
                        "{entity} IN (SELECT entity_id FROM attribute_values \
                         WHERE attr_id = {a} AND entity_kind = '{kind}'{test})"
                    )
                })
                .collect();
            if parts.is_empty() {
                "0".into()
            } else {
                parts.join(" OR ")
            }
        }
        Pred::NoteIds(ids) => format!(
            "n.id IN (SELECT value FROM json_each({}))",
            out.param(json(ids))
        ),
        Pred::CardIds(ids) => format!(
            "c.id IN (SELECT value FROM json_each({}))",
            out.param(json(ids))
        ),
    }
}

/// `FROM … WHERE …` selecting live cards of live notes matching `filter` (aliases `c`, `n`).
fn matching(filter: &Filter, now: Timestamp) -> Sql {
    let mut sql = Sql {
        text: String::new(),
        params: Vec::new(),
    };
    let cond = compile(filter, now, false, &mut sql);
    sql.text = format!(
        "FROM cards c JOIN notes n ON n.id = c.note_id \
         WHERE c.deleted_at IS NULL AND n.deleted_at IS NULL AND {cond}"
    );
    sql
}

fn order_by(sort: CardSort, desc: bool) -> String {
    let col = match sort {
        CardSort::SortField => "n.sort_field COLLATE NOCASE",
        CardSort::Created => "c.created_at",
        CardSort::Modified => "n.updated_at",
        CardSort::Due => "c.due",
        CardSort::Stability => "c.stability",
        CardSort::Difficulty => "c.difficulty",
        CardSort::Lapses => "c.lapses",
        CardSort::Reps => "c.reps",
    };
    let dir = if desc { "DESC" } else { "ASC" };
    format!("ORDER BY {col} {dir}, c.id {dir}")
}

fn card_ids(conn: &Connection, sql: &str, params: &[SqlValue]) -> StorageResult<Vec<CardId>> {
    all(conn, "cards", sql, params_from_iter(params.iter()), |r| {
        id(r, 0)
    })
}

impl<M: Mode> SqliteTx<'_, M> {
    fn hits(&self, ids: &[CardId]) -> StorageResult<Vec<CardHit>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut cards: HashMap<CardId, Card> = HashMap::new();
        for c in all(
            self.conn(),
            "cards",
            &format!(
                "SELECT {} FROM cards WHERE id IN (SELECT value FROM json_each(?1))",
                super::cards::COLS
            ),
            [json_list(ids)],
            super::cards::card_row,
        )? {
            cards.insert(c.id, c);
        }
        let note_ids: Vec<NoteId> = cards.values().map(|c| c.note_id).collect();
        let notes: HashMap<NoteId, (NoteTypeId, String, Timestamp)> = all(
            self.conn(),
            "notes",
            "SELECT id, notetype_id, sort_field, updated_at FROM notes
             WHERE id IN (SELECT value FROM json_each(?1))",
            [json_list(&note_ids)],
            |r| Ok((id(r, 0)?, (id(r, 1)?, r.get(2)?, ts(r, 3)?))),
        )?
        .into_iter()
        .collect();
        let mut tags: HashMap<NoteId, Vec<TagName>> = HashMap::new();
        for (n, t) in all(
            self.conn(),
            "note_tags",
            "SELECT nt.note_id, t.name FROM note_tags nt JOIN tags t ON t.id = nt.tag_id
             WHERE nt.note_id IN (SELECT value FROM json_each(?1))
             ORDER BY nt.note_id, nt.position, t.name",
            [json_list(&note_ids)],
            |r| Ok((id::<NoteId>(r, 0)?, r.get::<_, String>(1)?)),
        )? {
            if let Ok(name) = TagName::parse(&t) {
                tags.entry(n).or_default().push(name);
            }
        }
        Ok(ids
            .iter()
            .filter_map(|cid| {
                let card = cards.remove(cid)?;
                let (notetype_id, sort_field, note_updated_at) = notes.get(&card.note_id)?.clone();
                Some(CardHit {
                    tags: tags.get(&card.note_id).cloned().unwrap_or_default(),
                    card,
                    notetype_id,
                    sort_field,
                    note_updated_at,
                })
            })
            .collect())
    }
}

impl<M: Mode> SearchRead for SqliteTx<'_, M> {
    fn search_cards(&self, s: &CardSearch) -> StorageResult<SearchPage> {
        let m = matching(&s.filter, s.now);
        let from = if s.notes_only {
            format!(
                "FROM cards c JOIN notes n ON n.id = c.note_id WHERE c.id IN \
                 (SELECT min(c.id) {} GROUP BY c.note_id)",
                m.text
            )
        } else {
            m.text.clone()
        };
        // One pass: the window count is computed over every match before LIMIT applies.
        let mut params = m.params.clone();
        params.push(SqlValue::Integer(i64::from(s.limit)));
        params.push(SqlValue::Integer(i64::from(s.offset)));
        let n = params.len();
        let sql = format!(
            "SELECT c.id, count(*) OVER () {from} {} LIMIT ?{} OFFSET ?{n}",
            order_by(s.sort, s.descending),
            n - 1
        );
        let rows: Vec<(CardId, i64)> = all(
            self.conn(),
            "cards",
            &sql,
            params_from_iter(params.iter()),
            |r| Ok((id(r, 0)?, r.get(1)?)),
        )?;
        let total = match rows.first() {
            Some((_, t)) => *t,
            // Past the end: count separately.
            None if s.offset > 0 => one(
                self.conn(),
                "cards",
                &format!("SELECT count(*) {from}"),
                params_from_iter(m.params.iter()),
                |r| r.get(0),
            )?
            .unwrap_or(0),
            None => 0,
        };
        let ids: Vec<CardId> = rows.into_iter().map(|(c, _)| c).collect();
        Ok(SearchPage {
            hits: self.hits(&ids)?,
            total: u32::try_from(total).unwrap_or(u32::MAX),
        })
    }

    fn search_card_ids(&self, filter: &Filter, now: Timestamp) -> StorageResult<Vec<CardId>> {
        let m = matching(filter, now);
        card_ids(
            self.conn(),
            &format!("SELECT c.id {} ORDER BY c.created_at, c.id", m.text),
            &m.params,
        )
    }

    fn list_saved_queries(&self) -> StorageResult<Vec<SavedQuery>> {
        all(
            self.conn(),
            "saved_queries",
            "SELECT id, name, query, icon, position, created_at, updated_at FROM saved_queries
             ORDER BY position, name, id",
            [],
            saved_row,
        )
    }

    fn get_saved_query(&self, q: SavedQueryId) -> StorageResult<Option<SavedQuery>> {
        one(
            self.conn(),
            "saved_queries",
            "SELECT id, name, query, icon, position, created_at, updated_at FROM saved_queries
             WHERE id = ?1",
            [q.to_string()],
            saved_row,
        )
    }

    fn duplicate_note_groups(&self) -> StorageResult<Vec<Vec<NoteId>>> {
        let rows = all(
            self.conn(),
            "notes",
            "SELECT n.notetype_id, n.checksum, n.id FROM notes n
             JOIN (SELECT notetype_id, checksum FROM notes WHERE deleted_at IS NULL
                   GROUP BY notetype_id, checksum HAVING count(*) > 1) d
               ON d.notetype_id = n.notetype_id AND d.checksum = n.checksum
             WHERE n.deleted_at IS NULL
             ORDER BY n.notetype_id, n.checksum, n.id",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, id(r, 2)?)),
        )?;
        let mut groups: Vec<Vec<NoteId>> = Vec::new();
        let mut last: Option<(String, String)> = None;
        for (nt, sum, nid) in rows {
            let key = (nt, sum);
            if last.as_ref() == Some(&key) {
                if let Some(g) = groups.last_mut() {
                    g.push(nid);
                }
            } else {
                groups.push(vec![nid]);
                last = Some(key);
            }
        }
        Ok(groups)
    }

    fn list_trashed_notes(&self, limit: u32) -> StorageResult<Vec<Note>> {
        let ids: Vec<NoteId> = all(
            self.conn(),
            "notes",
            "SELECT id FROM notes WHERE deleted_at IS NOT NULL
             ORDER BY deleted_at DESC, id LIMIT ?1",
            [i64::from(limit)],
            |r| id(r, 0),
        )?;
        let mut out = Vec::with_capacity(ids.len());
        for n in ids {
            if let Some(note) = crate::repo::NoteRead::get_note(self, n)? {
                out.push(note);
            }
        }
        Ok(out)
    }

    fn card_reviews(&self, card: CardId) -> StorageResult<Vec<ReviewEntry>> {
        all(
            self.conn(),
            "review_log",
            "SELECT rating, state_before, state_after, scheduled_days, elapsed_days, duration_ms,
                    hints_used, mode, stability_after, difficulty_after, created_at
             FROM review_log WHERE card_id = ?1 ORDER BY created_at, id",
            [card.to_string()],
            |r| {
                Ok(ReviewEntry {
                    rating: r.get(0)?,
                    state_before: r.get(1)?,
                    state_after: r.get(2)?,
                    scheduled_days: r.get(3)?,
                    elapsed_days: r.get(4)?,
                    duration_ms: r.get(5)?,
                    hints_used: r.get(6)?,
                    mode: r.get(7)?,
                    stability_after: r.get(8)?,
                    difficulty_after: r.get(9)?,
                    created_at: ts(r, 10)?,
                })
            },
        )
    }

    fn integrity_report(&self) -> StorageResult<IntegrityReport> {
        let conn = self.conn();
        let cards = |sql: &str| all(conn, "cards", sql, [], |r| id::<CardId>(r, 0));
        let notes = |sql: &str| all(conn, "notes", sql, [], |r| id::<NoteId>(r, 0));
        Ok(IntegrityReport {
            orphan_cards: cards(
                "SELECT c.id FROM cards c LEFT JOIN notes n ON n.id = c.note_id
                 WHERE c.deleted_at IS NULL AND (n.id IS NULL OR n.deleted_at IS NOT NULL)
                 ORDER BY c.id",
            )?,
            cards_in_trashed_decks: cards(
                "SELECT c.id FROM cards c JOIN decks d ON d.id = c.deck_id
                 WHERE c.deleted_at IS NULL AND d.deleted_at IS NOT NULL ORDER BY c.id",
            )?,
            unscheduled_cards: cards(
                "SELECT id FROM cards WHERE deleted_at IS NULL AND state <> 'new'
                   AND due IS NULL ORDER BY id",
            )?,
            missing_search_index: notes(
                "SELECT n.id FROM notes n LEFT JOIN notes_fts f ON f.rowid = n.fts_rowid
                 WHERE n.deleted_at IS NULL AND f.rowid IS NULL ORDER BY n.id",
            )?,
            cardless_notes: notes(
                "SELECT n.id FROM notes n WHERE n.deleted_at IS NULL AND NOT EXISTS
                   (SELECT 1 FROM cards c WHERE c.note_id = n.id AND c.deleted_at IS NULL)
                 ORDER BY n.id",
            )?,
            dangling_values: one(
                conn,
                "attribute_values",
                "SELECT count(*) FROM attribute_values v
                 WHERE (v.entity_kind = 'note' AND NOT EXISTS
                          (SELECT 1 FROM notes WHERE id = v.entity_id))
                    OR (v.entity_kind = 'card' AND NOT EXISTS
                          (SELECT 1 FROM cards WHERE id = v.entity_id))",
                [],
                |r| r.get::<_, u32>(0),
            )?
            .unwrap_or(0),
        })
    }
}

fn saved_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SavedQuery> {
    Ok(SavedQuery {
        id: id(r, 0)?,
        name: r.get(1)?,
        query: r.get(2)?,
        icon: r.get(3)?,
        position: r.get(4)?,
        created_at: ts(r, 5)?,
        updated_at: ts(r, 6)?,
    })
}

impl SearchWrite for SqliteTx<'_, WriteMode> {
    fn upsert_saved_query(&mut self, q: &SavedQuery) -> StorageResult<()> {
        exec(
            self.conn(),
            "saved_queries",
            "INSERT INTO saved_queries (id, name, query, icon, position, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name = ?2, query = ?3, icon = ?4, position = ?5,
                 created_at = ?6, updated_at = ?7",
            params![
                q.id.to_string(),
                q.name,
                q.query,
                q.icon,
                q.position,
                q.created_at.0,
                q.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn delete_saved_query(&mut self, q: SavedQueryId) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            "saved_queries",
            "DELETE FROM saved_queries WHERE id = ?1",
            [q.to_string()],
        )? > 0)
    }
}
