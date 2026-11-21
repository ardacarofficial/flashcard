#!/usr/bin/env node
/**
 * Visual QA harness (docs/11 §5). Starts the mock-mode app (vite dev server, the mock
 * transport is dev-only), drives headless Chromium through every screen and key state for
 * TR/EN x light/dark x 1280/900, saves PNGs, a contact sheet and a JSON check report.
 *
 *   pnpm screens [--strict] [--quick] [--langs tr,en] [--themes light,dark] [--widths 1280,900]
 *                [--url http://localhost:1420] [--out screens] [--no-static]
 *
 * Screens are discovered, not listed: every button of the shell nav (`[data-testid="nav-item"]`,
 * else `nav ul button`), then every sub-navigation item inside the screen
 * (`[data-testid="subnav-item"]`, `[role=tab]`, `[data-testid^="settings-group"]`).
 * State hooks (all optional, see docs/11 §5): `deck-row`, `study-start`, `flip`, `study-end`,
 * `test-submit`, `match-tile` (+ `data-side`), `term-list`, `.view-toggle`, `browse-row`,
 * `browse-select`, `settings-select`, `update-open`, `open-dialog-*`; mock scenarios
 * `?mock=fresh|update|locked`. Without them the script falls back to the v1.0 markup.
 * Exit code is non-zero only with `--strict` and findings.
 */
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const flag = (n) => args.includes(`--${n}`);
const opt = (n, d) => {
  const i = args.indexOf(`--${n}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : d;
};
const strict = flag('strict');
const quick = flag('quick');
const langs = opt('langs', quick ? 'en' : 'tr,en').split(',');
const themes = opt('themes', quick ? 'light' : 'light,dark').split(',');
const widths = opt('widths', quick ? '1280' : '1280,900').split(',').map(Number);
const outDir = join(root, opt('out', 'screens'));
const PORT = Number(opt('port', 0)) || 20000 + Math.floor(Math.random() * 20000);
const baseUrl = opt('url', `http://127.0.0.1:${PORT}`);

const slug = (s) =>
  s
    .toLowerCase()
    .replace(/ı/g, 'i').replace(/ş/g, 's').replace(/ğ/g, 'g').replace(/ç/g, 'c').replace(/ö/g, 'o').replace(/ü/g, 'u').replace(/İ/gi, 'i')
    .normalize('NFKD')
    .replace(/[^\w]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 40) || 'x';

function browserPath() {
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE) return process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
  try {
    const p = chromium.executablePath();
    if (existsSync(p)) return p;
  } catch {
    /* not installed */
  }
  const cache = join(homedir(), '.cache', 'ms-playwright');
  if (existsSync(cache)) {
    for (const d of readdirSync(cache).filter((x) => x.startsWith('chromium-')).sort().reverse()) {
      const p = join(cache, d, 'chrome-linux', 'chrome');
      if (existsSync(p)) return p;
    }
  }
  throw new Error('No Chromium: run `npx playwright install chromium`.');
}

async function startServer() {
  if (opt('url', null)) return null;
  await new Promise((res, rej) => {
    const p = spawn(process.execPath, [join(root, 'scripts/merge-locales.mjs')], { cwd: root, stdio: 'inherit' });
    p.on('exit', (c) => (c === 0 ? res() : rej(new Error('merge-locales failed'))));
  });
  const vite = join(root, 'node_modules/vite/bin/vite.js');
  const child = spawn(process.execPath, [vite, '--mode', 'mock', '--host', '127.0.0.1', '--port', String(PORT), '--strictPort'], {
    cwd: root,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let log = '';
  child.stdout.on('data', (d) => (log += d));
  child.stderr.on('data', (d) => (log += d));
  for (let i = 0; i < 120; i++) {
    try {
      if ((await fetch(baseUrl)).ok) return child;
    } catch {
      /* not up yet */
    }
    if (child.exitCode !== null) throw new Error(`vite exited:\n${log}`);
    await new Promise((r) => setTimeout(r, 500));
  }
  child.kill();
  throw new Error(`vite did not start:\n${log}`);
}

/** Runs inside the page. Returns findings for the current DOM. */
function pageChecks() {
  const out = [];
  const vw = document.documentElement.clientWidth;
  const vh = window.innerHeight;
  const add = (check, el, detail) => out.push({ check, el: describe(el), detail });
  function describe(el) {
    if (!el || !el.tagName) return '';
    const id = el.getAttribute('data-testid');
    const cls = typeof el.className === 'string' && el.className ? '.' + el.className.trim().split(/\s+/).slice(0, 2).join('.') : '';
    const txt = (el.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 30);
    return `${el.tagName.toLowerCase()}${id ? `[${id}]` : ''}${cls}${txt ? ` "${txt}"` : ''}`;
  }
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    const s = getComputedStyle(el);
    return s.visibility !== 'hidden' && s.display !== 'none' && Number(s.opacity) > 0;
  };

  // 1. horizontal overflow of the page
  const doc = document.documentElement;
  if (doc.scrollWidth > vw + 1) add('overflow-x', doc, `scrollWidth ${doc.scrollWidth} > ${vw}`);

  const all = [...document.querySelectorAll('body *')].filter((e) => !['SCRIPT', 'STYLE'].includes(e.tagName));
  const clippedByAncestor = (el) => {
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      const s = getComputedStyle(p);
      if (s.overflowX !== 'visible' || s.overflowY !== 'visible') {
        const r = p.getBoundingClientRect();
        if (r.left >= -1 && r.right <= vw + 1) return true;
      }
    }
    return false;
  };

  // 2. elements outside the viewport (horizontally; vertical scrolling is normal)
  for (const el of all) {
    if (!visible(el) || getComputedStyle(el).position === 'fixed' && false) continue;
    const r = el.getBoundingClientRect();
    if ((r.right > vw + 1 || r.left < -1) && !clippedByAncestor(el)) {
      add('outside-viewport', el, `x ${Math.round(r.left)}..${Math.round(r.right)} of ${vw}`);
    }
    if (getComputedStyle(el).position === 'fixed' && (r.bottom > vh + 1 || r.top < -1)) {
      add('outside-viewport', el, `y ${Math.round(r.top)}..${Math.round(r.bottom)} of ${vh}`);
    }
  }

  // 3. clipped text
  const hasOwnText = (el) => [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
  for (const el of all) {
    if (!hasOwnText(el) || !visible(el)) continue;
    // Screen-reader-only text is clipped on purpose (1×1 px box).
    if (el.closest('.sr-only')) continue;
    const s = getComputedStyle(el);
    if (s.overflowX === 'visible' && s.textOverflow !== 'ellipsis') continue;
    if (el.scrollWidth > el.clientWidth + 1 && el.clientWidth > 0) {
      add('text-clipped', el, `scrollWidth ${el.scrollWidth} > clientWidth ${el.clientWidth}`);
    }
  }

  // 4. raw controls outside lib/ui (components of lib/ui mark themselves with data-fc-ui)
  for (const el of document.querySelectorAll('select, input[type=checkbox], input[type=radio]')) {
    if (el.closest('[data-fc-ui]')) continue;
    add('raw-control', el, el.tagName === 'SELECT' ? 'raw <select>' : `raw input[type=${el.type}]`);
  }

  // 5. raw i18n keys
  const KEY = /^[a-z][a-z0-9_]*(\.[a-z0-9_]+){1,5}$/;
  const FILE = /\.(json|js|ts|md|txt|png|svg|csv|apkg|html|css|toml|zip|db|com|org|io|dev|net)$/i;
  const isKey = (s) => KEY.test(s) && !FILE.test(s) && !/^\d/.test(s);
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const s = n.textContent.trim();
    if (s && isKey(s) && n.parentElement && visible(n.parentElement) && !n.parentElement.closest('code, pre, kbd')) {
      add('i18n-raw-key', n.parentElement, s);
    }
  }
  for (const el of document.querySelectorAll('[placeholder],[aria-label],[title]')) {
    for (const a of ['placeholder', 'aria-label', 'title']) {
      const v = el.getAttribute(a);
      if (v && isKey(v.trim())) add('i18n-raw-key', el, `${a}=${v}`);
    }
  }

  // 6. contrast (WCAG AA) of rendered text vs. composited background
  const cv = document.createElement('canvas');
  cv.width = cv.height = 1;
  const cx = cv.getContext('2d', { willReadFrequently: true });
  const rgba = (css) => {
    cx.clearRect(0, 0, 1, 1);
    cx.fillStyle = '#000';
    cx.fillStyle = css;
    cx.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = cx.getImageData(0, 0, 1, 1).data;
    return [r, g, b, a / 255];
  };
  const over = (top, bottom) => {
    const a = top[3] + bottom[3] * (1 - top[3]);
    if (a === 0) return [0, 0, 0, 0];
    return [0, 1, 2].map((i) => (top[i] * top[3] + bottom[i] * bottom[3] * (1 - top[3])) / a).concat(a);
  };
  const lum = ([r, g, b]) => {
    const f = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
  };
  const background = (el) => {
    const layers = [];
    for (let p = el; p; p = p.parentElement) {
      const s = getComputedStyle(p);
      if (s.backgroundImage !== 'none') return null; // gradients/images: cannot sample
      const c = rgba(s.backgroundColor);
      if (c[3] > 0) layers.push(c);
      if (c[3] === 1) break;
    }
    let acc = [255, 255, 255, 1];
    for (const l of layers.reverse()) acc = over(l, acc);
    return acc;
  };
  const seen = new Set();
  for (const el of all) {
    if (!hasOwnText(el) || !visible(el)) continue;
    if (el.closest('[disabled], [aria-disabled=true]') || el.matches('option')) continue;
    const s = getComputedStyle(el);
    const bg = background(el);
    if (!bg) continue;
    const fg = over(rgba(s.color), bg);
    const [l1, l2] = [lum(fg), lum(bg)];
    const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    const px = parseFloat(s.fontSize);
    const large = px >= 24 || (px >= 18.66 && Number(s.fontWeight) >= 700);
    const need = large ? 3 : 4.5;
    if (ratio < need) {
      const key = `${s.color}|${bg.map(Math.round)}|${px}`;
      if (seen.has(key)) continue;
      seen.add(key);
      add('contrast', el, `${ratio.toFixed(2)} < ${need} (${s.color} on rgb(${bg.slice(0, 3).map(Math.round)}))`);
    }
  }
  return out;
}

/** Static scan: raw selects/checkboxes in sources outside src/lib/ui. */
function staticRawControls() {
  const found = [];
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) {
        if (relative(join(root, 'src/lib'), p) === 'ui' || e.name === 'generated') continue;
        walk(p);
      } else if (e.name.endsWith('.svelte')) {
        const src = readFileSync(p, 'utf8');
        const re = /<select\b|type=["']?(checkbox|radio)/g;
        for (let m; (m = re.exec(src)); ) {
          found.push({ file: relative(root, p), line: src.slice(0, m.index).split('\n').length, match: m[0] });
        }
      }
    }
  };
  walk(join(root, 'src'));
  return found;
}

