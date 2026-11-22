#!/usr/bin/env node
// Builds a word-data pack (`fc-word-data`, format 1) for one language: the dictionary forms
// ("lemmas") most said first, inflected forms → lemma, CEFR levels from vocabulary profiles and
// word families ("actor" → "act"). See README.md next to this file.
//
//   node tools/word-data/build.mjs --frequency <path> [--cache <dir>] [--out <path>]
//                                  [--version <semver>] [--verbose]
//
//   --frequency  a frequency list in the Subtitle extension's format (public/frequency/<lang>.json:
//                {language, source, license, lemmas, forms, ceilings?}); its `language` picks the
//                configuration below. `ceilings` is ignored: ranks here are the plain list order.
//   --cache      keeps the downloaded profiles in <dir> and reuses them on the next run
//   --out        default crates/fc-providers/data/word-data/<language>.json
//   --version    the pack's version, default 1.0.0
//   --verbose    prints every family link, not only a sample
//
// Node ≥ 20, no dependencies. The output is deterministic: same inputs, same bytes.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const FORMAT = 'fc-word-data';
const FORMAT_VERSION = 1;
const LEVELS = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2'];

// ---------------------------------------------------------------------------------------------
// Sources and languages
// ---------------------------------------------------------------------------------------------

const SOURCE_FREQUENCY = {
  name: 'FrequencyWords (OpenSubtitles 2018 word counts)',
  url: 'https://github.com/hermitdave/FrequencyWords',
  licence: 'CC BY-SA 4.0',
  attribution:
    'Word counts by Hermit Dave (hermitdave/FrequencyWords), built from the OpenSubtitles 2018 corpus (opensubtitles.org).',
};
const SOURCE_FORMS = {
  name: 'Wiktionary inflected forms (kaikki.org / Wiktextract)',
  url: 'https://kaikki.org/',
  licence: 'CC BY-SA 4.0',
  attribution:
    'Inflected forms from Wiktionary (wiktionary.org), extracted by Wiktextract (kaikki.org), as compiled by the Subtitle extension\'s frequency build.',
};
const PROFILES_URL = 'https://raw.githubusercontent.com/openlanguageprofiles/olp-en-cefrj/master/';
const SOURCE_CEFRJ = {
  name: 'CEFR-J Wordlist Version 1.5',
  url: 'http://www.cefr-j.org/download.html',
  licence: 'Free for research and commercial use, provided the dataset is cited',
  attribution:
    'CEFR-J Wordlist Version 1.5, compiled by Yukio Tono, Tokyo University of Foreign Studies (http://www.cefr-j.org/download.html), via https://github.com/openlanguageprofiles/olp-en-cefrj.',
};
const SOURCE_OCTANOVE = {
  name: 'Octanove Vocabulary Profile C1/C2 1.0',
  url: 'https://github.com/openlanguageprofiles/olp-en-cefrj',
  licence: 'CC BY-SA 4.0',
  attribution: 'Octanove Vocabulary Profile C1/C2 1.0 by Octanove Labs, via https://github.com/openlanguageprofiles/olp-en-cefrj.',
};

// One entry per language. A language with a frequency list only leaves `profiles` empty and
// `familyRules` empty: it gets words and forms, no levels and no families.
const LANGUAGES = {
  en: {
    name: 'English: subtitle frequency and CEFR levels',
    // Single letters that are words; the other ones are tokenizer fragments or initials
    letterWords: ['a', 'i', 'o'],
    // What a tokenizer that splits on apostrophes leaves of contractions ("you'll", "didn't")
    fragments: ['ll', 've', 're', 'd', 's', 't', 'm', 'didn', 'doesn', 'isn', 'wasn', 'couldn', 'wouldn',
      'shouldn', 'aren', 'weren', 'haven', 'hasn', 'hadn', 'mustn', 'needn', 'don', 'ain'],
    profiles: [
      { file: 'cefrj-vocabulary-profile-1.5.csv', source: SOURCE_CEFRJ },
      { file: 'octanove-vocabulary-profile-c1c2-1.0.csv', source: SOURCE_OCTANOVE },
    ],
    familyRules: englishFamilyRules(),
    familyStop: englishFamilyStop(),
  },
};

