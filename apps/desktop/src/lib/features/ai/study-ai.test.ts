import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import AiScreen from './AiScreen.svelte';
import StudyAi from './StudyAi.svelte';
import { previewTask, sendPreviewed } from './ai';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx = (selectedDeck: string | null): ScreenCtx => ({
  decks: [],
  selectedDeck,
  selectedPath: null,
  selectDeck: () => {},
  guarded: async (fn) => {
    try {
      await fn();
      return true;
    } catch {
      return false;
    }
  },
});

async function claudeWithKey() {
  const p = await api.call('ai.providers.create', { kind: 'anthropic', name: null, base_url: null, default_model: null, is_default: null });
  await api.call('ai.providers.set_key', { provider_id: p.id, key: 'sk-test' });
  return p;
}

describe('AI on the study screen', () => {
  it('is hidden while no provider is set up', async () => {
    render(StudyAi, { cardId: 'card-1', revealed: false, typed: null });
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByRole('button', { name: 'AI hint' })).toBeNull();
  });

  it('previews a hint, then shows it as it streams in', async () => {
    await claudeWithKey();
    const task = { feature: 'explain', card_id: 'card-1', question: null } as const;
    const pieces: string[] = [];
    const done = await sendPreviewed(task, await previewTask(task), undefined, undefined, (t) => pieces.push(t));
    expect(done.status).toBe('succeeded');
    // Events are the whole text so far: pieces that came before `ai.run` returned collapse to
    // the latest one (the mock streams them all before returning).
    expect(pieces).toEqual(['An explanation.']);

    const { rerender } = render(StudyAi, { cardId: 'card-1', revealed: false, typed: 'sugar' });
    await fireEvent.click(await screen.findByRole('button', { name: 'AI hint' }));
    // Nothing is sent before the preview is confirmed.
    expect(await screen.findByText('Before sending')).toBeTruthy();
    expect(screen.queryByTestId('ai-text')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    expect((await screen.findByTestId('ai-text')).textContent).toBe('A hint.');

    // After the answer: explain and (with a typed answer) an AI check.
    await rerender({ cardId: 'card-1', revealed: true, typed: 'sugar' });
    expect(screen.queryByRole('button', { name: 'AI hint' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Check with AI' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect(await screen.findByText('Correct: Same meaning.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Explain' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Send' }));
    expect((await screen.findByTestId('ai-text')).textContent).toBe('An explanation.');

    // A new card starts clean.
    await rerender({ cardId: 'card-2', revealed: false, typed: null });
    expect(screen.queryByTestId('ai-text')).toBeNull();
  });

  it('is hidden when the ai module is off', async () => {
    const real = api.modules.list;
    const spy = vi.spyOn(api.modules, 'list').mockImplementation(async () => {
      const r = await real();
      return { modules: r.modules.map((m) => (m.id === 'ai' ? { ...m, enabled: false } : m)) };
    });
    render(StudyAi, { cardId: 'card-1', revealed: true, typed: 'x' });
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByRole('button', { name: 'Explain' })).toBeNull();
    spy.mockRestore();
  });
});

describe('AI screen: sources and providers', () => {
  it('fills the source from a web page or PDF pages', async () => {
    const deck = (await api.deck.tree()).roots[0]!.deck.id;
    render(AiScreen, { ctx: ctx(deck) });
    await fireEvent.click(await screen.findByRole('radio', { name: 'Web page' }));
    await fireEvent.input(screen.getByLabelText('Web page address'), { target: { value: 'https://example.org/a' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Get text' }));
    const source = (await screen.findByLabelText('Source')) as HTMLTextAreaElement;
    await vi.waitFor(() => expect(source.value).toBe('Readable text of https://example.org/a.'));

    const prompt = vi.spyOn(window, 'prompt').mockReturnValue('/tmp/bio.pdf');
    await fireEvent.click(screen.getByRole('radio', { name: 'PDF' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Choose PDF…' }));
    expect(await screen.findByText('Pages 1–3 of 3. Edit the text below as you like.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('From page'), { target: { value: '2' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Read pages' }));
    await vi.waitFor(() => expect(source.value).toBe('Text of pages 2–3 of /tmp/bio.pdf.'));
    prompt.mockRestore();
  });

  it('asks before a key follows a provider to a new address, and warns about session keys', async () => {
    const real = api.call.bind(api);
    const spy = vi.spyOn(api, 'call').mockImplementation((async (method: string, params: unknown) => {
      const r = await (real as (m: string, p: unknown) => Promise<unknown>)(method, params);
      return method === 'ai.providers.list' ? { ...(r as object), keychain_session_only: true } : r;
    }) as typeof api.call);
    render(AiScreen, { ctx: ctx(null) });
    await fireEvent.click(screen.getByRole('tab', { name: 'Providers' }));
    expect(await screen.findByText(/only for this session/)).toBeTruthy();
    spy.mockRestore();

    const address = (await screen.findAllByLabelText('Address'))[0] as HTMLInputElement;
    expect(screen.getAllByText('An API key is stored in the system keychain.').length).toBeGreaterThan(0);
    await fireEvent.change(address, { target: { value: 'https://proxy.example.org' } });
    expect((await screen.findByRole('alert')).textContent).toContain('Send it to the new address too?');
    await fireEvent.click(screen.getByRole('button', { name: 'Change and remove the key' }));
    await vi.waitFor(async () => {
      const p = (await api.call('ai.providers.list', {})).providers[0]!;
      expect([p.base_url, p.has_key]).toEqual(['https://proxy.example.org', false]);
    });
  });
});
