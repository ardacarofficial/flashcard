#!/usr/bin/env node
// Contract check (v1.2, docs/06 §5.1): drives the Subtitle extension's **own** HTTP client
// (`src/flashcards/api.ts` of the extension's repository, unchanged) against a real
// `fc --db <tmp> api serve`, the way the extension's background does: connection test, decks,
// media upload (and its retry), a capture (and its resend), a batch, known words (not shared by
// default, then shared after `subtitle.share_known_words`), a wrong key.
// Then (v1.4, `interrupted.mjs`) the release acceptance: the extension's own outbox and client
// through broken exchanges (line dropped, answer lost, server killed mid-batch and restarted,
// undo then resend), a backup restored into a fresh profile, and `subtitle.captures.recent`
// checked against every attempt.
//
//   cargo build -p fc-cli
//   [FC_BIN=target/debug/fc] [SUBTITLE_REPO=/path/to/subtitle] node tools/subtitle-contract/check.mjs
//
// Exits non-zero on the first broken expectation. Needs Node ≥ 22.7: the extension's sources
// are loaded with `--experimental-transform-types` (they use TypeScript parameter properties,
// which plain type stripping rejects); the script re-runs itself with that flag when it was not
// given. `FC_BIN` defaults to the repository's `target/debug/fc` when it exists, else `fc` on PATH.
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import assert from 'node:assert/strict';
import { FC, REPO, fcCli, freePort } from './harness.mjs';

if (!process.execArgv.includes('--experimental-transform-types')) {
  const r = spawnSync(
    process.execPath,
    ['--experimental-transform-types', '--no-warnings=ExperimentalWarning', ...process.execArgv, fileURLToPath(import.meta.url), ...process.argv.slice(2)],
    { stdio: 'inherit' },
  );
  process.exit(r.status ?? 1);
}

const { FlashcardsApi, FlashcardsApiError } = await import(pathToFileURL(join(REPO, 'src/flashcards/api.ts')).href);

const dir = mkdtempSync(join(tmpdir(), 'fc-subtitle-contract-'));
const db = join(dir, 'collection.db');
const fc = fcCli(db);