// Subtitle markup the corpus counted as words: ASS/SSA override tags ("{\fscx100}", "{\1c&H00FFFF&}",
// "{\blur3}") and letter-for-digit OCR errors ("0kay", "y0u"). Never words in any language.
const MARKUP = [
  /^\d*(?:ch|c|h|ah|alphah)[0-9a-f]{2,8}$/, // colour and alpha tags (needs a digit, checked below)
  /^(?:fsc[xy]|blur|bord|shad|be)[\d.]+$/, // scale, blur, border, shadow, edge blur
  /^0\p{L}+$/u, // "0kay", "0ne", "0ur"
  /^\p{L}+0\p{L}*$/u, // "y0u", "f0r", "shad0"
];
const isMarkup = (word) => /\d/.test(word) && MARKUP.some((pattern) => pattern.test(word));

// ---------------------------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------------------------

function parseArgs(argv) {
  const options = { frequency: null, cache: null, out: null, version: '1.0.0', verbose: false };
  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    const value = () => {
      const next = argv[++index];
      if (next === undefined) fail(`${arg} needs a value`);
      return next;
    };
    if (arg === '--frequency') options.frequency = resolve(value());
    else if (arg === '--cache') options.cache = resolve(value());
    else if (arg === '--out') options.out = resolve(value());
    else if (arg === '--version') options.version = value();
    else if (arg === '--verbose') options.verbose = true;
    else if (arg === '--help' || arg === '-h') {
      console.log('node tools/word-data/build.mjs --frequency <path> [--cache <dir>] [--out <path>] [--version <semver>] [--verbose]');
      process.exit(0);
    } else fail(`unknown argument ${arg}`);
  }
  if (!options.frequency) fail('--frequency <path> is required');
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(options.version)) fail(`--version ${options.version} is not semver`);
  return options;
}

function fail(message) {
  console.error(`build.mjs: ${message}`);
  process.exit(1);
}

