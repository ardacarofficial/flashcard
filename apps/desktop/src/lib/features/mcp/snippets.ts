/**
 * Client configuration snippets for the MCP server (`fc mcp serve`). Code, not UI text: the
 * same lines are documented in docs/06 §2.1.
 */
export const DB_PLACEHOLDER = '/path/to/collection.db';

export function claudeCodeCommand(db: string = DB_PLACEHOLDER): string {
  return `claude mcp add flashcard -- fc --db "${db}" mcp serve`;
}

export function claudeDesktopConfig(db: string = DB_PLACEHOLDER): string {
  return JSON.stringify(
    { mcpServers: { flashcard: { command: 'fc', args: ['--db', db, 'mcp', 'serve'] } } },
    null,
    2,
  );
}
