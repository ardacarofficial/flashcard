/**
 * v1.4.2 journeys J4–J7 of the Windows run (./README.md): what the owner's data goes through —
 * a Subtitle capture arriving over the local API, the app closed and opened again, a backup
 * restored, a damaged collection rescued. Each one checks the SAVED result by reading it back
 * through the app's own API.
 *
 * The journeys know nothing about WebDriver: they get an `env` with the app's API (`rpc`), how
 * to close and start the app, and (optionally) a UI. `run.mjs` gives them the installed app
 * (IPC through WebView2, the app started by msedgedriver, the native rescue dialog);
 * `local-check.mjs` gives them `fc dev serve` + `fc api serve` on a temporary collection, so the
 * requests and read-backs are checked on any machine before Windows CI runs them.
 *
 *   env.rpc(method, params)       one public API call, as the UI makes it
 *   env.ui                        null, or { ready(), reload(), shot(name) } (the installed app)
 *   env.closeApp(label)           the app stops (its files are closed afterwards)
 *   env.startApp(label)           the app starts again and its API answers
 *   env.startDamaged(label)       the app starts on a damaged collection: drives the rescue
 *                                 choice (restore the verified backup), returns
 *                                 `{ rescued_by, ... }` once the app answers
 *   env.fc(args)                  the `fc` CLI shipped with the app → stdout
 *   env.dataDir                   optional: the collection folder where the app does not report
 *                                 one (`fc dev serve`: `maintenance.status.data_dir` is null)
 *   env.out                       results folder (backups of J6 go there)
 *   env.tag                       unique per run
 *   env.log(...)                  progress line
 *
 * Zero npm dependencies (Node 22: global fetch).
 */
import { existsSync, mkdirSync, readdirSync, statSync, truncateSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

/** The built-in Classic note type (same constant as ../e2e/fixtures.ts). */
const CLASSIC_NOTETYPE_ID = '01900000-0000-7000-8000-000000000001';

/** A one-pixel JPEG and a tiny WAV header: what the extension uploads (base64 in its outbox). */
const JPEG = Buffer.from(
  '/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////2wBDAf//////////////////////////////////////////////////////////////////////////////////////wAARCAABAAEDASIAAhEBAxEB/8QAFAABAAAAAAAAAAAAAAAAAAAAA//EABQQAQAAAAAAAAAAAAAAAAAAAAD/xAAUAQEAAAAAAAAAAAAAAAAAAAAA/8QAFBEBAAAAAAAAAAAAAAAAAAAAAP/aAAwDAQACEQMRAD8AP//Z',
  'base64',
);
const WAV = Buffer.concat([Buffer.from('RIFF'), Buffer.from([36, 0, 0, 0]), Buffer.from('WAVEfmt '), Buffer.alloc(24)]);

/** A failed expectation: what was expected and what was read back. */
export function expect(cond, expected, readBack) {
  if (!cond) {
    const got = typeof readBack === 'string' ? readBack : JSON.stringify(readBack);
    throw new Error(`expected ${expected}; read back: ${got?.slice(0, 800)}`);
  }
}

/** Polls `fn` until it returns something truthy (no fixed sleeps). */
export async function until(what, fn, timeoutMs = 30_000, intervalMs = 250) {
  const end = Date.now() + timeoutMs;
  let last;
  for (;;) {
    try {
      const v = await fn();
      if (v) return v;
      last = v;
    } catch (e) {
      last = e;
    }
    if (Date.now() > end) {
      const detail = last instanceof Error ? last.message : JSON.stringify(last);
      throw new Error(`timed out after ${timeoutMs / 1000}s waiting for ${what} (last: ${String(detail).slice(0, 600)})`);
    }
    await sleep(intervalMs);
  }
}

/** A UUIDv7-shaped id (the extension's capture ids are UUIDv7). */
function captureId() {
  const hex = [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, '0')).join('');
  const ms = Date.now().toString(16).padStart(12, '0').slice(-12);
  return `${ms.slice(0, 8)}-${ms.slice(8, 12)}-7${hex.slice(13, 16)}-${((parseInt(hex[16], 16) & 3) | 8).toString(16)}${hex.slice(17, 20)}-${hex.slice(20, 32)}`;
}