let serve;
let failed = false;
try {
  fc('call', 'modules.set_enabled', JSON.stringify({ id: 'subtitle', enabled: true }));
  const { secret } = fc('call', 'subtitle.keys.create', JSON.stringify({ name: 'contract check' }));
  const port = await freePort();
  serve = spawn(FC, ['--db', db, 'api', 'serve', '--port', String(port)], { stdio: ['ignore', 'ignore', 'inherit'] });
  const url = `http://127.0.0.1:${port}/subtitle`;
  const api = new FlashcardsApi(url, secret);
  for (let i = 0; ; i++) {
    try {
      await api.me();
      break;
    } catch (e) {
      if (i > 100) throw e;
      await new Promise((r) => setTimeout(r, 200));
    }
  }

  const me = await api.me();
  assert.equal(me.user.name, 'contract check');
  // v1.4: known words are not shared unless the user turns `subtitle.share_known_words` on.
  assert.deepEqual(me.features, { media: true, batch: true, known_words: false });
  console.log('ok  GET /v1/me', JSON.stringify(me));

  const deck = fc('call', 'decks.create', JSON.stringify({ path: 'Diziler' }));
  const decks = await api.decks();
  assert.ok(decks.some((d) => d.id === deck.id && d.name === 'Diziler' && d.cards === 0));
  console.log('ok  GET /v1/decks', JSON.stringify(decks));

  // The contract's example capture, media first (a JPEG frame, a WAV clip, base64 as the
  // outbox keeps them).
  const captureId = '0192812a-6f3c-7a41-9c2e-5d1f0b8e4a77';
  const jpeg = Buffer.from('/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////2wBDAf//////////////////////////////////////////////////////////////////////////////////////wAARCAABAAEDASIAAhEBAxEB/8QAFAABAAAAAAAAAAAAAAAAAAAAA//EABQQAQAAAAAAAAAAAAAAAAAAAAD/xAAUAQEAAAAAAAAAAAAAAAAAAAAA/8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAwDAQACEQMRAD8AP//Z', 'base64').toString('base64');
  const wav = Buffer.concat([Buffer.from('RIFF'), Buffer.from([36, 0, 0, 0]), Buffer.from('WAVEfmt '), Buffer.alloc(24)]).toString('base64');
  const shot = await api.uploadMedia({ data: jpeg, mime: 'image/jpeg' }, captureId, 'screenshot');
  assert.equal(await api.uploadMedia({ data: jpeg, mime: 'image/jpeg' }, captureId, 'screenshot'), shot, 'a retried upload keeps its id');
  const audio = await api.uploadMedia({ data: wav, mime: 'audio/wav' }, captureId, 'audio');
  console.log('ok  POST /v1/media', shot, audio);

  const capture = {
    schema: 1,
    capture_id: captureId,
    kind: 'word',
    lang: { target: 'en', native: 'tr' },
    term: { surface: 'keys', lemma: 'key', pos: 'noun' },
    definition: { text: 'anahtar', source: 'dict' },
    context: {
      sentence: 'Almost. I just need to pick up my keys.',
      translation: 'Neredeyse. Sadece anahtarlarımı almam lazım.',
      term_offset: [34, 38],
    },
    source: {
      type: 'video',
      url: 'https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=83s',
      title: 'Never Gonna Give You Up',
      site: 'youtube',
      video_id: 'dQw4w9WgXcQ',
      t_start_ms: 83450,
      t_end_ms: 86120,
    },
    media: { screenshot: { upload_id: shot, mime: 'image/jpeg' }, audio: { upload_id: audio, mime: 'audio/wav' } },
    deck_id: null,
    tags: ['subtitle'],
    created_at: '2026-10-12T10:00:00.000Z',
    client: { name: 'subtitle-extension', ext_version: '3.1.3' },
  };
  assert.equal(await api.createCapture(capture), 'created');
  assert.equal(await api.createCapture(capture), 'duplicate', 'a resend is a duplicate');
  console.log('ok  POST /v1/captures (created, then duplicate)');

  const second = { ...capture, capture_id: '0192812a-6f3c-7a41-9c2e-000000000002', term: { surface: 'pick up', lemma: 'pick up', pos: 'phrase' }, context: { ...capture.context, term_offset: [23, 30] }, media: {}, deck_id: deck.id };
  const bad = { ...capture, capture_id: '0192812a-6f3c-7a41-9c2e-000000000003', kind: 'idiom' };
  const results = await api.createCaptures([second, capture, bad]);
  assert.deepEqual(results.map((r) => r.status), ['created', 'duplicate', 'error']);
  assert.equal(results[2].error.code, 'invalid_capture');
  assert.equal(results[2].error.retryable, false);
  console.log('ok  POST /v1/captures:batch', JSON.stringify(results.map((r) => r.status)));

  // Not shared (the default): the client reads the documented "not shared" answer as an
  // empty last page — no error, no retry.
  const notShared = await api.knownWords('en');
  assert.deepEqual(notShared, { words: [], next_cursor: null });
  const raw = await fetch(`${url}/v1/known-words?lang=en`, { headers: { Authorization: `Bearer ${secret}` } });
  assert.equal(raw.status, 200);
  assert.deepEqual(await raw.json(), { words: [], next_cursor: null, shared: false });
  console.log('ok  GET /v1/known-words while not shared', JSON.stringify(notShared));

  fc('call', 'settings.set', JSON.stringify({ scope: { kind: 'app' }, key: 'subtitle.share_known_words', value: true }));
  assert.equal(fc('call', 'subtitle.status', '{}').share_known_words, true);
  assert.equal((await api.me()).features.known_words, true);
  const known = await api.knownWords('en');
  assert.deepEqual(known.words.map((w) => [w.lemma, w.status]), [['key', 'learning'], ['pick up', 'learning']]);
  assert.equal(known.next_cursor, null);
  console.log('ok  GET /v1/known-words', JSON.stringify(known.words));

  // The cards are in the app: the example in the Subtitle deck, the second one in Diziler.
  const found = fc('call', 'search.cards', JSON.stringify({ query: 'deck:Subtitle' }));
  assert.equal(found.total, 1);
  const note = fc('call', 'notes.get', JSON.stringify({ id: found.items[0].note_id }));
  assert.ok(note.fields.sentence.includes('<b>keys</b>'));
  assert.ok(note.fields.image.startsWith('media:') && note.fields.audio.startsWith('media:'));
  assert.equal(fc('call', 'search.cards', JSON.stringify({ query: 'deck:Diziler' })).total, 1);
  console.log('ok  notes in the app');

  // An invalid capture alone: the extension sees "rejected" with the server's message.
  await assert.rejects(api.createCapture(bad), (e) => e instanceof FlashcardsApiError && e.kind === 'rejected' && e.status === 422 && e.message.includes('kind must be one of'));
  // A wrong key: "auth".
  await assert.rejects(new FlashcardsApi(url, 'sk_fc_wrong').me(), (e) => e instanceof FlashcardsApiError && e.kind === 'auth' && e.status === 401);
  console.log('ok  errors (422 rejected, 401 auth)');
  console.log('contract check passed');
} catch (e) {
  failed = true;
  console.error('contract check FAILED:', e);
} finally {
  serve?.kill();
  rmSync(dir, { recursive: true, force: true });
}
if (!failed) {
  try {
    const { runInterrupted } = await import('./interrupted.mjs');
    await runInterrupted();
    console.log('interruption acceptance passed');
  } catch (e) {
    failed = true;
    console.error('interruption acceptance FAILED:', e);
  }
}
process.exit(failed ? 1 : 0);
