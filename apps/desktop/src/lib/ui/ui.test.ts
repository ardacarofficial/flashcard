/** Interactive design-system components: keyboard, ARIA and focus behaviour. */
import { describe, expect, it, vi } from 'vitest';
import { createRawSnippet } from 'svelte';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { Checkbox, Dialog, Flashcard, Menu, NumberedRow, OptionTile, Select, Switch, Table, Tabs } from './index';

i18n.locale = 'en';

const html = (s: string) => createRawSnippet(() => ({ render: () => s }));

const FRUITS = [
  { value: 'apple', label: 'Apple' },
  { value: 'banana', label: 'Banana' },
  { value: 'cherry', label: 'Cherry', disabled: true },
  { value: 'date', label: 'Date' },
];

describe('Select', () => {
  it('opens with the keyboard, moves past disabled options, picks with Enter, closes with Esc', async () => {
    const onchange = vi.fn();
    render(Select<string>, { options: FRUITS, value: 'apple', label: 'Fruit', onchange });
    const box = screen.getByRole('combobox', { name: 'Fruit' });
    expect(box.textContent).toContain('Apple');
    expect(box.getAttribute('aria-expanded')).toBe('false');

    await fireEvent.keyDown(box, { key: 'ArrowDown' });
    expect(box.getAttribute('aria-expanded')).toBe('true');
    const list = screen.getByRole('listbox');
    expect(list.querySelectorAll('[role=option]')).toHaveLength(4);
    // Opens on the selected option; ↓ → Banana, ↓ skips disabled Cherry → Date.
    const active = () => document.getElementById(box.getAttribute('aria-activedescendant')!)?.textContent?.trim();
    expect(active()).toBe('Apple');
    await fireEvent.keyDown(box, { key: 'ArrowDown' });
    await fireEvent.keyDown(box, { key: 'ArrowDown' });
    expect(active()).toBe('Date');
    await fireEvent.keyDown(box, { key: 'Enter' });
    expect(onchange).toHaveBeenCalledWith('date');
    expect(box.textContent).toContain('Date');
    expect(screen.queryByRole('listbox')).toBeNull();
    expect(document.activeElement).toBe(box);

    await fireEvent.keyDown(box, { key: ' ' });
    await fireEvent.keyDown(box, { key: 'Home' });
    expect(active()).toBe('Apple');
    await fireEvent.keyDown(box, { key: 'Escape' });
    expect(screen.queryByRole('listbox')).toBeNull();
    expect(onchange).toHaveBeenCalledTimes(1);
  });

  it('typeahead picks while closed; click picks; shows the placeholder without a value', async () => {
    const onchange = vi.fn();
    render(Select<string>, { options: FRUITS, value: null, label: 'Fruit', onchange });
    const box = screen.getByRole('combobox', { name: 'Fruit' });
    expect(box.textContent).toContain('Choose…');
    await fireEvent.keyDown(box, { key: 'b' });
    expect(onchange).toHaveBeenLastCalledWith('banana');
    await fireEvent.click(box);
    await fireEvent.click(screen.getByRole('option', { name: 'Apple' }));
    expect(onchange).toHaveBeenLastCalledWith('apple');
    expect(screen.getByRole('combobox').getAttribute('aria-expanded')).toBe('false');
  });
});

describe('Checkbox and Switch', () => {
  it('checkbox toggles by click and label, reports changes, supports indeterminate', async () => {
    const onchange = vi.fn();
    render(Checkbox, { label: 'Suspend', onchange });
    const box = screen.getByRole('checkbox', { name: 'Suspend' }) as HTMLInputElement;
    await fireEvent.click(box);
    expect(box.checked).toBe(true);
    expect(onchange).toHaveBeenLastCalledWith(true);
    await fireEvent.click(screen.getByText('Suspend'));
    expect(box.checked).toBe(false);
  });

  it('indeterminate is reflected on the input', () => {
    render(Checkbox, { ariaLabel: 'Select all', indeterminate: true });
    expect((screen.getByRole('checkbox', { name: 'Select all' }) as HTMLInputElement).indeterminate).toBe(true);
  });

  it('switch is role=switch with aria-checked; disabled does not toggle', async () => {
    const onchange = vi.fn();
    const { rerender } = render(Switch, { label: 'Reminders', onchange });
    const sw = screen.getByRole('switch', { name: 'Reminders' });
    expect(sw.getAttribute('aria-checked')).toBe('false');
    await fireEvent.click(sw);
    expect(sw.getAttribute('aria-checked')).toBe('true');
    expect(onchange).toHaveBeenLastCalledWith(true);
    await rerender({ label: 'Reminders', onchange, disabled: true });
    await fireEvent.click(sw);
    expect(onchange).toHaveBeenCalledTimes(1);
  });
});

