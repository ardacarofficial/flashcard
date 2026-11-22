import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { resetDefaultsCache } from '../core/kit/defaults.svelte';
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
    expect(await screen.findByText('Today: 12 due, 20 new.', {}, { timeout: 5000 })).toBeTruthy();
    await fireEvent.click(screen.getByRole('switch', { name: 'Remind me every day' }));
    expect(await setting('reminders.enabled')).toBe(true);
    const [hour, minute, period] = screen.getAllByRole('combobox', { name: 'Time' });
    expect(hour!.textContent).toContain('7')
    expect(period!.textContent).toContain('PM');
    await fireEvent.click(minute!);
    await fireEvent.click(await screen.findByRole('option', { name: '30' }));
    expect(await setting('reminders.minute_of_day')).toBe(19 * 60 + 30);
    await fireEvent.click(screen.getByRole('button', { name: 'Sun' }));
    expect(await setting('reminders.weekdays')).toBe(0b011_1111);
    await fireEvent.click(screen.getByRole('switch', { name: 'Closing the window keeps the app in the tray' }));
    expect(await setting('reminders.keep_in_tray')).toBe(true);

    const due: unknown[] = [];
    const off = await api.on('reminder_due', (e) => due.push(e.event));
    await fireEvent.click(screen.getByRole('button', { name: 'Send a test notification' }));
    expect(await screen.findByText('Test reminder sent.')).toBeTruthy();
    expect(due).toEqual([{ type: 'reminder_due', data: { counts: { due: 12, new: 20 }, test: true } }]);
    off();
  });

  it('marks a changed setting and puts its default back with settings.reset', async () => {
    resetDefaultsCache();
    const real = api.call.bind(api);
    const reset: unknown[] = [];
    const spy = vi.spyOn(api, 'call').mockImplementation(((method: string, params: never) => {
      if (method === 'settings.schema') {
        const def = (key: string, d: unknown) => ({
          module: 'reminders',
          def: { key, kind: { type: 'bool' }, default: d, label_key: `settings.${key}.label`, description_key: null, advanced: false, scopes: ['app'] },
        });
        return Promise.resolve({ settings: [def('reminders.enabled', false), def('reminders.keep_in_tray', false)] });
      }
      if (method === 'settings.reset') {
        reset.push(params);
        return Promise.resolve({ key: 'reminders.keep_in_tray', value: false, source: null });
      }
      return (real as (m: string, p: unknown) => Promise<unknown>)(method, params);
    }) as typeof api.call);
    try {
      render(RemindersScreen, { ctx });
      const sw = await screen.findByRole('switch', { name: 'Closing the window keeps the app in the tray' }, { timeout: 5000 });
      const row = sw.closest('.fc-form-row') as HTMLElement;
      await waitFor(() => expect(within(row).getByText('default')).toBeTruthy());
      await fireEvent.click(sw);
      await fireEvent.click(await within(row).findByRole('button', { name: 'Reset to default' }));
      await waitFor(() => expect(reset).toEqual([{ scope: { kind: 'app' }, key: 'reminders.keep_in_tray' }]));
    } finally {
      spy.mockRestore();
      resetDefaultsCache();
    }
  });
});
