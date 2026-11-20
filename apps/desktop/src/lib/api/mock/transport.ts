/**
 * DEV-ONLY in-memory backend so the UI is demoable in a plain browser
 * (`pnpm dev:mock`) and testable without Rust. It is NOT a source of truth for
 * behaviour: the real rules live in fc-app. Loaded only when
 * `import.meta.env.DEV` (see ../index.ts) and by tests. Speaks the real wire
 * protocol: qualified `v1.*` names, generated request/response types, `ApiError`
 * DTOs on failure and `EventEnvelope`s on the event stream.
 */
import { API_VERSION, METHODS, type MethodName } from '../generated/methods';
import type { ModuleDto } from '../types';
import type { Transport } from '../transport';
import { createMockEnv, err, type Handlers, type MockFeature } from './env';

/** Every `features/<feature>/mock.ts` contributes its modules and handlers. */
const features = Object.values(
  import.meta.glob<{ default: MockFeature }>('../../features/*/mock.ts', { eager: true }),
).map((m) => m.default);

export function createMockTransport(): Transport {
  const env = createMockEnv();
  // Own copy: `modules.set_enabled` mutates it.
  const modules: ModuleDto[] = structuredClone(features.flatMap((f) => f.modules ?? []));
  // A module owns the methods it lists, else those named after it (`attributes.*`).
  const claimed = new Set(modules.flatMap((m) => m.methods));
  for (const m of modules) {
    if (m.methods.length === 0) {
      m.methods = (Object.keys(METHODS) as MethodName[]).filter((n) => n.startsWith(`${m.id}.`) && !claimed.has(n));
    }
  }
  const ui = new Map(modules.map((m) => [m.id, m.ui]));
  const handlers: Handlers = Object.assign(
    {
      'modules.list': () => ({ modules }),
      'modules.set_enabled': ({ id, enabled }) => {
        const m = modules.find((x) => x.id === id);
        if (!m) throw err('NOT_FOUND_MODULE', 'errors.not_found.module', id);
        if (!m.can_disable && !enabled) throw err('CONFLICT_MODULE_REQUIRED', 'errors.conflict.module_required', id);
        m.enabled = enabled;
        m.ui = enabled ? (ui.get(id) ?? []) : [];
        env.emit({ type: 'module_toggled', data: { id, enabled } });
        return m;
      },
    } satisfies Handlers,
    ...features.map((f) => f.handlers(env)),
  );

  return {
    async invoke(command, payload) {
      const prefix = `${API_VERSION}.`;
      const method = (command.startsWith(prefix) ? command.slice(prefix.length) : '') as MethodName;
      const h = handlers[method] as ((req: unknown) => unknown) | undefined;
      if (!h) throw err('NOT_FOUND_METHOD', 'errors.not_found.method', command);
      // Like the real registry: a switched-off module's methods are rejected.
      const owner = modules.find((m) => m.methods.includes(method));
      if (owner && !owner.enabled) throw err('CONFLICT_MODULE_DISABLED', 'errors.conflict.module_disabled', command);
      // Deep-copy in and out so callers can't mutate mock state (mirrors IPC serialisation).
      return structuredClone(h(structuredClone(payload ?? {})));
    },
    async listen(handler) {
      env.listeners.add(handler);
      return () => env.listeners.delete(handler);
    },
  };
}
