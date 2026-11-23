/**
 * Helpers for journeys (see ./README.md). A journey drives the real UI with the keyboard where
 * the UI allows it and asserts the PERSISTED result through `rpc()` — the same public API and
 * dispatch the desktop app uses — never pixels.
 */
import { expect, type Page } from '@playwright/test';
import { spawn, type ChildProcess } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

export { expect, test } from '@playwright/test';

function need(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set: run journeys with \`pnpm e2e\` (e2e/run.mjs)`);
  return v;
}

/** Where things are (set by run.mjs). */
export const env = {
  /** The vite dev server in bridge mode. */
  get url() {
    return need('FC_E2E_URL');
  },
  get bridge() {
    return need('FC_BRIDGE_URL');
  },
  get token() {
    return need('FC_BRIDGE_TOKEN');
  },
  /** The temporary profile folder (write fixture files here). */
  get profile() {
    return need('FC_E2E_PROFILE');
  },
  /** The collection file. */
  get db() {
    return need('FC_E2E_DB');
  },
  /** The `fc` binary. */
  get fc() {
    return need('FC_BIN');
  },
};

/** What `seed.mjs` created. */
export interface Seed {
  decks: { english: string; subtitle: string };
  subtitle_notetype_id: string;
  english: [string, string][];
  subtitle: { lemma: string; surface: string; meaning: string; sentence: string; translation: string }[];
}

let seeded: Seed | null = null;
export function seed(): Seed {
  seeded ??= JSON.parse(readFileSync(need('FC_E2E_SEED'), 'utf8')) as Seed;
  return seeded;
}

/** The `ApiError` DTO of a refused call. */
export interface ApiErrorDto {
  code: string;
  message_key: string;
  message: string;
  details: Record<string, unknown>;
  retryable: boolean;
}

export class RpcError extends Error {
  constructor(
    readonly method: string,
    readonly api: ApiErrorDto,
  ) {
    super(`${method}: ${api.code}: ${api.message}`);
  }
}

/**
 * Calls a public method (`decks.tree` or `v1.decks.tree`) through the dev bridge, as the user —
 * the same dispatch the desktop IPC uses. Throws {@link RpcError} when the app refuses.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function rpc<T = any>(method: string, params: unknown = {}): Promise<T> {
  const res = await fetch(`${env.bridge}/rpc`, {
    method: 'POST',
    headers: { authorization: `Bearer ${env.token}`, 'content-type': 'application/json' },
    body: JSON.stringify({ method, params }),
  });
  const reply = (await res.json().catch(() => null)) as { ok?: boolean; result?: T; error?: ApiErrorDto } | null;
  if (reply?.ok === true) return reply.result as T;
  throw new RpcError(method, reply?.error ?? { code: 'INTERNAL', message_key: '', message: `HTTP ${res.status}`, details: {}, retryable: false });
}

/** The built-in Classic note type. */
export const CLASSIC_NOTETYPE_ID = '01900000-0000-7000-8000-000000000001';

/** A name no other journey (or earlier run on a kept profile) uses. */
export function uniq(prefix: string): string {
  return `${prefix} ${Date.now().toString(36)}${Math.floor(Math.random() * 1296).toString(36)}`;
}

// ------------------------------------------------------------------ i18n

const dictionaries: Record<string, unknown> = {};
const generated = join(dirname(fileURLToPath(import.meta.url)), '..', 'src', 'lib', 'i18n', 'generated');

/**
 * The UI text of `key` (`quickadd.add`) in `lang`, `{param}`s filled in: select by the words
 * the user sees without hard-coding them (wording may change, keys rarely do).
 */
export function t(key: string, params: Record<string, string | number> = {}, lang: 'tr' | 'en' = 'tr'): string {
  dictionaries[lang] ??= JSON.parse(readFileSync(join(generated, `${lang}.json`), 'utf8'));
  let node: unknown = dictionaries[lang];
  for (const part of key.split('.')) node = (node as Record<string, unknown> | undefined)?.[part];
  if (typeof node !== 'string') throw new Error(`no i18n text for ${key} (${lang})`);
  return node.replace(/\{(\w+)\}/g, (m, name: string) => (name in params ? String(params[name]) : m));
}

// ------------------------------------------------------------------ the app

/** Opens the app in `lang` and waits until the shell (sidebar) is there. */
export async function openApp(page: Page, { lang = 'tr' as 'tr' | 'en' } = {}): Promise<void> {
  await page.goto(`/?lang=${lang}`);
  await expect(page.locator('[data-testid="top-bar"]')).toBeVisible();
  // The menus come from `modules.list` (the backend): wait until Home is listed, then until
  // the first screen has loaded its data (no skeletons left) and the shell's keys are bound.
  await expect(sidebarButton(page, 'shell.section.home')).toBeVisible();
  await expect(page.locator('#fc-main')).toBeVisible();
  await expect(page.locator('#fc-main [aria-busy="true"]')).toHaveCount(0);
  await expect(page.locator('#fc-main h1, #fc-main h2').first()).toBeVisible();
}

