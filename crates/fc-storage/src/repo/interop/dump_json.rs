//! Streaming read of `collection.json` (v1.4 Z2): the JSON a [`CollectionDump`] serializes
//! to, read table by table and [`CHUNK`] rows at a time, so a restore or a backup check never
//! holds the review history in memory.
//!
//! A dump at this build's schema (the usual case: `schema_versions` comes first, as a dump
//! writes it) streams into a [`DumpSink`]. Any other dump — an older schema, which
//! `upgrade_dump` needs whole, or a hand-made file listing `tables` before `schema_versions`
//! — is read whole and handed back ([`DumpStream::Whole`]) or replayed into the sink.

use super::{
    CollectionDump, DumpStream, DumpStreamError, DumpSummary, RowHook, SqlValue, TableDump,
};
use crate::error::{StorageError, StorageResult};
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;

/// Rows handed to the hook and the sink at a time.
pub(crate) const CHUNK: usize = 512;

/// Where a streamed dump goes.
pub(crate) trait DumpSink {
    /// The dump is at this build's schema; its tables follow.
    fn begin(&mut self) -> StorageResult<()>;
    /// Table `name` with `columns` starts.
    fn table(&mut self, name: &str, columns: &[String]) -> StorageResult<()>;
    /// Rows of the table last started, each as long as its columns.
    fn rows(&mut self, rows: &[Vec<SqlValue>]) -> StorageResult<()>;
    /// Every table was read.
    fn finish(&mut self) -> StorageResult<()>;
}

/// Keeps nothing.
struct Scan;

impl DumpSink for Scan {
    fn begin(&mut self) -> StorageResult<()> {
        Ok(())
    }
    fn table(&mut self, _: &str, _: &[String]) -> StorageResult<()> {
        Ok(())
    }
    fn rows(&mut self, _: &[Vec<SqlValue>]) -> StorageResult<()> {
        Ok(())
    }
    fn finish(&mut self) -> StorageResult<()> {
        Ok(())
    }
}

/// Reads a dump's JSON without keeping its rows (a restore's check before it replaces
/// anything): counts rows per table after `hook` (see
/// [`super::InteropWrite::load_collection_json`]). A dump at another schema is returned whole.
pub fn scan_dump_json(
    json: &mut dyn Read,
    hook: RowHook<'_>,
) -> Result<DumpStream, DumpStreamError> {
    read_dump_json(json, &mut Scan, hook)
}

/// Migration versions of this build, as a dump at its schema lists them.
pub(crate) fn current_schema() -> Vec<u32> {
    crate::migrate::MIGRATIONS
        .iter()
        .map(|m| m.version)
        .collect()
}

/// Streams a dump's JSON into `sink` (see the module docs). Storage errors (the sink's, the
/// hook's, ragged rows) win over the JSON error they cause.
pub(crate) fn read_dump_json(
    json: &mut dyn Read,
    sink: &mut dyn DumpSink,
    hook: RowHook<'_>,
) -> Result<DumpStream, DumpStreamError> {
    let mut st = State {
        sink,
        hook,
        current: current_schema(),
        counts: BTreeMap::new(),
        failed: None,
    };
    let mut de = serde_json::Deserializer::from_reader(json);
    let out = DumpSeed(&mut st)
        .deserialize(&mut de)
        .and_then(|v| de.end().map(|()| v));
    match (out, st.failed.take()) {
        (_, Some(e)) => Err(DumpStreamError::Storage(e)),
        (Ok(v), None) => Ok(v),
        (Err(e), None) => Err(DumpStreamError::Json(e)),
    }
}

fn corrupt(message: String) -> StorageError {
    StorageError::Corrupt {
        table: "backup",
        message,
    }
}

struct State<'a> {
    sink: &'a mut dyn DumpSink,
    hook: RowHook<'a>,
    current: Vec<u32>,
    counts: BTreeMap<String, u64>,
    failed: Option<StorageError>,
}

/// How the `tables` field was read.
enum Tables {
    Streamed,
    Whole(Vec<TableDump>),
}

