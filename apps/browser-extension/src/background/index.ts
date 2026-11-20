/**
 * MV3 service worker: context menu + shortcut capture the selection, the popup collects
 * front/back/deck/classifications, and everything that talks to the app goes through `Service`.
 */
import { capturePage, toCapture, type PageSelection, type StoredCapture } from '../lib/context';
import { t } from '../lib/i18n';
import { KEYS, chromeKv } from '../lib/kv';
import type { Request } from '../lib/messages';
import { handle } from '../lib/router';
import { Service } from '../lib/service';

const MENU_ID = 'fc-add-selection';
const FLUSH_ALARM = 'fc-flush';

const service = new Service({
  local: chromeKv(chrome.storage.local),
  now: () => Date.now(),
  uuid: () => crypto.randomUUID(),
  clientName: t('pair_client_name'),
});
const session = chromeKv(chrome.storage.session);

async function refreshBadge(): Promise<void> {
  const { waiting, failed } = await service.outbox.counts();
  const n = waiting + failed;
  await chrome.action.setBadgeText({ text: n > 0 ? String(n) : '' });
  await chrome.action.setBadgeBackgroundColor({ color: failed > 0 ? '#b3261e' : '#5f6368' });
}

async function flush(): Promise<void> {
  await service.flush();
  await refreshBadge();
}

/** Reads the selection (with its sentence) from the tab; falls back to what the menu reported. */
async function captureTab(tab: chrome.tabs.Tab | undefined, fallbackText?: string, pageUrl?: string): Promise<StoredCapture | null> {
  let sel: PageSelection | undefined;
  if (tab?.id !== undefined) {
    try {
      const results = await chrome.scripting.executeScript({ target: { tabId: tab.id, allFrames: true }, func: capturePage });
      sel = results.map((r) => r.result as PageSelection | undefined).find((s) => !!s && s.text.trim() !== '');
    } catch {
      // chrome:// pages, the web store, PDFs: no script access — use the menu's data.
    }
  }
  if (!sel) {
    const text = fallbackText ?? '';
    if (text.trim() === '') return null;
    sel = { text, block: '', start: -1, end: -1, title: tab?.title ?? '', url: pageUrl ?? tab?.url ?? '' };
  }
  const capture: StoredCapture = { ...toCapture(sel), capturedAt: Date.now() };
  await session.set(KEYS.capture, capture);
  return capture;
}

async function openPopup(): Promise<void> {
  try {
    await chrome.action.openPopup();
  } catch {
    await chrome.windows.create({ url: chrome.runtime.getURL('popup.html?window=1'), type: 'popup', width: 420, height: 640 });
  }
}

async function captureAndOpen(tab: chrome.tabs.Tab | undefined, fallbackText?: string, pageUrl?: string): Promise<void> {
  await captureTab(tab, fallbackText, pageUrl);
  await openPopup();
}

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: MENU_ID, title: t('menu_add_selection'), contexts: ['selection'] });
  });
  void chrome.alarms.create(FLUSH_ALARM, { periodInMinutes: 1 });
  void refreshBadge();
});

chrome.runtime.onStartup.addListener(() => {
  void chrome.alarms.create(FLUSH_ALARM, { periodInMinutes: 1 });
  void flush();
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === FLUSH_ALARM) void flush();
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === MENU_ID) void captureAndOpen(tab, info.selectionText, info.pageUrl);
});

chrome.commands.onCommand.addListener((command, tab) => {
  if (command === 'add-selection') void captureAndOpen(tab);
});

/** Popup-only request: capture the active tab's selection when the popup opened from the toolbar. */
type LocalRequest = { type: 'capture_active' };

chrome.runtime.onMessage.addListener((msg: Request | LocalRequest, sender, sendResponse) => {
  // Only our own pages (popup/options); there are no content scripts that could relay page input.
  if (sender.id !== chrome.runtime.id || !sender.url?.startsWith(chrome.runtime.getURL(''))) return false;
  const work =
    msg.type === 'capture_active'
      ? chrome.tabs.query({ active: true, lastFocusedWindow: true }).then(([tab]) => captureTab(tab))
      : handle(service, msg);
  work
    .then(async (value) => {
      await refreshBadge();
      sendResponse(value);
    })
    .catch(() => sendResponse(undefined));
  return true;
});

// e2e hook (test builds only, `vite build --mode e2e`): lets the Playwright test run the same
// capture the context menu runs (menus cannot be clicked by automation). `fallbackText`/`pageUrl` are what the
// menu reports as `selectionText`/`pageUrl`. The service-worker scope is private to the extension.
if (__FC_E2E__) {
  (globalThis as { __fcCapture?: unknown }).__fcCapture = (tabId: number, fallbackText?: string, pageUrl?: string) =>
    chrome.tabs.get(tabId).then((tab) => captureTab(tab, fallbackText, pageUrl));
}
