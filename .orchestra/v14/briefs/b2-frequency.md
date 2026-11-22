# B2 — frequency, level and word family data in the core (wave 2; the word index from A3 is merged)
OWN: new data/pipeline for frequency + level (a `fc-providers` or `fc-core` module for lookup; storage tables
in migrations 1440–1459; a build script under `tools/`), enrichment of `words.*` results, docs.
LEAVE ALONE: query language (B3), MCP/plugins/automation (B4), backup, scheduler.

Why: the owner should spend time first on words he will hear often. The read-only Subtitle repo already has
frequency lists and CEFR ceilings (`/home/user/subtitle/public/frequency/*.json`, `scripts/frequency/build.ts`,
`src/utils/frequency/`, `src/utils/words.ts`; sources and licences are cited there: hermitdave FrequencyWords
CC BY-SA 4.0, CEFR-J/Octanove). Read how it ranks and caps levels; reuse the IDEAS and the data format, do not
import that repo at runtime.

Do:
1. A versioned, local word-data pack per language (start: English; structure ready for more): lemma → rank,
   estimated CEFR band (and whether the band is from a profile list or estimated from rank), with source and
   licence recorded in the pack and shown through the API. Ship the English pack with the app if the licence
   allows redistribution (check; record attribution in docs + an in-app-readable `words.data.sources`); if in
   doubt, make it an importable pack (`words.data.import`) and ship a small test fixture only. Say what you chose.
2. `words.lookup` / `words.list` / `words.stats` return rank + band; `words.list` can sort/filter by them
   ("most frequent words I don't know yet"). Inflection joins the lemma; derivation ("actor"/"act") stays a
   separate entry but a `family` relation may link them WITHOUT making family members known.
3. Expressions (phrasal verbs like "give up") get a level by a documented rule (see `expressionLevel` in the
   Subtitle repo) and are marked as estimated.
4. Module/setting to switch it off; no network at runtime; tests incl. a 50k-entry pack load time and lookups.