describe('Dialog', () => {
  it('moves focus in, traps Tab, closes on Esc and returns focus', async () => {
    const opener = document.createElement('button');
    document.body.appendChild(opener);
    opener.focus();
    const onclose = vi.fn();
    render(Dialog, {
      open: true,
      title: 'Rename deck',
      onclose,
      children: html('<div><input aria-label="Name" /><button>Middle</button></div>'),
      footer: html('<div><button>Save</button></div>'),
    });
    const dialog = screen.getByRole('dialog', { name: 'Rename deck' });
    expect(dialog.getAttribute('aria-modal')).toBe('true');
    const name = screen.getByRole('textbox', { name: 'Name' });
    const close = screen.getByRole('button', { name: 'Close' });
    const save = screen.getByRole('button', { name: 'Save' });
    await waitFor(() => expect(document.activeElement).toBe(name));

    // Tab from the last control wraps to the first (the close button); Shift+Tab wraps back.
    save.focus();
    await fireEvent.keyDown(dialog, { key: 'Tab' });
    expect(document.activeElement).toBe(close);
    await fireEvent.keyDown(dialog, { key: 'Tab', shiftKey: true });
    expect(document.activeElement).toBe(save);

    await fireEvent.keyDown(dialog, { key: 'Escape' });
    expect(onclose).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it('a non-dismissible dialog ignores Esc and has no close button', async () => {
    const onclose = vi.fn();
    render(Dialog, { open: true, title: 'Working', dismissible: false, onclose, children: html('<p>…</p>') });
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(onclose).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: 'Close' })).toBeNull();
  });
});

describe('Menu', () => {
  it('opens on ArrowDown into the first item, arrows wrap, Enter runs, Esc returns focus', async () => {
    const rename = vi.fn();
    const remove = vi.fn();
    render(Menu, {
      label: 'Deck actions',
      items: [
        { id: 'rename', label: 'Rename', onselect: rename },
        { id: 'sep', separator: true },
        { id: 'export', label: 'Export', disabled: true, onselect: vi.fn() },
        { id: 'delete', label: 'Delete', danger: true, onselect: remove },
      ],
    });
    const trigger = screen.getByRole('button', { name: 'Deck actions' });
    expect(trigger.getAttribute('aria-haspopup')).toBe('menu');
    await fireEvent.keyDown(trigger, { key: 'ArrowDown' });
    const items = await screen.findAllByRole('menuitem');
    expect(items.map((i) => i.textContent?.trim())).toEqual(['Rename', 'Export', 'Delete']);
    await waitFor(() => expect(document.activeElement).toBe(items[0]));
    const menu = screen.getByRole('menu');
    // Disabled Export is skipped.
    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items[2]);
    await fireEvent.keyDown(menu, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(items[0]);
    await fireEvent.keyDown(menu, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(trigger);

    await fireEvent.click(trigger);
    await fireEvent.keyDown(screen.getByRole('menu'), { key: 'd' });
    expect(document.activeElement?.textContent?.trim()).toBe('Delete');
    await fireEvent.click(document.activeElement!);
    expect(remove).toHaveBeenCalledOnce();
    expect(rename).not.toHaveBeenCalled();
    expect(screen.queryByRole('menu')).toBeNull();
  });
});

describe('Tabs', () => {
  it('arrow keys move and activate with one tab stop; panel follows', async () => {
    const onchange = vi.fn();
    render(Tabs, {
      label: 'Deck sections',
      tabs: [
        { id: 'cards', label: 'Cards' },
        { id: 'stats', label: 'Stats', disabled: true },
        { id: 'options', label: 'Options', badge: 3 },
      ],
      value: 'cards',
      onchange,
      children: createRawSnippet((id: () => string) => ({ render: () => `<p>panel ${id()}</p>` })),
    });
    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((t) => t.tabIndex)).toEqual([0, -1, -1]);
    expect(screen.getByRole('tabpanel').textContent).toContain('panel cards');
    tabs[0]!.focus();
    await fireEvent.keyDown(tabs[0]!, { key: 'ArrowRight' });
    expect(onchange).toHaveBeenLastCalledWith('options');
    expect(document.activeElement).toBe(tabs[2]);
    expect(tabs[2]!.getAttribute('aria-selected')).toBe('true');
    expect(screen.getByRole('tabpanel').getAttribute('aria-labelledby')).toBe(tabs[2]!.id);
    await fireEvent.keyDown(tabs[2]!, { key: 'ArrowRight' });
    expect(onchange).toHaveBeenLastCalledWith('cards');
    await fireEvent.keyDown(tabs[0]!, { key: 'End' });
    expect(document.activeElement).toBe(tabs[2]);
  });
});

