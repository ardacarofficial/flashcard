import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import ProvidersPanel from './ProvidersPanel.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx: ScreenCtx = {
  decks: [],
  selectedDeck: null,
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
};

const providers = async () => (await api.call('ai.providers.list', {})).providers;

async function openGallery() {
  render(ProvidersPanel, { ctx });
  await fireEvent.click(await screen.findByRole('button', { name: 'Add provider' }, { timeout: 5000 }));
  return screen.findByRole('dialog');
}

describe('<ProvidersPanel> provider setup on the dev mock', () => {
  it('says what AI is for and that it is off until set up', async () => {
    render(ProvidersPanel, { ctx });
    const lead = await screen.findByTestId('ai-what-for');
    expect(lead.textContent).toContain('optional');
    expect(lead.textContent).toContain('nothing is sent');
  });

  it('adds OpenRouter in three steps: key → model → test', async () => {
    const dialog = await openGallery();
    // The gallery names OpenRouter (v1.2 only offered protocol kinds, Claude preselected).
    expect(within(dialog).getByTestId('ai-preset-openrouter').textContent).toContain('some are free');
    expect(within(dialog).getByTestId('ai-preset-ollama').textContent).toContain('local · free');
    await fireEvent.click(within(dialog).getByTestId('ai-preset-openrouter'));

    // Step 1: the key field is there and required; the page to get a key is linked.
    expect(within(dialog).getByText('Paste the key')).toBeTruthy();
    const link = within(dialog).getByRole('link', { name: /openrouter\.ai\/keys/ });
    expect(link.getAttribute('href')).toBe('https://openrouter.ai/keys');
    const next = within(dialog).getByRole('button', { name: 'Next' }) as HTMLButtonElement;
    expect(next.disabled).toBe(true);
    await fireEvent.input(within(dialog).getByLabelText('API key'), { target: { value: 'sk-or-v1-abc' } });
    expect(next.disabled).toBe(false);
    await fireEvent.click(next);

    // Step 2: the list fetched from the provider, searchable; Enter picks the match.
    const search = await within(dialog).findByRole('combobox', { name: 'Model' });
    expect(await within(dialog).findByText('meta-llama/llama-3.3-70b-instruct:free')).toBeTruthy();
    await fireEvent.input(search, { target: { value: 'llama free' } });
    await fireEvent.keyDown(search, { key: 'Enter' });
    expect(within(dialog).getByText('chosen: meta-llama/llama-3.3-70b-instruct:free')).toBeTruthy();
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Next' }));

    // Step 3: test, read the verdict inline, done.
    await fireEvent.click(await within(dialog).findByRole('button', { name: 'Test connection' }));
    expect((await within(dialog).findByTestId('ai-test-result')).textContent).toContain('The connection works.');
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Done' }));

    // Persisted: a key-needing compatible provider with its key and the chosen model.
    const [p] = await providers();
    expect(p).toMatchObject({
      name: 'OpenRouter',
      preset: 'openrouter',
      kind: 'openai_compatible',
      needs_key: true,
      has_key: true,
      default_model: 'meta-llama/llama-3.3-70b-instruct:free',
      status: 'ready',
    });
    const row = await screen.findByTestId('ai-provider-row');
    expect(row.textContent).toContain('ready');
  });

  it('types a model that is not in the list (free-text fallback)', async () => {
    const dialog = await openGallery();
    await fireEvent.click(within(dialog).getByTestId('ai-preset-ollama'));
    // Local: no key needed, the field is optional and Next is enabled right away.
    expect(within(dialog).getByLabelText('API key (optional)')).toBeTruthy();
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    const search = await within(dialog).findByRole('combobox', { name: 'Model' });
    await fireEvent.input(search, { target: { value: 'my-own-model' } });
    const typed = within(dialog).getByRole('option', { name: 'Use “my-own-model”' });
    await fireEvent.click(within(typed).getByRole('button'));
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    await waitFor(async () => {
      const p = (await providers()).find((x) => x.preset === 'ollama');
      expect(p?.default_model).toBe('my-own-model');
    });
  });

  it('explains a rejected key in plain words', async () => {
    const dialog = await openGallery();
    await fireEvent.click(within(dialog).getByTestId('ai-preset-groq'));
    await fireEvent.input(within(dialog).getByLabelText('API key'), { target: { value: 'gsk-wrong-key' } });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    const search = await within(dialog).findByRole('combobox', { name: 'Model' });
    await fireEvent.input(search, { target: { value: 'llama-3.1-8b-instant' } });
    await fireEvent.keyDown(search, { key: 'Enter' });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    await fireEvent.click(await within(dialog).findByRole('button', { name: 'Test connection' }));
    const result = await within(dialog).findByTestId('ai-test-result');
    expect(result.dataset.outcome).toBe('wrong_key');
    expect(result.textContent).toContain('The key was rejected');
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Done' }));
    const p = (await providers()).find((x) => x.preset === 'groq');
    expect(p?.status).toBe('test_failed');
  });

  it('leaves nothing behind when cancelled before a model is chosen', async () => {
    const before = (await providers()).length;
    const dialog = await openGallery();
    await fireEvent.click(within(dialog).getByTestId('ai-preset-gemini'));
    await fireEvent.input(within(dialog).getByLabelText('API key'), { target: { value: 'AIza-test' } });
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Next' }));
    await within(dialog).findByRole('combobox', { name: 'Model' });
    expect((await providers()).length).toBe(before + 1);
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Close' }));
    await waitFor(async () => expect((await providers()).length).toBe(before));
  });

  it('shows the key field for an existing OpenRouter provider without a key', async () => {
    // Made the v1.2 way (kind + address): it must still ask for a key.
    await api.call('ai.providers.create', {
      kind: 'openai_compatible',
      name: 'My OpenRouter',
      base_url: 'https://openrouter.ai/api/v1',
      default_model: null,
      is_default: null,
    });
    render(ProvidersPanel, { ctx });
    const row = (await screen.findAllByTestId('ai-provider-row')).find((r) => r.textContent?.includes('My OpenRouter'))!;
    expect(row.dataset.status).toBe('needs_key');
    expect(within(row).getByText('needs a key')).toBeTruthy();
    expect(within(row).getByLabelText('API key')).toBeTruthy();
    expect(within(row).getByRole('link', { name: /openrouter\.ai\/keys/ })).toBeTruthy();
  });
});
