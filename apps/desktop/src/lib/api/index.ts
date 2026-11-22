import { createApiClient } from './client';
import { createTauriTransport, isTauri, type Transport } from './transport';

export {
  ApiError,
  CLASSIC_NOTETYPE_ID,
  createApiClient,
  toApiError,
  wireName,
  type ApiClient,
  type EventType,
} from './client';
export type * from './types';

/**
 * Pick the transport. The mock and the dev bridge are dev-only: `import.meta.env.DEV` is
 * statically false in production builds, so neither module is ever bundled there.
 * `vite --mode bridge` (`VITE_FC_BRIDGE_URL`, `VITE_FC_BRIDGE_TOKEN`) talks to `fc dev serve`
 * (the real Rust core); otherwise the mock outside Tauri, Tauri IPC inside.
 */
async function pickTransport(): Promise<Transport> {
  if (import.meta.env.DEV && import.meta.env.MODE === 'bridge') {
    const url = import.meta.env.VITE_FC_BRIDGE_URL;
    const token = import.meta.env.VITE_FC_BRIDGE_TOKEN;
    if (!url || !token) throw new Error('vite --mode bridge needs VITE_FC_BRIDGE_URL and VITE_FC_BRIDGE_TOKEN');
    const { createBridgeTransport } = await import('./bridge');
    return createBridgeTransport(url, token);
  }
  if (import.meta.env.DEV && (import.meta.env.VITE_FC_MOCK === '1' || !isTauri())) {
    const { createMockTransport } = await import('./mock/transport');
    return createMockTransport();
  }
  return createTauriTransport();
}

let picked: Promise<Transport> | null = null;
const lazy: Transport = {
  invoke: async (c, p) => (await (picked ??= pickTransport())).invoke(c, p),
  listen: async (h) => (await (picked ??= pickTransport())).listen(h),
};

/** The app-wide api client. */
export const api = createApiClient(lazy);
