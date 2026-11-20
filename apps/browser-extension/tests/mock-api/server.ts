/** Serves `MockApi` over real HTTP on 127.0.0.1 (e2e). */
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { MockApi } from './handler';

export type Running = { api: MockApi; server: Server; port: number; close: () => Promise<void> };

export async function startMockApi(api = new MockApi(), port = 0): Promise<Running> {
  const server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on('data', (c: Buffer) => chunks.push(c));
    req.on('end', () => {
      const headers: Record<string, string> = {};
      for (const [k, v] of Object.entries(req.headers)) if (typeof v === 'string') headers[k] = v;
      const origin = headers.origin;
      const cors: Record<string, string> =
        origin && origin.startsWith('chrome-extension://')
          ? { 'access-control-allow-origin': origin, 'access-control-allow-headers': 'authorization, content-type, x-flashcard-client', 'access-control-allow-methods': 'GET, POST, OPTIONS' }
          : {};
      if (req.method === 'OPTIONS') {
        res.writeHead(204, cors).end();
        return;
      }
      // Test page with text to select (served from the same origin, which is in host_permissions).
      if (req.method === 'GET' && req.url === '/page.html') {
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        res.end('<!doctype html><title>Badgers of Europe</title><article><p id="p">Badgers live in setts. The badger dug a deep hole under the old oak. Then it slept.</p></article>');
        return;
      }
      const out = api.handle({ method: req.method ?? 'GET', path: req.url ?? '/', headers, body: Buffer.concat(chunks).toString('utf8') || undefined });
      res.writeHead(out.status, { 'content-type': 'application/json', ...cors, ...out.headers });
      res.end(JSON.stringify(out.body));
    });
  });
  await new Promise<void>((resolve) => server.listen(port, '127.0.0.1', resolve));
  const actual = (server.address() as AddressInfo).port;
  api.port = actual;
  return {
    api,
    server,
    port: actual,
    close: () =>
      new Promise<void>((resolve) => {
        server.close(() => resolve());
        server.closeAllConnections();
      }),
  };
}
