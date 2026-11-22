#!/usr/bin/env node
/**
 * Windows WebDriver pilot (v1.3, NON-gating): drives the INSTALLED desktop app (WebView2)
 * through tauri-driver + msedgedriver and runs three journeys of the Linux suite
 * (`../e2e/journeys`) against the real app, IPC included. See ./README.md.
 *
 *   node apps/desktop/e2e-windows/run.mjs --app "%LOCALAPPDATA%\Flashcard\flashcard.exe" \
 *     --native-driver C:\path\msedgedriver.exe [--tauri-driver tauri-driver] [--out dir]
 *
 * Env equivalents: FC_EXE, MSEDGEDRIVER, TAURI_DRIVER, FC_WIN_E2E_OUT, FC_WIN_E2E_PORT.
 *
 * Journeys (in order, one app launch, one fresh profile):
 *   P  preparation: skip the first-run guide and create decks/notes through the app's own IPC
 *   J1 the app launches and Home renders
 *   J2 Quick Add: N, type front, Tab, type back, Ctrl+Enter → the note is in the chosen deck
 *   J3 Home → Enter → launcher → Enter → Space×3 → 2 → exactly one card answered
 *
 * Prints one PASS/FAIL line per journey, writes screenshots + summary.json into the output
 * dir, exits 1 when anything failed. tauri-driver and the app are always stopped.
 *
 * Zero npm dependencies on purpose (Node 22: global fetch); the WebDriver client is
 * ./webdriver.mjs.
 */
import { spawn, spawnSync } from 'node:child_process';
import { createWriteStream, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { createConnection } from 'node:net';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as sleep } from 'node:timers/promises';
import { KEY, WebDriver } from './webdriver.mjs';

const here = dirname(fileURLToPath(import.meta.url));

// ------------------------------------------------------------------ options

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : undefined;
}

const opts = {
  app: arg('app') ?? process.env.FC_EXE,
  nativeDriver: arg('native-driver') ?? process.env.MSEDGEDRIVER,
  tauriDriver: arg('tauri-driver') ?? process.env.TAURI_DRIVER ?? 'tauri-driver',
  out: resolve(arg('out') ?? process.env.FC_WIN_E2E_OUT ?? join(here, '.results')),
  port: Number(arg('port') ?? process.env.FC_WIN_E2E_PORT ?? 4444),
};
const CI = process.env.CI === 'true';

/** How long a condition may take before a journey fails (no fixed sleeps anywhere). */
const TIMEOUT_MS = 30_000;
/** The first launch of WebView2 on a cold runner is slow. */
const LAUNCH_TIMEOUT_MS = 90_000;

/** The built-in Classic note type (same constant as ../e2e/fixtures.ts). */
const CLASSIC_NOTETYPE_ID = '01900000-0000-7000-8000-000000000001';
/** Tauri `identifier` (src-tauri/tauri.conf.json): the profile is %APPDATA%\<identifier>. */
const IDENTIFIER = 'app.flashcard.desktop';
/** localStorage key Quick Add reads for its preselected deck (QuickAddDialog.svelte). */
const QUICKADD_LAST_DECK = 'fc.quickadd.last_deck';

// ------------------------------------------------------------------ small helpers

const log = (...a) => console.log('[e2e-windows]', ...a);

/** Polls `fn` until it returns something truthy; throws with the last value/error on timeout. */
async function waitFor(what, fn, timeoutMs = TIMEOUT_MS, intervalMs = 250) {
  const until = Date.now() + timeoutMs;
  let last;
  for (;;) {
    try {
      const v = await fn();
      if (v) return v;
      last = v;
    } catch (e) {
      last = e;
    }
    if (Date.now() > until) {
      const detail = last instanceof Error ? last.message : JSON.stringify(last);
      throw new Error(`timed out after ${timeoutMs / 1000}s waiting for ${what} (last: ${detail?.slice(0, 600)})`);
    }
    await sleep(intervalMs);
  }
}

function assert(cond, message) {
  if (!cond) throw new Error(`assertion failed: ${message}`);
}

