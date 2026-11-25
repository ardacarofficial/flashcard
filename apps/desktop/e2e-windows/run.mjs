#!/usr/bin/env node
/**
 * Windows journeys (v1.3 pilot; v1.4.2 a gate): drives the INSTALLED desktop app (WebView2)
 * through tauri-driver + msedgedriver and runs journeys against the real app, IPC included.
 * See ./README.md.
 *
 *   node apps/desktop/e2e-windows/run.mjs --app "%LOCALAPPDATA%\Flashcard\flashcard.exe" \
 *     --native-driver C:\path\msedgedriver.exe [--tauri-driver tauri-driver] [--out dir]
 *
 * Env equivalents: FC_EXE, MSEDGEDRIVER, TAURI_DRIVER, FC_WIN_E2E_OUT, FC_WIN_E2E_PORT,
 * FC_WIN_E2E_CDP_PORT, FC_WIN_E2E_STRATEGY (`--strategy`: only that session strategy),
 * FC_WIN_E2E_VERBOSE_DRIVER=0 (`--no-verbose-driver`), FC_CLI (`--fc`: the installed fc.exe,
 * default beside the app), FC_WIN_E2E_BLOCKING (`--blocking`: journeys whose failure fails
 * the run, default BLOCKING below), FC_WIN_E2E_ONLY (`--only J4,J6`: run only these + P/J1).
 *
 * Before the journeys:
 *   PREFLIGHT the installed app runs on its own for 15 s (alive? its window? an error box?
 *             does WebView2 open its DevTools port?), then it is closed
 *   SESSION   strategies tried in order until one gives a WebDriver session (STRATEGIES):
 *             driver+policy, attach+policy, driver; every failed try leaves a diagnosis
 *             (processes with command lines, windows, DevToolsActivePort, CDP, a desktop
 *             screenshot) and a verbose msedgedriver log in the output dir
 *
 * Journeys (in order, one fresh profile; each needs only P/J1, J5–J7 restart the app):
 *   P  preparation: skip the first-run guide and create decks/notes through the app's own IPC
 *   J1 the app launches and Home renders
 *   J2 Quick Add: N, type front, Tab, type back, Ctrl+Enter → the note is in the chosen deck
 *   J3 Home → Enter → launcher → Enter → Space×3 → 2 → exactly one card answered
 *   J4 a Subtitle capture sent to the app's local API (pairing, media, word + sentence) is a
 *      card with its encounter                                    (./data-journeys.mjs)
 *   J5 close the app, start it again: the captured card, its review and encounter are there
 *   J6 backup, a change, restore: the change is gone, the earlier state is back
 *   J7 the app stopped, the collection file truncated, the app started: the native rescue
 *      dialog's "restore backup" leads to a working collection (NON-blocking: native dialog)
 *
 * Prints one PASS/FAIL line per journey, writes screenshots + summary.json into the output
 * dir, exits 1 when a BLOCKING journey (or the session) failed; a non-blocking failure is a
 * warning. tauri-driver and the app are always stopped.
 *
 * Zero npm dependencies on purpose (Node 22: global fetch); the WebDriver client is
 * ./webdriver.mjs.
 */
import { spawn, spawnSync } from 'node:child_process';
import { appendFileSync, cpSync, createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createConnection } from 'node:net';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as sleep } from 'node:timers/promises';
import { execFileSync } from 'node:child_process';
import { j4Capture, j5Reopen, j6BackupRestore, j7Rescue } from './data-journeys.mjs';
import { KEY, WebDriver } from './webdriver.mjs';
import { IS_WINDOWS, POLICY_KEY, WebView2Policy, answerAppDialog, cdp, desktopScreenshot, isElevated, processes, stopApp, windows } from './win.mjs';

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
  /** The fixed DevTools port of the preflight and the attach strategy. */
  cdpPort: Number(arg('cdp-port') ?? process.env.FC_WIN_E2E_CDP_PORT ?? 9333),
  strategy: arg('strategy') ?? process.env.FC_WIN_E2E_STRATEGY ?? null,
  verboseDriver: !process.argv.includes('--no-verbose-driver') && process.env.FC_WIN_E2E_VERBOSE_DRIVER !== '0',
  /** The `fc` CLI the installer puts beside the app (J7 folds the WAL and, as a fallback, rescues). */
  fcCli: arg('fc') ?? process.env.FC_CLI ?? null,
  blocking: (arg('blocking') ?? process.env.FC_WIN_E2E_BLOCKING ?? null)?.split(',').map((s) => s.trim()).filter(Boolean) ?? null,
  only: (arg('only') ?? process.env.FC_WIN_E2E_ONLY ?? null)?.split(',').map((s) => s.trim()).filter(Boolean) ?? null,
};
if (!opts.fcCli && opts.app) opts.fcCli = join(dirname(opts.app), IS_WINDOWS ? 'fc.exe' : 'fc');