/** A sidebar entry by its i18n label key (`shell.section.create`). */
export function sidebarButton(page: Page, labelKey: string) {
  return page.getByRole('navigation').getByRole('button', { name: t(labelKey), exact: false }).first();
}

/** Presses keys one after another on whatever has focus (`press(page, 'n')`, `'Control+Enter'`). */
export async function press(page: Page, ...keys: string[]): Promise<void> {
  for (const k of keys) await page.keyboard.press(k);
}

/** Opens a sidebar section by its i18n label key (`shell.section.create`). */
export async function goToSection(page: Page, labelKey: string): Promise<void> {
  const button = sidebarButton(page, labelKey);
  if (!(await button.isVisible())) {
    // v1.4: Library, Create and Search sit in the sidebar only with the one "Advanced" switch
    // on (Settings, bottom of the list): turn it on the way the user does, then go.
    await sidebarButton(page, 'shell.settings').click();
    const sw = page.getByTestId('advanced-switch').getByRole('switch');
    await expect(sw).toBeVisible();
    if ((await sw.getAttribute('aria-checked')) !== 'true') await sw.click();
    await expect(button).toBeVisible();
  }
  await button.click();
}

// ------------------------------------------------------------------ persisted state

export interface NoteRow {
  id: string;
  home_deck_id: string;
  notetype_id: string;
  fields: Record<string, string>;
  tags: string[];
}

/** Notes of a deck with their fields (`notes.list` then `notes.get`), read back from the collection. */
export async function notesIn(deckId: string): Promise<NoteRow[]> {
  const page = await rpc<{ items: { id: string }[] }>('notes.list', {
    deck_id: deckId,
    include_subdecks: false,
    page: { cursor: null, limit: null },
  });
  return Promise.all(page.items.map((n) => rpc<NoteRow>('notes.get', { id: n.id })));
}

interface TreeNode {
  deck: { id: string; name: string; path?: string };
  children: TreeNode[];
}

/** The id of the deck at `path` (`A::B`), or null. */
export async function deckIdByPath(path: string): Promise<string | null> {
  const tree = await rpc<{ roots: TreeNode[] }>('decks.tree', {});
  const parts = path.split('::');
  let level = tree.roots;
  let found: TreeNode | undefined;
  for (const p of parts) {
    found = level.find((n) => n.deck.name === p);
    if (!found) return null;
    level = found.children;
  }
  return found?.deck.id ?? null;
}

/** Creates a deck with Classic notes through the API (setup for a journey, not the journey). */
export async function makeDeck(name: string, pairs: [string, string][]): Promise<string> {
  const deck = await rpc<{ id: string }>('decks.create', { path: name, parent_id: null });
  for (const [front, back] of pairs) {
    await rpc('notes.create', { notetype_id: CLASSIC_NOTETYPE_ID, deck_id: deck.id, fields: { front, back }, tags: [], attributes: [] });
  }
  return deck.id;
}

/** Picks `option` in the `$lib/ui` Select (a combobox) labelled `label`. */
export async function choose(page: Page, label: string, option: string, scope = page.locator('body')): Promise<void> {
  await scope.getByRole('combobox', { name: label, exact: true }).click();
  await page.getByRole('option', { name: option, exact: true }).click();
}

/** The element that has keyboard focus, for assertions (`await expect(focused(page)).…`). */
export function focused(page: Page) {
  return page.locator('*:focus');
}

/**
 * Drops every saved form draft (`maintenance.drafts.*`: Quick Add, the Create page … keep what
 * was typed in the collection and restore it on open). Journeys share one profile: call this
 * first when a journey needs empty forms, so another journey's leftover typing can't leak in.
 */
export async function discardDrafts(): Promise<void> {
  const { items } = await rpc<{ items: { key: string }[] }>('maintenance.drafts.list', {});
  for (const d of items) await rpc('maintenance.drafts.discard', { key: d.key });
}

// ------------------------------------------------------------------ study (v1.3 journeys)

/** The client's UTC offset in minutes, as the app sends it (study-day boundary). */
export const tz = -new Date().getTimezoneOffset();

export interface SessionRow {
  id: string;
  kind: string;
  mode: string;
  deck_id: string | null;
  ended_at: number | null;
  answered: number;
  next_due: number | null;
  current: { card_id: string } | null;
}

