import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import McpScreen from './McpScreen.svelte';
import { toast } from '$lib/ui';
import { claudeCodeCommand, claudeDesktopConfig } from './snippets';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

/** The servers tab (v1.3: "connect your assistant" and "external servers" are separate tabs). */
async function openServers() {
  await fireEvent.click(screen.getByRole('tab', { name: /External MCP servers/ }));
}

const DB = 'C:\\Users\\Deniz\\AppData\\Roaming\\app.flashcard.desktop\\collection.db';

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
    // The first call of the file builds the whole mock backend (seconds on a loaded machine).
    expect(await screen.findByText('Add notes', {}, { timeout: 8000 })).toBeTruthy();
    expect(screen.getByText('Set a classification (asks for confirmation)')).toBeTruthy();
    await openServers();
    expect(await screen.findByText('No servers yet.')).toBeTruthy();

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
    await openServers();
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
    await openServers();
    expect(await screen.findByText('Not approved')).toBeTruthy();
    expect((screen.getByRole('switch', { name: /Restored/ }) as HTMLInputElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    expect(await screen.findByText(/never been approved/)).toBeTruthy();
    spy.mockRestore();
    await api.call('mcp.servers.remove', { id: ext.id });
  });

  it('fills the real collection path in, JSON-escaped for Claude Desktop', async () => {
    render(McpScreen, { ctx });
    const pre = await screen.findByTestId('mcp-snippet');
    // Claude Desktop is preselected; the backend's Windows path, backslashes escaped in JSON.
    expect(pre.textContent).toContain('"C:\\\\Users\\\\Deniz\\\\AppData');
    expect(JSON.parse(pre.textContent!)).toEqual({
      mcpServers: { flashcard: { command: 'fc', args: ['--db', DB, 'mcp', 'serve'] } },
    });
    expect(pre.textContent).not.toContain('/path/to/collection.db');
    expect(screen.getByText('%APPDATA%\\Claude\\claude_desktop_config.json')).toBeTruthy();
    expect(screen.getByText(/Ask your assistant: "List my decks\."/)).toBeTruthy();
  });

  it('asks for the fc program when the backend found none and updates the setting live', async () => {
    render(McpScreen, { ctx });
    expect(await screen.findByText('The fc program was not found')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Path of the fc program'), {
      target: { value: 'C:\\Tools\\fc.exe' },
    });
    expect(JSON.parse(screen.getByTestId('mcp-snippet').textContent!).mcpServers.flashcard.command).toBe('C:\\Tools\\fc.exe');
    // Claude Code: one terminal command, paths quoted as they are on Windows.
    await fireEvent.click(screen.getByRole('radio', { name: 'Claude Code' }));
    expect(screen.getByTestId('mcp-snippet').textContent).toBe(
      `claude mcp add flashcard -- "C:\\Tools\\fc.exe" --db "${DB}" mcp serve`,
    );
    expect(screen.getByText(/Typing \/mcp in it/)).toBeTruthy();
    // Any other client: program and arguments separately.
    await fireEvent.click(screen.getByRole('radio', { name: 'Other MCP client' }));
    expect(screen.getByText('C:\\Tools\\fc.exe')).toBeTruthy();
    expect(screen.getByText(DB)).toBeTruthy();
  });

  it('uses the fc program the backend found and asks nothing', async () => {
    const real = api.call.bind(api);
    const spy = vi.spyOn(api, 'call').mockImplementation(((method: string, params: never) =>
      method === 'mcp.setup'
        ? Promise.resolve({ db_path: '/home/deniz/.local/share/app.flashcard.desktop/collection.db', fc_path: '/opt/Flashcard/fc', platform: 'linux' })
        : real(method as never, params)) as never);
    render(McpScreen, { ctx });
    await fireEvent.click(await screen.findByRole('radio', { name: 'Claude Code' }));
    expect(screen.getByTestId('mcp-snippet').textContent).toBe(
      'claude mcp add flashcard -- /opt/Flashcard/fc --db /home/deniz/.local/share/app.flashcard.desktop/collection.db mcp serve',
    );
    expect(screen.queryByText('The fc program was not found')).toBeNull();
    spy.mockRestore();
  });

  it('copies the setting and says so', async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    render(McpScreen, { ctx });
    const pre = await screen.findByTestId('mcp-snippet');
    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));
    expect(writeText).toHaveBeenCalledWith(pre.textContent);
    await vi.waitFor(() => expect(toast.items.some((i) => i.message.startsWith('Copied.'))).toBe(true));
    Reflect.deleteProperty(navigator, 'clipboard');
  });

  it('read only is the allow-writes setting, inverted', async () => {
    const call = vi.spyOn(api, 'call');
    render(McpScreen, { ctx });
    const sw = await screen.findByRole('switch', { name: 'Read only' });
    expect(sw.getAttribute('aria-checked')).toBe('false');
    expect(screen.getByText('It can change')).toBeTruthy();
    await fireEvent.click(sw);
    expect(call).toHaveBeenCalledWith('settings.set', { scope: { kind: 'app' }, key: 'mcp.server.allow_writes', value: false });
    call.mockRestore();
  });

  it('documents the exact client configuration', () => {
    expect(claudeCodeCommand({ db: 'C:\\data\\collection.db', platform: 'windows' })).toBe(
      'claude mcp add flashcard -- fc --db "C:\\data\\collection.db" mcp serve',
    );
    expect(JSON.parse(claudeDesktopConfig({ db: '/x/collection.db' }))).toEqual({
      mcpServers: { flashcard: { command: 'fc', args: ['--db', '/x/collection.db', 'mcp', 'serve'] } },
    });
  });
});
