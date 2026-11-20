/**
 * Dev-only mock of the `mcp` module (see `api/mock/transport.ts`). Validation is minimal:
 * the real rules live in fc-app `modules/mcp/servers.rs`.
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { McpServerDto, McpToolInfo } from '$lib/api/types';

const TOOLS: McpToolInfo[] = [
  ['list_decks', 'read', false],
  ['list_note_types', 'read', false],
  ['list_classifications', 'read', false],
  ['search_cards', 'read', false],
  ['get_note', 'read', false],
  ['create_deck', 'write', false],
  ['add_notes', 'write', false],
  ['update_note', 'write', true],
  ['set_classification', 'write', true],
].map(([name, access, destructive]) => ({
  name: name as string,
  access: access as 'read' | 'write',
  destructive: destructive as boolean,
  methods: [],
}));

export default {
  modules: [module('mcp', [menu('mcp.main', 'nav.mcp', '/mcp', 80)])],
  handlers: ({ now, id }: MockEnv) => {
    const servers: McpServerDto[] = [];
    const find = (sid: string) => {
      const s = servers.find((x) => x.id === sid);
      if (!s) throw err('NOT_FOUND_MCP_SERVER', 'errors.not_found.mcp_server', sid);
      return s;
    };
    return {
      'mcp.tools.list': () => ({ tools: TOOLS, writes_allowed: true }),
      'mcp.servers.list': () => ({
        servers: [...servers].sort((a, b) => a.name.localeCompare(b.name, 'tr')),
      }),
      'mcp.servers.add': ({ name, transport, command, args, url, enabled }) => {
        if (servers.some((s) => s.name.toLowerCase() === name.trim().toLowerCase())) {
          throw err('CONFLICT_MCP_SERVER_NAME', 'errors.conflict.mcp_server_name', name);
        }
        const t = now();
        const s: McpServerDto = {
          id: id('mcp'),
          name: name.trim(),
          transport,
          command: transport === 'stdio' ? (command ?? '').trim() : '',
          args: transport === 'stdio' ? (args ?? []) : [],
          url: transport === 'http' ? (url ?? null) : null,
          enabled: enabled ?? true,
          approved: true,
          created_at: t,
          updated_at: t,
        };
        servers.push(s);
        return s;
      },
      'mcp.servers.update': (req) => {
        const s = find(req.id);
        Object.assign(s, { ...req, approved: true, updated_at: now() });
        return s;
      },
      'mcp.servers.remove': ({ id: sid }) => {
        const s = find(sid);
        servers.splice(servers.indexOf(s), 1);
        return s;
      },
    };
  },
} satisfies MockFeature;
