/**
 * Dev-only mock of the `profiles` module (profiles + app lock). Minimal: the real rules
 * (Argon2id hashes, back-off, separate collections, the IPC gate) live in fc-app.
 */
import { menu, module, err, mockScenario, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { LockKind, LockState, ProfileDto } from '$lib/api/types';

const IDLE_CHOICES = [0, 1, 5, 15, 30, 60];

/** Test hook: simulates a removed keychain entry (the lock's hash is gone; fails closed). */
export const mockLockHooks = { loseHash: () => {} };

export default {
  modules: [{ ...module('profiles', [menu('profiles.screen', 'nav.profiles', '/profiles', 96)]), can_disable: false }],
  handlers: ({ now, id, emit }: MockEnv) => {
    const profiles: ProfileDto[] = [{ id: 'default', name: null, active: true, locked: false, created_at: 0 }];
    // The mock keeps the secret itself (the real app keeps only an Argon2id hash).
    let lock: { kind: LockKind; secret: string; idle: number } | null = null;
    let locked = false;
    if (mockScenario() === 'locked') {
      lock = { kind: 'pin', secret: '1234', idle: 15 };
      locked = true;
      profiles[0]!.locked = true;
    }
    let failures = 0;
    let hashLost = false;
    mockLockHooks.loseHash = () => {
      hashLost = true;
      locked = true;
      emit({ type: 'lock_changed', data: { locked: true } });
    };
    const state = (): LockState => ({
      available: true,
      enabled: lock !== null,
      kind: lock?.kind ?? null,
      locked,
      recovery_required: lock !== null && hashLost,
      idle_minutes: lock?.idle ?? 15,
      idle_choices: IDLE_CHOICES,
      failures,
      retry_after_ms: 0,
      secure_storage: true,
      profile_id: 'default',
      profile_name: null,
    });
    const check = (secret: string | null | undefined) => {
      if (!lock) return;
      if (secret !== lock.secret) {
        failures += 1;
        throw err('PERMISSION_LOCK_SECRET', 'errors.permission.lock_secret', 'wrong');
      }
      failures = 0;
    };
    const find = (pid: string) => {
      const p = profiles.find((x) => x.id === pid);
      if (!p) throw err('NOT_FOUND_PROFILE', 'errors.not_found.profile', pid);
      return p;
    };
    const named = (raw: string, except?: string) => {
      const name = raw.trim();
      if (!name) throw err('VALIDATION_PROFILE_NAME', 'errors.validation.profile_name', raw);
      if (profiles.some((p) => p.id !== except && p.name?.toLocaleLowerCase() === name.toLocaleLowerCase())) {
        throw err('CONFLICT_PROFILE_NAME', 'errors.conflict.profile_name', name);
      }
      return name;
    };
    return {
      'profiles.list': () => ({ available: true, profiles: structuredClone(profiles), active: 'default' }),
      'profiles.create': ({ name }) => {
        const p: ProfileDto = { id: id('profile'), name: named(name), active: false, locked: false, created_at: now() };
        profiles.push(p);
        return p;
      },
      'profiles.rename': ({ id: pid, name }) => {
        const p = find(pid);
        p.name = named(name, pid);
        return p;
      },
      'profiles.delete': ({ id: pid }) => {
        const p = find(pid);
        if (p.active || p.id === 'default') {
          throw err('CONFLICT_PROFILE_ACTIVE', 'errors.conflict.profile_active', pid);
        }
        profiles.splice(profiles.indexOf(p), 1);
        return {};
      },
      'profiles.switch': ({ id: pid }) => ({ profile: find(pid), restart_required: pid !== 'default' }),
      'lock.status': () => state(),
      'lock.set': ({ kind, secret, current, idle_minutes }) => {
        const ok = kind === 'pin' ? /^\d{4,12}$/.test(secret) : secret.trim().length >= 6;
        if (!ok) throw err('VALIDATION_LOCK_SECRET', 'errors.validation.lock_secret', 'shape');
        if (lock && current == null) throw err('CONFLICT_LOCK_STATE', 'errors.conflict.lock_state', 'current');
        check(current);
        lock = { kind, secret, idle: idle_minutes ?? lock?.idle ?? 15 };
        profiles[0]!.locked = true;
        return state();
      },
      'lock.configure': ({ idle_minutes }) => {
        if (!lock) throw err('CONFLICT_LOCK_STATE', 'errors.conflict.lock_state', 'not_set');
        lock.idle = idle_minutes;
        return state();
      },
      'lock.clear': ({ current }) => {
        if (!lock) throw err('CONFLICT_LOCK_STATE', 'errors.conflict.lock_state', 'not_set');
        check(current);
        lock = null;
        profiles[0]!.locked = false;
        return state();
      },
      'lock.unlock': ({ secret }) => {
        if (locked && hashLost) {
          const e = err('CONFLICT_LOCK_STATE', 'errors.conflict.lock_state', 'recovery_required');
          e.details = { reason: 'recovery_required' };
          throw e;
        }
        if (locked) {
          check(secret);
          locked = false;
          emit({ type: 'lock_changed', data: { locked: false } });
        }
        return state();
      },
      'lock.recover': () => {
        if (!lock || !hashLost) throw err('CONFLICT_LOCK_STATE', 'errors.conflict.lock_state', 'not_recoverable');
        lock = null;
        hashLost = false;
        profiles[0]!.locked = false;
        if (locked) {
          locked = false;
          emit({ type: 'lock_changed', data: { locked: false } });
        }
        return state();
      },
      'lock.lock': () => {
        if (!lock) throw err('CONFLICT_LOCK_STATE', 'errors.conflict.lock_state', 'not_set');
        if (!locked) {
          locked = true;
          emit({ type: 'lock_changed', data: { locked: true } });
        }
        return state();
      },
      'lock.activity': () => ({}),
    };
  },
} satisfies MockFeature;