describe('Flashcard', () => {
  it('flips with Space, Enter and click; the hidden face is aria-hidden', async () => {
    const onflip = vi.fn();
    render(Flashcard, { front: html('<span>der Hund</span>'), back: html('<span>the dog</span>'), onflip });
    const card = screen.getByRole('button', { name: 'Flip card' });
    const back = screen.getByText('the dog').closest('.face')!;
    const front = screen.getByText('der Hund').closest('.face')!;
    expect(back.getAttribute('aria-hidden')).toBe('true');
    expect(card.getAttribute('aria-describedby')).toContain(screen.getByText('der Hund').parentElement!.id);

    await fireEvent.keyDown(card, { key: ' ' });
    expect(onflip).toHaveBeenLastCalledWith(true);
    expect(front.getAttribute('aria-hidden')).toBe('true');
    expect(back.getAttribute('aria-hidden')).toBeNull();
    expect(card.textContent).toContain('Showing the back');
    await fireEvent.keyDown(card, { key: 'Enter' });
    expect(onflip).toHaveBeenLastCalledWith(false);
    await fireEvent.click(card);
    expect(onflip).toHaveBeenCalledTimes(3);
  });

  it('honours prefers-reduced-motion (no 3D flip) and disabled', async () => {
    const mm = vi.spyOn(window, 'matchMedia').mockImplementation(
      (q: string) => ({ matches: q.includes('reduce'), media: q, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList,
    );
    const onflip = vi.fn();
    const { container } = render(Flashcard, { front: html('<span>a</span>'), back: html('<span>b</span>'), onflip, disabled: true });
    expect(container.querySelector('.fc-flashcard')?.classList.contains('instant')).toBe(true);
    await fireEvent.keyDown(screen.getByRole('button'), { key: ' ' });
    expect(onflip).not.toHaveBeenCalled();
    mm.mockRestore();
  });
});

describe('Table', () => {
  interface Row {
    id: string;
    front: string;
    back: string;
  }
  const rows: Row[] = Array.from({ length: 10_000 }, (_, i) => ({ id: `n${i}`, front: `front ${i}`, back: `back ${i}` }));
  const columns = [
    { id: 'front', header: 'Front', value: (r: Row) => r.front },
    { id: 'back', header: 'Back', value: (r: Row) => r.back },
  ];

  it('renders only the rows in view and keeps the full row count for assistive tech', () => {
    render(Table<Row>, { rows, columns, rowKey: (r) => r.id, label: 'Cards', rowHeight: 40 });
    const grid = screen.getByRole('grid', { name: 'Cards' });
    expect(grid.getAttribute('aria-rowcount')).toBe('10001');
    const rendered = grid.querySelectorAll('.body [role=row]');
    expect(rendered.length).toBeGreaterThan(10);
    expect(rendered.length).toBeLessThan(40);
    expect(screen.getAllByRole('columnheader').map((h) => h.textContent)).toEqual(['Front', 'Back']);
  });

  it('↓/End/Home move the active row (scrolling it into the window); Enter opens it', async () => {
    const onrowclick = vi.fn();
    render(Table<Row>, { rows, columns, rowKey: (r) => r.id, label: 'Cards', rowHeight: 40, onrowclick });
    const grid = screen.getByRole('grid');
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    const active = () => document.getElementById(grid.getAttribute('aria-activedescendant')!);
    expect(active()?.textContent).toContain('front 1');
    await fireEvent.keyDown(grid, { key: 'End' });
    await fireEvent.scroll(grid);
    await waitFor(() => expect(active()?.textContent).toContain('front 9999'));
    await fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onrowclick).toHaveBeenLastCalledWith(rows[9999]);
    await fireEvent.keyDown(grid, { key: 'Home' });
    await waitFor(() => expect(active()?.textContent).toContain('front 0'));
  });

  it('shows the empty text without rows', () => {
    render(Table<Row>, { rows: [], columns, rowKey: (r) => r.id, label: 'Cards' });
    expect(screen.getByText('Nothing here yet.')).toBeTruthy();
  });
});

describe('OptionTile', () => {
  it('is a radio with a letter marker when `checked` is set, and shows the result with an icon label', async () => {
    const onclick = vi.fn();
    const { rerender } = render(OptionTile, { label: 'gitmek', letter: 'A', checked: false, onclick });
    const tile = screen.getByRole('radio', { name: /gitmek/ });
    expect(tile.getAttribute('aria-checked')).toBe('false');
    await fireEvent.click(tile);
    expect(onclick).toHaveBeenCalledOnce();
    await rerender({ label: 'gitmek', letter: 'A', checked: true, result: 'miss', missLabel: 'Your pick', disabled: true });
    expect(screen.getByRole('radio').getAttribute('aria-checked')).toBe('true');
    expect(screen.getByRole('img', { name: 'Your pick' })).toBeTruthy();
  });

  it('is a plain button with a shortcut key otherwise', () => {
    render(OptionTile, { label: 'kedi', kbd: '1' });
    const tile = screen.getByRole('button', { name: /kedi/ });
    expect(tile.querySelector('kbd')?.textContent).toBe('1');
  });
});

describe('NumberedRow', () => {
  it('shows its number, the fields, the actions and an error', () => {
    render(NumberedRow, { n: 3, error: 'Already in the deck', children: html('<span>fields</span>'), actions: html('<button>x</button>') });
    const row = screen.getByRole('listitem');
    expect(row.querySelector('.num')?.textContent).toBe('3');
    expect(row.textContent).toContain('fields');
    expect(screen.getByRole('alert').textContent).toBe('Already in the deck');
    expect(row.classList.contains('invalid')).toBe(false);
  });
});
