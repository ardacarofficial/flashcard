import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import PaceSettings from './PaceSettings.svelte';
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

/** The `planning.apply_pace` requests the page sent (the call still reaches the mock). */
function spyPace() {
  const call = api.call.bind(api);
  const sent: Record<string, unknown>[] = [];
  vi.spyOn(api, 'call').mockImplementation(((method: string, req: unknown) => {
    if (method === 'planning.apply_pace') sent.push(req as Record<string, unknown>);
    return call(method as never, req as never);
  }) as typeof api.call);
  return sent;
}

afterEach(() => vi.restoreAllMocks());

describe('<PaceSettings> (v1.4 Basic › Daily pace)', () => {
  it('answers the everyday questions with the deck’s values and what the pace costs', async () => {
    const decks = (await api.deck.tree()).roots;
    const eff = await api.call('presets.for_deck', { deck_id: decks[0]!.deck.id });
    render(PaceSettings, { ctx: { ...ctx, decks } });
    expect(await screen.findByRole('heading', { level: 1, name: 'How many cards a day?' })).toBeTruthy();
    const hero = await screen.findByTestId('pace-hero');
    expect(within(hero).getByText('A day')).toBeTruthy();
    expect(within(hero).getByRole('img', { name: /Minutes a day: today/ })).toBeTruthy();
    expect(((await screen.findByLabelText('How many new words a day?')) as HTMLInputElement).value).toBe(String(eff.config.new_per_day));
    expect((screen.getByLabelText('At most how many reviews a day?') as HTMLInputElement).value).toBe(String(eff.config.reviews_per_day));
    // A/B 7 = A: 10 is the recommended pace; the four ready-made paces are one group.
    expect(screen.getByText(/Recommended: 10 a day\./)).toBeTruthy();
    const picks = screen.getByRole('radiogroup', { name: 'Ready-made paces' });
    expect(within(picks).getAllByRole('radio')).toHaveLength(4);
    const firm = screen.getByRole('radiogroup', { name: 'How firmly do you want to remember?' });
    expect(within(firm).getByRole('radio', { name: /Balanced %90/ }).getAttribute('aria-checked')).toBe(String(eff.config.desired_retention === 0.9));
    // The full editor is not here: steps, order and FSRS are behind a related link.
    expect(screen.queryByLabelText('Learning steps (minutes)')).toBeNull();
  });

  it('saves each change at once through planning.apply_pace for the chosen deck', async () => {
    const decks = (await api.deck.tree()).roots;
    const sent = spyPace();
    render(PaceSettings, { ctx: { ...ctx, decks } });
    const deck = decks[0]!.deck.id;
    const picks = await screen.findByRole('radiogroup', { name: 'Ready-made paces' });
    await fireEvent.click(within(picks).getByRole('radio', { name: /^5 · Easy/ }));
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0]).toMatchObject({ deck_id: deck, new_per_day: 5, reviews_per_day: null, desired_retention: null });

    const reviews = screen.getByLabelText('At most how many reviews a day?') as HTMLInputElement;
    await fireEvent.input(reviews, { target: { value: '150' } });
    await fireEvent.change(reviews);
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]).toMatchObject({ deck_id: deck, reviews_per_day: 150 });

    const firm = screen.getByRole('radiogroup', { name: 'How firmly do you want to remember?' });
    await fireEvent.click(within(firm).getByRole('radio', { name: /Strict %95/ }));
    await vi.waitFor(() => expect(sent).toHaveLength(3));
    expect(sent[2]).toMatchObject({ deck_id: deck, desired_retention: 0.95 });
  });

  it('a slow reload never undoes or overwrites the next change: saves in order, the typed value stays', async () => {
    // A tiny core: apply_pace writes the deck's values, for_deck reads them. While `slow`, the
    // page's reads wait until released (a loaded machine; the P1 journey failed this way).
    const core = { new_per_day: 20, reviews_per_day: 200, desired_retention: 0.9 };
    const call = api.call.bind(api);
    const sent: Record<string, unknown>[] = [];
    const held: (() => void)[] = [];
    let slow = false;
    vi.spyOn(api, 'call').mockImplementation(((method: string, req: Record<string, unknown>) => {
      let p: Promise<unknown>;
      if (method === 'planning.apply_pace') {
        sent.push(req);
        core.new_per_day = req.new_per_day as number;
        if (req.reviews_per_day != null) core.reviews_per_day = req.reviews_per_day as number;
        if (req.desired_retention != null) core.desired_retention = req.desired_retention as number;
        p = call(method as never, req as never);
      } else if (method === 'presets.for_deck') {
        p = call(method as never, req as never).then((r) => ({ ...(r as object), config: { ...(r as { config: object }).config, ...core } }));
      } else {
        p = call(method as never, req as never);
      }
      if (slow && (method === 'presets.for_deck' || method === 'planning.forecast')) {
        return p.then((v) => new Promise((resolve) => held.push(() => resolve(v as never))));
      }
      return p;
    }) as typeof api.call);
    const release = () => held.splice(0).forEach((go) => go());

    const decks = (await api.deck.tree()).roots;
    render(PaceSettings, { ctx: { ...ctx, decks } });
    const reviews = (await screen.findByLabelText('At most how many reviews a day?')) as HTMLInputElement;
    await vi.waitFor(() => expect(reviews.value).toBe('200'));

    slow = true;
    const picks = screen.getByRole('radiogroup', { name: 'Ready-made paces' });
    await fireEvent.click(within(picks).getByRole('radio', { name: /^5 · Easy/ }));
    // (each save first reads the deck's values: let that read through)
    await vi.waitFor(() => {
      release();
      expect(sent).toHaveLength(1);
    });
    // Typed while save #1's reload is still on its way.
    await fireEvent.input(reviews, { target: { value: '150' } });
    await fireEvent.change(reviews);
    await new Promise((r) => setTimeout(r, 20));
    expect(sent).toHaveLength(1); // save #2 waits for save #1 and its reload

    // The old reload arrives: it neither resets the typed 150 nor lets save #2 send old values.
    await vi.waitFor(() => {
      release();
      expect(sent).toHaveLength(2);
    });
    expect(reviews.value).toBe('150');
    expect(sent[1]).toMatchObject({ new_per_day: 5, reviews_per_day: 150 });
    slow = false;
    release();
    await vi.waitFor(() => expect(reviews.value).toBe('150'));
    expect(core).toMatchObject({ new_per_day: 5, reviews_per_day: 150 });
  });

  it('links the rest (full editor, study screen, planning) and opens Advanced ones in Settings', async () => {
    const decks = (await api.deck.tree()).roots;
    const navigate = vi.fn();
    render(PaceSettings, { ctx: { ...ctx, decks, navigate } });
    const link = await screen.findByRole('button', { name: /Learning steps, card order, longest interval, FSRS/ });
    expect(within(link).getByText('In Advanced')).toBeTruthy();
    await fireEvent.click(link);
    expect(navigate).toHaveBeenCalledWith('/presets', 'settings');
    await fireEvent.click(screen.getByRole('button', { name: /What time a new day starts/ }));
    expect(navigate).toHaveBeenLastCalledWith('/mode-settings', 'settings');
  });

  it('teaches what to do first when there is no deck yet', async () => {
    const navigate = vi.fn();
    render(PaceSettings, { ctx: { ...ctx, navigate } });
    expect(screen.getByText('No decks yet')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Add cards' }));
    expect(navigate).toHaveBeenCalledWith('/notes/new');
  });
});
