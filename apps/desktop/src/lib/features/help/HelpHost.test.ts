import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import { openHelp, shellCommands } from '$lib/shell/commands.svelte';
import HelpHost from './HelpHost.svelte';

i18n.locale = 'en';

const press = (target: EventTarget, init: KeyboardEventInit) =>
  target.dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }));

describe('<HelpHost> on the dev mock', () => {
  it('`?` opens the sheet with the live table; a rebound key shows on the next open; the editor link', { timeout: 30000 }, async () => {
    // Generic over the table's content (its definitions are not this feature's).
    const { items } = await api.call('shortcuts.list', {});
    const target = items.find((i) => i.combo && i.context === 'study') ?? items.find((i) => i.combo)!;
    const row = () => document.querySelector(`li[data-action="${target.action}"]`);
    const openShortcuts = vi.fn();
    const { unmount } = render(HelpHost, { openShortcuts });
    expect(shellCommands.canHelp).toBe(true);

    // Not while typing.
    const field = document.body.appendChild(document.createElement('input'));
    press(field, { key: '?' });
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole('dialog')).toBeNull();
    field.remove();
    // Not with a modifier.
    press(window, { key: '?', ctrlKey: true });
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByRole('dialog')).toBeNull();

    press(window, { key: '?' });
    const dialog = await screen.findByRole('dialog', { name: 'Keyboard shortcuts' }, { timeout: 15000 });
    await waitFor(() => expect(row()?.textContent).toContain(target.combo!.split('+').at(-1)), { timeout: 5000 });
    expect(dialog.querySelectorAll('li[data-action]')).toHaveLength(items.filter((i) => i.combo).length);
    // Unbound rows are not listed; the search field has focus.
    expect(dialog.querySelectorAll('li[data-action^="v1."]')).toHaveLength(0);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole('searchbox', { name: 'Search shortcuts' })));

    await fireEvent.input(screen.getByRole('searchbox'), { target: { value: target.action } });
    await waitFor(() => expect(Array.from(dialog.querySelectorAll('li[data-action]'), (l) => l.getAttribute('data-action'))).toEqual([target.action]));
    await fireEvent.input(screen.getByRole('searchbox'), { target: { value: 'zzzz' } });
    expect(await screen.findByText('No shortcut matches your search.')).toBeTruthy();

    // Esc closes; rebinding (as the Shortcuts screen does) shows on the next open.
    await fireEvent.keyDown(dialog, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    await api.call('shortcuts.set', { action: target.action, combo: 'Alt+Shift+F9' });
    expect(openHelp()).toBe(true);
    const again = await screen.findByRole('dialog', { name: 'Keyboard shortcuts' }, { timeout: 15000 });
    await waitFor(() => expect(row()?.textContent).toContain('F9'), { timeout: 5000 });
    expect(again.contains(row())).toBe(true);
    await api.call('shortcuts.reset', { action: target.action });

    // While a dialog is open, `?` does not stack another one.
    press(window, { key: '?' });
    expect(screen.getAllByRole('dialog')).toHaveLength(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Change shortcuts' }));
    expect(openShortcuts).toHaveBeenCalledOnce();
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    unmount();
    expect(shellCommands.canHelp).toBe(false);
  });

  it('hides the editor link when the Shortcuts screen is not there', { timeout: 30000 }, async () => {
    const { unmount } = render(HelpHost, { openShortcuts: null });
    openHelp();
    await screen.findByRole('dialog', { name: 'Keyboard shortcuts' }, { timeout: 15000 });
    await waitFor(() => expect(document.querySelector('[data-action]')).toBeTruthy());
    expect(screen.queryByRole('button', { name: 'Change shortcuts' })).toBeNull();
    unmount();
  });
});
