import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import type { RenderedSides } from '$lib/api/types';
import { i18n } from '$lib/i18n/index.svelte';

i18n.locale = 'en';

vi.mock('./media', () => ({
  resolveMedia: (html: string) => Promise.resolve(html.replace(/src="media:([0-9a-f]+)"/g, 'src="blob:$1"')),
}));
vi.mock('./modules.svelte', () => ({ moduleOn: () => true }));

import CardView from './CardView.svelte';

const H = '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08';
const sides = (over: Partial<RenderedSides> = {}): RenderedSides => ({
  template_key: 'forward',
  template_name: '1',
  card_type: 'classic',
  question: `Q <a class="fc-link" href="note:n-1">Porsuk</a> <img src="media:${H}" alt=""> <span class="math math-inline">x^2</span>`,
  answer: 'A <details class="fc-hint"><summary></summary>h</details>',
  css: '.x { color: red }',
  interaction: { kind: 'self_grade' },
  tts: [],
  ...over,
});

describe('<CardView>', () => {
  it('shows media, typesets math and opens linked notes', async () => {
    const onopennote = vi.fn();
    const { container } = render(CardView, { sides: sides(), onopennote });
    await waitFor(() => expect(container.querySelector('img')?.getAttribute('src')).toBe(`blob:${H}`));
    await waitFor(() => expect(container.querySelector('.katex')).not.toBeNull());
    await fireEvent.click(screen.getByText('Porsuk'));
    expect(onopennote).toHaveBeenCalledWith('n-1');
  });

  it('labels hints on the answer side', async () => {
    const { container } = render(CardView, { sides: sides(), showAnswer: true });
    await waitFor(() => expect(container.querySelector('summary')?.textContent).toBe('Hint'));
  });

  it('marks the picked and the right option of a choice card', async () => {
    render(CardView, { sides: sides({ interaction: { kind: 'choice', options: ['dog', 'cat'], correct: 1 } }) });
    await fireEvent.click(screen.getByRole('button', { name: 'dog' }));
    expect(screen.getByRole('button', { name: 'dog' }).classList.contains('wrong')).toBe(true);
    expect(screen.getByRole('button', { name: 'cat' }).classList.contains('right')).toBe(true);
  });
});
