import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import LockHost from './LockHost.svelte';
import ProfilesScreen from './ProfilesScreen.svelte';
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

describe('profiles and app lock (dev mock)', () => {
  it('adds a profile, sets a PIN, locks, refuses a wrong PIN and unlocks', async () => {
    const onunlock = vi.fn();
    render(LockHost, { onunlock });
    render(ProfilesScreen, { ctx });
    expect(await screen.findByText('Main profile', {}, { timeout: 5000 })).toBeTruthy();

    await fireEvent.input(screen.getByLabelText('New profile name'), { target: { value: 'Work' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add profile' }));
    expect(await screen.findByText('Work')).toBeTruthy();

    expect(screen.getByText('No lock is set for this profile.')).toBeTruthy();
    const secretInputs = screen.getAllByLabelText('New PIN/password');
    const input = secretInputs.find((e) => e.tagName === 'INPUT')!;
    await fireEvent.input(input, { target: { value: '4821' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Set lock' }));
    expect(await screen.findByText('Locked with a PIN.')).toBeTruthy();
    expect(screen.getByText(/salted Argon2id hash/)).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Lock now' }));
    expect(await screen.findByRole('heading', { name: 'Flashcard is locked' })).toBeTruthy();
    // Another profile can be opened from the lock screen.
    expect(await screen.findByRole('button', { name: 'Work' })).toBeTruthy();

    const pin = screen.getByLabelText('Enter your PIN or password');
    await fireEvent.input(pin, { target: { value: '0000' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Unlock' }));
    expect(await screen.findByText('Wrong PIN or password.')).toBeTruthy();
    await fireEvent.input(pin, { target: { value: '4821' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Unlock' }));
    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Flashcard is locked' })).toBeNull());
    expect(onunlock).toHaveBeenCalled();
    expect((await api.call('lock.status', {})).locked).toBe(false);
  });
});
