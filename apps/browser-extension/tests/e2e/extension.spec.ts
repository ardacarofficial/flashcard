/**
 * End-to-end: the unpacked extension in (headless) Chromium, a page with text, and a mock of the
 * desktop app's local API (docs/06 §3) on 127.0.0.1. Proves the v0.9 acceptance criterion
 * "a word added from the browser extension lands in the inbox", with deck + classification + context,
 * plus pairing, queueing while the app is closed, and the Turkish UI.
 */
import { chromium, expect, test, type BrowserContext, type Page, type Worker } from '@playwright/test';
import { fileURLToPath } from 'node:url';
import { MockApi, sampleData } from '../mock-api/handler';
import { startMockApi, type Running } from '../mock-api/server';

const DIST = fileURLToPath(new URL('../../dist', import.meta.url));

type Ext = { context: BrowserContext; sw: Worker; id: string };

async function launch(lang = 'en'): Promise<Ext> {
  const context = await chromium.launchPersistentContext('', {
    channel: 'chromium',
    headless: true,
    locale: lang,
    args: [`--disable-extensions-except=${DIST}`, `--load-extension=${DIST}`, `--lang=${lang}`],
  });
  const sw = context.serviceWorkers()[0] ?? (await context.waitForEvent('serviceworker'));
  const id = new URL(sw.url()).host;
  // The worker is reported before its extension APIs are bound.
  await expect.poll(() => sw.evaluate(() => typeof chrome.storage !== 'undefined')).toBe(true);
  return { context, sw, id };
}

async function usePort(ext: Ext, port: number): Promise<void> {
  await ext.sw.evaluate((p) => chrome.storage.local.set({ 'fc.settings': { port: p } }), port);
}

/** Selects `word` inside `#p` and runs the context-menu capture on that tab. */
async function selectAndCapture(ext: Ext, page: Page, word: string, menuText?: string, menuUrl?: string): Promise<void> {
  await page.evaluate((w) => {
    const node = document.getElementById('p')?.firstChild;
    if (!node?.textContent) throw new Error('no text');
    const start = node.textContent.indexOf(w);
    const range = document.createRange();
    range.setStart(node, start);
    range.setEnd(node, start + w.length);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
  }, word);
  const url = page.url();
  await ext.sw.evaluate(
    async ([u, text, pageUrl]) => {
      // `tab.url` is hidden without host permission, so a URL query finds nothing there: use the active tab.
      const [tab] = (await chrome.tabs.query({ url: u })).concat(await chrome.tabs.query({ active: true, lastFocusedWindow: true }));
      if (tab?.id === undefined) throw new Error('tab not found');
      const capture = (globalThis as unknown as { __fcCapture: (id: number, text?: string, pageUrl?: string) => Promise<unknown> }).__fcCapture;
      await capture(tab.id, text, pageUrl);
    },
    [url, menuText, menuUrl] as const,
  );
}

async function openPopup(ext: Ext): Promise<Page> {
  const popup = await ext.context.newPage();
  await popup.goto(`chrome-extension://${ext.id}/popup.html`);
  return popup;
}

let server: Running;
let ext: Ext;

test.beforeEach(async () => {
  const api = new MockApi();
  sampleData(api);
  server = await startMockApi(api);
});

test.afterEach(async () => {
  await ext?.context.close();
  await server.close();
});

test('a word selected on a page lands in the inbox with deck, classification and context', async () => {
  ext = await launch();
  await usePort(ext, server.port);
  const page = await ext.context.newPage();
  await page.goto(`http://127.0.0.1:${server.port}/page.html`);
  await selectAndCapture(ext, page, 'badger');

  const popup = await openPopup(ext);
  // First use: pair with the app (the mock approves on the first poll, like a user clicking "Allow").
  await expect(popup.locator('#view-pair')).toBeVisible();
  await popup.click('#pair-start');
  await expect(popup.locator('#view-add')).toBeVisible();
  await expect(popup.locator('#conn')).toHaveText('Connected');

  await expect(popup.locator('#front')).toHaveValue('badger');
  await expect(popup.locator('#ctx-sentence')).toHaveText('The badger dug a deep hole under the old oak.');
  await popup.fill('#back', 'porsuk');
  await popup.selectOption('#deck', { label: 'İngilizce' });
  // The deck's classifications come from the local API (attributes.effective).
  await expect(popup.locator('#classes')).toBeVisible();
  await popup.selectOption('#attr-attr-seviye', { label: 'B1' });
  await popup.click('#save');
  await expect(popup.locator('#add-msg')).toHaveText('Added to the inbox.');

  const { inbox } = server.api;
  expect(inbox).toHaveLength(1);
  const item = inbox[0]?.item;
  expect(item).toMatchObject({
    front: 'badger',
    back: 'porsuk',
    deck_id: 'd-en',
    attributes: { 'attr-seviye': 'opt-B1' },
    context: { url: page.url(), title: 'Badgers of Europe', sentence: 'The badger dug a deep hole under the old oak.' },
  });
  expect(item?.client_ref).toMatch(/^[0-9a-f-]{36}$/);
  expect(Object.keys(item ?? {})).not.toContain('source');
  // Requests carry the extension's own origin (Chromium omits Origin on service-worker GETs).
  const authed = server.api.log.filter((r) => r.headers.authorization);
  expect(authed.length).toBeGreaterThan(0);
  for (const r of authed) expect([undefined, `chrome-extension://${ext.id}`]).toContain(r.headers.origin);
  const posts = server.api.log.filter((r) => r.method === 'POST');
  for (const r of posts) expect(r.headers.origin).toBe(`chrome-extension://${ext.id}`);

  // Next time the deck and level are remembered.
  await selectAndCapture(ext, page, 'oak');
  const again = await openPopup(ext);
  await expect(again.locator('#front')).toHaveValue('oak');
  await expect(again.locator('#deck')).toHaveValue('d-en');
  await expect(again.locator('#attr-attr-seviye')).toHaveValue('opt-B1');
});