/** Waits for a background job (`jobs.get`) to finish; its result, or the job's error. */
export async function job(env, started, what, timeoutMs = 180_000) {
  const done = await until(
    `${what} (job ${started.id})`,
    async () => {
      const j = await env.rpc('jobs.get', { id: started.id });
      return j.status === 'succeeded' || j.status === 'failed' || j.status === 'cancelled' ? j : null;
    },
    timeoutMs,
    500,
  );
  expect(done.status === 'succeeded', `${what} to succeed`, { status: done.status, error: done.error });
  return done.result;
}

/** Notes of a deck with their fields and cards. */
export async function notesIn(env, deckId) {
  const page = await env.rpc('notes.list', { deck_id: deckId, include_subdecks: false, page: { cursor: null, limit: null } });
  const out = [];
  for (const n of page.items) out.push(await env.rpc('notes.get', { id: n.id }));
  return out;
}

async function deckWith(env, name, pairs = []) {
  const deck = await env.rpc('decks.create', { path: name, parent_id: null });
  for (const [front, back] of pairs) {
    await env.rpc('notes.create', { notetype_id: CLASSIC_NOTETYPE_ID, deck_id: deck.id, fields: { front, back }, tags: [], attributes: [] });
  }
  return deck.id;
}

const fronts = (notes) => notes.map((n) => n.fields.front).sort();

// ------------------------------------------------------------------ the Subtitle extension

/**
 * What the extension does, on the wire: the user turns the Subtitle module on and makes an
 * access key in the app (Settings › Subtitle: `subtitle.keys.create`); pairing in the extension
 * is the server address + that key and its connection test (`GET /v1/me`); a capture uploads
 * its picture and sound (`POST /v1/media`) and then the word with its sentence
 * (`POST /v1/captures`, `Idempotency-Key` = capture id). Requests exactly as the extension's
 * `src/flashcards/api.ts` makes them (tools/subtitle-contract runs that file itself on Linux CI).
 */
export async function pairExtension(env, name) {
  await env.rpc('modules.set_enabled', { id: 'subtitle', enabled: true });
  const { secret } = await env.rpc('subtitle.keys.create', { name });
  const status = await env.rpc('subtitle.status', {});
  const base = status.address; // http://127.0.0.1:<port>/subtitle
  const call = async (path, init = {}) => {
    const res = await fetch(`${base}${path}`, {
      ...init,
      headers: { Accept: 'application/json', Authorization: `Bearer ${secret}`, ...init.headers },
      signal: AbortSignal.timeout(20_000),
    });
    const text = await res.text();
    let body = null;
    try {
      body = text ? JSON.parse(text) : null;
    } catch {
      body = text;
    }
    return { status: res.status, body };
  };
  // The app starts its server within a couple of seconds of the module switch (it polls).
  const me = await until(`GET ${base}/v1/me answers 200 (the extension's connection test)`, async () => {
    const r = await call('/v1/me').catch((e) => ({ status: 0, body: e?.cause?.code ?? e.message }));
    return r.status === 200 ? r : null;
  });
  expect(me.body?.user?.name === name, `GET /v1/me to name the key "${name}"`, me);
  return { base, secret, call, me: me.body };
}

/** One capture as the extension sends it: picture, sound, then the word with its sentence. */
export async function sendCapture(ext, { term, lemma, meaning, sentence, translation, deckId, videoId, tStart }) {
  const id = captureId();
  const media = {};
  for (const [kind, mime, bytes] of [
    ['screenshot', 'image/jpeg', JPEG],
    ['audio', 'audio/wav', WAV],
  ]) {
    const r = await ext.call('/v1/media', { method: 'POST', headers: { 'Content-Type': mime, 'X-Capture-Id': id, 'X-Media-Kind': kind }, body: bytes });
    expect(r.status === 200 || r.status === 201, `POST /v1/media (${kind}) to be accepted`, r);
    expect(typeof r.body?.upload_id === 'string', `POST /v1/media (${kind}) to answer an upload_id`, r);
    media[kind] = { upload_id: r.body.upload_id, mime };
  }
  const at = sentence.indexOf(term);
  const capture = {
    schema: 1,
    capture_id: id,
    kind: 'word',
    lang: { target: 'en', native: 'tr' },
    term: { surface: term, lemma, pos: 'noun' },
    definition: { text: meaning, source: 'dict' },
    context: { sentence, translation, term_offset: [at, at + term.length] },
    source: {
      type: 'video',
      url: `https://www.youtube.com/watch?v=${videoId}`,
      title: 'Windows journey',
      site: 'youtube',
      video_id: videoId,
      t_start_ms: tStart,
      t_end_ms: tStart + 2500,
    },
    media,
    deck_id: deckId,
    tags: ['subtitle'],
    created_at: new Date().toISOString(),
    client: { name: 'subtitle-extension', ext_version: 'windows-journey' },
  };
  const r = await ext.call('/v1/captures', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'Idempotency-Key': id },
    body: JSON.stringify(capture),
  });
  expect(r.status === 201 && r.body?.status === 'created', 'POST /v1/captures → 201 {"status":"created"}', r);
  return { id, card_id: r.body.card_id, capture };
}

