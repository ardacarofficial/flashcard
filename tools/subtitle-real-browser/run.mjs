#!/usr/bin/env node
// Real-browser check (v1.4 G2, docs/14-subtitle-gercek-tarayici.md): the owner's Subtitle extension,
// BUILT from its own repository (unchanged) and loaded into real Chromium, against a real
// `fc --db <tmp> api serve`. Driven like the owner: Settings → Öğrenme servisi → Subtitle Kartları,
// server address + key, "Bağlantıyı test et"; then on a video page: play, hover a word, press "+";
// the same word from a second line; an expression; Flashcard closed; a wrong key; Flashcard
// restarted. Every step reads the PERSISTED result back from Flashcard's API (`fc call`).
//
//   cargo build -p fc-cli
//   [FC_BIN=target/debug/fc] [SUBTITLE_REPO=/home/user/subtitle] [SUBTITLE_DIST=<built dist>] \
//   [CHROMIUM=/opt/pw-browsers/chromium] node tools/subtitle-real-browser/run.mjs
//
// Skips (exit 0, "SKIP: …") when the extension's repository or Chromium is missing. Not part of
// the default CI. Needs Playwright (`playwright` resolvable from here, from apps/desktop or from the
// global node_modules) and pnpm (to build the extension, unless SUBTITLE_DIST names a built one).
//
// What is real and what is not (see the doc): the extension (background service worker, content
// scripts incl. YouTube's MAIN-world page script, options page) is the shipped build; Flashcard is
// the real `fc`. The YouTube page is a small stand-in served by the test at https://www.youtube.com
// (Playwright answers that host), so the extension treats it as YouTube; Google Translate's answers
// are canned (no network here). Nothing else leaves the machine: every other outside request is
// aborted and listed.
import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '../..');
const localFc = join(ROOT, 'target/debug', process.platform === 'win32' ? 'fc.exe' : 'fc');
const FC = process.env.FC_BIN ?? (existsSync(localFc) ? localFc : 'fc');
const REPO = process.env.SUBTITLE_REPO ?? '/home/user/subtitle';
const CHROMIUM = process.env.CHROMIUM ?? '/opt/pw-browsers/chromium';
const KEEP = process.env.KEEP === '1';
const SHOTS = process.env.SHOTS ?? null;

const skip = (why) => {
  console.log(`SKIP: ${why}`);
  process.exit(0);
};
if (!existsSync(join(REPO, 'manifest.js'))) skip(`no Subtitle repository at ${REPO} (SUBTITLE_REPO)`);
if (!existsSync(CHROMIUM)) skip(`no Chromium at ${CHROMIUM} (CHROMIUM)`);

// Service-worker requests (the extension's background) go through Playwright's routes only with this.
process.env.PW_EXPERIMENTAL_SERVICE_WORKER_NETWORK_EVENTS = '1';
function loadPlaywright() {
  const roots = [import.meta.url, join(ROOT, 'apps/desktop/package.json')];
  try {
    roots.push(join(execFileSync('npm', ['root', '-g'], { encoding: 'utf8' }).trim(), 'x.js'));
  } catch {}
  for (const root of roots) {
    try {
      return createRequire(root)('playwright');
    } catch {}
  }
  skip('Playwright is not installed');
}
const { chromium } = loadPlaywright();

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const step = async (name, fn) => {
  const t = Date.now();
  try {
    const out = await fn();
    results.push({ name, ok: true, ms: Date.now() - t });
    console.log(`✓ ${name}`);
    return out;
  } catch (e) {
    results.push({ name, ok: false, ms: Date.now() - t, error: String(e?.stack ?? e) });
    console.log(`✗ ${name}\n${String(e?.stack ?? e).split('\n').slice(0, 8).join('\n')}`);
    throw e;
  }
};

// ---- The extension, built from a copy of its repository --------------------------------------

const work = mkdtempSync(join(tmpdir(), 'fc-subtitle-real-'));
function buildExtension() {
  if (process.env.SUBTITLE_DIST) return resolve(process.env.SUBTITLE_DIST);
  const copy = join(work, 'subtitle');
  // A copy of the committed tree (the repository is read-only for us)
  const clone = spawnSync('git', ['clone', '-q', REPO, copy], { stdio: 'inherit' });
  if (clone.status !== 0) cpSync(REPO, copy, { recursive: true, filter: (p) => !p.includes('node_modules') });
  for (const args of [['install', '--frozen-lockfile'], ['build']]) {
    const r = spawnSync('pnpm', args, { cwd: copy, encoding: 'utf8', maxBuffer: 64 << 20 });
    if (r.status !== 0) throw new Error(`pnpm ${args.join(' ')} failed in ${copy}:\n${(r.stdout + r.stderr).slice(-4000)}`);
  }
  return join(copy, 'dist');
}

