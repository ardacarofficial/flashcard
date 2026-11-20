<script lang="ts">
  /**
   * App lock screen (v1.0, docs/02 §29). Covers the window while the backend says the app is
   * locked (at start, after the idle timeout, "lock now", the tray). While locked the IPC
   * bridge only lets the lock screen's methods through, so nothing behind it can load. User
   * input restarts the backend's idle timer (`lock.activity`, at most every 30 s).
   */
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { LockState, ProfileDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';

  interface Props {
    /** Called after unlocking (the shell reloads what it could not load while locked). */
    onunlock?: () => void;
  }
  let { onunlock }: Props = $props();

  const ACTIVITY_EVERY_MS = 30_000;

  let lock = $state<LockState | null>(null);
  let secret = $state('');
  let error = $state<string | null>(null);
  let others = $state<ProfileDto[]>([]);
  let lastActivity = 0;

  const locked = $derived(lock?.available === true && lock.locked);

  async function refresh() {
    try {
      const before = lock?.locked ?? false;
      lock = await api.call('lock.status', {});
      if (lock.locked) {
        const list = await api.call('profiles.list', {});
        others = list.profiles.filter((p) => !p.active);
      } else if (before) {
        onunlock?.();
      }
    } catch {
      lock = null;
    }
  }

  async function unlock(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    try {
      lock = await api.call('lock.unlock', { secret });
      secret = '';
      if (!lock.locked) onunlock?.();
    } catch (err) {
      const a = err as ApiError;
      secret = '';
      if (a.code === 'CONFLICT_LOCK_RETRY') {
        const ms = Number(a.details['retry_after_ms'] ?? 0);
        error = t('lock.wait', { seconds: Math.max(1, Math.ceil(ms / 1000)) });
      } else if (a.code === 'PERMISSION_LOCK_SECRET') {
        error = t('lock.wrong');
      } else {
        error = td(a.messageKey ?? 'errors.internal.unexpected');
      }
    }
  }

  async function open(p: ProfileDto) {
    try {
      await api.call('profiles.switch', { id: p.id });
    } catch (err) {
      error = td((err as ApiError).messageKey ?? 'errors.internal.unexpected');
    }
  }

  function activity() {
    if (!lock?.enabled || lock.locked) return;
    const now = Date.now();
    if (now - lastActivity < ACTIVITY_EVERY_MS) return;
    lastActivity = now;
    void api.call('lock.activity', {}).catch(() => {});
  }

  onMount(() => {
    void refresh();
    const unsub = api.on('lock_changed', () => void refresh());
    window.addEventListener('keydown', activity, { passive: true });
    window.addEventListener('pointerdown', activity, { passive: true });
    return () => {
      void unsub.then((u) => u());
      window.removeEventListener('keydown', activity);
      window.removeEventListener('pointerdown', activity);
    };
  });
</script>

{#if locked && lock}
  <div class="lock" role="dialog" aria-modal="true" aria-labelledby="lock-title">
    <form onsubmit={unlock}>
      <h1 id="lock-title">{t('lock.screen_title')}</h1>
      <p class="muted">{t('lock.screen_profile', { name: lock.profile_name ?? t('profiles.default_name') })}</p>
      <label>
        {t('lock.enter')}
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="password"
          autocomplete="current-password"
          inputmode={lock.kind === 'pin' ? 'numeric' : 'text'}
          bind:value={secret}
          autofocus
        />
      </label>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <button class="primary" type="submit" disabled={secret === ''}>{t('lock.unlock')}</button>
      {#if others.length > 0}
        <h2>{t('lock.other_profile')}</h2>
        <ul>
          {#each others as p (p.id)}
            <li><button type="button" onclick={() => void open(p)}>{p.name ?? t('profiles.default_name')}</button></li>
          {/each}
        </ul>
      {/if}
    </form>
  </div>
{/if}

<style>
  .lock {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: var(--fc-bg);
    z-index: 50;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    width: min(360px, 90vw);
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
  ul {
    list-style: none;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-2);
  }
</style>
