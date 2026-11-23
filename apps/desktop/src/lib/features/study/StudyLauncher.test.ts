/**
 * v1.3 study launcher on the dev mock (which implements CONTRACT §1 `study.options` and the
 * scoped `study.start`): preselection, empty scopes with their reason, "Continue" first and
 * focused, Enter starts, an empty start keeps the dialog open and the old session alive.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api, CLASSIC_NOTETYPE_ID } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import type { ScreenCtx } from '../types';
import LauncherHost from './LauncherHost.svelte';
// The host loads the dialog lazily; load it up front so a cold transform is not a timeout.
import './StudyLauncher.svelte';
import { launcher, openLauncher, takeHandoff, type LaunchRequest } from './launcher.svelte';
import { tzOffsetMins } from './format';

i18n.locale = 'en';

afterEach(() => {
  cleanup();
  try {
    localStorage.clear();
  } catch {
    /* no storage */
  }
});

function ctx(navigate = vi.fn()): ScreenCtx {
  return {
    decks: [],
    selectedDeck: null,
    selectedPath: null,
    selectDeck: () => {},
    guarded: async (fn) => {
      await fn();
      return true;
    },
    navigate,
  };
}

async function deckWith(name: string, words: [string, string][]) {
  const deck = await api.call('decks.create', { path: name, parent_id: null });
  for (const [front, back] of words) {
    await api.call('notes.create', { notetype_id: CLASSIC_NOTETYPE_ID, deck_id: deck.id, fields: { front, back }, tags: [], attributes: [] });
  }
  return deck;
}

/** Mounts the host and opens the launcher once it is registered. */
async function open(req: LaunchRequest, navigate = vi.fn()) {
  render(LauncherHost, { ctx: ctx(navigate) });
  await waitFor(() => expect(launcher.available).toBe(true));
  openLauncher(req);
  await screen.findByTestId('study-launcher', {}, { timeout: 5_000 });
  await waitFor(() => expect(screen.getByTestId('launcher-start')).toBeTruthy());
  return navigate;
}

const startButton = () => screen.getByTestId('launcher-start') as HTMLButtonElement;
const scopeButton = (s: string) => screen.getByTestId(`launcher-scope-${s}`).querySelector('button')!;

