/**
 * F1: the one "Advanced" switch — a slow first read (`load`) must not undo a flip the user
 * made while it was on its way (seen with a slowed bridge: the sidebar lost Search).
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { api } from '$lib/api';
import { advanced } from './advanced.svelte';

afterEach(() => vi.restoreAllMocks());

describe('advanced switch', () => {
  it('a late load does not undo the flip made meanwhile', async () => {
    await advanced.set(false);
    const call = api.call.bind(api);
    let release = () => {};
    vi.spyOn(api, 'call').mockImplementation(((method: string, req: unknown) => {
      const p = call(method as never, req as never);
      if (method !== 'settings.get') return p;
      return p.then((v) => new Promise((resolve) => (release = () => resolve(v as never))));
    }) as typeof api.call);
    const loading = advanced.load(); // reads "off"; its answer is held
    await new Promise((r) => setTimeout(r, 0));
    await advanced.set(true); // the user turns it on
    release();
    await loading;
    expect(advanced.on).toBe(true);
    vi.restoreAllMocks();
    await advanced.load(); // a read after the flip shows what was saved
    expect(advanced.on).toBe(true);
    await advanced.set(false);
  });

  it('a read that ran while the save was on its way does not undo it', async () => {
    await advanced.set(false);
    const call = api.call.bind(api);
    let release = () => {};
    vi.spyOn(api, 'call').mockImplementation(((method: string, req: unknown) => {
      if (method !== 'settings.set') return call(method as never, req as never);
      // the save reaches the core only after the read below has been answered
      return new Promise((resolve) => (release = () => void call(method as never, req as never).then(resolve as never)));
    }) as typeof api.call);
    const saving = advanced.set(true);
    await advanced.load(); // answers "off": the save is not in yet
    expect(advanced.on).toBe(true);
    release();
    await saving;
    expect(advanced.on).toBe(true);
    vi.restoreAllMocks();
    await advanced.set(false);
  });
});
