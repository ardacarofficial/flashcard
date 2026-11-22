/**
 * Seeds a fresh journey profile through REAL commands only (no SQL):
 *
 * * "İngilizce": 30 Classic notes (front = English, back = Turkish) via `notes.create`;
 * * "Subtitle": 10 captures sent the way the Subtitle browser extension sends them —
 *   `POST /subtitle/v1/captures` on `fc api serve` with an access key from
 *   `subtitle.keys.create` — so they become "Subtitle card" notes (word, surface, meaning,
 *   highlighted sentence, translation, source) exactly as in the app;
 * * the first-run guide skipped (`onboarding.skip`).
 *
 * Returns what journeys need (`e2e/seed.json`, read by `fixtures.ts`).
 */
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { bridgeCall, CLASSIC_NOTETYPE_ID } from './lib/rpc.mjs';
import { stop } from './lib/procs.mjs';

/** 30 everyday words: front (English) → back (Turkish). */
export const ENGLISH = [
  ['apple', 'elma'], ['book', 'kitap'], ['chair', 'sandalye'], ['door', 'kapı'], ['window', 'pencere'],
  ['water', 'su'], ['bread', 'ekmek'], ['cheese', 'peynir'], ['street', 'sokak'], ['city', 'şehir'],
  ['friend', 'arkadaş'], ['teacher', 'öğretmen'], ['answer', 'cevap'], ['question', 'soru'], ['money', 'para'],
  ['market', 'pazar'], ['river', 'nehir'], ['mountain', 'dağ'], ['island', 'ada'], ['bridge', 'köprü'],
  ['summer', 'yaz'], ['winter', 'kış'], ['morning', 'sabah'], ['evening', 'akşam'], ['kitchen', 'mutfak'],
  ['garden', 'bahçe'], ['letter', 'mektup'], ['ticket', 'bilet'], ['journey', 'yolculuk'], ['weather', 'hava durumu'],
];

/** 10 subtitle captures: lemma, surface as heard, meaning, the line, its translation. */
export const SUBTITLE = [
  { lemma: 'go', surface: 'went', meaning: 'gitmek', sentence: 'I went home after the party.', translation: 'Partiden sonra eve gittim.' },
  { lemma: 'take', surface: 'took', meaning: 'almak', sentence: 'She took the last train.', translation: 'Son treni aldı.' },
  { lemma: 'buy', surface: 'bought', meaning: 'satın almak', sentence: 'We bought a new car.', translation: 'Yeni bir araba aldık.' },
  { lemma: 'see', surface: 'saw', meaning: 'görmek', sentence: 'I saw her at the station.', translation: 'Onu istasyonda gördüm.' },
  { lemma: 'think', surface: 'thought', meaning: 'düşünmek', sentence: 'I thought you were joking.', translation: 'Şaka yaptığını düşündüm.' },
  { lemma: 'leave', surface: 'left', meaning: 'ayrılmak', sentence: 'They left before dinner.', translation: 'Akşam yemeğinden önce ayrıldılar.' },
  { lemma: 'key', surface: 'keys', meaning: 'anahtar', sentence: 'I just need to pick up my keys.', translation: 'Sadece anahtarlarımı almam lazım.' },
  { lemma: 'bring', surface: 'brought', meaning: 'getirmek', sentence: 'He brought his sister to the party.', translation: 'Kız kardeşini partiye getirdi.' },
  { lemma: 'forget', surface: 'forgot', meaning: 'unutmak', sentence: 'I forgot my umbrella again.', translation: 'Şemsiyemi yine unuttum.' },
  { lemma: 'run', surface: 'running', meaning: 'koşmak', sentence: 'He was running to catch the bus.', translation: 'Otobüse yetişmek için koşuyordu.' },
];