/** The capture's note in `deckId`, its card, and its encounter, read back through the app. */
async function readCapture(env, deckId, sent, term) {
  const notes = await notesIn(env, deckId);
  expect(notes.length === 1, `exactly one note in the capture's deck`, notes.map((n) => n.fields));
  const note = notes[0];
  const cardIds = note.cards.map((c) => c.id);
  expect(cardIds.includes(sent.card_id), `the note to own card ${sent.card_id} (the id POST /v1/captures answered)`, cardIds);
  const values = Object.values(note.fields).join(' | ');
  expect(values.includes(term), `the note's fields to hold the word "${term}"`, note.fields);
  expect(values.includes(`<b>${term}</b>`), `the sentence with "${term}" highlighted`, note.fields);
  const media = Object.values(note.fields).filter((v) => typeof v === 'string' && v.startsWith('media:'));
  expect(media.length >= 2, 'the picture and the sound as media fields', note.fields);
  const { encounters } = await env.rpc('subtitle.encounters.list', { note_id: note.id });
  const mine = encounters.find((e) => e.capture_id === sent.id);
  expect(mine, `an encounter with capture id ${sent.id}`, encounters);
  expect(mine.is_primary === true, 'the encounter to be the primary one', mine);
  expect(mine.facts?.video_id === sent.capture.source.video_id, `the encounter's video ${sent.capture.source.video_id}`, mine.facts);
  expect(mine.facts?.t_end_ms === sent.capture.source.t_end_ms, `the encounter's end time ${sent.capture.source.t_end_ms}`, mine.facts);
  return { note, encounters };
}

// ------------------------------------------------------------------ J4 capture

/** J4: with the app running, a capture sent to its local API is a card with its encounter. */
export async function j4Capture(env, facts) {
  const deckId = await deckWith(env, `pilot_capture_${env.tag}`);
  const ext = await pairExtension(env, `windows journey ${env.tag}`);
  facts.address = ext.base;
  facts.me = ext.me;
  const term = 'lighthouse';
  const sent = await sendCapture(ext, {
    term,
    lemma: term,
    meaning: 'deniz feneri',
    sentence: `Keep the ${term} burning tonight.`,
    translation: 'Bu gece deniz fenerini yanık tut.',
    deckId,
    videoId: 'pilotJ4cap',
    tStart: 61_000,
  });
  facts.capture = { id: sent.id, card_id: sent.card_id };
  // The same request again (the extension's outbox resends after a lost answer): no new card.
  const again = await ext.call('/v1/captures', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', 'Idempotency-Key': sent.id },
    body: JSON.stringify(sent.capture),
  });
  expect(again.status === 200 && again.body?.status === 'duplicate' && again.body?.card_id === sent.card_id, 'the resend → 200 {"status":"duplicate"} with the same card', again);
  const { note, encounters } = await readCapture(env, deckId, sent, term);
  facts.note = note.id;
  facts.encounters = encounters.length;
  // The app sees it as the owner would: the recent captures list (Settings › Subtitle).
  const recent = await env.rpc('subtitle.captures.recent', {}).catch((e) => ({ error: e.message }));
  facts.recent = recent?.attempts?.length ?? recent;
  if (env.ui) {
    await env.ui.reload();
    await env.ui.ready();
  }
}

// ------------------------------------------------------------------ J5 reopen