describe('study launcher', () => {
  it('preselects the asked mode and shows exact counts; Enter starts it', async () => {
    const deck = await deckWith('Launch A', [
      ['dog', 'köpek'],
      ['cat', 'kedi'],
      ['bird', 'kuş'],
    ]);
    const navigate = await open({ deckId: deck.id, kind: 'test' });
    // v1.4: the paper (`test`) is called "Exam"; "Test" is the four-option quiz.
    expect(screen.getByRole('radio', { name: 'Exam' }).getAttribute('aria-checked')).toBe('true');
    // v1.4: one line under the mode says what it does; the footer says what it does to the schedule.
    expect(screen.getByTestId('launcher-mode-desc').textContent).toMatch(/A paper of mixed questions/);
    expect(screen.getByTestId('launcher-mode-test').textContent).toMatch(/practice/);
    expect(screen.getByTestId('launcher-schedule').textContent).toMatch(/Practice: your schedule stays the same/);
    // Test defaults to every card; the counts are the backend's.
    expect(scopeButton('all').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('launcher-scope-all').textContent).toMatch(/3/);
    expect(screen.getByTestId('launcher-scope-due').textContent).toMatch(/3/);
    // Focus sits on the chosen mode; Enter starts.
    await waitFor(() => expect(document.activeElement?.getAttribute('aria-checked')).toBe('true'));
    await fireEvent.keyDown(document.activeElement!, { key: 'Enter' });
    await waitFor(() => expect(navigate).toHaveBeenCalledWith('/test'));
    const h = takeHandoff('test');
    expect(h?.session.kind).toBe('test');
    expect(h?.scope).toBe('all');
    expect(screen.queryByTestId('study-launcher')).toBeNull();
  });

  it('an empty scope is disabled and says why; practice scopes are tagged', async () => {
    const deck = await deckWith('Launch B', [['sun', 'güneş']]);
    await open({ deckId: deck.id });
    // v1.4: today's cards first; the other four scopes are one click away.
    expect(screen.queryByTestId('launcher-scope-studied_today')).toBeNull();
    await fireEvent.click(screen.getByTestId('launcher-more-scopes'));
    const studied = scopeButton('studied_today');
    expect(studied.disabled).toBe(true);
    expect(screen.getByTestId('launcher-scope-studied_today').textContent).toMatch(/You haven’t studied today yet/);
    expect(screen.getByTestId('launcher-scope-struggled_today').textContent).toMatch(/Nothing you struggled with today/);
    expect(screen.getByTestId('launcher-scope-studied_today').textContent).toMatch(/practice · schedule unchanged/);
    expect(screen.getByTestId('launcher-scope-due').textContent).not.toMatch(/practice/);
    // Cards: today's queue, and the button says what will happen.
    expect(scopeButton('due').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('launcher-summary').textContent).toMatch(/Cards · Today’s cards · 1 card/);
    expect(screen.getByTestId('launcher-schedule').textContent).toMatch(/Changes your schedule/);
    expect(screen.getByTestId('launcher-mode-normal').textContent).toMatch(/changes schedule/);
  });

  it('after a session, "struggled today" holds the missed cards and starts as practice', async () => {
    const deck = await deckWith('Launch C', [
      ['red', 'kırmızı'],
      ['blue', 'mavi'],
    ]);
    const s = await api.call('study.start', { kind: 'normal', deck_id: deck.id, tz_offset_mins: tzOffsetMins() });
    const first = s.current!;
    const next = await api.call('study.answer', { session_id: s.id, card_id: first.card_id, rating: 'again', duration_ms: 1, reveal_ms: null });
    await api.call('study.answer', { session_id: s.id, card_id: next.current!.card_id, rating: 'good', duration_ms: 1, reveal_ms: null });
    await api.call('study.end', { session_id: s.id });

    const navigate = await open({ deckId: deck.id, scope: 'struggled_today' });
    expect(scopeButton('struggled_today').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByTestId('launcher-scope-struggled_today').textContent).toMatch(/1/);
    expect(screen.getByTestId('launcher-scope-studied_today').textContent).toMatch(/2/);
    await fireEvent.click(startButton());
    await waitFor(() => expect(navigate).toHaveBeenCalledWith('/study'));
    const h = takeHandoff('normal');
    expect(h?.session.mode).toBe('trial');
    expect(h?.session.current?.card_id).toBe(first.card_id);
  });

  it('puts "Continue" first and focused when a session is unfinished', async () => {
    const deck = await deckWith('Launch D', [
      ['one', 'bir'],
      ['two', 'iki'],
    ]);
    const s = await api.call('study.start', { kind: 'normal', deck_id: deck.id, tz_offset_mins: tzOffsetMins() });
    await api.call('study.answer', { session_id: s.id, card_id: s.current!.card_id, rating: 'good', duration_ms: 1, reveal_ms: null });
    const navigate = await open({ deckId: deck.id });
    const resume = await screen.findByTestId('launcher-resume');
    expect(resume.textContent).toMatch(/Continue/);
    expect(resume.textContent).toMatch(/Launch D · Cards/);
    expect(resume.textContent).toMatch(/1 answered · 1 left/);
    await waitFor(() => expect(document.activeElement).toBe(resume));
    await fireEvent.click(resume);
    await waitFor(() => expect(navigate).toHaveBeenCalledWith('/study'));
    expect(takeHandoff('normal')?.session.id).toBe(s.id);
  });

  it('nothing to study at Start: stays open with the reason, the open session survives', async () => {
    const deck = await deckWith('Launch E', [['last', 'son']]);
    const other = await deckWith('Launch E2', [['keep', 'tut']]);
    await open({ deckId: deck.id });
    expect(startButton().disabled).toBe(false);
    // Meanwhile the only card is studied elsewhere (scheduled), and another session is open.
    const s = await api.call('study.start', { kind: 'normal', deck_id: deck.id, tz_offset_mins: 0 });
    await api.call('study.answer', { session_id: s.id, card_id: s.current!.card_id, rating: 'good', duration_ms: 1, reveal_ms: null });
    await api.call('study.end', { session_id: s.id });
    const keep = await api.call('study.start', { kind: 'normal', deck_id: other.id, tz_offset_mins: 0 });

    await fireEvent.click(startButton());
    expect((await screen.findByTestId('launcher-notice')).textContent).toMatch(/Can’t start right now: Nothing is due today/);
    expect(screen.getByTestId('study-launcher')).toBeTruthy();
    expect((await api.call('study.session', { session_id: keep.id })).ended_at).toBeNull();
    // The refreshed counts disable the scope.
    await waitFor(() => expect(scopeButton('due').disabled).toBe(true));
  });
});