/**
 * Journeys whose failure fails the run (v1.4.2). P and J1–J3 have been green on every run
 * since the session strategy was fixed; J4–J6 use only that same machinery (the session, the
 * app's IPC, a loopback HTTP request). J7 is NOT here: it answers a native Win32 message box
 * from outside the app and kills/damages files, the flaky native piece the gate leaves out.
 */
const BLOCKING = opts.blocking ?? ['P', 'J1', 'J2', 'J3', 'J4', 'J5', 'J6'];
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
  elevated: null,
  preflight: null,
  /** Session attempts in order: `{ strategy, try, ok, ms, policy, error, diagnosis }`. */
  attempts: [],
  /** The strategy that gave the session. */
  strategy: null,
  capabilities: null,
  journeys: results,
};

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
  if (opts.only && !['P', 'J1', ...opts.only].includes(id)) return;
  const t0 = Date.now();
  const row = { id, name, status: 'FAIL', blocking: BLOCKING.includes(id), ms: 0, error: null, files: [], facts: {} };
  results.push(row);
  const missing = needs.filter((n) => results.find((r) => r.id === n)?.status !== 'PASS');
  if (missing.length > 0) {
    row.status = 'SKIP';
    row.error = `needs ${missing.join(', ')}`;
    console.log(`SKIP ${id} ${name} (${row.error})`);
    return;
  }
  try {
    // A journey before this one may have left no session (an app restart that failed).
    if (!wd.sessionId && id !== 'P') await reopenSession(`${id}-session`);
    await fn(row.facts);
    row.status = 'PASS';
    const png = await shot(`${id}-pass`);
    if (png) row.files.push(png);
  } catch (e) {
    row.error = String(e?.stack ?? e);
    row.files.push(...(await evidence(id)));
  }
  row.ms = Date.now() - t0;
  const tagLine = row.blocking ? '' : ' [non-blocking]';
  console.log(`${row.status} ${id}${tagLine} ${name} (${(row.ms / 1000).toFixed(1)}s)${row.error ? `\n     ${row.error.split('\n')[0]}` : ''}`);
  if (row.facts.rescued_by) console.log(`     rescued by: ${row.facts.rescued_by}`);
  if (row.status !== 'PASS' && process.env.GITHUB_ACTIONS === 'true') {
    const level = row.blocking ? 'error' : 'warning';
    console.log(`::${level} title=Windows journey ${id}${tagLine}::${(row.error ?? '').split('\n')[0].replace(/[\r\n]/g, ' ').slice(0, 900)}`);
  }
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
  // A forced stop (preflight, a failed session try) makes this start "unclean": its recovery
  // dialog would cover Home. Dismissed for this run; the reload below does not bring it back.
  facts.recovery = await rpc('maintenance.recovery.get', {}).catch((e) => `unavailable: ${e.message}`);
  await rpc('maintenance.recovery.dismiss', {}).catch(() => undefined);
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

let driverProc = null;
/** The app when the pilot started it itself (preflight, attach strategy). */
let appProc = null;
const APP_EXE = opts.app ? basename(opts.app) : null;
const policy = APP_EXE ? new WebView2Policy(APP_EXE) : null;

/**
 * A msedgedriver wrapper that adds `--verbose --log-path` (tauri-driver passes no flags to
 * the native driver). tauri-driver runs it like the exe (Rust runs .cmd files through cmd).
 */
function verboseWrapper(label) {
  const cmd = join(opts.out, `msedgedriver-${label}.cmd`);
  const logPath = join(opts.out, `msedgedriver-${label}.log`);
  writeFileSync(cmd, `@echo off\r\n"${opts.nativeDriver}" %* --verbose "--log-path=${logPath}"\r\n`);
  return cmd;
}