export interface StudyOptionsRow {
  resumable: { session: SessionRow; answered: number; remaining: number }[];
  scopes: { scope: string; count: number; mode: string; empty_reason: string | null; next_available_at: number | null }[];
}

/** `study.options` for a deck (`null` = every deck). */
export function studyOptions(deckId: string | null): Promise<StudyOptionsRow> {
  return rpc<StudyOptionsRow>('study.options', { deck_id: deckId, include_subdecks: null, tz_offset_mins: tz });
}

/** Ends every unfinished session (journeys share a profile: "Continue" must not take over). */
export async function endOpenSessions(): Promise<void> {
  for (const r of (await studyOptions(null)).resumable) await rpc('study.end', { session_id: r.session.id }).catch(() => undefined);
}

export interface ReviewRow {
  id: string;
  card_id: string;
  session_id: string | null;
  rating: string;
  mode: string;
}
export interface CardSnapshot {
  id: string;
  state: string;
  due: number | null;
  stability: number | null;
  difficulty: number | null;
  reps: number;
  lapses: number;
  last_review: number | null;
  suspended: boolean;
  buried_until: number | null;
}

/** A card's scheduling fields and its review log, read back from the collection. */
export function cardHistory(cardId: string): Promise<{ card: CardSnapshot; reviews: ReviewRow[] }> {
  return rpc('study.card_history', { card_id: cardId });
}

/** The scheduling fields only (what a practice session must leave alone). */
export async function scheduling(cardId: string): Promise<Omit<CardSnapshot, 'id'>> {
  const { card } = await cardHistory(cardId);
  const { state, due, stability, difficulty, reps, lapses, last_review, suspended, buried_until } = card;
  return { state, due, stability, difficulty, reps, lapses, last_review, suspended, buried_until };
}

/** Card ids of a deck's notes. */
export async function cardsIn(deckId: string): Promise<string[]> {
  return (await notesIn(deckId)).flatMap((n) => ((n as unknown as { cards: { id: string }[] }).cards ?? []).map((c) => c.id));
}

/** The session as the core has it now. */
export function sessionNow(id: string): Promise<SessionRow> {
  return rpc<SessionRow>('study.session', { session_id: id });
}

/**
 * v1.4 launcher: "Which cards?" shows today's cards first; the other four scopes are one click
 * away ("Başka kartlar seç"). Opens them (no-op when they are already shown).
 */
export async function showAllScopes(page: Page): Promise<void> {
  const more = page.getByTestId('launcher-more-scopes');
  if ((await more.getAttribute('aria-expanded')) !== 'true') await more.click();
  await expect(more).toHaveAttribute('aria-expanded', 'true');
}

// ------------------------------------------------------------------ Subtitle captures (F1)

/** One capture as the Subtitle extension sends it (`seed.mjs` makes the same body). */
export interface CaptureSpec {
  lemma: string;
  /** As heard; must occur in `sentence` (it is marked there). */
  surface: string;
  meaning: string;
  sentence: string;
  translation?: string;
  /** Upload a short WAV first (`POST /subtitle/v1/media`, kind audio): the film's own sound. */
  audio?: boolean;
}

/** A UUIDv7 (the contract wants one per capture). */
function uuid7(): string {
  const ms = Date.now().toString(16).padStart(12, '0');
  const r = crypto.randomUUID().replace(/-/g, '');
  const variant = (8 | (parseInt(r[16]!, 16) & 3)).toString(16);
  return `${ms.slice(0, 8)}-${ms.slice(8)}-7${r.slice(13, 16)}-${variant}${r.slice(17, 20)}-${r.slice(20, 32)}`;
}

/** 0.1 s of a quiet tone, 8 kHz mono PCM: a real `audio/wav` the core accepts. */
function tinyWav(): Buffer {
  const n = 800;
  const buf = Buffer.alloc(44 + n * 2);
  buf.write('RIFF', 0);
  buf.writeUInt32LE(36 + n * 2, 4);
  buf.write('WAVE', 8);
  buf.write('fmt ', 12);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20);
  buf.writeUInt16LE(1, 22);
  buf.writeUInt32LE(8000, 24);
  buf.writeUInt32LE(16000, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write('data', 36);
  buf.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) buf.writeInt16LE(Math.round(3000 * Math.sin(i / 5)), 44 + i * 2);
  return buf;
}