/** J5: a captured card, reviewed once; the app closes and starts again; all three are there. */
export async function j5Reopen(env, facts) {
  const deckId = await deckWith(env, `pilot_reopen_${env.tag}`);
  const ext = await pairExtension(env, `windows reopen ${env.tag}`);
  const term = 'harbour';
  const sent = await sendCapture(ext, {
    term,
    lemma: term,
    meaning: 'liman',
    sentence: `The ship waited outside the ${term} all night.`,
    translation: 'Gemi bütün gece limanın dışında bekledi.',
    deckId,
    videoId: 'pilotJ5reo',
    tStart: 125_000,
  });
  const tz = -new Date().getTimezoneOffset();
  const session = await env.rpc('study.start', { kind: 'normal', deck_id: deckId, tz_offset_mins: tz });
  const card = session.current?.card_id;
  expect(card, 'the new session to show a card of the capture', session);
  const after = await env.rpc('study.answer', { session_id: session.id, card_id: card, rating: 'good', duration_ms: 2000 });
  expect(after.answered === 1, 'answered = 1 after one answer', after);
  await env.rpc('study.end', { session_id: session.id }).catch(() => undefined);
  const before = await env.rpc('study.card_history', { card_id: card });
  expect(before.reviews.length === 1, 'one review before closing', before.reviews);
  facts.before = { card, reviews: before.reviews.length, state: before.card?.state, capture: sent.id };

  await env.closeApp('j5-close');
  await env.startApp('j5-reopen');

  const { note } = await readCapture(env, deckId, sent, term);
  const history = await env.rpc('study.card_history', { card_id: card });
  facts.after = { reviews: history.reviews.length, rating: history.reviews[0]?.rating, state: history.card?.state };
  expect(history.reviews.length === 1, 'the review done before closing (exactly one)', history.reviews);
  expect(history.reviews[0].rating === 'good' && history.reviews[0].session_id === session.id, `the review: rating good, session ${session.id}`, history.reviews[0]);
  expect(note.cards.some((c) => c.id === card), `card ${card} still on the note`, note.cards);
  if (env.ui) await env.ui.ready();
}

// ------------------------------------------------------------------ J6 backup + restore

/** J6: backup, a change, restore: the change is gone and the earlier state is back. */
export async function j6BackupRestore(env, facts) {
  const deckId = await deckWith(env, `pilot_backup_${env.tag}`, [
    ['anchor', 'çapa'],
    ['sail', 'yelken'],
  ]);
  const path = join(env.out, `j6-${env.tag}.fcbackup`);
  const report = await job(env, await env.rpc('io.backup.create', { path }), 'io.backup.create');
  facts.backup = { path: report?.path, bytes: report?.bytes, notes: report?.notes };
  expect(existsSync(path), `the backup file at ${path}`, report);

  // The change: one more note, one note edited.
  await env.rpc('notes.create', { notetype_id: CLASSIC_NOTETYPE_ID, deck_id: deckId, fields: { front: 'rudder', back: 'dümen' }, tags: [], attributes: [] });
  const sail = (await notesIn(env, deckId)).find((n) => n.fields.front === 'sail');
  await env.rpc('notes.update', { id: sail.id, fields: { front: 'sail', back: 'CHANGED' }, tags: [], attributes: [] });
  const changed = await notesIn(env, deckId);
  expect(fronts(changed).join() === 'anchor,rudder,sail', 'three notes after the change', fronts(changed));

  const safety = join(env.out, `j6-${env.tag}-pre-restore.fcbackup`);
  const restored = await job(env, await env.rpc('io.backup.restore', { path, safety_backup_path: safety }), 'io.backup.restore');
  facts.restore = { safety: restored?.safety_backup_path, counts: restored?.counts };
  if (env.ui) {
    // The UI reloads after a restore (Settings › Backup does the same).
    await env.ui.reload();
    await env.ui.ready();
  }
  const back = await notesIn(env, deckId);
  facts.after = back.map((n) => n.fields);
  expect(fronts(back).join() === 'anchor,sail', 'the two notes of the backup, "rudder" gone', fronts(back));
  const sailBack = back.find((n) => n.fields.front === 'sail');
  expect(sailBack.fields.back === 'yelken', 'the edit undone by the restore (back = "yelken")', sailBack.fields);
  expect(existsSync(safety), `the safety backup of the state before the restore at ${safety}`, restored);
}

// ------------------------------------------------------------------ J7 rescue

/** Folder of the collection and the file. */
async function collectionFile(env) {
  const st = await env.rpc('maintenance.status', {});
  const dir = st.data_dir ?? env.dataDir;
  expect(dir, 'maintenance.status.data_dir (the collection folder)', st);
  const db = join(dir, 'collection.db');
  expect(existsSync(db), `the collection file ${db}`, dir);
  return { dir, db };
}

/**
 * J7: a verified backup, a later change, the app stopped, the collection file truncated, the
 * app started: the rescue choice "restore the backup" leads to a working collection holding the
 * backup's state; the damaged file is kept in `damaged/`.
 */
