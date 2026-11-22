//! [`MemTx`] repository traits: the word data (v1.4 B2), with the SQLite store's semantics.

use super::*;

/// `word_data_packs`, `word_data`, `word_family`.
#[derive(Debug, Clone, Default)]
pub struct WordDataState {
    packs: BTreeMap<String, WordDataPackRecord>,
    rows: BTreeMap<(String, String), WordDataRecord>,
    families: BTreeMap<(String, String), String>,
}

fn status(w: &WordIndexRecord, threshold: u32) -> String {
    if let Some(m) = &w.mark {
        return m.clone();
    }
    if w.cards > 0 && w.max_interval_days.is_some_and(|d| d >= threshold) {
        "known".into()
    } else if w.cards > 0 {
        "learning".into()
    } else {
        "new".into()
    }
}

/// Sort tuple of a row: (band, rank, language, key) as the SQLite query orders it.
fn sort_key(sort: WordDataSort, w: &WordIndexRecord, d: Option<&WordDataRecord>) -> (String, u32, String, String) {
    let rank = d.and_then(|d| d.rank).unwrap_or(NO_RANK);
    let band = d.and_then(|d| d.band.clone()).unwrap_or_else(|| NO_BAND.to_owned());
    match sort {
        WordDataSort::Key => (String::new(), 0, w.language.clone(), w.key.clone()),
        WordDataSort::Rank => (String::new(), rank, w.language.clone(), w.key.clone()),
        WordDataSort::Band => (band, rank, w.language.clone(), w.key.clone()),
    }
}

impl WordDataRead for MemTx<'_> {
    fn word_data_packs(&self) -> StorageResult<Vec<WordDataPackRecord>> {
        Ok(self.s().domains.word_data.packs.values().cloned().collect())
    }

    fn word_data_get(&self, language: &str, forms: &[String]) -> StorageResult<Vec<WordDataRecord>> {
        let rows = &self.s().domains.word_data.rows;
        Ok(forms
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter_map(|f| rows.get(&(language.to_owned(), f.clone())).cloned())
            .collect())
    }

    fn word_families_of(&self, language: &str, lemmas: &[String]) -> StorageResult<Vec<(String, String)>> {
        let mut v: Vec<(String, String)> = self
            .s()
            .domains
            .word_data
            .families
            .iter()
            .filter(|((l, m), h)| l == language && (lemmas.contains(m) || lemmas.contains(h)))
            .map(|((_, m), h)| (m.clone(), h.clone()))
            .collect();
        v.sort_by(|a, b| (&a.1, &a.0).cmp(&(&b.1, &b.0)));
        Ok(v)
    }

    fn word_index_page_with_data(
        &self,
        q: &WordIndexQuery,
        d: &WordDataQuery,
    ) -> StorageResult<Vec<WordWithData>> {
        let s = self.s();
        let rows = &s.domains.word_data.rows;
        let after = q.after.as_ref().map(|(l, k)| {
            let (b, r) = d
                .after_sort
                .clone()
                .unwrap_or_else(|| (NO_BAND.to_owned(), NO_RANK));
            match d.sort {
                WordDataSort::Key => (String::new(), 0, l.clone(), k.clone()),
                WordDataSort::Rank => (String::new(), r, l.clone(), k.clone()),
                WordDataSort::Band => (b, r, l.clone(), k.clone()),
            }
        });
        let mut v: Vec<((String, u32, String, String), WordWithData)> = s
            .domains
            .words
            .rows()
            .filter(|w| q.language.as_ref().is_none_or(|l| *l == w.language))
            .filter(|w| q.statuses.is_empty() || q.statuses.contains(&status(w, q.threshold_days)))
            .map(|w| {
                let data = rows.get(&(w.language.clone(), w.key.clone())).cloned();
                (sort_key(d.sort, w, data.as_ref()), (w.clone(), data))
            })
            .filter(|(_, (_, data))| {
                (d.bands.is_empty()
                    || data.as_ref().and_then(|x| x.band.as_ref()).is_some_and(|b| d.bands.contains(b)))
                    && d.max_rank.is_none_or(|m| data.as_ref().and_then(|x| x.rank).is_some_and(|r| r <= m))
            })
            .filter(|(k, _)| after.as_ref().is_none_or(|a| k > a))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(v.into_iter().take(q.limit as usize).map(|(_, r)| r).collect())
    }

    fn word_index_band_counts(
        &self,
        threshold_days: u32,
    ) -> StorageResult<Vec<fc_storage::repo::BandCount>> {
        let s = self.s();
        let rows = &s.domains.word_data.rows;
        let mut counts: BTreeMap<(String, Option<String>, String), u32> = BTreeMap::new();
        for w in s.domains.words.rows() {
            let band = rows
                .get(&(w.language.clone(), w.key.clone()))
                .and_then(|d| d.band.clone());
            *counts
                .entry((w.language.clone(), band, status(w, threshold_days)))
                .or_default() += 1;
        }
        Ok(counts.into_iter().map(|((l, b, s), n)| (l, b, s, n)).collect())
    }
}

impl WordDataWrite for MemTx<'_> {
    fn replace_word_data(
        &mut self,
        pack: &WordDataPackRecord,
        rows: &[WordDataRecord],
        families: &[(String, String)],
    ) -> StorageResult<()> {
        self.remove_word_data(&pack.language)?;
        let st = &mut self.m().domains.word_data;
        st.packs.insert(pack.language.clone(), pack.clone());
        for r in rows {
            let mut r = r.clone();
            r.language.clone_from(&pack.language);
            st.rows.insert((pack.language.clone(), r.form.clone()), r);
        }
        for (m, h) in families {
            st.families
                .insert((pack.language.clone(), m.clone()), h.clone());
        }
        Ok(())
    }

    fn remove_word_data(&mut self, language: &str) -> StorageResult<()> {
        let st = &mut self.m().domains.word_data;
        st.packs.remove(language);
        st.rows.retain(|(l, _), _| l != language);
        st.families.retain(|(l, _), _| l != language);
        Ok(())
    }
}