// ---- Flashcard: a temp profile, the module on, a key made like the app's Subtitle screen ------

const db = join(work, 'collection.db');
const call = (method, params = {}) => {
  const out = execFileSync(FC, ['--db', db, '--json', 'call', method, JSON.stringify(params)], { encoding: 'utf8' });
  return out.trim() ? JSON.parse(out) : null;
};
const freePort = async () => {
  const { createServer } = await import('node:net');
  return new Promise((done) => {
    const s = createServer();
    s.listen(0, '127.0.0.1', () => {
      const { port } = s.address();
      s.close(() => done(port));
    });
  });
};
let server = null;
let port = 0;
async function startFlashcard() {
  server = spawn(FC, ['--db', db, 'api', 'serve', '--port', String(port)], { stdio: ['ignore', 'ignore', 'inherit'] });
  for (let i = 0; ; i++) {
    try {
      await fetch(`http://127.0.0.1:${port}/subtitle/v1/me`);
      return;
    } catch (e) {
      if (i > 150 || server.exitCode !== null) throw new Error(`fc api serve did not start: ${e}`);
      await sleep(100);
    }
  }
}
async function stopFlashcard() {
  if (!server || server.exitCode !== null) return;
  const p = server;
  await new Promise((r) => {
    p.once('exit', r);
    p.kill('SIGTERM');
  });
}

// ---- The stand-in YouTube page and the canned translations ------------------------------------

const VIDEO_ID = 'fcRealTest1';
const LINES = [
  { start: 1000, end: 5500, text: 'I just need to pick up my keys.', tr: 'Sadece anahtarlarımı almam gerekiyor.' },
  { start: 7000, end: 11500, text: 'Where did you put the keys?', tr: 'Anahtarları nereye koydun?' },
  { start: 13000, end: 17500, text: 'Did you turn off the lights?', tr: 'Işıkları kapattın mı?' },
  { start: 19000, end: 23500, text: 'The train leaves at seven.', tr: 'Tren yedide kalkıyor.' },
  { start: 25000, end: 29500, text: 'We should go to the station.', tr: 'İstasyona gitmeliyiz.' },
];
// Word → [main translation, part of speech number (Google's: 1 noun, 2 verb)]
const WORDS = {
  keys: ['anahtarlar', 1, 'anahtar'],
  key: ['anahtar', 1, 'anahtar'],
  lights: ['ışıklar', 1, 'ışık'],
  train: ['tren', 1, 'tren'],
  station: ['istasyon', 1, 'istasyon'],
  'pick up': ['almak', 2, 'almak'],
};
const translateText = (text) => {
  const t = text.trim();
  const line = LINES.find((l) => l.text === t);
  if (line) return line.tr;
  const w = WORDS[t.toLowerCase().replace(/[.,!?]/g, '')];
  return w ? w[0] : `[tr] ${t}`;
};

const timedtext = () => ({
  events: LINES.map((l) => ({ tStartMs: l.start, dDurationMs: l.end - l.start, segs: [{ utf8: l.text }] })),
});

const watchPage = `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>The Night Train - YouTube</title>
<style>body{margin:0;background:#0f0f0f;color:#fff;font-family:sans-serif}
#movie_player{position:relative;width:960px;height:540px;background:#000}
video{width:100%;height:100%}
.ytp-chrome-bottom{position:absolute;left:0;right:0;bottom:0;height:48px;display:flex;justify-content:space-between}
.ytp-right-controls{display:flex;gap:4px}
button{background:none;border:0;color:#fff}</style></head>
<body><div id="movie_player" class="html5-video-player">
<video src="/__fc/video.webm" muted playsinline crossorigin="anonymous"></video>
<div class="ytp-chrome-bottom"><div class="ytp-left-controls"><button class="ytp-play-button" aria-label="Oynat">▶</button></div>
<div class="ytp-right-controls"><button class="ytp-subtitles-button" aria-pressed="true">CC</button>
<button class="ytp-settings-button">⚙</button><button class="ytp-size-button">▭</button></div></div></div>
<h1 id="title">The Night Train</h1>
<script>
  // The player asks for its captions like YouTube's does: a signed /api/timedtext request
  const v = document.querySelector('video');
  document.querySelector('.ytp-play-button').onclick = () => (v.paused ? v.play() : v.pause());
  setTimeout(() => fetch('/api/timedtext?v=${VIDEO_ID}&lang=en&kind=&fmt=json3&signature=x'), 300);
</script></body></html>`;

