//! Word data (v1.4 B2, migration 1440): installed packs, their rows and word families.

use super::{all, exec, opt_ts, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{in_table, StorageResult};
use crate::repo::{
    WordDataPackRecord, WordDataQuery, WordDataRead, WordDataRecord, WordDataSort, WordDataWrite,
    WordIndexQuery, WordIndexRecord, WordWithData, NO_BAND, NO_RANK,
};
use rusqlite::{params, Row};

const PACKS: &str = "word_data_packs";
const DATA: &str = "word_data";
const FAMILY: &str = "word_family";

/// The status of a `word_index w` row (`?1` = threshold), as `fc_core::words` derives it.
const STATUS: &str = "CASE WHEN w.mark IS NOT NULL THEN w.mark
    WHEN w.cards > 0 AND w.max_interval_days >= ?1 THEN 'known'
    WHEN w.cards > 0 THEN 'learning' ELSE 'new' END";

fn keys_json(keys: &[String]) -> String {
    serde_json::to_string(keys).unwrap_or_else(|_| "[]".into())
}

fn u32_of(v: Option<i64>) -> Option<u32> {
    v.map(|v| u32::try_from(v.max(0)).unwrap_or(u32::MAX))
}

fn data_row(r: &Row<'_>, at: usize) -> rusqlite::Result<WordDataRecord> {
    Ok(WordDataRecord {
        language: r.get(at)?,
        form: r.get(at + 1)?,
        lemma: r.get(at + 2)?,
        rank: u32_of(r.get(at + 3)?),
        band: r.get(at + 4)?,
        band_origin: r.get(at + 5)?,
    })
}

fn word_row(r: &Row<'_>) -> rusqlite::Result<WordIndexRecord> {
    Ok(WordIndexRecord {
        language: r.get(0)?,
        key: r.get(1)?,
        mark: r.get(2)?,
        mark_at: opt_ts(r, 3)?,
        notes: u32_of(r.get(4)?).unwrap_or(0),
        cards: u32_of(r.get(5)?).unwrap_or(0),
        max_interval_days: u32_of(r.get(6)?),
        last_review: opt_ts(r, 7)?,
        first_seen: opt_ts(r, 8)?,
    })
}

impl<M: Mode> WordDataRead for SqliteTx<'_, M> {
    fn word_data_packs(&self) -> StorageResult<Vec<WordDataPackRecord>> {
        all(
            self.conn(),
            PACKS,
            "SELECT language, version, name, licence, sources, notes, origin, words, forms,
                    families, checksum, installed_at
             FROM word_data_packs ORDER BY language",
            [],
            |r| {
                let sources: String = r.get(4)?;
                Ok(WordDataPackRecord {
                    language: r.get(0)?,
                    version: r.get(1)?,
                    name: r.get(2)?,
                    licence: r.get(3)?,
                    sources: serde_json::from_str(&sources).unwrap_or_default(),
                    notes: r.get(5)?,
                    origin: r.get(6)?,
                    words: u32_of(r.get(7)?).unwrap_or(0),
                    forms: u32_of(r.get(8)?).unwrap_or(0),
                    families: u32_of(r.get(9)?).unwrap_or(0),
                    checksum: r.get(10)?,
                    installed_at: ts(r, 11)?,
                })
            },
        )
    }

    fn word_data_get(
        &self,
        language: &str,
        forms: &[String],
    ) -> StorageResult<Vec<WordDataRecord>> {
        all(
            self.conn(),
            DATA,
            "SELECT language, form, lemma, rank, band, band_origin FROM word_data
             WHERE language = ?1 AND form IN (SELECT value FROM json_each(?2))",
            params![language, keys_json(forms)],
            |r| data_row(r, 0),
        )
    }

    fn word_families_of(
        &self,
        language: &str,
        lemmas: &[String],
    ) -> StorageResult<Vec<(String, String)>> {
        all(
            self.conn(),
            FAMILY,
            "SELECT lemma, head FROM word_family
             WHERE language = ?1 AND (lemma IN (SELECT value FROM json_each(?2))
                                      OR head IN (SELECT value FROM json_each(?2)))
             ORDER BY head, lemma",
            params![language, keys_json(lemmas)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    fn word_index_page_with_data(
        &self,
        q: &WordIndexQuery,
        d: &WordDataQuery,
    ) -> StorageResult<Vec<WordWithData>> {
        let rank = format!("coalesce(d.rank, {NO_RANK})");
        let band = format!("coalesce(d.band, '{NO_BAND}')");
        // Sort columns and the cursor's row value (`?10` band, `?11` rank, `?4`/`?5` key).
        let (order, after) = match d.sort {
            WordDataSort::Key => (
                "w.language, w.key".to_owned(),
                "(w.language, w.key) > (?4, ?5)".to_owned(),
            ),
            WordDataSort::Rank => (
                format!("{rank}, w.language, w.key"),
                format!("({rank}, w.language, w.key) > (?11, ?4, ?5)"),
            ),
            WordDataSort::Band => (
                format!("{band}, {rank}, w.language, w.key"),
                format!("({band}, {rank}, w.language, w.key) > (?10, ?11, ?4, ?5)"),
            ),
        };
        let (after_lang, after_key) = q
            .after
            .as_ref()
            .map_or(("", ""), |(l, k)| (l.as_str(), k.as_str()));
        let (after_band, after_rank) = d
            .after_sort
            .clone()
            .unwrap_or_else(|| (NO_BAND.to_owned(), NO_RANK));
        let sql = format!(
            "SELECT w.language, w.key, w.mark, w.mark_at, w.notes, w.cards, w.max_interval_days,
                    w.last_review, w.first_seen,
                    d.language, d.form, d.lemma, d.rank, d.band, d.band_origin
             FROM word_index w
             LEFT JOIN word_data d ON d.language = w.language AND d.form = w.key
             WHERE (?2 IS NULL OR w.language = ?2)
               AND (?6 = '[]' OR ({STATUS}) IN (SELECT value FROM json_each(?6)))
               AND (?8 = '[]' OR d.band IN (SELECT value FROM json_each(?8)))
               AND (?9 IS NULL OR d.rank <= ?9)
               AND (?3 = 0 OR {after})
               -- every mode binds the cursor's sort values, not every mode reads them
               AND (?10 IS NULL OR ?11 IS NULL OR 1)
             ORDER BY {order} LIMIT ?7"
        );
        all(
            self.conn(),
            DATA,
            &sql,
            params![
                i64::from(q.threshold_days),
                q.language,
                i64::from(q.after.is_some()),
                after_lang,
                after_key,
                keys_json(&q.statuses),
                i64::from(q.limit),
                keys_json(&d.bands),
                d.max_rank.map(i64::from),
                after_band,
                i64::from(after_rank),
            ],
            |r| {
                let data = match r.get::<_, Option<String>>(9)? {
                    Some(_) => Some(data_row(r, 9)?),
                    None => None,
                };
                Ok((word_row(r)?, data))
            },
        )
    }

    fn word_index_band_counts(
        &self,
        threshold_days: u32,
    ) -> StorageResult<Vec<(String, Option<String>, String, u32)>> {
        all(
            self.conn(),
            DATA,
            &format!(
                "SELECT w.language, d.band, {STATUS} AS status, count(*)
                 FROM word_index w
                 LEFT JOIN word_data d ON d.language = w.language AND d.form = w.key
                 GROUP BY w.language, d.band, status ORDER BY w.language, d.band, status"
            ),
            [i64::from(threshold_days)],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    u32_of(r.get(3)?).unwrap_or(0),
                ))
            },
        )
    }
}