const NAV = '[data-testid="nav-item"], nav ul button';
const SUB = '[data-testid="subnav-item"], [data-testid^="settings-group"], main [role=tab]';

async function settle(page) {
  await page.waitForLoadState('networkidle').catch(() => {});
  await page.waitForTimeout(250);
}

async function newPage(browser, { lang, theme, width }, query = '') {
  const ctx = await browser.newContext({
    viewport: { width, height: 800 },
    colorScheme: theme,
    locale: lang === 'tr' ? 'tr-TR' : 'en-US',
  });
  await ctx.addInitScript(
    ([l, th]) => {
      try {
        localStorage.setItem('fc.locale', l);
        localStorage.removeItem('fc.theme.v2');
      } catch {
        /* ignore */
      }
      document.addEventListener('DOMContentLoaded', () => (document.documentElement.dataset.theme = th));
    },
    [lang, theme],
  );
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e.message)));
  // Console errors name no URL ("Failed to load resource"): log the failing requests themselves.
  page.on('console', (m) => m.type() === 'error' && !/^Failed to load resource/.test(m.text()) && errors.push(m.text()));
  page.on('requestfailed', (r) => errors.push(`request failed: ${r.url()} (${r.failure()?.errorText ?? '?'})`));
  page.on('response', (r) => r.status() >= 400 && errors.push(`HTTP ${r.status()}: ${r.url()}`));
  await page.goto(baseUrl + query);
  await page.waitForSelector(query.includes('mock=locked') ? '[role=dialog], h1' : NAV, { timeout: 20000 });
  await settle(page);
  return { ctx, page, errors };
}