async function download(url, name, cacheDir) {
  const cached = cacheDir && resolve(cacheDir, name);
  if (cached && existsSync(cached)) return readFileSync(cached, 'utf8');
  console.log(`Downloading ${url}`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  const text = await response.text();
  if (cached) {
    mkdirSync(cacheDir, { recursive: true });
    writeFileSync(cached, text);
  }
  return text;
}

// ---------------------------------------------------------------------------------------------
// Words and forms
// ---------------------------------------------------------------------------------------------

const hasLetter = (word) => /\p{L}/u.test(word);

function keepWord(word, config) {
  if (!hasLetter(word)) return false;
  if ([...word].length === 1 && !config.letterWords.includes(word)) return false;
  if (config.fragments.includes(word)) return false;
  return !isMarkup(word);
}

function buildWords(frequency, config) {
  const words = [];
  const seen = new Set();
  const dropped = [];
  for (const lemma of frequency.lemmas) {
    if (!keepWord(lemma, config)) dropped.push(lemma);
    else if (!seen.has(lemma)) {
      seen.add(lemma);
      words.push(lemma);
    }
  }
  return { words, dropped };
}

function buildForms(frequency, wordSet, config) {
  const forms = new Map();
  for (const [form, index] of Object.entries(frequency.forms)) {
    const lemma = frequency.lemmas[index];
    if (lemma === undefined) throw new Error(`form "${form}" points to lemma #${index}, past the list`);
    if (form === lemma || !wordSet.has(lemma) || wordSet.has(form) || !keepWord(form, config)) continue;
    forms.set(form, lemma);
  }
  return forms;
}

// ---------------------------------------------------------------------------------------------
// Profiles (CEFR levels and parts of speech)
// ---------------------------------------------------------------------------------------------

// Minimal RFC 4180 reader: the profiles quote their topic columns ("News, lifestyles and ...")
function parseCsv(text) {
  const rows = [];
  let row = [];
  let field = '';
  let quoted = false;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (quoted) {
      if (char === '"' && text[index + 1] === '"') {
        field += '"';
        index++;
      } else if (char === '"') quoted = false;
      else field += char;
    } else if (char === '"') quoted = true;
    else if (char === ',') {
      row.push(field);
      field = '';
    } else if (char === '\n' || char === '\r') {
      if (char === '\r' && text[index + 1] === '\n') index++;
      row.push(field);
      rows.push(row);
      row = [];
      field = '';
    } else field += char;
  }
  if (field !== '' || row.length > 0) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

// As the Subtitle extension's normalizeWord: NFC, typographic apostrophes, edge punctuation off,
// lower case; plus trimmed and inner whitespace collapsed ("look  after")
const EDGE_PUNCTUATION = /^[\s"“”„«»‹›'‘’`´([{¿¡*_~.,;:!?…–—-]+|[\s"“”„«»‹›'‘’`´)\]}*_~.,;:!?…–—-]+$/g;
function normalizeSpelling(text) {
  return text
    .normalize('NFC')
    .replace(/[’‘ʼ`´]/g, "'")
    .replace(EDGE_PUNCTUATION, '')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();
}

// The parts of speech family rules use; the profiles' others (pronoun, number, ...) don't matter
function partOfSpeech(raw) {
  const pos = raw.trim().toLowerCase();
  if (pos === 'verb' || pos === 'vern' || /^(?:be|do|have)-verb$/.test(pos)) return 'verb'; // "vern": a typo in Octanove
  if (pos === 'noun' || pos === 'adjective' || pos === 'adverb') return pos;
  return null;
}

// headword → lowest level index, headword → parts of speech; like parseProfile in the Subtitle repo
function parseProfile(csv, levels, parts) {
  for (const [headword, pos = '', cefr = ''] of parseCsv(csv).slice(1)) {
    const level = LEVELS.indexOf(cefr.trim().toUpperCase());
    if (!headword || level === -1) continue;
    const part = partOfSpeech(pos);
    for (const spelling of headword.split('/')) {
      const word = normalizeSpelling(spelling);
      if (!hasLetter(word)) continue;
      levels.set(word, Math.min(levels.get(word) ?? level, level));
      if (part) {
        if (!parts.has(word)) parts.set(word, new Set());
        parts.get(word).add(part);
      }
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Word families
// ---------------------------------------------------------------------------------------------

// Derived part of speech ← base part of speech, by suffix. `stems`
// gives extra spellings of the base for a stem (what is left after the suffix), after the common
// ones: the stem itself, + "e" (arrival → arrive), i → y (happiness → happy), undoubled
// consonant (beginner → begin).
function englishFamilyRules() {
  const latin = (stem) => [
    stem.endsWith('ss') && stem.slice(0, -2) + 't', // permission → permit
    stem.endsWith('ns') && stem.slice(0, -1) + 'd', // extension → extend
    stem.endsWith('s') && !stem.endsWith('ss') && stem.slice(0, -1) + 'de', // decision → decide
  ];
  return [
    { derived: 'noun', base: 'verb', suffixes: ['ation'], stems: (stem) => [stem.endsWith('ic') && stem.slice(0, -2) + 'y'] }, // application → apply
    { derived: 'noun', base: 'verb', suffixes: ['tion', 'ion'], stems: latin },
    { derived: 'noun', base: 'verb', suffixes: ['ment', 'ance', 'ence', 'er', 'or', 'al', 'ure'] },
    { derived: 'noun', base: 'adjective', suffixes: ['ness'] },
    { derived: 'noun', base: 'adjective', suffixes: ['ity'], stems: (stem) => [stem.endsWith('bil') && stem.slice(0, -2) + 'le'] }, // possibility → possible
    {
      derived: 'adverb', base: 'adjective', suffixes: ['ly'],
      stems: (stem) => [stem + 'le', stem.endsWith('ical') && stem.slice(0, -2)], // gently → gentle, basically → basic
    },
    { derived: 'adjective', base: 'noun', suffixes: ['ful', 'less'] },
    { derived: 'adjective', base: 'verb', suffixes: ['ative'] }, // talkative → talk
    { derived: 'adjective', base: 'verb', suffixes: ['able', 'ible', 'ive'], stems: latin },
  ];
}

// Pairs the rules would link though the words are not one family ("flower" isn't one who flows).
// Reviewed by reading every link the rules make; a derived word listed here gets no family.
function englishFamilyStop() {
  return new Set([
    // Same letters, other words
    'batter', 'billion', 'business', 'career', 'charter', 'comment', 'compliment', 'dial', 'faction',
    'final', 'finance', 'flicker', 'flower', 'letter', 'liver', 'manner', 'mission', 'notion', 'number',
    'passion', 'passive', 'peer', 'posture', 'ration', 'seal', 'session', 'shower', 'station', 'tension',
    'version', 'intensive',
    // Related long ago, apart now ("profession" isn't professing)
    'audition', 'competence', 'confidence', 'creature', 'department', 'expedition', 'importance',
    'profession', 'professor', 'responsible', 'rubber',
  ]);
}

const VOWEL = /[aeiouy]/;
const CONSONANT = /[b-df-hj-np-tv-z]/;
const SHORT_CVC = /^[^aeiouy]*[aeiou][b-df-hj-np-tv]$/;

function baseCandidates(stem, rule) {
  const out = [stem, stem + 'e'];
  if (stem.endsWith('i')) out.push(stem.slice(0, -1) + 'y');
  const last = stem.at(-1);
  if (stem.length >= 2 && last === stem.at(-2) && CONSONANT.test(last)) out.push(stem.slice(0, -1));
  if (rule.stems) out.push(...rule.stems(stem).filter(Boolean));
  return out;
}

function buildFamily({ words, wordSet, parts, rawForms, rules, stop }) {
  const links = new Map();
  for (const word of words) {
    const wordParts = parts.get(word);
    if (!wordParts || stop.has(word)) continue;
    // Every rule and suffix that fits. The stem itself as a word wins, from the longest suffix
    // ("formation": "form", not "format"; "singer": "sing", not "singe"); else the longest respelt
    // base ("donation": "donate", not "done"). A short stem ending vowel + consonant before a
    // vowel suffix would have doubled its consonant ("rid" → "ridder"), so "rider" is "ride".
    let best = null;
    const consider = (base, rank) => {
      if (best === null || rank < best.rank) best = { base, rank };
    };
    for (const rule of rules) {
      if (!wordParts.has(rule.derived)) continue;
      for (const suffix of rule.suffixes) {
        if (!word.endsWith(suffix)) continue;
        const stem = word.slice(0, -suffix.length);
        const valid = (base) =>
          base !== word && base.length >= 3 && VOWEL.test(base) && wordSet.has(base) &&
          parts.get(base)?.has(rule.base) &&
          rawForms.get(word) !== base; // an inflection ("faster"), not a derivation
        const undoubled = /^[aeiou]/.test(suffix) && SHORT_CVC.test(stem) && valid(stem + 'e');
        for (const base of baseCandidates(stem, rule)) {
          if (!valid(base) || (base === stem && undoubled)) continue;
          consider(base, base === stem ? -1000 - suffix.length : -base.length);
        }
      }
    }
    if (best !== null) links.set(word, best.base);
  }
  // Chains to the ultimate head: actively → active → act gives actively → act
  const family = new Map();
  for (const word of links.keys()) {
    let head = word;
    const seen = new Set([word]);
    while (links.has(head) && !seen.has(links.get(head))) {
      head = links.get(head);
      seen.add(head);
    }
    if (head !== word) family.set(word, head);
  }
  return family;
}

// ---------------------------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------------------------

// Code point order (= UTF-8 byte order, as Rust's BTreeMap<String, _> sorts)
const byCodePoint = (a, b) => {
  const x = [...a];
  const y = [...b];
  for (let index = 0; index < Math.min(x.length, y.length); index++) {
    const d = x[index].codePointAt(0) - y[index].codePointAt(0);
    if (d !== 0) return d;
  }
  return x.length - y.length;
};
const sortedObject = (map) => Object.fromEntries([...map].sort(([a], [b]) => byCodePoint(a, b)));

function serialize(pack) {
  return `{\n${Object.entries(pack).map(([key, value]) => `${JSON.stringify(key)}:${JSON.stringify(value)}`).join(',\n')}\n}\n`;
}

function validate(pack) {
  const problems = [];
  const wordSet = new Set(pack.words);
  if (wordSet.size !== pack.words.length) problems.push('duplicate words');
  for (const word of pack.words) if (!hasLetter(word)) problems.push(`word without a letter: "${word}"`);
  for (const [form, lemma] of Object.entries(pack.forms)) {
    if (!wordSet.has(lemma)) problems.push(`form "${form}" → "${lemma}", not a word`);
    if (wordSet.has(form)) problems.push(`form "${form}" is a word itself`);
    if (form === lemma) problems.push(`form "${form}" → itself`);
  }
  for (const [word, level] of Object.entries(pack.profile)) {
    if (!LEVELS.includes(level)) problems.push(`profile "${word}": level ${level}`);
    if (!hasLetter(word)) problems.push(`profile "${word}" has no letter`);
  }
  for (const [word, head] of Object.entries(pack.family)) {
    if (!wordSet.has(word) || !wordSet.has(head)) problems.push(`family "${word}" → "${head}": not both words`);
    if (word === head) problems.push(`family "${word}" → itself`);
    if (pack.family[head] !== undefined) problems.push(`family "${word}" → "${head}", which is not a head`);
  }
  for (const source of pack.sources) {
    for (const key of ['name', 'url', 'licence', 'attribution']) if (!source[key]) problems.push(`source without ${key}`);
  }
  if (problems.length > 0) fail(`invalid pack:\n  ${problems.slice(0, 50).join('\n  ')}`);
}

// ---------------------------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------------------------

const options = parseArgs(process.argv.slice(2));
const frequency = JSON.parse(readFileSync(options.frequency, 'utf8'));
const language = frequency.language;
const config = LANGUAGES[language];
if (!config) fail(`no configuration for language "${language}" (have: ${Object.keys(LANGUAGES).join(', ')})`);
if (!Array.isArray(frequency.lemmas) || typeof frequency.forms !== 'object') fail(`${options.frequency}: not a frequency list`);

const { words, dropped } = buildWords(frequency, config);
const wordSet = new Set(words);
const forms = buildForms(frequency, wordSet, config);
const rawForms = new Map(Object.entries(frequency.forms).map(([form, index]) => [form, frequency.lemmas[index]]));

const levels = new Map();
const parts = new Map();
for (const profile of config.profiles) {
  parseProfile(await download(PROFILES_URL + profile.file, profile.file, options.cache), levels, parts);
}
const profile = new Map([...levels].map(([word, level]) => [word, LEVELS[level]]));
const family = config.familyRules.length > 0
  ? buildFamily({ words, wordSet, parts, rawForms, rules: config.familyRules, stop: config.familyStop ?? new Set() })
  : new Map();

const hasProfiles = config.profiles.length > 0;
const pack = {
  format: FORMAT,
  format_version: FORMAT_VERSION,
  language,
  version: options.version,
  name: config.name,
  licence: 'CC BY-SA 4.0',
  sources: [SOURCE_FREQUENCY, SOURCE_FORMS, ...config.profiles.map((p) => p.source)],
  notes:
    'Ranks: the dictionary forms of the FrequencyWords OpenSubtitles 2018 list (top 50,000 spellings), each counted ' +
    'with all its inflected forms, most said first; rank = position in "words" + 1. Tokenizer fragments (single ' +
    'letters other than a/i/o, contraction pieces such as "ll", "didn"), subtitle markup and words without a letter ' +
    'are dropped and the ranks renumbered. Forms: the inflected spellings Wiktionary (via kaikki.org/Wiktextract) ' +
    'links to their dictionary form, as the Subtitle extension compiled them, limited to forms that are not words ' +
    'of their own. ' +
    (hasProfiles
      ? 'Levels: the lowest CEFR level (A1-C2) the CEFR-J Wordlist 1.5 (A1-B2) or the Octanove Vocabulary Profile ' +
        'C1/C2 1.0 teaches a headword at, spellings split on "/" and lower-cased; headwords with no rank (multi-word ' +
        'or rare) are kept. Families: a derived word links to the head of its word family by suffix rules gated by ' +
        'the profiles\' parts of speech (-er/-or/-ion/-ment/-ance/-ence/-al/-ure nouns from verbs, -ness/-ity nouns ' +
        'and -ly adverbs from adjectives, -ful/-less adjectives from nouns, -able/-ible/-ive adjectives from verbs, ' +
        'with dropped e, y→i and doubled consonants), chains resolved to the ultimate head, a reviewed list of ' +
        'false pairs excluded; both words must be in "words". Family links are a hint, not knowledge: knowing ' +
        '"act" does not make "actor" known.'
      : 'This language has no vocabulary profile: no levels and no families.'),
  words,
  profile: sortedObject(profile),
  forms: sortedObject(forms),
  family: sortedObject(family),
};
validate(pack);

const out = options.out ?? resolve(REPO, 'crates/fc-providers/data/word-data', `${language}.json`);
const text = serialize(pack);
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, text);

const ranked = [...profile.keys()].filter((word) => wordSet.has(word)).length;
const familyLinks = Object.entries(pack.family);
console.log(`${out}`);
console.log(`  words            ${words.length} (dropped ${dropped.length} of ${frequency.lemmas.length} lemmas)`);
console.log(`  forms            ${forms.size}`);
console.log(`  profile entries  ${profile.size} (${ranked} with a rank)`);
console.log(`  family links     ${familyLinks.length}`);
console.log(`  size             ${Buffer.byteLength(text)} bytes`);
const sample = options.verbose
  ? familyLinks
  : familyLinks.filter((_, index) => index % Math.max(1, Math.floor(familyLinks.length / 30)) === 0).slice(0, 30);
console.log(`  family ${options.verbose ? 'links' : 'sample'}: ${sample.map(([word, head]) => `${word}→${head}`).join(', ')}`);
