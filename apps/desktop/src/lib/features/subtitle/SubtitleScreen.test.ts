import { beforeAll, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import SubtitleScreen from './SubtitleScreen.svelte';
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

describe('<SubtitleScreen> on the dev mock', () => {
  beforeAll(async () => {
    // Off by default, like the real module.
    await api.call('modules.set_enabled', { id: 'subtitle', enabled: true });
  });

  it('shows the address and the setup steps', async () => {
    render(SubtitleScreen, { ctx });
    expect((await screen.findByTestId('subtitle-address')).textContent).toBe('http://127.0.0.1:28735/subtitle');
    expect(screen.getByText('Create an access key below and copy it.')).toBeTruthy();
    expect(screen.getByText(/Bağlantıyı test et/)).toBeTruthy();
    expect(screen.getByText('No keys yet.')).toBeTruthy();
    expect(screen.getByTestId('subtitle-received').textContent).toContain('No words received yet.');
  });

  it('says so when it cannot load, and loads on retry', async () => {
    const real = api.call.bind(api);
    let fail = true;
    const spy = vi.spyOn(api, 'call').mockImplementation(((method: string, params: unknown) =>
      fail && method === 'subtitle.status'
        ? Promise.reject(new Error('down'))
        : (real as (m: string, p: unknown) => Promise<unknown>)(method, params)) as typeof api.call);
    try {
      render(SubtitleScreen, { ctx });
      expect(await screen.findByText('Could not load the Subtitle settings.')).toBeTruthy();
      fail = false;
      await fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
      expect((await screen.findByTestId('subtitle-address')).textContent).toContain('/subtitle');
      expect(screen.queryByText('Could not load the Subtitle settings.')).toBeNull();
    } finally {
      spy.mockRestore();
    }
  });

  it('creates a key, shows it once, and revokes it', async () => {
    render(SubtitleScreen, { ctx });
    await screen.findByText('No keys yet.');
    await fireEvent.input(screen.getByLabelText('Key name'), { target: { value: 'Chrome' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Create key' }));
    const secret = (await screen.findByTestId('subtitle-secret')).textContent ?? '';
    expect(secret.startsWith('sk_fc_')).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(screen.queryByTestId('subtitle-secret')).toBeNull());
    // The list shows the name and the prefix only, never the key.
    expect(await screen.findByText('Chrome')).toBeTruthy();
    expect(document.body.textContent).not.toContain(secret);
    await fireEvent.click(screen.getByRole('button', { name: 'Revoke' }));
    expect(await screen.findByText('Revoked')).toBeTruthy();
    const [k] = (await api.call('subtitle.keys.list', {})).keys;
    expect(k!.revoked_at).not.toBeNull();
  });
});