/** Resolves once something accepts TCP connections on 127.0.0.1:port. */
function portOpen(port) {
  return new Promise((ok) => {
    const s = createConnection({ host: '127.0.0.1', port });
    s.once('connect', () => {
      s.destroy();
      ok(true);
    });
    s.once('error', () => ok(false));
  });
}

// ------------------------------------------------------------------ in-page functions
// Serialized into the webview by Execute Script: they must be self-contained.

/**
 * One public API call through the app's single Tauri command, exactly as the frontend's
 * transport does it (src/lib/api/transport.ts: `invoke('invoke', { command, payload })`,
 * which `@tauri-apps/api/core` forwards to `window.__TAURI_INTERNALS__.invoke`).
 */
function pageRpc(method, params) {
  return window.__TAURI_INTERNALS__.invoke('invoke', { command: `v1.${method}`, payload: params });
}

/** Facts about CSS selectors: present, visible, text, value, disabled, aria-checked. */
function pageProbe(selectors) {
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return r.width > 0 && r.height > 0 && cs.visibility !== 'hidden' && cs.display !== 'none';
  };
  const out = {};
  for (const [k, sel] of Object.entries(selectors)) {
    const el = document.querySelector(sel);
    out[k] = el
      ? {
          visible: visible(el),
          text: (el.textContent || '').trim().slice(0, 300),
          value: 'value' in el ? el.value : null,
          disabled: el.disabled === true || el.getAttribute('aria-disabled') === 'true',
          checked: el.getAttribute('aria-checked'),
        }
      : null;
  }
  out.busy = document.querySelectorAll('#fc-main [aria-busy="true"]').length;
  out.dialogs = document.querySelectorAll('[role=dialog], [role=alertdialog]').length;
  const a = document.activeElement;
  out.active = a ? `${a.tagName.toLowerCase()}${a.id ? `#${a.id}` : ''}` : null;
  return out;
}

// ------------------------------------------------------------------ the run

const results = [];
const summary = {
  started_at: new Date().toISOString(),
  app: opts.app,
  native_driver: opts.nativeDriver,
  node: process.version,
  platform: `${process.platform} ${process.arch}`,
  capabilities: null,
  journeys: results,
};

let driverProc = null;
const wd = new WebDriver(`http://127.0.0.1:${opts.port}`);
const rpc = (method, params = {}) => wd.execAsync(pageRpc, method, params);
const probe = (selectors) => wd.exec(pageProbe, selectors);

async function shot(name) {
  try {
    const file = join(opts.out, `${name}.png`);
    writeFileSync(file, await wd.screenshot());
    return basename(file);
  } catch (e) {
    log(`screenshot ${name} failed: ${e.message}`);
    return null;
  }
}

/** On failure: screenshot + the DOM, for the uploaded artifact. */
async function evidence(id) {
  const files = [];
  const png = await shot(`${id}-fail`);
  if (png) files.push(png);
  try {
    const html = await wd.exec(() => document.documentElement.outerHTML);
    writeFileSync(join(opts.out, `${id}-fail.html`), String(html));
    files.push(`${id}-fail.html`);
  } catch (e) {
    log(`DOM dump ${id} failed: ${e.message}`);
  }
  return files;
}

/** Runs one journey, records PASS/FAIL. A journey whose prerequisite failed is SKIP (= failed run). */
async function journey(id, name, fn, { needs = [] } = {}) {
  const t0 = Date.now();
  const row = { id, name, status: 'FAIL', ms: 0, error: null, files: [], facts: {} };
  results.push(row);
  const missing = needs.filter((n) => results.find((r) => r.id === n)?.status !== 'PASS');
  if (missing.length > 0) {
    row.status = 'SKIP';
    row.error = `needs ${missing.join(', ')}`;
    console.log(`SKIP ${id} ${name} (${row.error})`);
    return;
  }
  try {
    await fn(row.facts);
    row.status = 'PASS';
    const png = await shot(`${id}-pass`);
    if (png) row.files.push(png);
  } catch (e) {
    row.error = String(e?.stack ?? e);
    row.files.push(...(await evidence(id)));
  }
  row.ms = Date.now() - t0;
  console.log(`${row.status} ${id} ${name} (${(row.ms / 1000).toFixed(1)}s)${row.error ? `\n     ${row.error.split('\n')[0]}` : ''}`);
}