test('while the app is closed the word is queued and delivered once it opens', async () => {
  ext = await launch();
  await usePort(ext, server.port);
  const page = await ext.context.newPage();
  await page.goto(`http://127.0.0.1:${server.port}/page.html`);

  const popup = await openPopup(ext);
  await popup.click('#pair-start');
  await expect(popup.locator('#view-add')).toBeVisible();
  await popup.selectOption('#deck', { label: 'İngilizce' }); // caches decks + classifications
  await expect(popup.locator('#classes')).toBeVisible();
  await popup.close();

  await selectAndCapture(ext, page, 'setts');
  const { api, port } = server;
  await server.close(); // app closed

  const offline = await openPopup(ext);
  await expect(offline.locator('#conn')).toHaveText('App closed');
  await expect(offline.locator('#front')).toHaveValue('setts');
  await expect(offline.locator('#cached-note')).toBeVisible();
  await offline.selectOption('#deck', { label: 'İngilizce' }); // from the cache
  await offline.selectOption('#attr-attr-seviye', { label: 'C1' });
  await offline.click('#save');
  await expect(offline.locator('#add-msg')).toHaveText('The app is closed. Saved; it will be sent when the app opens.');
  expect(api.inbox).toHaveLength(0);

  server = await startMockApi(api, port); // app opened again
  const options = await ext.context.newPage();
  await options.goto(`chrome-extension://${ext.id}/options.html`);
  await expect(options.locator('#outbox-state')).toHaveText('1 waiting, 0 rejected by the app.');
  await options.click('#flush');
  await expect(options.locator('#outbox-state')).toHaveText('0 waiting, 0 rejected by the app.');
  expect(api.inbox.map((i) => i.item.front)).toEqual(['setts']);
  expect(api.inbox[0]?.item.attributes).toEqual({ 'attr-seviye': 'opt-C1' });
});

test('on an origin outside host_permissions the page is not scripted: the menu text is used, without a sentence', async () => {
  // https://example.test is not in host_permissions (only http://127.0.0.1/*). A real toolbar click or
  // context-menu click would grant `activeTab` for that tab; automation cannot produce that user
  // gesture, so what is provable here is the other half: with no grant, `executeScript` is denied
  // and the capture degrades to the context menu's `selectionText` + page URL/title.
  ext = await launch();
  await usePort(ext, server.port);
  const page = await ext.context.newPage();
  await page.route('https://example.test/page.html', (route) =>
    route.fulfill({
      contentType: 'text/html; charset=utf-8',
      body: '<!doctype html><title>Badgers of Europe</title><article><p id="p">Badgers live in setts. The badger dug a deep hole under the old oak.</p></article>',
    }),
  );
  await page.goto('https://example.test/page.html');
  await page.bringToFront();
  await selectAndCapture(ext, page, 'badger', 'badger', 'https://example.test/page.html');

  const stored = await ext.sw.evaluate(async () => (await chrome.storage.session.get(null)) as Record<string, unknown>);
  const capture = Object.values(stored).find((v) => (v as { front?: string }).front === 'badger') as
    | { front: string; sentence?: string; url: string; title: string }
    | undefined;
  expect(capture).toBeDefined();
  expect(capture?.url).toBe('https://example.test/page.html');
  expect(capture?.title).toBe(''); // the tab's title is hidden too without a grant
  // Nothing was read from the page (no host permission, no activeTab grant): no sentence.
  expect(capture?.sentence ?? '').toBe('');
});

test('the UI is Turkish when the browser is', async () => {
  ext = await launch('tr');
  await usePort(ext, server.port);
  const popup = await openPopup(ext);
  await expect(popup.locator('#pair-start')).toHaveText('Bağlan');
  await popup.click('#pair-start');
  await expect(popup.locator('#save')).toHaveText('Gelen kutusuna ekle');
  await expect(popup.locator('#deck option').first()).toHaveText('Gelen kutusunda seç');
});
