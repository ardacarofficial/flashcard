# Word-data packs

A word-data pack tells the app, for one language, how often each word is said (its rank), which
CEFR level vocabulary profiles teach it at, which inflected spellings belong to it ("went" → "go")
and which word family it belongs to ("actor" → "act"). Family links are hints only: knowing "act"
does not make "actor" known.

The app ships `crates/fc-providers/data/word-data/en.json`, compiled into the binary; nothing is
fetched at runtime. `build.mjs` here is the only thing that writes that file.

## Rebuild

Needs Node ≥ 20, no `npm install`. The frequency list comes from the Subtitle extension's build
(`public/frequency/en.json` of that repository, made by its `scripts/frequency/build.ts`).

```bash
node tools/word-data/build.mjs \
  --frequency /path/to/subtitle/public/frequency/en.json \
  --cache ~/.cache/fc-word-data        # keeps the downloaded CEFR profiles, reuses them next time
# options: --out <path> (default crates/fc-providers/data/word-data/<language>.json)
#          --version <semver> (default 1.0.0)   --verbose (print every family link)
```

Behind an HTTPS proxy, Node's `fetch` needs `NODE_USE_ENV_PROXY=1` (and `NODE_EXTRA_CA_CERTS` for
a private CA). The output is deterministic: the same inputs give the same bytes. Bump `--version`
whenever the content changes. The script validates the pack before writing it and prints the counts.

Another language needs only an entry in `LANGUAGES` in `build.mjs` (name, single-letter words,
tokenizer fragments) and a frequency list in the same format; with no profiles and no family rules
its pack has empty `profile` and `family`.

## Format (`fc-word-data`, format 1)

One JSON object; map keys are sorted by code point.

| Field | Meaning |
|---|---|
| `format` | always `"fc-word-data"` |
| `format_version` | `1`; a reader rejects versions it does not know |
| `language` | language code, e.g. `"en"` |
| `version` | semver of the pack's content |
| `name` | human-readable name |
| `licence` | licence of the pack itself (`CC BY-SA 4.0`) |
| `sources` | `[{name, url, licence, attribution}]`, every dataset the pack is made from; show these to users |
| `notes` | one paragraph on how ranks, levels, forms and families were made |
| `words` | dictionary forms (lemmas), most said first; rank = index + 1 |
| `profile` | `{headword: "A1".."C2"}`: the lowest level a profile teaches it at. May hold headwords not in `words` (multi-word ones such as "according to", rare words): a level with no rank |
| `forms` | `{form: lemma}`: inflected spelling → its lemma; only forms that are not words themselves; every lemma is in `words` |
| `family` | `{derived: head}`: derived lemma → the head lemma of its word family (chains already resolved: "actively" → "act"); both in `words`, never a word to itself |

How it is made (English):

- **words**: the Subtitle list's lemmas in order, minus tokenizer fragments (single letters other
  than a/i/o; contraction pieces `ll ve re d s t m didn doesn … don ain`), subtitle markup the
  corpus counted as words (ASS override tags such as `ch00ffff`, `fscx100`, `blur3`; OCR errors
  such as `0kay`, `y0u`) and words without a letter; ranks renumbered. The Subtitle list's
  `ceilings` are ignored: ranks are the plain frequency order, levels are a separate field.
- **forms**: the Subtitle list's form → lemma map, without self-maps, fragments, forms of dropped
  lemmas and forms that are lemmas of their own.
- **profile**: CEFR-J 1.5 (A1–B2) and Octanove C1/C2; headwords split on `/`, lower-cased,
  edge punctuation trimmed, whitespace collapsed; the lowest level across rows and files.
- **family**: suffix rules gated by the profiles' parts of speech (both words must be in a
  profile): nouns in -er/-or/-ion/-tion/-ation/-ment/-ance/-ence/-al/-ure from verbs, nouns in
  -ness/-ity and adverbs in -ly from adjectives, adjectives in -ful/-less from nouns, adjectives in
  -able/-ible/-ive/-ative from verbs; with dropped e, y → i, doubled consonants and a few Latin
  stems (decision → decide, permission → permit). A reviewed stop list removes pairs that only
  look alike (flower/flow, letter/let, station/state).

## Licences and attribution

The pack is **CC BY-SA 4.0** (share-alike of the adapted data below). Its `sources` field carries
this attribution so the app can show it.

- **FrequencyWords** by Hermit Dave, word counts from the OpenSubtitles 2018 corpus —
  <https://github.com/hermitdave/FrequencyWords>, content CC BY-SA 4.0.
- **Wiktionary** inflected forms, extracted by Wiktextract (<https://kaikki.org/>), as compiled by
  the Subtitle extension's frequency build — CC BY-SA 4.0.
- **CEFR-J Wordlist Version 1.5**, compiled by Yukio Tono, Tokyo University of Foreign Studies —
  <http://www.cefr-j.org/download.html>, via <https://github.com/openlanguageprofiles/olp-en-cefrj>;
  free for research and commercial use, provided the dataset is cited.
- **Octanove Vocabulary Profile C1/C2 1.0** by Octanove Labs — via
  <https://github.com/openlanguageprofiles/olp-en-cefrj>, CC BY-SA 4.0.