// ------------------------------------------------------------------ app-level steps

/** Reloads the webview and waits until the NEW document has the Tauri IPC. */
async function reload() {
  await wd.exec(() => {
    window.__fcPilotOld = true;
    setTimeout(() => location.reload(), 50);
    return true;
  });
  await waitFor('the page to reload', () =>
    wd.exec(() => !window.__fcPilotOld && document.readyState === 'complete' && !!window.__TAURI_INTERNALS__),
  );
}

/**
 * The shell is up, as `openApp` in ../e2e/fixtures.ts checks it: top bar, the sidebar's
 * buttons (menus come from `modules.list`), the main area loaded (no skeletons), a heading.
 */
async function shellReady(timeoutMs = TIMEOUT_MS) {
  return waitFor(
    'the shell (top bar, sidebar, loaded main area)',
    async () => {
      const p = await probe({
        top: '[data-testid="top-bar"]',
        nav: 'nav button, [role=navigation] button',
        main: '#fc-main',
        heading: '#fc-main h1, #fc-main h2',
      });
      return p.top?.visible && p.nav?.visible && p.main?.visible && p.busy === 0 && p.heading?.visible ? p : null;
    },
    timeoutMs,
  );
}

/** Home with its Start button (HomeScreen.svelte `data-testid="home-start"`). */
async function homeReady() {
  await shellReady();
  return waitFor('Home (data-testid=home-start)', async () => {
    const p = await probe({ start: '[data-testid="home-start"]' });
    return p.start?.visible && p.dialogs === 0 ? p : null;
  });
}

/** Moves focus off any control, so page-level keys (Enter, N) go to the page. */
function blur() {
  return wd.exec(() => {
    const a = document.activeElement;
    if (a && a !== document.body && typeof a.blur === 'function') a.blur();
    return true;
  });
}

/** A deck with Classic notes, through IPC (setup, like `makeDeck` in ../e2e/fixtures.ts). */
async function makeDeck(name, pairs) {
  const deck = await rpc('decks.create', { path: name, parent_id: null });
  for (const [front, back] of pairs) {
    await rpc('notes.create', { notetype_id: CLASSIC_NOTETYPE_ID, deck_id: deck.id, fields: { front, back }, tags: [], attributes: [] });
  }
  return deck.id;
}

/** Notes of a deck with their fields (`notesIn` in ../e2e/fixtures.ts). */
async function notesIn(deckId) {
  const page = await rpc('notes.list', { deck_id: deckId, include_subdecks: false, page: { cursor: null, limit: null } });
  const out = [];
  for (const n of page.items) out.push(await rpc('notes.get', { id: n.id }));
  return out;
}

// ------------------------------------------------------------------ journeys

const tag = Date.now().toString(36);
const ctx = { studyDeck: null, quickDeck: null, quickDeckName: `Pilot quick add ${tag}`, tz: 0 };

async function prepare(facts) {
  await waitFor(
    'the webview with Tauri IPC',
    () => wd.exec(() => document.readyState === 'complete' && !!window.__TAURI_INTERNALS__),
    LAUNCH_TIMEOUT_MS,
  );
  facts.url = await wd.getUrl();
  facts.lang = await wd.exec(() => document.documentElement.lang || navigator.language);
  ctx.tz = await wd.exec(() => -new Date().getTimezoneOffset());
  // A fresh profile opens the first-run guide; the Linux seed skips it the same way.
  facts.onboarding_before = await rpc('onboarding.status', {});
  await rpc('onboarding.skip', {});
  ctx.studyDeck = await makeDeck(`Pilot study ${tag}`, [
    ['apple', 'elma'],
    ['book', 'kitap'],
    ['chair', 'sandalye'],
    ['door', 'kapı'],
    ['window', 'pencere'],
  ]);
  ctx.quickDeck = await makeDeck(ctx.quickDeckName, []);
  // Quick Add preselects the last deck it added to (localStorage): make that the pilot's deck.
  await wd.exec((k, v) => localStorage.setItem(k, v), QUICKADD_LAST_DECK, ctx.quickDeck);
  facts.decks = { study: ctx.studyDeck, quick: ctx.quickDeck };
  await reload();
}

