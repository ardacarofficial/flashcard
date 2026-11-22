import { describe, expect, it } from 'vitest';
import {
  DB_PLACEHOLDER,
  claudeCodeCommand,
  claudeDesktopConfig,
  claudeDesktopConfigFile,
  otherClientFields,
  serverCommandLine,
  shellQuote,
  snippetFor,
} from './snippets';

const WIN = { db: 'C:\\Users\\Deniz\\AppData\\Roaming\\app.flashcard.desktop\\collection.db', platform: 'windows' };

describe('MCP client snippets', () => {
  it('escapes Windows backslashes in the Claude Desktop JSON and round-trips the real path', () => {
    const text = claudeDesktopConfig({ ...WIN, fc: 'C:\\Program Files\\Flashcard\\fc.exe' });
    expect(text).toContain('"C:\\\\Program Files\\\\Flashcard\\\\fc.exe"');
    expect(text).toContain('"C:\\\\Users\\\\Deniz\\\\AppData');
    expect(JSON.parse(text)).toEqual({
      mcpServers: {
        flashcard: { command: 'C:\\Program Files\\Flashcard\\fc.exe', args: ['--db', WIN.db, 'mcp', 'serve'] },
      },
    });
  });

  it('quotes paths for a terminal: as they are on Windows, escaped for POSIX shells', () => {
    expect(claudeCodeCommand({ ...WIN, fc: 'C:\\Program Files\\Flashcard\\fc.exe' })).toBe(
      `claude mcp add flashcard -- "C:\\Program Files\\Flashcard\\fc.exe" --db "${WIN.db}" mcp serve`,
    );
    expect(shellQuote('/Users/deniz/Library/Application Support/x.db', 'macos')).toBe(
      '"/Users/deniz/Library/Application Support/x.db"',
    );
    expect(shellQuote('/tmp/a"b$c`d\\e', 'linux')).toBe('"/tmp/a\\"b\\$c\\`d\\\\e"');
    expect(shellQuote('/opt/fc', 'linux')).toBe('/opt/fc');
  });

  it('falls back to "fc" and a placeholder path when they are unknown', () => {
    expect(serverCommandLine({})).toBe(`fc --db ${DB_PLACEHOLDER} mcp serve`);
    expect(serverCommandLine({ fc: '   ', db: null })).toBe(`fc --db ${DB_PLACEHOLDER} mcp serve`);
    expect(otherClientFields({ db: '/x.db' })).toEqual({ command: 'fc', args: ['--db', '/x.db', 'mcp', 'serve'] });
  });

  it('picks the snippet per client', () => {
    expect(snippetFor('claude_desktop', WIN)).toBe(claudeDesktopConfig(WIN));
    expect(snippetFor('claude_code', WIN)).toBe(claudeCodeCommand(WIN));
    expect(snippetFor('other', WIN)).toBe(`fc --db "${WIN.db}" mcp serve`);
  });

  it("knows where Claude Desktop keeps its configuration on each OS", () => {
    expect(claudeDesktopConfigFile('windows')).toBe('%APPDATA%\\Claude\\claude_desktop_config.json');
    expect(claudeDesktopConfigFile('macos')).toBe('~/Library/Application Support/Claude/claude_desktop_config.json');
    expect(claudeDesktopConfigFile('linux')).toBe('~/.config/Claude/claude_desktop_config.json');
  });
});
