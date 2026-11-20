/** "Add word" popup: pairing when needed, then front/back/deck/classifications → inbox. */
import { collect, fieldsFor, type Field } from '../lib/attributes';
import { CAPTURE_TTL_MS, type StoredCapture } from '../lib/context';
import type { AttributeValue, DeckDto } from '../lib/contract';
import { errorText } from '../lib/errors';
import { localize, t } from '../lib/i18n';
import { KEYS, chromeKv } from '../lib/kv';
import { send } from '../lib/messages';
import type { Status, SubmitResult } from '../lib/service';
import { readFields, renderFields } from './fields';

/** Last deck and, per deck, the last classification values (docs/02 §8.1 "son kullanılanı hatırla"). */
type Remembered = { deckId?: string; perDeck?: Record<string, Record<string, AttributeValue | null>> };

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const local = chromeKv(chrome.storage.local);
const session = chromeKv(chrome.storage.session);

const views = ['view-offline', 'view-pair', 'view-add'] as const;
function show(view: (typeof views)[number]): void {
  for (const v of views) $(v).hidden = v !== view;
}

function setConn(status: Status): void {
  const conn = $('conn');
  conn.textContent = t(status.app === 'online' ? 'conn_online' : 'conn_offline');
  conn.dataset.state = status.app;
}

let fields: Field[] = [];
let capture: StoredCapture | undefined;
let pollTimer: number | undefined;

async function loadCapture(): Promise<StoredCapture | undefined> {
  let c = await session.get<StoredCapture>(KEYS.capture);
  if (!c || Date.now() - c.capturedAt > CAPTURE_TTL_MS) {
    // Opened from the toolbar: take the current selection, if any.
    c = (await chrome.runtime.sendMessage({ type: 'capture_active' })) as StoredCapture | undefined;
  }
  return c ?? undefined;
}

async function remembered(): Promise<Remembered> {
  return (await local.get<Remembered>(KEYS.remembered)) ?? {};
}

async function loadClassifications(deckId: string): Promise<void> {
  const box = $('classes');
  if (deckId === '') {
    fields = [];
    box.hidden = true;
    return;
  }
  const [{ list, cached }, mem] = await Promise.all([send({ type: 'classifications', deckId }), remembered()]);
  fields = fieldsFor(list, mem.perDeck?.[deckId] ?? {});
  renderFields($('class-fields'), fields);
  box.hidden = fields.length === 0;
  if (cached) $('cached-note').hidden = false;
}

function fillDecks(decks: DeckDto[], selected: string | undefined): void {
  const sel = $<HTMLSelectElement>('deck');
  const keep = sel.options[0];
  sel.replaceChildren(...(keep ? [keep] : []));
  for (const d of decks) sel.append(Object.assign(document.createElement('option'), { value: d.id, textContent: d.path }));
  sel.value = selected && decks.some((d) => d.id === selected) ? selected : '';
}

async function showAdd(): Promise<void> {
  show('view-add');
  capture = await loadCapture();
  $<HTMLTextAreaElement>('front').value = capture?.front ?? '';
  if (capture && (capture.sentence || capture.url)) {
    $('ctx').hidden = false;
    $('ctx-sentence').textContent = capture.sentence;
    $('ctx-sentence').hidden = capture.sentence === '';
    $('ctx-source').textContent = [capture.title, capture.url].filter((s) => s !== '').join(' · ');
  }
  const [{ decks, cached }, mem] = await Promise.all([send({ type: 'decks' }), remembered()]);
  $('cached-note').hidden = !cached;
  fillDecks(decks, mem.deckId);
  await loadClassifications($<HTMLSelectElement>('deck').value);
  $<HTMLTextAreaElement>(capture?.front ? 'back' : 'front').focus();
}

function message(id: string, text: string, kind: 'ok' | 'error' | 'info' = 'info'): void {
  const p = $(id);
  p.textContent = text;
  p.dataset.kind = kind;
}

function resultText(r: SubmitResult): [string, 'ok' | 'error' | 'info'] {
  switch (r.status) {
    case 'sent':
      return [t('added_to_inbox'), 'ok'];
    case 'queued':
      return [t(r.reason === 'unpaired' ? 'queued_unpaired' : r.reason === 'busy' ? 'queued_busy' : 'queued_offline'), 'info'];
    case 'rejected':
      return [errorText(r.error), 'error'];
    case 'full':
      return [t('queue_full'), 'error'];
  }
}