export async function j7Rescue(env, facts) {
  const { dir, db } = await collectionFile(env);
  facts.db = db;
  const deckId = await deckWith(env, `pilot_rescue_${env.tag}`, [['compass', 'pusula']]);
  // The rescue restores the NEWEST backup. A fresh profile's first automatic backup (queued at
  // startup, `io.auto_backup` = daily) must not land after the change below: wait until it
  // exists (the next one is a day away); if it does not come, switch the schedule off and say so.
  const settled = await until(
    'the first automatic backup (io.backup.status.automatic)',
    async () => {
      const s = await env.rpc('io.backup.status', {});
      return !s.folder || s.schedule === 'off' || s.automatic.length > 0 ? s : null;
    },
    90_000,
    1000,
  ).catch(() => null);
  if (settled) {
    facts.auto_backup = settled.folder ? `${settled.automatic.length} automatic, schedule ${settled.schedule}` : 'no collection folder (not the app)';
  } else {
    await env.rpc('settings.set', { scope: { kind: 'app' }, key: 'io.auto_backup', value: 'off' });
    facts.auto_backup = 'none after 90 s: io.auto_backup switched off for this journey';
  }
  // A verified backup where the rescue looks (`backups/`, and `status.json` records it).
  mkdirSync(join(dir, 'backups'), { recursive: true });
  const path = join(dir, 'backups', `pilot-rescue-${env.tag}.fcbackup`);
  const report = await job(env, await env.rpc('io.backup.create', { path }), 'io.backup.create');
  facts.backup = report?.path;
  const status = await env.rpc('io.backup.status', {});
  facts.last_verified = status.last_verified?.path ?? null;
  await env.rpc('notes.create', { notetype_id: CLASSIC_NOTETYPE_ID, deck_id: deckId, fields: { front: 'after-backup', back: 'kaybolur' }, tags: [], attributes: [] });

  await env.closeApp('j7-stop');
  // Fold the write-ahead log into the file first (the last connection to close checkpoints it),
  // so the damage below cannot be masked by pages still waiting in the log.
  facts.fold = env.fc(['--db', db, '--json', 'call', 'maintenance.status', '{}']).slice(0, 200) ? 'ok' : 'empty';
  const wal = `${db}-wal`;
  facts.wal_after_fold = existsSync(wal) ? statSync(wal).size : null;
  if (facts.wal_after_fold) truncateSync(wal, 0);
  const size = statSync(db).size;
  truncateSync(db, Math.floor((size * 2) / 5));
  facts.truncated = { from: size, to: statSync(db).size };
  // What `fc rescue` sees (read-only): does the file still open, does it pass its check.
  try {
    facts.health = JSON.parse(env.fc(['--db', db, 'rescue'])).health;
  } catch (e) {
    facts.health = `fc rescue (check) failed: ${e.message}`;
  }
  const damagedBefore = existsSync(join(dir, 'damaged')) ? readdirSync(join(dir, 'damaged')) : [];

  const how = await env.startDamaged('j7-damaged', { db, dir });
  Object.assign(facts, how);

  // The result, read back from the app that now runs on the rescued collection.
  const notes = await notesIn(env, deckId);
  facts.after = notes.map((n) => n.fields.front);
  expect(fronts(notes).join() === 'compass', 'the backup\'s state: "compass" only ("after-backup" was written after the backup)', fronts(notes));
  const damaged = existsSync(join(dir, 'damaged')) ? readdirSync(join(dir, 'damaged')).filter((d) => !damagedBefore.includes(d)) : [];
  expect(damaged.length === 1, `one new folder in ${join(dir, 'damaged')} with the damaged file`, damaged);
  const kept = join(dir, 'damaged', damaged[0], 'collection.db');
  expect(existsSync(kept) && statSync(kept).size === facts.truncated.to, `the damaged file kept as it was (${facts.truncated.to} bytes) at ${kept}`, existsSync(kept) ? statSync(kept).size : 'missing');
  facts.kept_in = join(dir, 'damaged', damaged[0]);
  // It works: integrity, and a write read back.
  const check = await env.rpc('maintenance.integrity.check', { quick: false });
  expect(check.ok === true, 'the rescued collection to pass its integrity check', check);
  await env.rpc('notes.create', { notetype_id: CLASSIC_NOTETYPE_ID, deck_id: deckId, fields: { front: 'after-rescue', back: 'yeni' }, tags: [], attributes: [] });
  expect(fronts(await notesIn(env, deckId)).join() === 'after-rescue,compass', 'a note written after the rescue to be read back', fronts(await notesIn(env, deckId)));
  if (env.ui) await env.ui.ready();
}
