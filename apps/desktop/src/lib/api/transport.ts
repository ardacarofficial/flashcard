import type { FcEvent } from './types';

export type Unsubscribe = () => void;

/**
 * How the api client reaches the backend. The real one is Tauri IPC; tests and
 * `pnpm dev:mock` use an in-memory one. The contract is the same as the local
 * HTTP / MCP gates (docs/03 §7): one generic call + one event stream.
 */
export interface Transport {
  invoke(command: string, payload: unknown): Promise<unknown>;
  listen(handler: (event: FcEvent) => void): Promise<Unsubscribe>;
}

/** Tauri command name of the single generic IPC entry point (src-tauri/src/ipc.rs). */
export const IPC_COMMAND = 'invoke';
/** Tauri event channel carrying domain events. */
export const EVENT_CHANNEL = 'fc://event';

export function createTauriTransport(): Transport {
  return {
    async invoke(command, payload) {
      const { invoke } = await import('@tauri-apps/api/core');
      return invoke(IPC_COMMAND, { command, payload });
    },
    async listen(handler) {
      const { listen } = await import('@tauri-apps/api/event');
      return listen<FcEvent>(EVENT_CHANNEL, (e) => handler(e.payload));
    },
  };
}

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