/** `fc --db <db> api serve --port 0` → its address (first stderr line), as `seed.mjs` does. */
function startLocalApi(): Promise<{ child: ChildProcess; url: string }> {
  return new Promise((resolve, reject) => {
    const childEnv = { ...process.env };
    delete childEnv.FC_DB;
    const child = spawn(env.fc, ['--db', env.db, 'api', 'serve', '--port', '0'], { stdio: ['ignore', 'ignore', 'pipe'], env: childEnv });
    const timer = setTimeout(() => {
      child.kill('SIGTERM');
      reject(new Error('fc api serve did not start'));
    }, 60_000);
    child.once('exit', (code) => reject(new Error(`fc api serve exited (${code})`)));
    createInterface({ input: child.stderr! }).once('line', (line) => {
      clearTimeout(timer);
      resolve({ child, url: `http://${line.trim().split('http://').pop()}` });
    });
  });
}

/**
 * Sends `specs` to the real Subtitle endpoint (`POST /subtitle/v1/captures` on `fc api serve`,
 * with a fresh access key from `subtitle.keys.create`) into `deckId`, in order: captures of one
 * word and meaning become encounters of one note, exactly as in the app. Returns the capture ids.
 */
export async function captureSubtitle(deckId: string, specs: CaptureSpec[]): Promise<string[]> {
  await rpc('modules.set_enabled', { id: 'subtitle', enabled: true });
  const key = await rpc<{ secret: string }>('subtitle.keys.create', { name: uniq('e2e') });
  const auth = { authorization: `Bearer ${key.secret}` };
  const api = await startLocalApi();
  const ids: string[] = [];
  try {
    for (const [i, c] of specs.entries()) {
      const id = uuid7();
      const media: Record<string, unknown> = {};
      if (c.audio) {
        const up = await fetch(`${api.url}/subtitle/v1/media`, {
          method: 'POST',
          headers: { ...auth, 'content-type': 'audio/wav', 'x-capture-id': id, 'x-media-kind': 'audio' },
          body: tinyWav(),
        });
        const body = (await up.json().catch(() => null)) as { upload_id?: string } | null;
        if (!up.ok || !body?.upload_id) throw new Error(`audio upload: HTTP ${up.status} ${JSON.stringify(body)}`);
        media.audio = { upload_id: body.upload_id, mime: 'audio/wav' };
      }
      const start = c.sentence.indexOf(c.surface);
      const res = await fetch(`${api.url}/subtitle/v1/captures`, {
        method: 'POST',
        headers: { ...auth, 'content-type': 'application/json', accept: 'application/json' },
        body: JSON.stringify({
          schema: 1,
          capture_id: id,
          kind: 'word',
          lang: { target: 'en', native: 'tr' },
          term: { surface: c.surface, lemma: c.lemma, pos: 'verb' },
          definition: { text: c.meaning, source: 'dict' },
          context: { sentence: c.sentence, translation: c.translation ?? null, term_offset: start >= 0 ? [start, start + c.surface.length] : null },
          source: {
            type: 'video',
            url: 'https://www.youtube.com/watch?v=e2e-f1',
            title: 'F1 fixture',
            site: 'youtube',
            video_id: 'e2e-f1',
            t_start_ms: 30_000 + i * 2000,
            t_end_ms: 31_500 + i * 2000,
          },
          media,
          deck_id: deckId,
          tags: ['subtitle'],
          created_at: new Date().toISOString(),
          client: { name: 'subtitle-extension', ext_version: 'e2e' },
        }),
      });
      const body = (await res.json().catch(() => null)) as { status?: string } | null;
      if (!res.ok || body?.status !== 'created') throw new Error(`capture ${c.lemma}: HTTP ${res.status} ${JSON.stringify(body)}`);
      ids.push(id);
    }
  } finally {
    api.child.removeAllListeners('exit');
    api.child.kill('SIGTERM');
  }
  return ids;
}

/** The note (in `deckId`) whose Subtitle `word` field is `word`. */
export async function noteByWord(deckId: string, word: string): Promise<NoteRow> {
  const note = (await notesIn(deckId)).find((n) => n.fields.word === word);
  if (!note) throw new Error(`no note of "${word}" in deck ${deckId}`);
  return note;
}

/** Opens a note's editor the way J3/E3N do: Search, type, Enter, ↓ Enter. */
export async function openNoteFromSearch(page: Page, query: string, rowText: RegExp | string) {
  await goToSection(page, 'shell.section.search');
  const box = page.getByRole('searchbox').first();
  // Opening Search puts the cursor in the box; already there (a second note), clear it first.
  if (await box.inputValue()) await box.fill('');
  await expect(box).toBeFocused();
  await page.keyboard.type(query);
  await press(page, 'Enter');
  await expect(page.getByTestId('browse-row').first()).toContainText(rowText);
  await press(page, 'ArrowDown', 'Enter');
  const editor = page.getByRole('dialog').filter({ has: page.getByTestId('note-extras') });
  await expect(editor).toBeVisible();
  return editor;
}
