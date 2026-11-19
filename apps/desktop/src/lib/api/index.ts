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
 * Pick the transport. The mock is dev-only: `import.meta.env.DEV` is statically
 * false in production builds, so the mock module is never bundled there.
 */
async function pickTransport(): Promise<Transport> {
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