async function main() {
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  const server = await startServer();
  const browser = await chromium.launch({ executablePath: browserPath(), args: ['--no-sandbox'] });
  const report = { generated: new Date().toISOString(), configs: [], findings: [], static: [], totals: {} };
  const sheet = [];
  let shots = 0;
  // Stable names across languages: discovered once, in the first config.
  let navNames = null;

  try {
    for (const lang of langs) {
      for (const theme of themes) {
        for (const width of widths) {
          const cfg = `${lang}-${theme}-${width}`;
          const dir = join(outDir, cfg);
          mkdirSync(dir, { recursive: true });
          const { ctx, page, errors } = await newPage(browser, { lang, theme, width });
          const snap = async (name) => {
            const checks = await page.evaluate(pageChecks);
            for (const c of checks) report.findings.push({ config: cfg, screen: name, ...c });
            await page.screenshot({ path: join(dir, `${name}.png`), fullPage: false });
            sheet.push({ cfg, name });
            shots++;
          };
          const items = page.locator(NAV);
          const count = await items.count();
          if (!navNames) {
            navNames = [];
            for (let i = 0; i < count; i++) {
              navNames.push(`${String(i + 1).padStart(2, '0')}-${slug((await items.nth(i).innerText()) || `item${i}`)}`);
            }
          }
          for (let i = 0; i < Math.min(count, navNames.length); i++) {
            const name = navNames[i];
            await page.locator(NAV).nth(i).click();
            await settle(page);
            await snap(name);

            // dialogs opened through documented hooks, on every page of the section
            const seenDialogs = new Set();
            const dialogs = async (reopen) => {
              const ids = [];
              const dlg = page.locator('[data-testid^="open-dialog"]');
              for (let d = 0; d < Math.min(await dlg.count(), 8); d++) ids.push(await dlg.nth(d).getAttribute('data-testid'));
              for (const id of ids) {
                if (seenDialogs.has(id)) continue;
                seenDialogs.add(id);
                const btn = page.locator(`[data-testid="${id}"]`).first();
                if (!(await btn.isVisible().catch(() => false)) || !(await btn.isEnabled().catch(() => false))) continue;
                await btn.click().catch(() => {});
                await settle(page);
                await snap(`${name}--dialog-${slug(id.replace('open-dialog-', '')) || seenDialogs.size}`);
                await page.keyboard.press('Escape');
                await settle(page);
                await reopen();
              }
            };
            await dialogs(async () => {
              await page.locator(NAV).nth(i).click();
              await settle(page);
            });

            // sub-navigation (settings groups, tabs); below 1024 px Settings has a Select instead
            const subs = page.locator(SUB);
            const sn = Math.min(await subs.count(), 30);
            const picker = page.locator('[data-testid="settings-select"] button[role=combobox], [data-testid="settings-select"] button').first();
            const narrowSettings = sn > 0 && !(await subs.first().isVisible()) && (await picker.isVisible().catch(() => false));
            for (let j = 0; j < sn; j++) {
              const openSub = async () => {
                await page.locator(NAV).nth(i).click();
                await settle(page);
                if (narrowSettings) {
                  await page.locator('[data-testid="settings-select"] button').first().click();
                  await page.locator('[data-testid="settings-select"] [role=option]').nth(j).click();
                  await settle(page);
                  return true;
                }
                const el = page.locator(SUB).nth(j);
                if (!(await el.isVisible())) return false;
                await el.click().catch(() => {});
                await settle(page);
                return true;
              };
              if (!(await openSub())) continue;
              await snap(`${name}--${String(j + 1).padStart(2, '0')}`);
              await dialogs(openSub);
            }
          }

          /** A running session hides the shell (v1.2): leave it with Esc, else reload. */
          const leaveSession = async () => {
            const shown = () => page.locator(NAV).first().isVisible().catch(() => false);
            if (await shown()) return;
            await page.keyboard.press('Escape');
            await settle(page);
            if (await shown()) return;
            await page.reload();
            await page.waitForSelector(NAV, { timeout: 20000 });
            await settle(page);
          };

          /** The end of a running mode: the summary / result / finished board. */
          const finish = async (mode) => {
            if (mode === 'cards') {
              // answer a few cards ("know it"), then end: the session summary
              for (let k = 0; k < 3; k++) {
                if (await page.locator('[data-testid="flip"]').count()) await page.keyboard.press('Space');
                await page.waitForTimeout(80);
                await page.keyboard.press('2');
                await page.waitForTimeout(120);
              }
              const end = page.locator('[data-testid="study-end"]').first();
              if (await end.count()) {
                await end.click().catch(() => {});
                await settle(page);
                await snap('state-cards-summary');
              }
            } else if (mode === 'test') {
              // pick the first option of every question, type the written ones, submit
              await page.evaluate(() => {
                for (const q of document.querySelectorAll('[data-testid^="question-"]')) {
                  q.querySelector('[data-testid^="option-"]')?.click();
                }
              });
              for (const input of await page.locator('[data-testid^="question-"] input').all()) await input.fill('—').catch(() => {});
              const submit = page.locator('[data-testid="test-submit"]').first();
              if (await submit.count()) {
                await submit.click().catch(() => {});
                await settle(page);
                await page.evaluate(() => document.querySelector('main')?.scrollTo(0, 0));
                await snap('state-test-result');
              }
            } else if (mode === 'match') {
              // solve the board: try every definition for each term (wrong pairs only count as mistakes)
              await page.evaluate(async () => {
                const wait = (ms) => new Promise((r) => setTimeout(r, ms));
                const tiles = (side) => [...document.querySelectorAll(`[data-testid="match-tile"][data-side="${side}"]:not([disabled])`)];
                for (let guard = 0; guard < 80 && tiles('term').length; guard++) {
                  const term = tiles('term')[0];
                  const text = term.textContent;
                  for (const def of tiles('definition')) {
                    const cur = tiles('term').find((x) => x.textContent === text);
                    if (!cur) break;
                    cur.click();
                    await wait(40);
                    def.click();
                    await wait(90);
                  }
                  if (!document.querySelector('[data-testid="match-board"]')) break;
                }
              });
              await settle(page);
              if (await page.locator('[data-testid="match-result"]').count()) await snap('state-match-finished');
            }
          };

          /** States outside the deck page: Library tree, Search info sheet and bulk bar, scenarios. */
          const extraStates = async () => {
            await leaveSession();
            // Library as a tree (the toggle must switch the layout)
            await page.locator(NAV).nth(0).click();
            await settle(page);
            await page.locator(NAV).nth(1).click();
            await settle(page);
            const firstTab = page.locator('main [role=tab]').first();
            if (await firstTab.count()) {
              await firstTab.click().catch(() => {});
              await settle(page);
            }
            const back = page.locator('main .fc-page-header .back').first();
            if (await back.count()) {
              await back.click().catch(() => {});
              await settle(page);
            }
            const tree = page.locator('.view-toggle [role=radio]').nth(1);
            if (await tree.count()) {
              await tree.click();
              await settle(page);
              await snap('state-library-tree');
              await page.locator('.view-toggle [role=radio]').nth(0).click();
              await settle(page);
            }
            // Search: card info sheet, then the bulk bar
            const search = navNames.findIndex((n) => /^04-/.test(n));
            if (search >= 0) {
              await page.locator(NAV).nth(search).click();
              await settle(page);
              const row = page.locator('[data-testid="browse-row"]').first();
              if (await row.count()) {
                await row.click();
                await settle(page);
                await snap('state-search-card-info');
                await page.keyboard.press('Escape');
                await settle(page);
              }
              const checks = page.locator('[data-testid="browse-select"]');
              for (let k = 0; k < Math.min(3, await checks.count()); k++) {
                await checks.nth(k).locator('input, [role=checkbox]').first().click({ force: true }).catch(() => {});
                await page.waitForTimeout(60);
              }
              await settle(page);
              if ((await checks.count()) > 0) await snap('state-search-bulk');
            }
          };

          // key states: deck page, then each study mode from its tile (idle, running, flipped)
          if (count > 0) {
            const openDeck = async () => {
              await leaveSession();
              // via Home, so the Library screen mounts afresh (its deck list, not the last deck page)
              await page.locator(NAV).nth(0).click();
              await settle(page);
              await page.locator(NAV).nth(1).click();
              await settle(page);
              // the section reopens its last tab: go to the first one (the deck list)
              const firstTab = page.locator('main [role=tab]').first();
              if (await firstTab.count()) {
                await firstTab.click().catch(() => {});
                await settle(page);
              }
              const deck = page.locator('[data-testid="deck-row"], .tree button.name, button.name').first();
              // the Library reopens the selected deck's page
              if (await page.locator('[data-testid^="deck-mode-"]').count()) return true;
              if (!(await deck.count())) return false;
              await deck.click().catch(() => {});
              await settle(page);
              return true;
            };
            if (await openDeck()) {
              await snap('state-deck-page');
              const terms = page.locator('[data-testid="term-list"]').first();
              if (await terms.count()) {
                await terms.scrollIntoViewIfNeeded().catch(() => {});
                await page.evaluate(() => document.querySelector('[data-testid="term-list"]')?.scrollIntoView({ block: 'start' }));
                await settle(page);
                await snap('state-deck-terms');
              }
              const tiles = page.locator('[data-testid^="deck-mode-"]');
              const ids = [];
              for (let k = 0; k < (await tiles.count()); k++) ids.push(await tiles.nth(k).getAttribute('data-testid'));
              for (const id of ids) {
                const mode = id.replace('deck-mode-', '');
                if (!(await openDeck())) break;
                await page.locator(`[data-testid="${id}"]`).click().catch(() => {});
                await settle(page);
                await snap(`state-${mode}`);
                const start = page.locator('[data-testid="study-start"]').first();
                if (!(await start.count())) continue;
                await start.click().catch(() => {});
                await settle(page);
                await snap(`state-${mode}-active`);
                const flip = page.locator('[data-testid="flip"]').first();
                if (await flip.count()) {
                  await flip.click().catch(() => {});
                  await settle(page);
                  await snap(`state-${mode}-flipped`);
                }
                await finish(mode);
              }
            }
            await extraStates();
          }
          // mock scenarios: first-run guide, lock screen, update notice
          for (const [scenario, shot] of [['fresh', 'state-onboarding'], ['locked', 'state-lock'], ['update', 'state-update']]) {
            const sp = await newPage(browser, { lang, theme, width }, `?mock=${scenario}`);
            const old = page;
            await settle(sp.page);
            await sp.page.waitForTimeout(400);
            const take = async (n) => {
              const checks = await sp.page.evaluate(pageChecks);
              for (const c of checks) report.findings.push({ config: cfg, screen: n, ...c });
              await sp.page.screenshot({ path: join(dir, `${n}.png`), fullPage: false });
              sheet.push({ cfg, name: n });
              shots++;
            };
            await take(shot);
            if (scenario === 'update') {
              const openBtn = sp.page.locator('[data-testid="update-open"]').first();
              if (await openBtn.count()) {
                await openBtn.click();
                await settle(sp.page);
                await take('state-update-screen');
              }
            }
            errors.push(...sp.errors);
            void old;
            await sp.ctx.close();
          }
          report.configs.push({ config: cfg, pageErrors: [...new Set(errors)].slice(0, 10) });
          for (const e of new Set(errors)) report.findings.push({ config: cfg, screen: '*', check: 'page-error', el: '', detail: e });
          await ctx.close();
        }
      }
    }
  } finally {
    await browser.close();
    server?.kill();
  }

  if (!flag('no-static')) report.static = staticRawControls();

  const byCheck = {};
  for (const f of report.findings) byCheck[f.check] = (byCheck[f.check] ?? 0) + 1;
  if (report.static.length) byCheck['raw-control-static'] = report.static.length;
  report.totals = { screenshots: shots, findings: report.findings.length + report.static.length, byCheck };
  writeFileSync(join(outDir, 'report.json'), JSON.stringify(report, null, 2));

  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
  const groups = [...new Set(sheet.map((s) => s.cfg))];
  writeFileSync(
    join(outDir, 'index.html'),
    `<!doctype html><meta charset=utf-8><title>Screens</title><style>
body{font:14px system-ui;margin:16px;background:#f4f4f5;color:#111}h2{margin:24px 0 8px}
.g{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:12px}
figure{margin:0;background:#fff;border:1px solid #ddd;padding:6px}img{width:100%;display:block}
figcaption{font-size:12px;padding-top:4px}</style><h1>Screens (${shots})</h1>
<p>Findings: ${esc(JSON.stringify(report.totals.byCheck))}</p>${groups
      .map(
        (g) =>
          `<h2>${esc(g)}</h2><div class=g>${sheet
            .filter((s) => s.cfg === g)
            .map((s) => `<figure><a href="${g}/${s.name}.png"><img loading=lazy src="${g}/${s.name}.png"></a><figcaption>${esc(s.name)}</figcaption></figure>`)
            .join('')}</div>`,
      )
      .join('')}`,
  );

  console.log(`Screens: ${shots} PNGs in ${relative(process.cwd(), outDir) || outDir}`);
  console.log(`Findings: ${report.totals.findings}`, byCheck);
  console.log(`Report: ${join(outDir, 'report.json')}  Contact sheet: ${join(outDir, 'index.html')}`);
  if (strict && report.totals.findings > 0) process.exit(1);
}

main().catch((e) => {
  console.error(e);
  process.exit(2);
});
