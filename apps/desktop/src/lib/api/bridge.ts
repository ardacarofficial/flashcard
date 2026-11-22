/**
 * DEV-ONLY transport to the dev bridge (`fc dev serve`, v1.3): the UI in a plain browser
 * against the REAL Rust core and a real collection file, through the same dispatch the
 * desktop IPC uses (see `crates/fc-cli/src/dev/bridge.rs`). Selected by `vite --mode bridge`
 * with `VITE_FC_BRIDGE_URL` and `VITE_FC_BRIDGE_TOKEN` (see ./index.ts); never part of a
 * production bundle. The Playwright journeys (`apps/desktop/e2e`) run on it.
 *
 * Wire: `POST <url>/rpc {method, params}` → `{ok: true, result}` | `{ok: false, error}` (the
 * `ApiError` DTO the IPC command rejects with); `GET <url>/events` is Server-Sent Events,
 * one `EventEnvelope` per message.
 */
import type { EventEnvelope } from './types';
import type { Transport, Unsubscribe } from './transport';

interface Reply {
  ok?: boolean;
  result?: unknown;
  error?: unknown;
}

export function createBridgeTransport(url: string, token: string): Transport {
  const base = url.replace(/\/+$/, '');
  // ONE event stream for the whole page, fanned out to every listener: the browser allows
  // only a few HTTP/1.1 connections per host, and a stream per `api.on` would starve calls.
  const handlers = new Set<(event: EventEnvelope) => void>();
  let source: EventSource | null = null;
  const connect = () => {
    if (source) return;
    source = new EventSource(`${base}/events?token=${encodeURIComponent(token)}`);
    source.onmessage = (m: MessageEvent<string>) => {
      let env: EventEnvelope;
      try {
        env = JSON.parse(m.data) as EventEnvelope;
      } catch {
        return;
      }
      for (const h of [...handlers]) h(env);
    };
  };

  return {
    async invoke(command, payload) {
      const res = await fetch(`${base}/rpc`, {
        method: 'POST',
        headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
        body: JSON.stringify({ method: command, params: payload ?? null }),
      });
      const reply = (await res.json().catch(() => null)) as Reply | null;
      if (reply?.ok === true) return reply.result;
      // The ApiError DTO, normalised by the client's `toApiError` like an IPC rejection.
      if (reply?.error) throw reply.error;
      throw new Error(`dev bridge: HTTP ${res.status} for ${command}`);
    },
    async listen(handler): Promise<Unsubscribe> {
      handlers.add(handler);
      connect();
      return () => {
        handlers.delete(handler);
        if (handlers.size === 0 && source) {
          source.close();
          source = null;
        }
      };
    },
  };
}
