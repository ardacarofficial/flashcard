import { describe, expect, it } from 'vitest';
import { createMockEnv } from './env';

describe('mock clock', () => {
  it('speaks milliseconds since the epoch, like fc_core::Timestamp', () => {
    const before = Date.now();
    const env = createMockEnv();
    const a = env.now();
    const b = env.now();
    // Recent (not 1970) and strictly increasing.
    expect(a).toBeGreaterThan(before - 2 * 86_400_000);
    expect(a).toBeLessThanOrEqual(before + 1000);
    expect(b).toBeGreaterThan(a);
    expect(new Date(a).getFullYear()).toBe(new Date(before).getFullYear());
    // Records created through the env carry the same clock.
    const deck = env.addDeck('Zaman', null);
    expect(deck.created_at).toBeGreaterThan(b);
  });
});