const googleTokens = '<html><script>window.WIZ_global_data = {"FdrFJe":"1","cfb2h":"boq","SNlM0e":"at"};</script></html>';
function batchexecute(text) {
  const key = text.trim().toLowerCase();
  const w = WORDS[key];
  const main = w ? w[0] : translateText(text);
  const content = [
    [null],
    [[[null, null, null, null, null, [[main]]]], 'tr'],
    null,
    w ? [null, null, null, null, null, [[[w[2], [[w[2], null, [main], 1]], null, null, w[1]]]]] : null,
  ];
  const payload = JSON.stringify([['wrb.fr', 'MkEWBc', JSON.stringify(content), null, null, null, 'generic']]);
  return `)]}'\n\n${payload.length}\n${payload}\n`;
}

const outside = new Map();
const translated = [];
async function routes(context) {
  await context.route(/^https?:\/\/(?!127\.0\.0\.1[:/]|localhost[:/])/, async (route) => {
    const req = route.request();
    const url = new URL(req.url());
    if (url.hostname === 'www.youtube.com') {
      if (url.pathname === '/watch') return route.fulfill({ contentType: 'text/html; charset=utf-8', body: watchPage });
      if (url.pathname === '/api/timedtext') return route.fulfill({ json: timedtext() });
      if (url.pathname === '/__fc/video.webm') {
        // With byte ranges, so the player can seek like on a real video host
        const body = readFileSync(join(REPO, 'playground/public/media/sample.webm'));
        const range = /bytes=(\d+)-(\d*)/.exec(req.headers().range ?? '');
        const headers = { 'access-control-allow-origin': '*', 'accept-ranges': 'bytes', 'content-type': 'video/webm' };
        if (!range) return route.fulfill({ status: 200, headers, body });
        const from = Number(range[1]);
        const to = range[2] ? Math.min(Number(range[2]), body.length - 1) : body.length - 1;
        return route.fulfill({
          status: 206,
          headers: { ...headers, 'content-range': `bytes ${from}-${to}/${body.length}` },
          body: body.subarray(from, to + 1),
        });
      }
    }
    if (url.hostname === 'translate.google.com') {
      translated.push(`${req.method()} ${url.pathname} ${(req.postData() ?? '').slice(0, 160)}`);
      if (url.pathname === '/' && req.method() === 'GET') return route.fulfill({ contentType: 'text/html', body: googleTokens });
      if (url.pathname.endsWith('/batchexecute')) {
        const fReq = new URLSearchParams(req.postData() ?? '').get('f.req');
        const text = JSON.parse(JSON.parse(fReq)[0][0][1])[0][0];
        return route.fulfill({ contentType: 'application/json', body: batchexecute(text) });
      }
      if (url.pathname === '/translate_a/single') {
        const q = new URLSearchParams(req.postData() ?? '').get('q') ?? url.searchParams.get('q') ?? '';
        // Several lines in one request (the second line): one sentence per line, like Google
        const parts = q.split('\n');
        const sentences = parts.map((part, i) => {
          const nl = i < parts.length - 1 ? '\n' : '';
          return { trans: translateText(part) + nl, orig: part + nl };
        });
        return route.fulfill({ json: { sentences, src: 'en' } });
      }
    }
    outside.set(`${url.origin}${url.pathname}`, (outside.get(`${url.origin}${url.pathname}`) ?? 0) + 1);
    return route.abort();
  });
}

// ---- Reading Flashcard back ---------------------------------------------------------------------

const recent = () => call('subtitle.captures.recent', { limit: 50 });
const notesOf = (lemma) => call('subtitle.encounters.list', { lang: 'en', lemma });

// ---- The run --------------------------------------------------------------------------------------