async function onSubmit(ev: Event): Promise<void> {
  ev.preventDefault();
  const front = $<HTMLTextAreaElement>('front').value.trim();
  const back = $<HTMLTextAreaElement>('back').value.trim();
  if (front === '') {
    message('add-msg', t('front_required'), 'error');
    $('front').focus();
    return;
  }
  const deckId = $<HTMLSelectElement>('deck').value;
  const got = collect(fields, readFields($('class-fields'), fields));
  if (got.invalid.length > 0) {
    message('add-msg', t('class_invalid', [got.invalid.join(', ')]), 'error');
    return;
  }
  if (got.missing.length > 0) {
    message('add-msg', t('class_required', [got.missing.join(', ')]), 'error');
    return;
  }
  const withContext = $<HTMLInputElement>('ctx-on').checked && capture !== undefined;
  const save = $<HTMLButtonElement>('save');
  save.disabled = true;
  const result = await send({
    type: 'submit',
    item: {
      front,
      fields: {},
      tags: [],
      attributes: got.attributes,
      ...(back !== '' ? { back } : {}),
      ...(deckId !== '' ? { deck_id: deckId } : {}),
      ...(withContext && capture
        ? { context: { url: capture.url || null, title: capture.title || null, sentence: capture.sentence || null } }
        : {}),
    },
  });
  const [text, kind] = resultText(result);
  message('add-msg', text, kind);
  if (result.status === 'sent' || result.status === 'queued') {
    const mem = await remembered();
    const perDeck = { ...(mem.perDeck ?? {}) };
    if (deckId !== '') perDeck[deckId] = got.remember;
    await local.set<Remembered>(KEYS.remembered, { deckId, perDeck });
    await session.remove(KEYS.capture);
    setTimeout(() => window.close(), 1200);
  } else {
    save.disabled = false;
  }
}

function stopPolling(): void {
  if (pollTimer !== undefined) clearInterval(pollTimer);
  pollTimer = undefined;
}

function startPolling(): void {
  stopPolling();
  $('pair-wait').hidden = false;
  $<HTMLButtonElement>('pair-start').disabled = true;
  pollTimer = window.setInterval(() => {
    void send({ type: 'pair_poll' }).then(async (r) => {
      if (!r.ok) return; // app briefly unreachable: keep waiting until the request expires
      if (r.value === 'pending') return;
      stopPolling();
      $('pair-wait').hidden = true;
      $<HTMLButtonElement>('pair-start').disabled = false;
      if (r.value === 'paired') {
        void send({ type: 'flush' });
        await showAdd();
      } else {
        message('pair-msg', t(r.value === 'denied' ? 'pair_denied' : 'pair_expired'), 'error');
      }
    });
  }, 1500);
}

async function onPairStart(): Promise<void> {
  message('pair-msg', '');
  const r = await send({ type: 'pair_start' });
  if (r.ok) {
    startPolling();
    return;
  }
  const f = r.failure;
  message('pair-msg', f.kind === 'api' ? errorText(f.error) : t(f.kind === 'rate_limited' ? 'pair_rate_limited' : 'offline_intro'), 'error');
}

async function render(): Promise<void> {
  const status = await send({ type: 'status' });
  setConn(status);
  if (status.paired) {
    if (status.app === 'online' && status.outbox.waiting > 0) void send({ type: 'flush' });
    await showAdd();
    return;
  }
  if (status.app === 'offline') {
    show('view-offline');
    return;
  }
  show('view-pair');
  if (status.last === 'revoked') message('pair-msg', t('pair_revoked'), 'error');
  if (status.pairing === 'pending') startPolling();
}

localize(document);
$('open-options').addEventListener('click', () => void chrome.runtime.openOptionsPage());
$('retry').addEventListener('click', () => void render());
$('pair-start').addEventListener('click', () => void onPairStart());
$('deck').addEventListener('change', (e) => void loadClassifications((e.target as HTMLSelectElement).value));
$('view-add').addEventListener('submit', (e) => void onSubmit(e));
void render();