async function j1Launch(facts) {
  const shell = await shellReady(LAUNCH_TIMEOUT_MS);
  const home = await homeReady();
  facts.title = await wd.getTitle();
  facts.home_start = home.start.text;
  facts.active = shell.active;
}

/** Mirrors ../e2e/journeys/j1-quick-add.spec.ts (one note instead of two). */
async function j2QuickAdd(facts) {
  await homeReady();
  await blur();
  await wd.press('n');
  await waitFor('the Quick Add dialog with its fields', async () => {
    const p = await probe({ dialog: '[role=dialog]', term: '[role=dialog] textarea', deck: '[role=dialog] [role=combobox]' });
    return p.dialog?.visible && p.term?.visible && p.deck ? p : null;
  });

  // The deck: preselected through localStorage; else pick it like `choose()` does.
  const deckText = (await probe({ deck: '[role=dialog] [role=combobox]' })).deck?.text ?? '';
  if (!deckText.includes(ctx.quickDeckName)) {
    await wd.click(await wd.find('[role=dialog] [role=combobox]'));
    const option = await waitFor('the deck option', () =>
      wd.exec((name) => {
        const o = [...document.querySelectorAll('[role=option]')].find((x) => x.textContent.trim() === name);
        if (!o) return null;
        o.setAttribute('data-fc-pilot-option', '1');
        return true;
      }, ctx.quickDeckName),
    );
    if (option) await wd.click(await wd.find('[data-fc-pilot-option="1"]'));
    await waitFor('the chosen deck in the combobox', async () =>
      (await probe({ deck: '[role=dialog] [role=combobox]' })).deck?.text.includes(ctx.quickDeckName),
    );
  }
  facts.deck_select = (await probe({ deck: '[role=dialog] [role=combobox]' })).deck?.text;

  // Unique per run: a word already in the collection would (rightly) stop at the duplicate check.
  const front = `harbour ${tag}`;
  const back = 'liman';
  const term = await wd.find('[role=dialog] textarea');
  await wd.click(term);
  await waitFor('the term field focused and empty', () =>
    wd.exec(() => {
      const t = document.querySelector('[role=dialog] textarea');
      return document.activeElement === t && t.value === '';
    }),
  );
  await wd.sendKeys(term, front);
  await wd.press(KEY.Tab);
  await wd.sendKeys(await wd.active(), back);
  await wd.press([KEY.Control, KEY.Enter]);

  // Added: the fields clear and the counter says 1.
  await waitFor('the fields to clear and the count to show 1', async () => {
    const p = await probe({ term: '[role=dialog] textarea', count: '[role=dialog] [data-testid="quick-add-count"]' });
    return p.term?.value === '' && p.count?.text.includes('1') ? p : null;
  });

  const notes = await waitFor('the note in the chosen deck (notes.list)', async () => {
    const n = await notesIn(ctx.quickDeck);
    return n.length === 1 ? n : null;
  });
  facts.fields = notes[0].fields;
  assert(notes[0].fields.front === front, `front is ${JSON.stringify(notes[0].fields.front)}, expected ${JSON.stringify(front)}`);
  assert(notes[0].fields.back === back, `back is ${JSON.stringify(notes[0].fields.back)}, expected ${JSON.stringify(back)}`);

  // Close (empty fields: one Esc closes) so the next journey starts from a clean window.
  // Not part of the assertion: J3 reloads the page anyway.
  await wd.press(KEY.Escape);
  facts.closed_with_esc = await waitFor('the dialog to close', async () => (await probe({})).dialogs === 0, 5_000).catch(() => false);
}