impl State<'_> {
    /// Keeps the storage error; the parser stops with a placeholder.
    fn fail<E: de::Error>(&mut self, e: StorageError) -> E {
        self.failed = Some(e);
        E::custom("the dump was refused")
    }

    fn start(&mut self, name: &str, columns: &[String]) -> StorageResult<()> {
        self.sink.table(name, columns)?;
        self.counts.entry(name.to_owned()).or_insert(0);
        Ok(())
    }

    fn push(&mut self, cur: &mut TableDump, row: Vec<SqlValue>) -> StorageResult<()> {
        if row.len() != cur.columns.len() {
            return Err(corrupt(format!("ragged rows in {}", cur.name)));
        }
        cur.rows.push(row);
        if cur.rows.len() >= CHUNK {
            self.flush(cur)?;
        }
        Ok(())
    }

    /// Hands the buffered rows (through the hook) to the sink.
    fn flush(&mut self, cur: &mut TableDump) -> StorageResult<()> {
        if cur.rows.is_empty() {
            return Ok(());
        }
        (self.hook)(cur)?;
        self.sink.rows(&cur.rows)?;
        if let Some(n) = self.counts.get_mut(&cur.name) {
            *n += cur.rows.len() as u64;
        }
        cur.rows.clear();
        Ok(())
    }

    /// A table read whole (unusual field order): the same path as a streamed one.
    fn whole_table(&mut self, t: TableDump) -> StorageResult<()> {
        self.start(&t.name, &t.columns)?;
        let mut cur = TableDump {
            name: t.name,
            columns: t.columns,
            rows: Vec::with_capacity(CHUNK),
        };
        for row in t.rows {
            self.push(&mut cur, row)?;
        }
        self.flush(&mut cur)
    }

    fn finish<E: de::Error>(
        &mut self,
        versions: Vec<u32>,
        tables: Tables,
    ) -> Result<DumpStream, E> {
        match tables {
            Tables::Whole(tables) if versions != self.current => {
                return Ok(DumpStream::Whole(CollectionDump {
                    schema_versions: versions,
                    tables,
                }))
            }
            Tables::Whole(tables) => {
                let replay = (|| {
                    self.sink.begin()?;
                    tables.into_iter().try_for_each(|t| self.whole_table(t))
                })();
                replay.map_err(|e| self.fail(e))?;
            }
            Tables::Streamed => {}
        }
        self.sink.finish().map_err(|e| self.fail(e))?;
        Ok(DumpStream::Streamed(DumpSummary {
            schema_versions: versions,
            counts: std::mem::take(&mut self.counts),
        }))
    }
}

fn invalid_length<E: de::Error>(n: usize, what: &'static str) -> E {
    E::invalid_length(n, &what)
}

/// The whole `collection.json`.
struct DumpSeed<'s, 'a>(&'s mut State<'a>);

impl<'de> DeserializeSeed<'de> for DumpSeed<'_, '_> {
    type Value = DumpStream;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<DumpStream, D::Error> {
        d.deserialize_struct("CollectionDump", &["schema_versions", "tables"], self)
    }
}

impl<'de> Visitor<'de> for DumpSeed<'_, '_> {
    type Value = DumpStream;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("struct CollectionDump")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<DumpStream, A::Error> {
        let st = self.0;
        let mut versions: Option<Vec<u32>> = None;
        let mut tables: Option<Tables> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "schema_versions" => {
                    if versions.is_some() {
                        return Err(de::Error::duplicate_field("schema_versions"));
                    }
                    versions = Some(map.next_value()?);
                }
                "tables" => {
                    if tables.is_some() {
                        return Err(de::Error::duplicate_field("tables"));
                    }
                    tables = Some(if versions.as_ref() == Some(&st.current) {
                        st.sink.begin().map_err(|e| st.fail(e))?;
                        map.next_value_seed(TablesSeed(st))?;
                        Tables::Streamed
                    } else {
                        Tables::Whole(map.next_value()?)
                    });
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        let versions = versions.ok_or_else(|| de::Error::missing_field("schema_versions"))?;
        let tables = tables.ok_or_else(|| de::Error::missing_field("tables"))?;
        st.finish(versions, tables)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<DumpStream, A::Error> {
        const WHAT: &str = "struct CollectionDump with 2 elements";
        let st = self.0;
        let versions: Vec<u32> = seq.next_element()?.ok_or_else(|| invalid_length(0, WHAT))?;
        let tables = if versions == st.current {
            st.sink.begin().map_err(|e| st.fail(e))?;
            seq.next_element_seed(TablesSeed(st))?
                .ok_or_else(|| invalid_length(1, WHAT))?;
            Tables::Streamed
        } else {
            Tables::Whole(seq.next_element()?.ok_or_else(|| invalid_length(1, WHAT))?)
        };
        st.finish(versions, tables)
    }
}

/// `tables`, streamed.
struct TablesSeed<'s, 'a>(&'s mut State<'a>);

impl<'de> DeserializeSeed<'de> for TablesSeed<'_, '_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for TablesSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while seq.next_element_seed(TableSeed(&mut *self.0))?.is_some() {}
        Ok(())
    }
}

