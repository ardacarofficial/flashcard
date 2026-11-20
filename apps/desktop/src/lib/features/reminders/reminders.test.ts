import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import RemindersScreen from './RemindersScreen.svelte';
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

const setting = async (key: string) =>
  (await api.call('settings.get', { scope: { kind: 'app' }, keys: [key] })).values[0]?.value;

describe('reminders screen (dev mock)', () => {
  it('turns the daily reminder on, picks a time and days, and sends a test', async () => {
    render(RemindersScreen, { ctx });
    expect(await screen.findByText('Today: 12 due, 20 new.')).toBeTruthy();
    await fireEvent.click(screen.getByLabelText('Remind me every day'));
    expect(await setting('reminders.enabled')).toBe(true);
    const time = screen.getByLabelText('Time') as HTMLInputElement;
    expect(time.value).toBe('19:00');
    await fireEvent.change(time, { target: { value: '07:30' } });
    expect(await setting('reminders.minute_of_day')).toBe(7 * 60 + 30);
    await fireEvent.click(screen.getByLabelText('Sun'));
    expect(await setting('reminders.weekdays')).toBe(0b011_1111);
    await fireEvent.click(screen.getByLabelText('Closing the window keeps the app in the tray'));
    expect(await setting('reminders.keep_in_tray')).toBe(true);

    const due: unknown[] = [];
    const off = await api.on('reminder_due', (e) => due.push(e.event));
    await fireEvent.click(screen.getByRole('button', { name: 'Send a test notification' }));
    expect(await screen.findByText('Test reminder sent.')).toBeTruthy();
    expect(due).toEqual([{ type: 'reminder_due', data: { counts: { due: 12, new: 20 }, test: true } }]);
    off();
  });
});
