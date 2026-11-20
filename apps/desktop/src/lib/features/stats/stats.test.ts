import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
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
    expect(await screen.findByText('No leeches.')).toBeTruthy();
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

  it('formats without computing', () => {
    expect(pct(0.8734, 1)).toBe('87.3%');
    expect(pct(null)).toBe('–');
    expect(seconds(1500)).toBe('1.5');
  });
});
