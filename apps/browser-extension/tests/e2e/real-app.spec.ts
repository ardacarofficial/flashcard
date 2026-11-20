/**
 * The same flow against the **real** local API: `fc --db <tmp> api serve` (track I, docs/06 §3)
 * instead of the mock. Skipped unless `FC_BIN` points at an `fc` binary that has `api serve`
 * (`cargo build -p fc-cli`, then `FC_BIN=../../target/debug/fc pnpm e2e`). Proves the v0.9
 * acceptance criterion end to end: the item is read back from the app with `fc call inbox.list`.
 */
import { chromium, expect, test, type BrowserContext, type Worker } from '@playwright/test';
import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const FC = process.env.FC_BIN;
const DIST = fileURLToPath(new URL('../../dist', import.meta.url));

test.skip(!FC, 'set FC_BIN to an fc binary with `api serve` to run against the real app');

let dir = '';
let db = '';
let serve: ChildProcess | undefined;
let context: BrowserContext | undefined;

function fc(...args: string[]): unknown {
  const out = execFileSync(FC ?? 'fc', ['--db', db, '--json', ...args], { encoding: 'utf8' });
  return out.trim() === '' ? null : JSON.parse(out);
}

async function freePort(): Promise<number> {
  const s = createServer();
  await new Promise<void>((r) => s.listen(0, '127.0.0.1', r));
  const port = (s.address() as AddressInfo).port;
  await new Promise<void>((r) => s.close(() => r()));
  return port;
}

test.afterEach(async () => {
  await context?.close();
  serve?.kill();
  if (dir) rmSync(dir, { recursive: true, force: true });
});

test('a word from the extension lands in the real app inbox', async () => {
  dir = mkdtempSync(join(tmpdir(), 'fc-ext-e2e-'));
  db = join(dir, 'collection.db');
  const deck = fc('call', 'decks.create', JSON.stringify({ path: 'İngilizce' })) as { id: string };
  fc('attribute', 'apply-template', 'cefr', '--deck', 'İngilizce', '--locale', 'tr');
  fc('call', 'modules.set_enabled', JSON.stringify({ id: 'local_api', enabled: true }));
  const port = await freePort();
  serve = spawn(FC ?? 'fc', ['--db', db, 'api', 'serve', '--port', String(port)], { stdio: 'inherit' });
  await expect.poll(async () => fetch(`http://127.0.0.1:${port}/v1/ping`).then((r) => r.ok, () => false), { timeout: 20_000 }).toBe(true);

  // A page with text, served on 127.0.0.1 (in the extension's host permissions).
  const page = createServer((_req, res) => {
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    res.end('<!doctype html><title>Badgers of Europe</title><p id="p">Badgers live in setts. The badger dug a deep hole under the old oak.</p>');
  });
  await new Promise<void>((r) => page.listen(0, '127.0.0.1', r));
  const pagePort = (page.address() as AddressInfo).port;

  context = await chromium.launchPersistentContext('', {
    channel: 'chromium',
    headless: true,
    args: [`--disable-extensions-except=${DIST}`, `--load-extension=${DIST}`],
  });
  const sw: Worker = context.serviceWorkers()[0] ?? (await context.waitForEvent('serviceworker'));
  await expect.poll(() => sw.evaluate(() => typeof chrome.storage !== 'undefined')).toBe(true);
  const id = new URL(sw.url()).host;
  await sw.evaluate((p) => chrome.storage.local.set({ 'fc.settings': { port: p } }), port);

  const tab = await context.newPage();
  await tab.goto(`http://127.0.0.1:${pagePort}/`);
  await tab.evaluate(() => {
    const node = document.getElementById('p')?.firstChild;
    const text = node?.textContent ?? '';
    const range = document.createRange();
    if (!node) throw new Error('no text');
    range.setStart(node, text.indexOf('badger'));
    range.setEnd(node, text.indexOf('badger') + 'badger'.length);
    window.getSelection()?.addRange(range);
  });
  await sw.evaluate(async (u) => {
    const [t] = await chrome.tabs.query({ url: u });
    const capture = (globalThis as unknown as { __fcCapture: (id: number) => Promise<unknown> }).__fcCapture;
    if (t?.id !== undefined) await capture(t.id);
  }, tab.url());

  const popup = await context.newPage();
  await popup.goto(`chrome-extension://${id}/popup.html`);
  await popup.click('#pair-start');
  // The user approves in the app; headless: the same command the app's dialog runs.
  let clientId = '';
  await expect
    .poll(() => {
      const list = fc('call', 'local_api.clients.list', '{}') as { clients: { id: string; status: string }[] };
      clientId = list.clients.find((c) => c.status === 'pending')?.id ?? '';
      return clientId;
    })
    .not.toBe('');
  fc('call', 'local_api.clients.approve', JSON.stringify({ id: clientId, scopes: null }));
  await expect(popup.locator('#view-add')).toBeVisible({ timeout: 10_000 });

  await expect(popup.locator('#front')).toHaveValue('badger');
  await popup.selectOption('#deck', { label: 'İngilizce' });
  await expect(popup.locator('#classes')).toBeVisible();
  await popup.locator('#class-fields select').first().selectOption({ label: 'B1' });
  await popup.click('#save');
  await expect(popup.locator('#add-msg')).toHaveText('Added to the inbox.');

  const inbox = fc('call', 'inbox.list', '{}') as { items: { source: unknown; status: string; item: Record<string, unknown> }[] };
  expect(inbox.items).toHaveLength(1);
  const got = inbox.items[0];
  expect(got?.status).toBe('pending');
  expect(got?.source).toMatchObject({ kind: 'local_api', client_id: clientId });
  expect(got?.item).toMatchObject({
    front: 'badger',
    deck_id: deck.id,
    context: { url: tab.url(), title: 'Badgers of Europe', sentence: 'The badger dug a deep hole under the old oak.' },
  });
  expect(Object.keys((got?.item.attributes as Record<string, unknown>) ?? {})).toHaveLength(1);
  // Chromium keeps its connection to the page alive: close it, or `close` waits forever.
  await new Promise<void>((r) => {
    page.close(() => r());
    page.closeAllConnections();
  });
});
