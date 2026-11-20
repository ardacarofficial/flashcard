import { describe, expect, it } from 'vitest';
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
    expect(await screen.findByText('npx -y @modelcontextprotocol/server-filesystem')).toBeTruthy();
    const [s] = (await api.call('mcp.servers.list', {})).servers;
    expect(s!.args).toEqual(['-y', '@modelcontextprotocol/server-filesystem']);

    await fireEvent.click(screen.getByRole('button', { name: 'Remove' }));
    expect(await screen.findByText('No servers yet.')).toBeTruthy();
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