let context;
const report = { started: new Date().toISOString(), fc: FC, repo: REPO };
try {
  const dist = await step('extension builds from a copy of its repository', async () => {
    const d = buildExtension();
    assert.ok(existsSync(join(d, 'manifest.json')), 'no manifest.json in the build');
    report.extension_version = JSON.parse(readFileSync(join(d, 'manifest.json'), 'utf8')).version;
    report.extension_commit = spawnSync('git', ['-C', REPO, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).stdout.trim();
    return d;
  });

  const secret = await step('Flashcard: subtitle module on, a key made, the server listens', async () => {
    call('modules.set_enabled', { id: 'subtitle', enabled: true });
    const key = call('subtitle.keys.create', { name: 'Chromium' });
    port = await freePort();
    await startFlashcard();
    return key.secret;
  });
  const serverUrl = `http://127.0.0.1:${port}/subtitle`;

  context = await chromium.launchPersistentContext(join(work, 'profile'), {
    executablePath: CHROMIUM,
    headless: true,
    locale: 'tr-TR',
    viewport: { width: 1280, height: 800 },
    args: [`--disable-extensions-except=${dist}`, `--load-extension=${dist}`, '--lang=tr', '--autoplay-policy=no-user-gesture-required'],
  });
  await routes(context);
  const worker = context.serviceWorkers()[0] ?? (await context.waitForEvent('serviceworker'));
  const extId = new URL(worker.url()).host;
  const consoleErrors = [];
  worker.on('console', (m) => m.type() === 'error' && consoleErrors.push(`[background] ${m.text()}`));

  // -- Settings page: pick Subtitle Kartları, enter the server and the key, test --------------
  const options = await context.newPage();
  options.on('console', (m) => m.type() === 'error' && consoleErrors.push(`[options] ${m.text()}`));
  // "Kaydet" asks Chrome for the server's host (chrome.permissions.request): a browser dialog no
  // automation can click. The stand-in answers like a declined dialog (nothing is granted, which is
  // the truth) and records the ask; the extension then reaches Flashcard through CORS, which the
  // server allows for extension origins. The "Allow" path needs the owner's own browser.
  await options.addInitScript(() => {
    globalThis.__permissionAsks = [];
    if (globalThis.chrome?.permissions)
      chrome.permissions.request = async (asked) => {
        globalThis.__permissionAsks.push(asked);
        return false;
      };
  });
  const openLearning = async () => {
    await options.goto(`chrome-extension://${extId}/src/pages/options/index.html#ogrenme-servisi`);
    await options.getByRole('heading', { name: 'Öğrenme servisi' }).waitFor();
  };
  const saveServer = async (url, key) => {
    await options.getByLabel('Sunucu adresi').fill(url);
    await options.getByLabel('Erişim anahtarı').fill(key);
    // "Kaydet" asks for the server's host (chrome.permissions.request); a click grants it here
    await options.getByRole('button', { name: 'Kaydet' }).click();
  };
  const testConnection = async () => {
    await options.getByRole('button', { name: 'Bağlantıyı test et' }).click();
    const result = options.locator('.es-cards__result');
    await result.waitFor({ timeout: 15000 });
    await options.waitForFunction(() => !document.querySelector('.es-cards__result')?.textContent?.includes('Deneniyor'));
    return (await result.textContent()).trim();
  };

  await step('settings: Öğrenme servisi → Subtitle Kartları, address + key, connection test', async () => {
    await openLearning();
    const row = options.locator('.es-settings-content__item, .setting-row, div').filter({ hasText: /^Öğrenme servisi/ }).last();
    await options.locator('.es-select').first().click();
    await options.getByRole('option', { name: 'Subtitle Kartları', exact: true }).click();
    await options.getByText('Kendi kart uygulamanın sunucusu').waitFor();
    void row;
    await saveServer(serverUrl, secret);
    const text = await testConnection();
    report.test_ok = text;
    report.permission_asks = await options.evaluate(() => globalThis.__permissionAsks);
    report.save_notice = await options.locator('.es-cards__notice').textContent().catch(() => null);
    assert.match(text, /^Bağlandı/, `connection test said: ${text}`);
  });
  if (SHOTS) await options.screenshot({ path: join(SHOTS, '01-settings-connected.png'), fullPage: true });

  // -- The video page --------------------------------------------------------------------------
  const page = await context.newPage();
  page.on('console', (m) => m.type() === 'error' && consoleErrors.push(`[page] ${m.text()}`));
  const subs = page.locator('#es-subs');
  const toast = page.locator('.es-toast').getByRole('status');
  const word = (text) => subs.locator('.es-sub-item').getByText(new RegExp(`^\\W*${text}\\W*$`));
  const playLine = async (index) => {
    const line = LINES[index];
    await page.mouse.move(5, 790);
    const at = await page.evaluate(async (t) => {
      const v = document.querySelector('video');
      v.pause();
      const seeked = new Promise((r) => v.addEventListener('seeked', r, { once: true }));
      v.currentTime = t;
      await seeked;
      await v.play();
      return v.currentTime;
    }, (line.start + 400) / 1000);
    await page
      .waitForFunction((text) => document.querySelector('#es-subs')?.textContent?.includes(text), line.text, { timeout: 15000 })
      .catch(async (e) => {
        const now = await page.evaluate(() => document.querySelector('video').currentTime);
        throw new Error(`line ${index} not shown (seeked to ${at}, now ${now}): ${e.message}`);
      });
    await page.evaluate(() => document.querySelector('video').pause());
  };
  // Hover like a person: if the line re-renders under the pointer (its translation arrives), move
  // away and come back
  const hoverWord = async (text, src = null) => {
    const popover = subs.locator('.es-popover--word');
    for (let attempt = 0; ; attempt++) {
      await word(text).first().hover();
      try {
        await popover.locator('.es-title.es-addable').first().waitFor({ timeout: 5000 });
        if (src) await popover.locator('.es-src-word', { hasText: src }).waitFor({ timeout: 5000 });
        return popover;
      } catch (e) {
        if (attempt >= 4) throw e;
        await page.mouse.move(5, 790);
        await sleep(300);
      }
    }
  };
  const toasts = [];
  const addWord = async (index, text, { expect: expected } = {}) => {
    await playLine(index);
    const popover = await hoverWord(text);
    await popover.locator('.es-title.es-addable').first().click();
    await toast.first().waitFor({ timeout: 20000 });
    const said = (await toast.first().textContent()).trim();
    toasts.push({ word: text, toast: said });
    if (expected) assert.match(said, expected, `toast for "${text}": ${said}`);
    // The toast goes away before the next one
    await page.mouse.move(5, 790);
    await toast.first().waitFor({ state: 'detached', timeout: 15000 }).catch(() => {});
    return said;
  };

  await step('the stand-in YouTube page runs the extension and shows the subtitles', async () => {
    await page.goto(`https://www.youtube.com/watch?v=${VIDEO_ID}`);
    await page.locator('.ytp-play-button').click();
    await playLine(0);
  });

  await step('"+" on "keys" (line 1) → "Karta eklendi", a card in Flashcard', async () => {
    const said = await addWord(0, 'keys', { expect: /^Karta eklendi$/ });
    return said;
  });
  if (SHOTS) await page.screenshot({ path: join(SHOTS, '02-video-added.png') });

  const keyNote = await step('Flashcard has it: note, card, sentence, translation, picture, source + moment', async () => {
    const [attempt] = recent().attempts;
    assert.equal(attempt.result, 'created', JSON.stringify(attempt));
    const { encounters } = notesOf('key');
    assert.equal(encounters.length, 1, JSON.stringify(encounters));
    const [e] = encounters;
    const note = call('notes.get', { id: e.note_id });
    report.first_note = note;
    report.first_encounter = e;
    assert.equal(note.cards.length, 1);
    assert.equal(note.cards[0].id, attempt.card_id);
    assert.equal(note.fields.word, 'key');
    assert.equal(note.fields.surface, 'keys');
    assert.equal(note.fields.meaning, 'anahtarlar');
    assert.equal(note.fields.sentence, 'I just need to pick up my <b>keys</b>.');
    assert.match(note.fields.translation, /anahtarlarımı/);
    assert.match(note.fields.image, /^media:[0-9a-f]{64}$/);
    assert.match(note.fields.source, /The Night Train · 0:01/);
    assert.equal(e.facts.url, `https://www.youtube.com/watch?v=${VIDEO_ID}&t=1s`);
    assert.equal(e.facts.video_id, VIDEO_ID);
    assert.equal(e.facts.site, 'youtube');
    assert.deepEqual([e.facts.t_start_ms, e.facts.t_end_ms], [LINES[0].start, LINES[0].end]);
    assert.equal(e.facts.screenshot_origin, 'original');
    // The picture is a real JPEG frame of the video, stored in Flashcard's media
    const media = call('media.get', { hash: e.facts.screenshot_hash });
    report.first_picture = media;
    return { note, e };
  });

  await step('"+" on "keys" from a second line → the same note gets a second encounter (subtitle.same_word)', async () => {
    await addWord(1, 'keys', { expect: /^Karta eklendi$/ });
    const [attempt] = recent().attempts;
    assert.equal(attempt.result, 'attached', JSON.stringify(attempt));
    const { encounters } = notesOf('key');
    assert.equal(encounters.length, 2, JSON.stringify(encounters));
    assert.deepEqual([...new Set(encounters.map((x) => x.note_id))], [keyNote.e.note_id]);
    assert.deepEqual([...new Set(encounters.map((x) => x.card_id))], [keyNote.note.cards[0].id]);
    assert.equal(encounters.filter((x) => x.is_primary).length, 1);
    const note = call('notes.get', { id: keyNote.e.note_id });
    report.second_note = note;
    assert.equal(note.cards.length, 1, 'still one card');
    assert.match(note.fields.examples, /Where did you put the <b>keys<\/b>\?/);
    assert.equal(note.fields.sentence, keyNote.note.fields.sentence, 'the card keeps its first scene');
  });

  await step('an expression: hover "pick" → "pick up" → "+" → a phrase note', async () => {
    await playLine(0);
    const popover = await hoverWord('pick', 'pick up');
    await popover.locator('.es-title.es-addable').first().click();
    await toast.first().waitFor({ timeout: 20000 });
    const said = (await toast.first().textContent()).trim();
    toasts.push({ word: 'pick up', toast: said });
    assert.equal(said, 'Karta eklendi');
    await page.mouse.move(5, 790);
    await toast.first().waitFor({ state: 'detached', timeout: 15000 }).catch(() => {});
    const { encounters } = notesOf('pick up');
    assert.equal(encounters.length, 1, JSON.stringify(encounters));
    report.expression = encounters[0];
    assert.equal(encounters[0].facts.kind, 'phrase');
    const note = call('notes.get', { id: encounters[0].note_id });
    assert.equal(note.fields.word, 'pick up');
    assert.equal(note.fields.sentence, 'I just need to <b>pick up</b> my keys.');
  });

  const queueStatus = () => worker.evaluate(async () => {
    const all = await chrome.storage.local.get(null);
    return Object.fromEntries(Object.entries(all).filter(([k]) => k.startsWith('flashcards:')).map(([k, v]) => [k, typeof v === 'string' ? v.slice(0, 400) : v]));
  });

  await step('Flashcard closed: "+" keeps the word in the queue; after Flashcard starts again it arrives by itself', async () => {
    await stopFlashcard();
    const said = await addWord(2, 'lights');
    report.closed_toast = said;
    assert.match(said, /^Kuyruğa alındı/);
    assert.equal(notesOf('light').encounters.length + notesOf('lights').encounters.length, 0);
    await startFlashcard();
    const t = Date.now();
    // Nothing is clicked: the outbox's alarm (Chrome waits at least 30 s) sends it
    for (;;) {
      if (notesOf('light').encounters.length + notesOf('lights').encounters.length === 1) break;
      if (Date.now() - t > 150_000) throw new Error(`not delivered after restart: ${JSON.stringify(await queueStatus())}`);
      await sleep(2000);
    }
    report.restart_delivery_ms = Date.now() - t;
  });

  await step('after the restart "+" works at once', async () => {
    await addWord(3, 'train', { expect: /^Karta eklendi$/ });
    assert.equal(notesOf('train').encounters.length, 1);
  });

  await step('a wrong key: the connection test and "+" say so; nothing is written; the right key sends it', async () => {
    await openLearning();
    await saveServer(serverUrl, `${secret.slice(0, -4)}AAAA`);
    report.wrong_key_test = await testConnection();
    assert.doesNotMatch(report.wrong_key_test, /^Bağlandı/);
    const said = await addWord(4, 'station');
    report.wrong_key_toast = said;
    assert.equal(notesOf('station').encounters.length, 0);
    await options.reload();
    await options.getByText('Gönderilemeyenler').waitFor({ timeout: 15000 }).catch(() => {});
    report.wrong_key_queue = (await options.locator('.es-cards__failures, .yt-card').last().innerText()).slice(0, 600);
    if (SHOTS) await options.screenshot({ path: join(SHOTS, '03-settings-wrong-key.png'), fullPage: true });
    await saveServer(serverUrl, secret);
    assert.match(await testConnection(), /^Bağlandı/);
    const retry = options.getByRole('button', { name: 'Tümünü tekrar dene' });
    if (await retry.isVisible()) await retry.click();
    else await options.getByRole('button', { name: 'Şimdi gönder' }).click();
    const t = Date.now();
    while (notesOf('station').encounters.length !== 1) {
      if (Date.now() - t > 60_000) throw new Error(`not delivered with the right key: ${JSON.stringify(await queueStatus())}`);
      await sleep(1000);
    }
  });

  // The extension does not ask for known words today (docs/14 §4). What it would get, asked from
  // its own background (its origin, its stored key), before and after the owner shares them:
  await step('known words from the extension\'s origin: empty until shared, then the captured words', async () => {
    const ask = () =>
      worker.evaluate(async (base) => {
        const { 'persist:flashcardsServer': raw } = await chrome.storage.local.get('persist:flashcardsServer');
        const { token } = JSON.parse(raw);
        const headers = { Authorization: `Bearer ${token}` };
        const me = await (await fetch(`${base}/v1/me`, { headers })).json();
        const known = await (await fetch(`${base}/v1/known-words?lang=en`, { headers })).json();
        return { features: me.features, known };
      }, serverUrl);
    const before = await ask();
    report.known_words_off = before;
    assert.equal(before.features.known_words, false);
    assert.deepEqual(before.known.words, []);
    call('settings.set', { scope: { kind: 'app' }, key: 'subtitle.share_known_words', value: true });
    const after = await ask();
    report.known_words_on = after;
    assert.equal(after.features.known_words, true);
    const lemmas = after.known.words.map((w) => `${w.lemma}:${w.status}`).sort();
    for (const lemma of ['key', 'pick up', 'train', 'station']) assert.ok(lemmas.includes(`${lemma}:learning`), lemmas.join(', '));
  });

  await step('a revoked key: "+" says so and keeps the word', async () => {
    const [k] = call('subtitle.keys.list').keys.filter((x) => !x.revoked_at);
    call('subtitle.keys.revoke', { id: k.id });
    const said = await addWord(0, 'need');
    report.revoked_key_toast = said;
    assert.match(said, /401/);
    assert.equal(notesOf('need').encounters.length + notesOf('need to').encounters.length, 0);
    // The word waits in the extension's outbox (failed, for "Tekrar dene"), not lost
    const kept = await worker.evaluate(async () => {
      const { 'flashcards:outbox': outbox = [], 'flashcards:captures': captures = [] } = await chrome.storage.local.get([
        'flashcards:outbox',
        'flashcards:captures',
      ]);
      // Hovering "need" before "to" opens the expression "need to" (the extension's list)
      const ids = new Set(captures.filter((c) => /^need( to)?$/.test(c.term.lemma)).map((c) => c.capture_id));
      return outbox.filter((e) => ids.has(e.capture_id)).map((e) => e.state);
    });
    report.revoked_key_outbox = kept;
    assert.deepEqual(kept, ['failed']);
  });

  report.toasts = toasts;
  report.final_recent = recent();
  report.console_errors = consoleErrors.slice(0, 40);
  report.queue = await queueStatus();
} catch (e) {
  report.error = String(e?.stack ?? e);
  process.exitCode = 1;
  if (SHOTS && context)
    for (const [i, p] of context.pages().entries()) await p.screenshot({ path: join(SHOTS, `fail-${i}.png`), fullPage: true }).catch(() => {});
} finally {
  report.results = results;
  report.outside_requests = Object.fromEntries(outside);
  report.translate_requests = translated;
  if (context) await context.close().catch(() => {});
  await stopFlashcard();
  const out = process.env.REPORT ?? join(work, 'report.json');
  writeFileSync(out, JSON.stringify(report, null, 2));
  console.log(`${results.filter((r) => r.ok).length}/${results.length} steps passed · report: ${out}`);
  if (!KEEP) rmSync(work, { recursive: true, force: true });
}
