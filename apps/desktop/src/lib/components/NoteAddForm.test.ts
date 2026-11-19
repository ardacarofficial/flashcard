import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import type { AttributeDef } from '$lib/api/types';
import { i18n } from '$lib/i18n/index.svelte';

i18n.locale = 'en';

const level: AttributeDef = {
  id: 'lvl', scope_deck_id: null, key: 'lvl', name: 'Level', kind: 'enum',
  options: [{ id: 'a1', label: 'A1', color: null, order: 1 }],
  target: 'note', required: false, default_value: null, position: 0,
};
const calls: Array<(d: AttributeDef[]) => void> = [];
const createClassic = vi.fn().mockResolvedValue({});

vi.mock('$lib/api', () => ({
  api: {
    attribute: { effective: () => new Promise((res) => calls.push(res)) },
    note: { createClassic: (...a: unknown[]) => createClassic(...a) },
  },
}));

import NoteAddForm from './NoteAddForm.svelte';

describe('<NoteAddForm>', () => {
  it('drops the previous deck\'s fields while the new deck loads', async () => {
    const { rerender } = render(NoteAddForm, { deckId: 'd1', deckName: 'D1' });
    calls[0]!([level]);
    const select = (await screen.findByLabelText('Level')) as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: 'a1' } });

    await rerender({ deckId: 'd2', deckName: 'D2' });
    await waitFor(() => expect(screen.queryByLabelText('Level')).toBeNull());

    await fireEvent.submit(screen.getByRole('button', { name: 'Add' }).closest('form')!);
    expect(createClassic).toHaveBeenLastCalledWith('d2', '', '', {});
  });
});
