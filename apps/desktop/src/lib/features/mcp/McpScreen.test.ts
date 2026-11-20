import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import McpScreen from './McpScreen.svelte';
import { claudeCodeCommand, claudeDesktopConfig } from './snippets';
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

describe('<McpScreen> on the dev mock', () => {
  it('lists the server tools and registers / removes an external server', async () => {
    render(McpScreen, { ctx });
    expect(await screen.findByText('Add notes')).toBeTruthy();
    expect(screen.getByText('Set a classification (asks for confirmation)')).toBeTruthy();
    expect(screen.getByText('No servers yet.')).toBeTruthy();

    await fireEvent.input(screen.getByLabelText('Name'), { target: { value: 'Files' } });
    await fireEvent.input(screen.getByLabelText('Command'), { target: { value: 'npx' } });
    await fireEvent.input(screen.getByLabelText('Arguments (one per line)'), {
      target: { value: '-y\n@modelcontextprotocol/server-filesystem\n' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Add server' }));
    // The command the app will launch is shown for approval first; nothing is registered yet.
    expect(await screen.findByText(/npx -y @modelcontextprotocol\/server-filesystem\. Only continue/)).toBeTruthy();
    expect((await api.call('mcp.servers.list', {})).servers).toEqual([]);
    await fireEvent.click(screen.getByRole('button', { name: 'Add and trust' }));
    expect(await screen.findByText('npx -y @modelcontextprotocol/server-filesystem')).toBeTruthy();
    const [s] = (await api.call('mcp.servers.list', {})).servers;
    expect(s!.args).toEqual(['-y', '@modelcontextprotocol/server-filesystem']);

    await fireEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(await screen.findByText('No servers yet.')).toBeTruthy();
  });

  it('refetches on data_changed (Claude or the CLI wrote from another process)', async () => {
    const on = vi.spyOn(api, 'on');
    render(McpScreen, { ctx });
    await screen.findByText('Add notes');
    const subscribed = on.mock.calls.find(([types]) => (types as string[]).includes('data_changed'));
    expect(subscribed).toBeTruthy();

    const ext = await api.call('mcp.servers.add', {
      name: 'Ext',
      transport: 'stdio',
      command: 'x',
      args: [],
      url: null,
      enabled: true,
    });
    expect(screen.queryByText('Ext')).toBeNull();
    subscribed![1]({ seq: 1, at: 0, batch_id: null, event: { type: 'data_changed' } } as never);
    expect(await screen.findByText('Ext')).toBeTruthy();
    await api.call('mcp.servers.remove', { id: ext.id });
    on.mockRestore();
  });

  it('shows a server nobody approved and only runs it after the user approves', async () => {
    const ext = await api.call('mcp.servers.add', {
      name: 'Restored',
      transport: 'stdio',
      command: 'sh',
      args: ['-c', 'x'],
      url: null,
      enabled: true,
    });
    // The mock has no restore path: flip the flag the way a backup would.
    const real = api.call.bind(api);
    const spy = vi.spyOn(api, 'call').mockImplementation(((method: string, params: never) =>
      method === 'mcp.servers.list'
        ? real('mcp.servers.list', params).then((l) => ({
            servers: l.servers.map((s) => ({ ...s, approved: s.id === ext.id ? false : s.approved })),
          }))
        : real(method as never, params)) as never);
    render(McpScreen, { ctx });
    expect(await screen.findByText('Not approved')).toBeTruthy();
    expect((screen.getByRole('switch', { name: /Restored/ }) as HTMLInputElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    expect(await screen.findByText(/never been approved/)).toBeTruthy();
    spy.mockRestore();
    await api.call('mcp.servers.remove', { id: ext.id });
  });

  it('documents the exact client configuration', () => {
    expect(claudeCodeCommand('C:\\data\\collection.db')).toBe(
      'claude mcp add flashcard -- fc --db "C:\\data\\collection.db" mcp serve',
    );
    expect(JSON.parse(claudeDesktopConfig('/x/collection.db'))).toEqual({
      mcpServers: { flashcard: { command: 'fc', args: ['--db', '/x/collection.db', 'mcp', 'serve'] } },
    });
  });
});
