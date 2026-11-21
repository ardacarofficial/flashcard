import { beforeAll, describe, expect, it } from 'vitest';
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