impl WordDataWrite for SqliteTx<'_, WriteMode> {
    fn replace_word_data(
        &mut self,
        pack: &WordDataPackRecord,
        rows: &[WordDataRecord],
        families: &[(String, String)],
    ) -> StorageResult<()> {
        self.remove_word_data(&pack.language)?;
        exec(
            self.conn(),
            PACKS,
            "INSERT INTO word_data_packs (language, version, name, licence, sources, notes,
                 origin, words, forms, families, checksum, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                pack.language,
                pack.version,
                pack.name,
                pack.licence,
                pack.sources.to_string(),
                pack.notes,
                pack.origin,
                i64::from(pack.words),
                i64::from(pack.forms),
                i64::from(pack.families),
                pack.checksum,
                pack.installed_at.0,
            ],
        )?;
        let conn = self.conn();
        let mut st = conn
            .prepare_cached(
                "INSERT INTO word_data (language, form, lemma, rank, band, band_origin)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(|e| in_table(e, DATA))?;
        for r in rows {
            st.execute(params![
                pack.language,
                r.form,
                r.lemma,
                r.rank.map(i64::from),
                r.band,
                r.band_origin
            ])
            .map_err(|e| in_table(e, DATA))?;
        }
        let mut st = conn
            .prepare_cached("INSERT INTO word_family (language, lemma, head) VALUES (?1, ?2, ?3)")
            .map_err(|e| in_table(e, FAMILY))?;
        for (lemma, head) in families {
            st.execute(params![pack.language, lemma, head])
                .map_err(|e| in_table(e, FAMILY))?;
        }
        Ok(())
    }

    fn remove_word_data(&mut self, language: &str) -> StorageResult<()> {
        exec(
            self.conn(),
            PACKS,
            "DELETE FROM word_data_packs WHERE language = ?1",
            [language],
        )?;
        exec(
            self.conn(),
            DATA,
            "DELETE FROM word_data WHERE language = ?1",
            [language],
        )?;
        exec(
            self.conn(),
            FAMILY,
            "DELETE FROM word_family WHERE language = ?1",
            [language],
        )?;
        Ok(())
    }
}