/** Mirrors the first half of ../e2e/journeys/j0-s1-keyboard-cards.spec.ts. */
async function j3Study(facts) {
  const studyOptions = () => rpc('study.options', { deck_id: null, include_subdecks: null, tz_offset_mins: ctx.tz });
  const sessionNow = (id) => rpc('study.session', { session_id: id });
  // Nothing may take over the launcher as "Continue".
  for (const r of (await studyOptions()).resumable) await rpc('study.end', { session_id: r.session.id }).catch(() => undefined);
  facts.scopes_before = (await studyOptions()).scopes;

  await reload();
  await homeReady();
  await blur();
  await wd.press(KEY.Enter);
  await waitFor('the study launcher with Start enabled and "due" picked', async () => {
    const p = await probe({
      launcher: '[data-testid="study-launcher"]',
      start: '[data-testid="launcher-start"]',
      due: '[data-testid="launcher-scope-due"] [role=radio]',
    });
    return p.launcher?.visible && p.start && !p.start.disabled && p.due?.checked === 'true' ? p : null;
  });
  await wd.press(KEY.Enter);
  await waitFor('the launcher to close', async () => {
    const p = await probe({ l: '[data-testid="study-launcher"]' });
    return !p.l?.visible;
  });

  // The session the launcher started (the newest open one) and its first card.
  const session = await waitFor('an open session (study.options.resumable)', async () => (await studyOptions()).resumable[0]?.session);
  facts.session = { id: session.id, kind: session.kind, mode: session.mode };
  assert(session.kind === 'normal', `session kind ${session.kind}`);
  assert(session.mode === 'scheduled', `session mode ${session.mode}`);
  const first = (await sessionNow(session.id)).current?.card_id;
  assert(first, 'the session shows a card');
  const reviewsIn = async (card) =>
    (await rpc('study.card_history', { card_id: card })).reviews.filter((r) => r.session_id === session.id).length;

  // Space × 3: the card turns (the answer bar's rating group appears), nothing is graded.
  await waitFor('the study screen (data-testid=study-end)', async () => (await probe({ e: '[data-testid="study-end"]' })).e?.visible);
  await wd.press(KEY.Space, KEY.Space, KEY.Space);
  await waitFor('the answer side (.answer-bar [role=group])', async () => (await probe({ g: '.answer-bar [role=group]' })).g?.visible);
  assert((await reviewsIn(first)) === 0, 'Space graded the card');
  assert((await sessionNow(session.id)).answered === 0, 'Space answered a card');

  // 2 grades this card once; the next card is shown.
  await wd.press('2');
  await waitFor('answered = 1', async () => (await sessionNow(session.id)).answered === 1);
  // Exactly one: the only fixed wait in the pilot, a negative check (as in S1) — give a stray
  // second grade time to land, then read the state again.
  await sleep(1000);
  const now = await sessionNow(session.id);
  facts.answered = now.answered;
  assert(now.answered === 1, `answered is ${now.answered}, expected exactly 1`);
  assert((await reviewsIn(first)) === 1, 'the first card has exactly one review in the session');
  assert(now.current?.card_id && now.current.card_id !== first, 'the next card is shown');
}

// ------------------------------------------------------------------ processes

