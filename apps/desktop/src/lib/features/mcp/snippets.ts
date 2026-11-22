/**
 * Client configuration snippets for the MCP server (`fc --db <collection> mcp serve`). Code,
 * not UI text: the same lines are documented in docs/06 §2.1. Pure functions of the paths the
 * backend reports (`mcp.setup`) and the OS it runs on.
 */

/** Shown when the collection path is unknown (only outside the desktop app). */
export const DB_PLACEHOLDER = '/path/to/collection.db';
/** The program name when its full path is unknown: works once `fc` is on `PATH`. */
export const FC_FALLBACK = 'fc';
/** Server name the clients list it under. */
export const SERVER_NAME = 'flashcard';

/** MCP clients the setup guide knows. */
export type McpClient = 'claude_desktop' | 'claude_code' | 'other';
export const CLIENTS: readonly McpClient[] = ['claude_desktop', 'claude_code', 'other'];

export interface SnippetPaths {
  /** Collection file (`mcp.setup.db_path`); placeholder when unknown. */
  db?: string | null;
  /** The `fc` program (`mcp.setup.fc_path`, or what the user typed); `fc` when unknown. */
  fc?: string | null;
  /** `std::env::consts::OS` (`windows`, `macos`, `linux`, …). */
  platform?: string;
}

const dbOf = (p: SnippetPaths) => p.db?.trim() || DB_PLACEHOLDER;
const fcOf = (p: SnippetPaths) => p.fc?.trim() || FC_FALLBACK;

/** Arguments after the program. */
export function serverArgs(db: string): string[] {
  return ['--db', db, 'mcp', 'serve'];
}

/**
 * One argument for a terminal. Plain words stay bare; anything else is double-quoted —
 * on Windows as is (paths cannot contain `"`, and `\` is the separator there), elsewhere with
 * `\`, `"`, `$` and `` ` `` escaped for POSIX shells.
 */
export function shellQuote(arg: string, platform = 'linux'): string {
  if (/^[\w@%+=:,./-]+$/.test(arg)) return arg;
  return platform === 'windows' ? `"${arg}"` : `"${arg.replace(/(["\\$`])/g, '\\$1')}"`;
}

/** The whole server command line (for clients that take one line). */
export function serverCommandLine(p: SnippetPaths = {}): string {
  return [fcOf(p), ...serverArgs(dbOf(p))].map((a) => shellQuote(a, p.platform)).join(' ');
}

/** Claude Code: one terminal command that registers the server. */
export function claudeCodeCommand(p: SnippetPaths = {}): string {
  return `claude mcp add ${SERVER_NAME} -- ${serverCommandLine(p)}`;
}

/**
 * Claude Desktop: the `claude_desktop_config.json` content. JSON escapes Windows backslashes
 * (`C:\\Users\\…`).
 */
export function claudeDesktopConfig(p: SnippetPaths = {}): string {
  return JSON.stringify(
    { mcpServers: { [SERVER_NAME]: { command: fcOf(p), args: serverArgs(dbOf(p)) } } },
    null,
    2,
  );
}

/** Any other client: the program and its arguments, each on its own line. */
export function otherClientFields(p: SnippetPaths = {}): { command: string; args: string[] } {
  return { command: fcOf(p), args: serverArgs(dbOf(p)) };
}

/** What the copy button copies for `client`. */
export function snippetFor(client: McpClient, p: SnippetPaths = {}): string {
  switch (client) {
    case 'claude_desktop':
      return claudeDesktopConfig(p);
    case 'claude_code':
      return claudeCodeCommand(p);
    case 'other':
      return serverCommandLine(p);
  }
}

/** An example `fc` path for the path field's placeholder. */
export function fcPlaceholder(platform = 'windows'): string {
  return platform === 'windows' ? 'C:\\…\\fc.exe' : '/usr/local/bin/fc';
}

/** Where Claude Desktop keeps its configuration file on `platform`. */
export function claudeDesktopConfigFile(platform = 'windows'): string {
  switch (platform) {
    case 'windows':
      return '%APPDATA%\\Claude\\claude_desktop_config.json';
    case 'macos':
      return '~/Library/Application Support/Claude/claude_desktop_config.json';
    default:
      return '~/.config/Claude/claude_desktop_config.json';
  }
}
