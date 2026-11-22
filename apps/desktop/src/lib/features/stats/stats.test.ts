import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
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
    // v1.2: the figure and its unit are separate (big mono number, small unit).
    expect((await screen.findByTestId('stat-streak', {}, { timeout: 5000 })).textContent).toMatch(/4\s*days/);
    expect(screen.getByText('Longest: 12 days')).toBeTruthy();
    expect(screen.getByText('88.6%')).toBeTruthy();
    expect(screen.getAllByText('Target 90%').length).toBeGreaterThan(0);
    // v1.3: the forecast is the "what's ahead?" chart (7 days by default).
    expect(screen.getByRole('img', { name: 'Reviews in the next 7 days' })).toBeTruthy();
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
    const sunday = screen.getByRole('radiogroup', { name: 'Sunday' });
    await fireEvent.click(within(sunday).getByRole('radio', { name: 'Minimum' }));
    const s = await api.call('settings.get', { scope: { kind: 'app' }, keys: ['planning.easy_days.sun'] });
    expect(s.values[0]!.value).toBe('minimum');
    // Optimizer and workload (optimizer module on in the mock): v1.3 keeps them under "Advanced".
    await fireEvent.click(screen.getByRole('button', { name: 'Show advanced settings' }));
    expect(await screen.findByText('Desired retention and workload')).toBeTruthy();
    expect(await screen.findByText(/90%\s*\(current\)/)).toBeTruthy();
  });

  it('goals: set a goal and see the pace forecast', async () => {
    render(GoalsScreen, { ctx });
    expect(await screen.findByText('No goals yet.')).toBeTruthy();
    // v1.3: a goal is a sentence with blanks; the deadline lives under "More options".
    await fireEvent.input(screen.getByLabelText('Number'), { target: { value: '500' } });
    await fireEvent.click(screen.getByRole('button', { name: 'More options' }));
    await fireEvent.input(screen.getByLabelText('Deadline'), { target: { value: '2026-12-31' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Set goal' }));
    expect(await screen.findByText("At this pace you'll be 3 days late")).toBeTruthy();
    expect(screen.getByText('30 / 500')).toBeTruthy();
  });

  it('goals: "Study {n} cards every day" creates a daily cards goal', async () => {
    const call = vi.spyOn(api, 'call');
    render(GoalsScreen, { ctx });
    await screen.findByTestId('goal-form');
    await fireEvent.input(screen.getByLabelText('Number'), { target: { value: '50' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Set goal' }));
    await waitFor(() => expect(call.mock.calls.some(([m]) => m === 'goals.create')).toBe(true));
    const req = call.mock.calls.find(([m]) => m === 'goals.create')![1] as { kind: string; target: number; deadline: string | null };
    expect(req).toMatchObject({ kind: 'daily_cards', target: 50, deadline: null });
    expect(await screen.findByText('Study 50 cards every day')).toBeTruthy();
    call.mockRestore();
  });

  it('overview answers three questions; "Study these" practises exactly the listed words', async () => {
    // The study mock's own card set does not matter here: the session it returns has a card.
    const real = api.call.bind(api);
    const call = vi.spyOn(api, 'call').mockImplementation(((m: string, b: unknown) =>
      m === 'study.start' ? Promise.resolve({ id: 'session-1', current: { card_id: 'card-struggle-0' } }) : real(m as never, b as never)) as never);
    const navigate = vi.fn();
    render(StatsScreen, { ctx: { ...ctx, navigate } });
    // 1 · today, with the verdict Rust computed.
    expect(await screen.findByText('Same pace as yesterday: 45 cards then, 42 today.', {}, { timeout: 5000 })).toBeTruthy();
    // 2 · what's ahead: the busiest day is named in words (not only by colour).
    expect((await screen.findByTestId('ahead-peak')).textContent).toMatch(/Busiest day: .* · \d+ cards/);
    // 3 · what am I forgetting: dense list, primary action first.
    const panel = await screen.findByTestId('stats-struggles');
    expect(await within(panel).findByText('went')).toBeTruthy();
    expect(within(panel).getByText('gitmek (geçmiş)')).toBeTruthy();
    const study = within(panel).getByRole('button', { name: 'Study these · 6' });
    await fireEvent.click(study);
    await waitFor(() => expect(navigate).toHaveBeenCalledWith('/study'));
    const start = call.mock.calls.find(([m]) => m === 'study.start')![1] as {
      kind: string;
      mode: string;
      filter: { query: string; due_only: boolean };
    };
    expect(start.kind).toBe('custom');
    expect(start.mode).toBe('trial');
    expect(start.filter.due_only).toBe(false);
    expect(start.filter.query).toBe(`cid:${Array.from({ length: 6 }, (_, i) => `card-struggle-${i}`).join(',')}`);
    // Suspend / tag stay as secondary row actions.
    expect(within(panel).getByRole('button', { name: 'More actions for went' })).toBeTruthy();
    // "Today" narrows the window (a new Rust query).
    await fireEvent.click(within(panel).getByRole('radio', { name: 'Today' }));
    await waitFor(() => expect(call.mock.calls.some(([m, b]) => m === 'stats.struggles' && (b as { days: number }).days === 1)).toBe(true));
    expect(await within(panel).findByRole('button', { name: 'Study these · 2' })).toBeTruthy();
    call.mockRestore();
  });

  it('planning: the pace drives a live forecast and Apply names what it changes', async () => {
    const call = vi.spyOn(api, 'call');
    const decks = [{ deck: { id: 'deck-1', path: 'İngilizce' }, counts: { new: 0, learning: 0, due: 0, total: 0 }, children: [] }] as unknown as ScreenCtx['decks'];
    render(PlanningScreen, { ctx: { ...ctx, decks, selectedDeck: 'deck-1' } });
    const sentence = await screen.findByTestId('pace-sentence');
    expect(sentence.textContent).toMatch(/At this pace, in 3 weeks: ~\d+ cards and ~\d+ min a day\./);
    const before = sentence.textContent;
    await fireEvent.input(screen.getByLabelText('New cards a day?'), { target: { value: '40' } });
    await waitFor(() =>
      expect(call.mock.calls.some(([m, b]) => m === 'planning.forecast' && (b as { new_per_day: number }).new_per_day === 40)).toBe(true),
    );
    await waitFor(() => expect(screen.getByTestId('pace-sentence').textContent).not.toBe(before));
    const apply = screen.getByTestId('pace-apply');
    expect(within(apply).getByText(/This deck has no preset: applying creates preset “İngilizce” with 40 new cards a day\./)).toBeTruthy();
    expect(within(apply).getByText('Decks affected: İngilizce')).toBeTruthy();
    await fireEvent.click(within(apply).getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(call.mock.calls.some(([m]) => m === 'planning.apply_pace')).toBe(true));
    expect(call.mock.calls.find(([m]) => m === 'planning.apply_pace')![1]).toMatchObject({ deck_id: 'deck-1', new_per_day: 40, exam_date: null });
    expect(await within(apply).findByText('Applied. Ctrl+Z undoes it.')).toBeTruthy();
    call.mockRestore();
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
