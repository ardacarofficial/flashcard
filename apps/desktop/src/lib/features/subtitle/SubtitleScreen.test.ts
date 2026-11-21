import { beforeAll, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
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
    // The real module is off by default (the mock starts it on, with seeded keys).
    await api.call('modules.set_enabled', { id: 'subtitle', enabled: true });
  });

  it('shows the address, the setup steps, the keys and what arrived', async () => {
    render(SubtitleScreen, { ctx });
    expect((await screen.findByTestId('subtitle-address')).textContent).toBe('http://127.0.0.1:28735/subtitle');
    expect(screen.getByText('Create an access key below and copy it.')).toBeTruthy();
    expect(screen.getByText(/Bağlantıyı test et/)).toBeTruthy();
    // Seeded: one key in use, one revoked (no revoke action on it).
    const rows = await screen.findAllByTestId('subtitle-key');
    expect(rows).toHaveLength(2);
    expect(within(rows[0]!).getByText('Chrome · ev')).toBeTruthy();
    expect(within(rows[0]!).getByRole('button', { name: 'Revoke “Chrome · ev”' })).toBeTruthy();
    expect(within(rows[1]!).getByText('Revoked')).toBeTruthy();
    expect(within(rows[1]!).queryByRole('button')).toBeNull();
    const received = screen.getByTestId('subtitle-received').textContent ?? '';
    expect(received).toContain('Words received');
    expect(received).toContain('12');
  });

  it('says the module is off and turns it on', async () => {
    await api.call('modules.set_enabled', { id: 'subtitle', enabled: false });
    render(SubtitleScreen, { ctx });
    expect(await screen.findByText('The Subtitle extension is off')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Turn on' }));
    expect((await screen.findByTestId('subtitle-address')).textContent).toContain('/subtitle');
    expect(screen.queryByText('The Subtitle extension is off')).toBeNull();
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
    await screen.findAllByTestId('subtitle-key');
    await fireEvent.input(screen.getByLabelText('Key name'), { target: { value: 'Chrome' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Create key' }));
    const secret = (await screen.findByTestId('subtitle-secret')).textContent ?? '';
    expect(secret.startsWith('sk_fc_')).toBe(true);
    expect(screen.getByText(/won’t see this key again/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(screen.queryByTestId('subtitle-secret')).toBeNull());
    // The list shows the name and the prefix only, never the key.
    expect(await screen.findByText('Chrome')).toBeTruthy();
    expect(document.body.textContent).not.toContain(secret);
    await fireEvent.click(screen.getByRole('button', { name: 'Revoke “Chrome”' }));
    const row = () => screen.getByText('Chrome').closest('li')!;
    await waitFor(() => expect(within(row()).getByText('Revoked')).toBeTruthy());
    const [k] = (await api.call('subtitle.keys.list', {})).keys;
    expect(k!.name).toBe('Chrome');
    expect(k!.revoked_at).not.toBeNull();
  });
});
