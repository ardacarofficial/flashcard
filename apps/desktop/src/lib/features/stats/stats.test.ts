import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import type { ScreenCtx } from '../types';
import GoalsScreen from './GoalsScreen.svelte';
import PlanningScreen from './PlanningScreen.svelte';
import StatsScreen from './StatsScreen.svelte';
import { pct, seconds } from './format';

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

describe('v0.6 screens on the dev mock', () => {
  it('statistics show the numbers the backend computed', async () => {
    render(StatsScreen, { ctx });
    // Streak, true retention and its target come straight from `stats.overview`.
    expect(await screen.findByText('4 days')).toBeTruthy();
    expect(screen.getByText('Longest: 12 days')).toBeTruthy();
    expect(screen.getByText('88.6%')).toBeTruthy();
    expect(screen.getByText('Target 90%')).toBeTruthy();
    expect(screen.getByRole('img', { name: 'Forecast' })).toBeTruthy();
    // Leech actions cover every leech of the scope: one Rust command, no ids from here.
    expect(await screen.findByText('60 cards forgotten at least 8 times')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Suspend' }));
    expect(await screen.findByText('60 changed, 0 already done')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Suspend' }));
    expect(await screen.findByText('0 changed, 60 already done')).toBeTruthy();
  });

  it('planning: exam dates and easy days', async () => {
    render(PlanningScreen, { ctx });
    expect(await screen.findByText('No exam dates.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'YKS' } });
    await fireEvent.input(screen.getByLabelText('Date'), { target: { value: '2027-06-20' } });
    await fireEvent.input(screen.getByLabelText('Cards (query)'), { target: { value: 'exam:TYT' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add exam' }));
    expect(await screen.findByText('YKS')).toBeTruthy();
    expect((await api.call('planning.exams.list', { tz_offset_mins: 0 })).items[0]!.exam.query).toBe('exam:TYT');
    // Easy days are settings of the planning module.
    await fireEvent.change(screen.getByLabelText('Sunday'), { target: { value: 'minimum' } });
    const s = await api.call('settings.get', { scope: { kind: 'app' }, keys: ['planning.easy_days.sun'] });
    expect(s.values[0]!.value).toBe('minimum');
    // Optimizer and workload (optimizer module on in the mock).
    expect(await screen.findByText('Desired retention and workload')).toBeTruthy();
    expect(await screen.findByText(/90%\s*\(current\)/)).toBeTruthy();
  });

  it('goals: set a goal and see the pace forecast', async () => {
    render(GoalsScreen, { ctx });
    expect(await screen.findByText('No goals yet.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Target'), { target: { value: '500' } });
    await fireEvent.input(screen.getByLabelText('Deadline'), { target: { value: '2026-12-31' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Set goal' }));
    expect(await screen.findByText("At this pace you'll be 3 days late")).toBeTruthy();
    expect(screen.getByText('30 / 500')).toBeTruthy();
  });

  it.each([
    ['statistics', StatsScreen, 'stats.overview'],
    ['planning', PlanningScreen, 'planning.exams.list'],
    ['goals', GoalsScreen, 'goals.list'],
  ] as const)('%s refetch on data_changed (Claude or the CLI wrote from another process)', async (_, Screen, method) => {
    const on = vi.spyOn(api, 'on');
    const call = vi.spyOn(api, 'call');
    render(Screen, { ctx });
    await waitFor(() => expect(call.mock.calls.some(([m]) => m === method)).toBe(true));
    const subscribed = on.mock.calls.find(([types]) => (types as string[]).includes('data_changed'));
    expect(subscribed).toBeTruthy();
    const before = call.mock.calls.filter(([m]) => m === method).length;
    subscribed![1]({ seq: 1, at: 0, batch_id: null, event: { type: 'data_changed' } } as never);
    await waitFor(() => expect(call.mock.calls.filter(([m]) => m === method).length).toBeGreaterThan(before));
    on.mockRestore();
    call.mockRestore();
  });

  it('formats without computing', () => {
    expect(pct(0.8734, 1)).toBe('87.3%');
    expect(pct(null)).toBe('–');
    expect(seconds(1500)).toBe('1.5');
  });
});
