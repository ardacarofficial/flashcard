/**
 * DEV-ONLY in-memory backend so the UI is demoable in a plain browser
 * (`pnpm dev:mock`) and testable without Rust. It is NOT a source of truth for
 * behaviour: the real rules live in fc-app. Loaded only when
 * `import.meta.env.DEV` (see ../index.ts) and by tests. Speaks the real wire
 * protocol: qualified `v1.*` names, generated request/response types, `ApiError`
 * DTOs on failure and `EventEnvelope`s on the event stream.
 */
import { API_VERSION, type MethodName } from '../generated/methods';
import type { ModuleDto } from '../types';
import type { Transport } from '../transport';
import { createMockEnv, err, type Handlers, type MockFeature } from './env';

/** Every `features/<feature>/mock.ts` contributes its modules and handlers. */
const features = Object.values(
  import.meta.glob<{ default: MockFeature }>('../../features/*/mock.ts', { eager: true }),
).map((m) => m.default);

export function createMockTransport(): Transport {
  const env = createMockEnv();
  const modules: ModuleDto[] = features.flatMap((f) => f.modules ?? []);
  const handlers: Handlers = Object.assign(
    { 'modules.list': () => ({ modules }) } satisfies Handlers,
    ...features.map((f) => f.handlers(env)),
  );

  return {
    async invoke(command, payload) {
      const prefix = `${API_VERSION}.`;
      const method = (command.startsWith(prefix) ? command.slice(prefix.length) : '') as MethodName;
      const h = handlers[method] as ((req: unknown) => unknown) | undefined;
      if (!h) throw err('NOT_FOUND_METHOD', 'errors.not_found.method', command);
      // Deep-copy in and out so callers can't mutate mock state (mirrors IPC serialisation).
      return structuredClone(h(structuredClone(payload ?? {})));
    },
    async listen(handler) {
      env.listeners.add(handler);
      return () => env.listeners.delete(handler);
    },
  };
}