/** The Subtitle extension's capture body (contract schema 1), media-less. */
function capture(c, i, deckId) {
  const start = c.sentence.indexOf(c.surface);
  return {
    schema: 1,
    capture_id: `0192812a-6f3c-7a41-9c2e-${(0xe2e000 + i).toString(16).padStart(12, '0')}`,
    kind: 'word',
    lang: { target: 'en', native: 'tr' },
    term: { surface: c.surface, lemma: c.lemma, pos: 'verb' },
    definition: { text: c.meaning, source: 'dict' },
    context: {
      sentence: c.sentence,
      translation: c.translation,
      term_offset: start >= 0 ? [start, start + c.surface.length] : null,
    },
    source: {
      type: 'video',
      url: 'https://www.youtube.com/watch?v=e2e-journeys',
      title: 'Journey fixture',
      site: 'youtube',
      video_id: 'e2e-journeys',
      t_start_ms: 60_000 + i * 1000,
      t_end_ms: 61_000 + i * 1000,
    },
    media: {},
    deck_id: deckId,
    tags: ['subtitle'],
    created_at: new Date().toISOString(),
    client: { name: 'subtitle-extension', ext_version: 'e2e' },
  };
}

/** `fc --db <db> api serve --port 0` → `{child, url}` (address from its first stderr line). */
function startApi(fc, db) {
  return new Promise((resolvePromise, reject) => {
    const env = { ...process.env };
    delete env.FC_DB;
    const child = spawn(fc, ['--db', db, 'api', 'serve', '--port', '0'], { stdio: ['ignore', 'ignore', 'pipe'], env });
    const timer = setTimeout(() => {
      stop(child);
      reject(new Error('fc api serve did not start'));
    }, 60_000);
    child.on('exit', (code) => reject(new Error(`fc api serve exited (${code})`)));
    createInterface({ input: child.stderr }).once('line', (line) => {
      clearTimeout(timer);
      const addr = line.trim().split('http://').pop();
      resolvePromise({ child, url: `http://${addr}` });
    });
  });
}

export async function seed({ fc, db, url, token }) {
  const call = (method, params) => bridgeCall(url, token, method, params);
  await call('onboarding.skip', {});

  const english = await call('decks.create', { path: 'İngilizce', parent_id: null });
  for (const [front, back] of ENGLISH) {
    await call('notes.create', {
      notetype_id: CLASSIC_NOTETYPE_ID,
      deck_id: english.id,
      fields: { front, back },
      tags: [],
      attributes: [],
    });
  }

  const subtitleDeck = await call('decks.create', { path: 'Subtitle', parent_id: null });
  await call('modules.set_enabled', { id: 'subtitle', enabled: true });
  const key = await call('subtitle.keys.create', { name: 'e2e' });
  const api = await startApi(fc, db);
  try {
    for (const [i, c] of SUBTITLE.entries()) {
      const res = await fetch(`${api.url}/subtitle/v1/captures`, {
        method: 'POST',
        headers: { authorization: `Bearer ${key.secret}`, 'content-type': 'application/json', accept: 'application/json' },
        body: JSON.stringify(capture(c, i, subtitleDeck.id)),
      });
      const body = await res.json().catch(() => null);
      if (!res.ok || body?.status !== 'created') {
        throw new Error(`capture ${c.lemma}: HTTP ${res.status} ${JSON.stringify(body)}`);
      }
    }
  } finally {
    stop(api.child);
  }
  const subtitleNotes = await call('notes.list', {
    deck_id: subtitleDeck.id,
    include_subdecks: false,
    page: { cursor: null, limit: null },
  });
  if (subtitleNotes.items?.length !== SUBTITLE.length) {
    throw new Error(`expected ${SUBTITLE.length} subtitle notes, got ${JSON.stringify(subtitleNotes).slice(0, 300)}`);
  }
  return {
    decks: { english: english.id, subtitle: subtitleDeck.id },
    subtitle_notetype_id: subtitleNotes.items[0].notetype_id,
    english: ENGLISH,
    subtitle: SUBTITLE,
  };
}