/** (Re)starts tauri-driver; its output goes to `tauri-driver-<label>.log` and the console. */
async function startDriver(label, { verbose = opts.verboseDriver } = {}) {
  await stopDriver();
  const free = async () => !(await portOpen(opts.port)) && !(await portOpen(opts.port + 1));
  await waitFor(`ports ${opts.port} and ${opts.port + 1} to be free`, free, 15_000).catch((e) => log(e.message));
  const native = verbose && opts.nativeDriver && IS_WINDOWS ? verboseWrapper(label) : opts.nativeDriver;
  const args = ['--port', String(opts.port), '--native-port', String(opts.port + 1)];
  if (native) args.push('--native-driver', native);
  log(`starting ${opts.tauriDriver} ${args.join(' ')}`);
  const logFile = createWriteStream(join(opts.out, `tauri-driver-${label}.log`));
  const proc = spawn(opts.tauriDriver, args, { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  driverProc = proc;
  for (const s of [proc.stdout, proc.stderr]) {
    s.on('data', (d) => {
      logFile.write(d);
      process.stdout.write(`[tauri-driver] ${d}`);
    });
  }
  proc.on('error', (e) => log(`tauri-driver failed to start: ${e.message}`));
  proc.on('exit', (code, sig) => {
    log(`tauri-driver exited (${code ?? sig})`);
    logFile.end();
  });
  // Both ears: tauri-driver (port) and the msedgedriver it started (port + 1).
  const up = await waitFor(
    `tauri-driver on ${opts.port} and msedgedriver on ${opts.port + 1}`,
    async () => (await portOpen(opts.port)) && (await portOpen(opts.port + 1)),
    30_000,
  ).then(
    () => true,
    (e) => {
      log(e.message);
      return false;
    },
  );
  if (up) return;
  if (native !== opts.nativeDriver) {
    log('the verbose msedgedriver wrapper did not come up: plain msedgedriver (no driver log file)');
    return startDriver(label, { verbose: false });
  }
  throw new Error(`tauri-driver/msedgedriver did not start (see tauri-driver-${label}.log)`);
}

/** Stops tauri-driver with its children (msedgedriver, the app it launched). Never throws. */
async function stopDriver() {
  const proc = driverProc;
  driverProc = null;
  if (proc && proc.exitCode === null && proc.signalCode === null) {
    if (IS_WINDOWS) spawnSync('taskkill', ['/F', '/T', '/PID', String(proc.pid)], { stdio: 'ignore' });
    else proc.kill('SIGTERM');
    for (let i = 0; i < 20 && proc.exitCode === null && proc.signalCode === null; i++) await sleep(250);
  }
  // On a CI runner nothing else is ours to keep.
  if (IS_WINDOWS && CI) {
    for (const image of ['msedgedriver.exe', 'tauri-driver.exe']) spawnSync('taskkill', ['/F', '/T', '/IM', image], { stdio: 'ignore' });
  }
}

/** Starts the installed app directly (no driver); stdout/stderr → `app-<label>.stdio.log`. */
function launchApp(label) {
  const env = { ...process.env };
  for (const k of Object.keys(env)) if (/^WEBVIEW2_/i.test(k)) delete env[k];
  const logFile = createWriteStream(join(opts.out, `app-${label}.stdio.log`));
  const proc = spawn(opts.app, [], { stdio: ['ignore', 'pipe', 'pipe'], env, windowsHide: false });
  appProc = proc;
  proc.stdout.pipe(logFile, { end: false });
  proc.stderr.pipe(logFile, { end: false });
  proc.on('error', (e) => log(`app (${label}) failed to start: ${e.message}`));
  proc.on('exit', (code, sig) => {
    log(`app (${label}) exited (${code ?? sig})`);
    logFile.end();
  });
  log(`started the app directly (${label}): pid ${proc.pid}`);
  return proc;
}

/** Stops every instance of the app (politely first) and the WebView2 processes of its profile. */
async function stopTheApp() {
  const did = await stopApp(APP_EXE, { marker: IDENTIFIER, ci: CI });
  appProc = null;
  return did;
}

/** `DevToolsActivePort` files: the app's own WebView2 folder and any `--user-data-dir` seen. */
function devToolsActivePort(dirs) {
  const out = {};
  for (const dir of dirs) {
    const f = join(dir, 'DevToolsActivePort');
    try {
      out[f] = readFileSync(f, 'utf8').trim().replace(/\s+/g, ' ');
    } catch {
      out[f] = null;
    }
  }
  return out;
}

/**
 * What is going on right now, for the artifact and the log: the app/WebView2/driver
 * processes with command lines, their windows (an error box has class #32770), whether the
 * WebView2 browser process got `--remote-debugging-port`, DevToolsActivePort files, the CDP
 * endpoint, a desktop screenshot. Writes `<label>-diagnosis.json`. Never throws.
 */
async function diagnose(label, { cdpPort = null } = {}) {
  const d = { label, at: new Date().toISOString() };
  try {
    if (!IS_WINDOWS) {
      d.skipped = 'not windows';
      return d;
    }
    const procs = processes(APP_EXE);
    d.processes = procs;
    const list = Array.isArray(procs) ? procs : [];
    const appPids = list.filter((p) => p.Name?.toLowerCase() === APP_EXE.toLowerCase()).map((p) => p.ProcessId);
    d.app_pids = appPids;
    d.windows = windows(appPids);
    // The WebView2 browser process is the msedgewebview2.exe without `--type=`.
    d.webview2_browsers = list
      .filter((p) => /msedgewebview2/i.test(p.Name ?? '') && !/--type=/.test(p.CommandLine ?? ''))
      .map((p) => {
        const cmd = p.CommandLine ?? '';
        return {
          pid: p.ProcessId,
          parent: p.ParentProcessId,
          remote_debugging: cmd.match(/--remote-debugging-port=\S+/)?.[0] ?? null,
          user_data_dir: cmd.match(/--user-data-dir=("[^"]+"|\S+)/)?.[1]?.replace(/"/g, '') ?? null,
        };
      });
    const dirs = new Set(d.webview2_browsers.map((b) => b.user_data_dir).filter(Boolean));
    if (process.env.LOCALAPPDATA) dirs.add(join(process.env.LOCALAPPDATA, IDENTIFIER, 'EBWebView'));
    d.devtools_active_port = devToolsActivePort([...dirs]);
    if (cdpPort) d.cdp = await cdp(cdpPort);
    const png = `${label}-desktop.png`;
    const err = desktopScreenshot(join(opts.out, png));
    d.desktop_screenshot = err ? `failed: ${err.slice(0, 300)}` : png;
  } catch (e) {
    d.error = String(e?.stack ?? e);
  }
  try {
    writeFileSync(join(opts.out, `${label}-diagnosis.json`), `${JSON.stringify(d, null, 2)}\n`);
  } catch (e) {
    log(`${label}-diagnosis.json: ${e.message}`);
  }
  const wins = Array.isArray(d.windows) ? d.windows.filter((w) => d.app_pids?.includes(w.pid) && w.visible) : [];
  log(
    `diagnosis ${label}: app ${d.app_pids?.length ? `running (pid ${d.app_pids.join(', ')})` : 'NOT running'}` +
      `; app windows: ${wins.map((w) => `"${w.title}" [${w.class}]`).join(', ') || 'none'}` +
      `; WebView2 browser: ${d.webview2_browsers?.map((b) => `${b.remote_debugging ?? 'NO --remote-debugging-port'} data=${b.user_data_dir ?? '?'}`).join(' | ') || 'none'}` +
      `; DevToolsActivePort: ${Object.entries(d.devtools_active_port ?? {}).filter(([, v]) => v).map(([k, v]) => `${k} (${v})`).join(', ') || 'none'}` +
      (d.cdp ? `; CDP ${cdpPort}: ${d.cdp.ok ? `${d.cdp.version?.Browser} (${d.cdp.targets?.length} targets)` : d.cdp.error}` : ''),
  );
  return d;
}

/** The app's own log and crash reports (default profile) into the artifact. */
function collectProfileLogs() {
  if (!process.env.APPDATA) return;
  for (const sub of ['logs', 'crash']) {
    const src = join(process.env.APPDATA, IDENTIFIER, sub);
    try {
      if (existsSync(src)) cpSync(src, join(opts.out, 'app-profile', sub), { recursive: true });
    } catch (e) {
      log(`copying ${src}: ${e.message}`);
    }
  }
}

// ------------------------------------------------------------------ preflight + session

/** How long the app runs on its own in the preflight. */
const PREFLIGHT_MS = 15_000;
/** One session attempt (msedgedriver itself gives up on DevToolsActivePort after 60 s). */
const SESSION_TIMEOUT_MS = 90_000;
/** Attach strategy: how long the app may take to open its DevTools port. */
const CDP_TIMEOUT_MS = 30_000;

/**
 * The installed app on its own (no driver) for 15 s, with the HKLM policy opening a fixed
 * DevTools port: does it start, stay up, show its window (or an error box), does WebView2
 * open the port? Then it is closed. Answers "app problem or driver problem" in one run.
 */
async function preflight() {
  const d = { cdp_port: opts.cdpPort };
  summary.preflight = d;
  if (!IS_WINDOWS) {
    d.skipped = 'not windows';
    return;
  }
  d.stopped_before = await stopTheApp();
  d.policy = policy.set(`--remote-debugging-port=${opts.cdpPort}`);
  log(`preflight: WebView2 policy ${d.policy.hive ? `${d.policy.hive}\\${POLICY_KEY} ${APP_EXE} = ${d.policy.readback}` : `NOT set (${d.policy.errors.join('; ')})`}`);
  const t0 = Date.now();
  const proc = launchApp('preflight');
  while (Date.now() - t0 < PREFLIGHT_MS && proc.exitCode === null) {
    await sleep(1000);
    if (!d.cdp_opened_after_ms && (await cdp(opts.cdpPort)).ok) d.cdp_opened_after_ms = Date.now() - t0;
  }
  d.ran_ms = Date.now() - t0;
  d.alive = proc.exitCode === null;
  d.exit_code = proc.exitCode;
  d.diagnosis = await diagnose('preflight', { cdpPort: opts.cdpPort });
  d.stopped_after = await stopTheApp();
  policy.set(null);
  const wins = (Array.isArray(d.diagnosis.windows) ? d.diagnosis.windows : []).filter((w) => d.diagnosis.app_pids?.includes(w.pid) && w.visible);
  d.line =
    `PREFLIGHT app ${d.alive ? `alive after ${(d.ran_ms / 1000).toFixed(0)}s` : `EXITED with ${d.exit_code} after ${(d.ran_ms / 1000).toFixed(1)}s`}` +
      `, window ${wins.map((w) => `"${w.title}" [${w.class}]`).join(', ') || 'none'}` +
      `, DevTools port ${opts.cdpPort} ${d.cdp_opened_after_ms ? `open after ${(d.cdp_opened_after_ms / 1000).toFixed(1)}s` : 'NEVER opened'}` +
      ` (closed: ${d.stopped_after})`;
  console.log(d.line);
}

/**
 * Ways to get a WebDriver session, tried in order; the first that works is used and logged
 * (`summary.strategy`). See ./README.md "Why the session failed".
 */
const STRATEGIES = [
  {
    id: 'driver+policy',
    how: 'launch',
    tries: 2,
    policy: () => '--remote-debugging-port=0',
    why: 'msedgedriver launches the app; the HKLM WebView2 policy (honoured by elevated hosts) opens the DevTools port',
  },
  {
    id: 'attach+policy',
    how: 'attach',
    tries: 2,
    policy: () => `--remote-debugging-port=${opts.cdpPort}`,
    why: 'the pilot starts the app with a fixed DevTools port (HKLM policy); msedgedriver attaches by debuggerAddress (no DevToolsActivePort file involved)',
  },
  {
    id: 'driver',
    how: 'launch',
    tries: 1,
    policy: () => null,
    why: 'as documented by Tauri: msedgedriver passes the port in WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS (ignored by elevated hosts since WebView2 150)',
  },
];

/**
 * Gets a session: every strategy in order, or only `only` (a restart reuses the one that
 * worked). `prefix` keeps the log files of later (re)starts apart.
 */
async function openSession(only = opts.strategy, prefix = 'session') {
  const list = only ? STRATEGIES.filter((s) => s.id === only) : STRATEGIES;
  if (list.length === 0) throw new Error(`unknown --strategy ${only} (one of ${STRATEGIES.map((s) => s.id).join(', ')})`);
  for (const s of list) {
    for (let i = 1; i <= s.tries; i++) {
      const label = `${prefix}-${s.id.replace('+', '-')}-${i}`;
      const a = { strategy: s.id, why: s.why, try: i, ok: false, ms: 0, policy: null, error: null };
      summary.attempts.push(a);
      const t0 = Date.now();
      let during = null;
      let timer = null;
      try {
        await stopTheApp();
        a.policy = policy ? policy.set(s.policy()) : null;
        await startDriver(label);
        log(`session strategy "${s.id}" try ${i}/${s.tries}: ${s.why}`);
        if (s.how === 'launch') {
          log(`creating a session for ${opts.app}`);
          // While msedgedriver waits for the app: look at what is on screen after 20 s.
          timer = setTimeout(() => (during = diagnose(`${label}-during`)), 20_000);
          await wd.newSession(opts.app, { timeoutMs: SESSION_TIMEOUT_MS });
        } else {
          const proc = launchApp(label);
          const c = await waitFor(
            `the DevTools endpoint 127.0.0.1:${opts.cdpPort} with a page`,
            async () => {
              if (proc.exitCode !== null) return { exited: proc.exitCode };
              const r = await cdp(opts.cdpPort);
              return r.ok && r.targets.some((t) => t.type === 'page') ? r : null;
            },
            CDP_TIMEOUT_MS,
            500,
          );
          if (c.exited !== undefined) throw new Error(`the app exited (${c.exited}) before opening its DevTools port`);
          a.cdp = { browser: c.version?.Browser, targets: c.targets };
          log(`attaching to 127.0.0.1:${opts.cdpPort} (${c.version?.Browser})`);
          await wd.attachSession(`127.0.0.1:${opts.cdpPort}`, { timeoutMs: 60_000 });
        }
        clearTimeout(timer);
        if (during) await during;
        a.ok = true;
        a.ms = Date.now() - t0;
        summary.strategy = s.id;
        log(`session strategy "${s.id}" try ${i}/${s.tries}: OK in ${(a.ms / 1000).toFixed(1)}s`);
        a.diagnosis = await diagnose(`${label}-ok`, { cdpPort: s.how === 'attach' ? opts.cdpPort : null });
        return s;
      } catch (e) {
        clearTimeout(timer);
        if (during) await during;
        a.ms = Date.now() - t0;
        a.error = String(e?.message ?? e);
        log(`session strategy "${s.id}" try ${i}/${s.tries}: FAILED after ${(a.ms / 1000).toFixed(1)}s: ${a.error.split('\n')[0]}`);
        a.diagnosis = await diagnose(label, { cdpPort: s.how === 'attach' ? opts.cdpPort : null });
        await wd.deleteSession();
        await stopDriver();
        await stopTheApp();
      }
    }
  }
  throw new Error(
    `no WebDriver session: every strategy failed (${list.map((s) => s.id).join(', ')}). ` +
      'See preflight-diagnosis.json, session-*-diagnosis.json, msedgedriver-*.log and tauri-driver-*.log in the results.',
  );
}

// ------------------------------------------------------------------ restarts (J5–J7)

/** Closes the app: politely first (WM_CLOSE), then by force; the session and drivers go with it. */
async function closeApp(label) {
  const did = await stopTheApp();
  log(`${label}: app closed (${did})`);
  await wd.deleteSession();
  await stopDriver();
}

/**
 * After a (re)start: the webview with IPC, the crash-recovery offer a forced stop leaves
 * behind dismissed (as in P), the page reloaded, the shell up.
 */
async function afterStart() {
  await wd.setTimeouts({ script: 60_000, pageLoad: 60_000, implicit: 0 });
  await waitFor(
    'the webview with Tauri IPC',
    () => wd.exec(() => document.readyState === 'complete' && !!window.__TAURI_INTERNALS__),
    LAUNCH_TIMEOUT_MS,
  );
  await rpc('maintenance.recovery.dismiss', {}).catch(() => undefined);
  await reload();
  await shellReady(LAUNCH_TIMEOUT_MS);
}

/** Starts the app again with the strategy that worked and waits until it is usable. */
async function reopenSession(label) {
  await stopTheApp();
  await openSession(summary.strategy ?? opts.strategy, label);
  await afterStart();
}

/**
 * J7: starts the app on a damaged collection. The shell shows its native rescue dialog before
 * any window (`session.rs` → `offer_rescue`); while msedgedriver waits for the webview, the
 * dialog is answered "Yes = restore the newest verified backup" from outside (a Win32 message
 * box: `answerAppDialog`), then the info box "rescued" with OK, and the session comes up on the
 * rescued collection. Two other outcomes are handled and NAMED in `rescued_by`:
 *  - the truncated file still opened (no native dialog): its integrity check must fail and
 *    the in-app rescue (Maintenance page → `maintenance.rescue.request`, the app restarts and
 *    rescues before opening) is used;
 *  - the dialog could not be answered / the app did not come up: the installed `fc rescue
 *    --from backup` (the same `rescue()` the dialog runs) makes the choice instead, and the
 *    app is started on its result. Then only the dialog itself is NOT covered.
 */
async function startDamaged(label, { db = join(process.env.APPDATA ?? '', IDENTIFIER, 'collection.db') } = {}) {
  const s = STRATEGIES.find((x) => x.id === (summary.strategy ?? opts.strategy ?? 'driver+policy'));
  const out = { strategy: s.id, dialogs: [] };
  await stopTheApp();
  policy?.set(s.policy());
  await startDriver(label);
  let settled = null;
  const session = (async () => {
    if (s.how === 'launch') return wd.newSession(opts.app, { timeoutMs: SESSION_TIMEOUT_MS });
    launchApp(label);
    await waitFor(`the DevTools endpoint 127.0.0.1:${opts.cdpPort}`, async () => (await cdp(opts.cdpPort)).ok, SESSION_TIMEOUT_MS, 500);
    return wd.attachSession(`127.0.0.1:${opts.cdpPort}`, { timeoutMs: 60_000 });
  })().then(
    () => (settled = { ok: true }),
    (e) => (settled = { ok: false, error: String(e?.message ?? e) }),
  );
  while (!settled) {
    await Promise.race([session, sleep(1500)]);
    if (settled || !IS_WINDOWS) continue;
    const seen = answerAppDialog(APP_EXE, { press: false });
    if (!seen.found) continue;
    // The dialog as the owner sees it, before it is answered.
    const png = `${label}-dialog-${out.dialogs.length + 1}.png`;
    out.dialog_screenshot_error = desktopScreenshot(join(opts.out, png));
    const answered = answerAppDialog(APP_EXE);
    out.dialogs.push({ ...answered, screenshot: png });
    log(`${label}: native dialog (${answered.kind ?? '?'}) "${(answered.text ?? seen.text ?? '').slice(0, 160)}" → ${answered.button ?? answered.error ?? 'not answered'}`);
    if (out.dialogs.length > 6) break;
  }
  await session;
  const yes = out.dialogs.some((d) => d.kind === 'rescue offer' && d.posted);
  if (settled.ok) {
    await afterStart();
    if (yes) {
      out.rescued_by = 'native dialog: Yes = "restore backup" (Win32 message box answered with WM_COMMAND IDYES), then OK';
      return out;
    }
    if (out.dialogs.length === 0) {
      // The file still opened: the app must see the damage and rescue on request.
      const check = await rpc('maintenance.integrity.check', { quick: false });
      out.integrity_before = { ok: check.ok };
      if (check.ok) throw new Error(`expected the truncated collection to be offered a rescue; read back: it opened without a dialog and passed its integrity check (${JSON.stringify(check).slice(0, 300)})`);
      const before = processes(APP_EXE);
      const oldPids = new Set((Array.isArray(before) ? before : []).filter((p) => p.Name?.toLowerCase() === APP_EXE.toLowerCase()).map((p) => p.ProcessId));
      await rpc('maintenance.rescue.request', { mode: 'backup' });
      // The app restarts itself and rescues before it opens the collection: wait for the new
      // process's window (the rescue is done by then), then take it over with a session.
      await waitFor(
        'the restarted app (new process) with its window',
        () => {
          const list = processes(APP_EXE);
          const pids = (Array.isArray(list) ? list : []).filter((p) => p.Name?.toLowerCase() === APP_EXE.toLowerCase() && !oldPids.has(p.ProcessId)).map((p) => p.ProcessId);
          const wins = pids.length ? windows(pids) : [];
          return Array.isArray(wins) && wins.some((w) => pids.includes(w.pid) && w.visible && w.class !== '#32770');
        },
        LAUNCH_TIMEOUT_MS,
        1000,
      );
      await closeApp(`${label}-after-request`);
      await reopenSession(`${label}-reopen`);
      const st = await rpc('maintenance.status', {});
      out.in_app_rescue = st.rescue;
      out.rescued_by = 'in-app rescue (the truncated file still opened, failed its integrity check; maintenance.rescue.request → restart). The native dialog was NOT shown';
      return out;
    }
  }
  // The dialog was not answered, or the app did not come up after it.
  out.session_error = settled.ok ? 'the app came up but the dialog was not "restore backup"' : settled.error;
  out.diagnosis = await diagnose(`${label}-failed`);
  await wd.deleteSession();
  await stopDriver();
  await stopTheApp();
  let report = null;
  try {
    report = JSON.parse(execFileSync(opts.fcCli, ['--db', db, 'rescue', '--from', 'backup'], { encoding: 'utf8', windowsHide: true }));
  } catch (e) {
    out.fc_rescue_error = String(e?.stderr ?? e?.message ?? e).slice(0, 600);
  }
  out.fc_rescue = report && { mode: report.mode, backup: report.backup, kept_in: report.kept_in };
  out.rescued_by = `FALLBACK fc rescue --from backup: the native dialog ${out.dialogs.length ? 'was answered but' : 'was not seen and'} the app did not come up (${out.session_error.split('\n')[0].slice(0, 200)}). The dialog is NOT covered by this run`;
  await reopenSession(`${label}-fallback`);
  return out;
}

async function stopAll() {
  await stopDriver();
  // The app msedgedriver launched is closed with the session; one the pilot started (attach)
  // or any leftover on a CI runner is stopped here.
  if (IS_WINDOWS && (CI || appProc)) await stopTheApp();
  else if (appProc && appProc.exitCode === null) appProc.kill('SIGTERM');
  policy?.restore();
}

async function main() {
  mkdirSync(opts.out, { recursive: true });
  if (!opts.app || !existsSync(opts.app)) throw new Error(`the app exe is missing: --app or FC_EXE (got ${opts.app})`);
  if (IS_WINDOWS && !opts.nativeDriver) throw new Error('--native-driver / MSEDGEDRIVER (msedgedriver.exe matching WebView2) is required');
  if (!IS_WINDOWS) log('warning: this pilot is meant for Windows (WebView2 + msedgedriver)');

  // The app always opens the real per-user profile (%APPDATA%\app.flashcard.desktop):
  // refuse to write test decks into somebody's collection outside CI.
  const profile = process.env.APPDATA ? join(process.env.APPDATA, IDENTIFIER) : null;
  summary.profile = profile;
  summary.profile_existed = profile ? existsSync(profile) : null;
  if (summary.profile_existed && !CI && process.env.FC_WIN_E2E_ALLOW_EXISTING_PROFILE !== '1') {
    throw new Error(`${profile} exists: this run would add decks to that collection. Use a throwaway Windows user, or set FC_WIN_E2E_ALLOW_EXISTING_PROFILE=1.`);
  }
  summary.elevated = IS_WINDOWS ? isElevated() : null;
  log(`elevated: ${summary.elevated} (an elevated WebView2 host ignores WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS since runtime 150)`);

  await preflight();
  await openSession();
  summary.capabilities = wd.capabilities;
  log(`session ${wd.sessionId} (${summary.strategy}): browserVersion (WebView2) ${wd.capabilities?.browserVersion}, msedgedriver ${wd.capabilities?.msedge?.msedgedriverVersion ?? '?'}`);
  await wd.setTimeouts({ script: 60_000, pageLoad: 60_000, implicit: 0 });

  await journey('P', 'preparation through IPC (skip onboarding, decks + notes)', prepare);
  await journey('J1', 'app launches and Home renders', j1Launch, { needs: ['P'] });
  await journey('J2', 'Quick Add: N, type, Ctrl+Enter → note in the chosen deck', j2QuickAdd, { needs: ['J1'] });
  await journey('J3', 'Home → Enter → Enter → Space×3, 2 → exactly one answered', j3Study, { needs: ['J1'] });

  // v1.4.2: the owner's data (./data-journeys.mjs). Each one makes its own decks/capture/key.
  if (opts.fcCli && !existsSync(opts.fcCli)) log(`warning: no fc CLI at ${opts.fcCli} (J7 needs it)`);
  const env = {
    tag,
    out: opts.out,
    rpc,
    log,
    ui: { ready: () => shellReady(), reload, shot },
    closeApp,
    startApp: reopenSession,
    startDamaged,
    fc: (args) => execFileSync(opts.fcCli, args, { encoding: 'utf8', windowsHide: true }),
  };
  await journey('J4', 'Subtitle capture over the local API → card with its encounter', (f) => j4Capture(env, f), { needs: ['J1'] });
  await journey('J5', 'close, start again: card, review and encounter are there', (f) => j5Reopen(env, f), { needs: ['J1'] });
  await journey('J6', 'backup → change → restore: the change is gone', (f) => j6BackupRestore(env, f), { needs: ['J1'] });
  await journey('J7', 'truncated collection → rescue dialog → restored from the verified backup', (f) => j7Rescue(env, f), { needs: ['J1'] });
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
  await stopAll();
  collectProfileLogs();
  summary.finished_at = new Date().toISOString();
  summary.fatal = fatal;
  summary.ok = !fatal && results.length > 0 && results.every((r) => r.status === 'PASS');
  summary.blocking = BLOCKING;
  /** The gate: no setup failure and every blocking journey that was due passed. */
  summary.gate_ok = !fatal && results.some((r) => r.blocking) && results.filter((r) => r.blocking).every((r) => r.status === 'PASS');
  summary.non_blocking_failed = results.filter((r) => !r.blocking && r.status !== 'PASS').map((r) => r.id);
  try {
    mkdirSync(opts.out, { recursive: true });
    writeFileSync(join(opts.out, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`);
  } catch (e) {
    log(`summary.json: ${e.message}`);
  }
  const tried = summary.attempts.map((a) => `${a.strategy}#${a.try}=${a.ok ? 'ok' : 'fail'}`).join(' ') || 'none';
  const sessionLine = `SESSION ${summary.strategy ? `via "${summary.strategy}"` : 'NONE'} (attempts: ${tried}; elevated: ${summary.elevated})`;
  const finalLine =
    `${summary.gate_ok ? 'PASS' : 'FAIL'} windows journeys: ${results.map((r) => `${r.id}=${r.status}${r.blocking ? '' : '(non-blocking)'}`).join(' ') || 'no journeys ran'}` +
    (summary.gate_ok && summary.non_blocking_failed.length ? ` — WARNING: non-blocking ${summary.non_blocking_failed.join(', ')} failed` : '');
  console.log(sessionLine);
  console.log(`\n${finalLine} → ${opts.out}`);
  // The workflow run's summary page: the three lines that answer "what happened".
  if (process.env.GITHUB_STEP_SUMMARY) {
    const rows = results.map(
      (r) =>
        `- ${r.status} ${r.id}${r.blocking ? '' : ' (non-blocking)'} ${r.name}${r.error ? `: ${r.error.split('\n')[0]}` : ''}${r.facts?.rescued_by ? ` — rescued by: ${r.facts.rescued_by}` : ''}`,
    );
    const md = ['### Windows journeys (installed app)', '', `- ${summary.preflight?.line ?? 'PREFLIGHT skipped'}`, `- ${sessionLine}`, ...rows, `- **${finalLine}**`, fatal ? `- setup: ${fatal.split('\n')[0]}` : ''];
    try {
      appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${md.filter(Boolean).join('\n')}\n`);
    } catch (e) {
      log(`step summary: ${e.message}`);
    }
  }
}
process.exit(summary.gate_ok ? 0 : 1);
