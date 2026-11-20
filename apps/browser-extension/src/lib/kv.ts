/**
 * Tiny key-value abstraction over `chrome.storage` so the logic is testable without a browser.
 * `local` survives restarts (pairing token, outbox, cache); `session` lives until the browser closes
 * (the selection waiting for the popup).
 */
export interface Kv {
  get<T>(key: string): Promise<T | undefined>;
  set<T>(key: string, value: T): Promise<void>;
  remove(key: string): Promise<void>;
}

/** Wraps a `chrome.storage` area. */
export function chromeKv(area: chrome.storage.StorageArea): Kv {
  return {
    async get<T>(key: string): Promise<T | undefined> {
      const got = await area.get(key);
      return got[key] as T | undefined;
    },
    async set<T>(key: string, value: T): Promise<void> {
      await area.set({ [key]: value });
    },
    async remove(key: string): Promise<void> {
      await area.remove(key);
    },
  };
}

/** In-memory store (tests). Values are deep-copied like `chrome.storage` does. */
export class MemoryKv implements Kv {
  readonly data = new Map<string, string>();

  async get<T>(key: string): Promise<T | undefined> {
    const raw = this.data.get(key);
    return raw === undefined ? undefined : (JSON.parse(raw) as T);
  }

  async set<T>(key: string, value: T): Promise<void> {
    this.data.set(key, JSON.stringify(value));
  }

  async remove(key: string): Promise<void> {
    this.data.delete(key);
  }
}

/** Storage keys. One place, so nothing collides. */
export const KEYS = {
  settings: 'fc.settings',
  pairing: 'fc.pairing',
  outbox: 'fc.outbox',
  cache: 'fc.cache',
  remembered: 'fc.remembered',
  capture: 'fc.capture',
} as const;
