import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import OnboardingHost from './OnboardingHost.svelte';
import StartersScreen from './StartersScreen.svelte';
import type { ScreenCtx } from '../types';

// The dev mock already holds decks (so it never shows the guide); pretend a new collection.
vi.mock('$lib/api/mock/transport', async (original) => {
  const real = await original<typeof import('$lib/api/mock/transport')>();
  return {
    createMockTransport: () => {
      const t = real.createMockTransport();
      let fresh = true;
      return {
        ...t,
        invoke: async (command: string, payload: unknown) => {
          if (command === 'v1.onboarding.status' && fresh) return { show: true, completed: false };
          if (command === 'v1.onboarding.apply' || command === 'v1.onboarding.skip') fresh = false;
          return t.invoke(command, payload);
        },
      };
    },
  };
});

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

describe('first-run guide (dev mock)', () => {
  it('asks what you study, shows the matching classification and sets up deck + starters', async () => {
    const ondone = vi.fn();
    render(OnboardingHost, { ondone });
    expect(await screen.findByRole('heading', { name: 'Welcome to Flashcard' })).toBeTruthy();
    const next = screen.getByRole('button', { name: 'Next' }) as HTMLButtonElement;
    expect(next.disabled).toBe(true);
    // English → CEFR; YKS → the exam sections.
    expect(screen.getAllByText('Classification: CEFR levels (A1–C2)').length).toBe(2); // English, other language
    expect(screen.getByText('Classification: YKS sections (TYT / AYT / YDT)')).toBeTruthy();
    await fireEvent.click(screen.getByLabelText(/^English/));
    await fireEvent.click(next);

    const name = screen.getByLabelText('Deck name') as HTMLInputElement;
    expect(name.value).toBe('English');
    await fireEvent.input(name, { target: { value: 'My English' } });
    const everyday = screen.getByLabelText(/Everyday English/) as HTMLInputElement;
    const guide = screen.getByLabelText(/How Flashcard works/) as HTMLInputElement;
    expect(everyday.checked && guide.checked).toBe(true);
    await fireEvent.click(guide);
    await fireEvent.click(screen.getByRole('button', { name: 'Set up' }));
    expect(await screen.findByText('Ready: 24 cards added. Undo removes the whole setup.')).toBeTruthy();
    expect(ondone).toHaveBeenCalled();

    const roots = (await api.deck.tree()).roots;
    const mine = roots.find((r) => r.deck.name === 'My English')!;
    expect(mine.children.map((c) => c.deck.name)).toEqual(['Everyday English']);
    expect((await api.call('onboarding.status', {})).completed).toBe(true);
  });

  it('starter decks can be added later from their screen', async () => {
    render(StartersScreen, { ctx });
    const add = await screen.findAllByRole('button', { name: 'Add' });
    expect(add.length).toBe(4);
    await fireEvent.click(add[2]!);
    expect(await screen.findByText('Added “World capitals” (16 cards).')).toBeTruthy();
  });
});