function startDriver() {
  const args = ['--port', String(opts.port), '--native-port', String(opts.port + 1)];
  if (opts.nativeDriver) args.push('--native-driver', opts.nativeDriver);
  log(`starting ${opts.tauriDriver} ${args.join(' ')}`);
  const logFile = createWriteStream(join(opts.out, 'tauri-driver.log'));
  driverProc = spawn(opts.tauriDriver, args, { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  for (const s of [driverProc.stdout, driverProc.stderr]) {
    s.on('data', (d) => {
      logFile.write(d);
      process.stdout.write(`[tauri-driver] ${d}`);
    });
  }
  driverProc.on('error', (e) => log(`tauri-driver failed to start: ${e.message}`));
  driverProc.on('exit', (code, sig) => log(`tauri-driver exited (${code ?? sig})`));
}

/** Stops tauri-driver with its children (msedgedriver → the app). Never throws. */
function stopAll() {
  if (driverProc && driverProc.exitCode === null) {
    if (process.platform === 'win32') {
      spawnSync('taskkill', ['/F', '/T', '/PID', String(driverProc.pid)], { stdio: 'ignore' });
    } else {
      driverProc.kill('SIGTERM');
    }
  }
  // On a CI runner nothing else is ours to keep: make sure no app or driver survives.
  if (process.platform === 'win32' && CI) {
    for (const image of [opts.app && basename(opts.app), 'msedgedriver.exe', 'tauri-driver.exe'].filter(Boolean)) {
      spawnSync('taskkill', ['/F', '/T', '/IM', image], { stdio: 'ignore' });
    }
  }
}

async function main() {
  mkdirSync(opts.out, { recursive: true });
  if (!opts.app || !existsSync(opts.app)) throw new Error(`the app exe is missing: --app or FC_EXE (got ${opts.app})`);
  if (process.platform === 'win32' && !opts.nativeDriver) throw new Error('--native-driver / MSEDGEDRIVER (msedgedriver.exe matching WebView2) is required');
  if (process.platform !== 'win32') log('warning: this pilot is meant for Windows (WebView2 + msedgedriver)');

  // The app always opens the real per-user profile (%APPDATA%\app.flashcard.desktop):
  // refuse to write test decks into somebody's collection outside CI.
  const profile = process.env.APPDATA ? join(process.env.APPDATA, IDENTIFIER) : null;
  summary.profile = profile;
  summary.profile_existed = profile ? existsSync(profile) : null;
  if (summary.profile_existed && !CI && process.env.FC_WIN_E2E_ALLOW_EXISTING_PROFILE !== '1') {
    throw new Error(`${profile} exists: this run would add decks to that collection. Use a throwaway Windows user, or set FC_WIN_E2E_ALLOW_EXISTING_PROFILE=1.`);
  }

  startDriver();
  await waitFor(`tauri-driver on 127.0.0.1:${opts.port}`, () => portOpen(opts.port), 30_000);
  log(`creating a session for ${opts.app}`);
  await wd.newSession(opts.app);
  summary.capabilities = wd.capabilities;
  log(`session ${wd.sessionId}: browserVersion (WebView2) ${wd.capabilities?.browserVersion}, msedgedriver ${wd.capabilities?.msedge?.msedgedriverVersion ?? '?'}`);
  await wd.setTimeouts({ script: 60_000, pageLoad: 60_000, implicit: 0 });

  await journey('P', 'preparation through IPC (skip onboarding, decks + notes)', prepare);
  await journey('J1', 'app launches and Home renders', j1Launch, { needs: ['P'] });
  await journey('J2', 'Quick Add: N, type, Ctrl+Enter → note in the chosen deck', j2QuickAdd, { needs: ['J1'] });
  await journey('J3', 'Home → Enter → Enter → Space×3, 2 → exactly one answered', j3Study, { needs: ['J1'] });
}

let fatal = null;
try {
  await main();
} catch (e) {
  fatal = String(e?.stack ?? e);
  console.log(`FAIL setup ${fatal}`);
  if (wd.sessionId) await evidence('setup');
} finally {
  await wd.deleteSession();
  stopAll();
  summary.finished_at = new Date().toISOString();
  summary.fatal = fatal;
  summary.ok = !fatal && results.length > 0 && results.every((r) => r.status === 'PASS');
  try {
    mkdirSync(opts.out, { recursive: true });
    writeFileSync(join(opts.out, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`);
  } catch (e) {
    log(`summary.json: ${e.message}`);
  }
  console.log(`\n${summary.ok ? 'PASS' : 'FAIL'} windows pilot: ${results.map((r) => `${r.id}=${r.status}`).join(' ') || 'no journeys ran'} → ${opts.out}`);
}
process.exit(summary.ok ? 0 : 1);