/// One table, its rows streamed once its name and columns are known.
struct TableSeed<'s, 'a>(&'s mut State<'a>);

impl<'de> DeserializeSeed<'de> for TableSeed<'_, '_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_struct("TableDump", &["name", "columns", "rows"], self)
    }
}

impl TableSeed<'_, '_> {
    /// Streams `rows` of table `name` with `columns` out of `next`.
    fn stream<E: de::Error>(
        st: &mut State<'_>,
        name: &str,
        columns: &[String],
        next: impl FnOnce(RowsSeed<'_, '_, '_>) -> Result<(), E>,
    ) -> Result<(), E> {
        st.start(name, columns).map_err(|e| st.fail(e))?;
        let mut cur = TableDump {
            name: name.to_owned(),
            columns: columns.to_vec(),
            rows: Vec::with_capacity(CHUNK),
        };
        next(RowsSeed {
            st: &mut *st,
            cur: &mut cur,
        })?;
        st.flush(&mut cur).map_err(|e| st.fail(e))
    }
}

impl<'de> Visitor<'de> for TableSeed<'_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("struct TableDump")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let st = self.0;
        let mut name: Option<String> = None;
        let mut columns: Option<Vec<String>> = None;
        let mut streamed = false;
        let mut buffered: Option<Vec<Vec<SqlValue>>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "name" => {
                    if name.is_some() {
                        return Err(de::Error::duplicate_field("name"));
                    }
                    name = Some(map.next_value()?);
                }
                "columns" => {
                    if columns.is_some() {
                        return Err(de::Error::duplicate_field("columns"));
                    }
                    columns = Some(map.next_value()?);
                }
                "rows" => {
                    if streamed || buffered.is_some() {
                        return Err(de::Error::duplicate_field("rows"));
                    }
                    match (&name, &columns) {
                        (Some(n), Some(c)) => {
                            Self::stream(st, n, c, |seed| map.next_value_seed(seed))?;
                            streamed = true;
                        }
                        _ => buffered = Some(map.next_value()?),
                    }
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        let name = name.ok_or_else(|| de::Error::missing_field("name"))?;
        let columns = columns.ok_or_else(|| de::Error::missing_field("columns"))?;
        if !streamed {
            let rows = buffered.ok_or_else(|| de::Error::missing_field("rows"))?;
            st.whole_table(TableDump {
                name,
                columns,
                rows,
            })
            .map_err(|e| st.fail(e))?;
        }
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        const WHAT: &str = "struct TableDump with 3 elements";
        let st = self.0;
        let name: String = seq.next_element()?.ok_or_else(|| invalid_length(0, WHAT))?;
        let columns: Vec<String> = seq.next_element()?.ok_or_else(|| invalid_length(1, WHAT))?;
        Self::stream(st, &name, &columns, |seed| {
            seq.next_element_seed(seed)?
                .ok_or_else(|| invalid_length(2, WHAT))
        })
    }
}

/// The rows of one table, handed on [`CHUNK`] at a time.
struct RowsSeed<'s, 'c, 'a> {
    st: &'s mut State<'a>,
    cur: &'c mut TableDump,
}

impl<'de> DeserializeSeed<'de> for RowsSeed<'_, '_, '_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        d.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for RowsSeed<'_, '_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a sequence")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        while let Some(row) = seq.next_element::<Vec<SqlValue>>()? {
            self.st.push(self.cur, row).map_err(|e| self.st.fail(e))?;
        }
        Ok(())
    }
}
