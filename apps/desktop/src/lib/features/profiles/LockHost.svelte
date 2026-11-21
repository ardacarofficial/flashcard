<script lang="ts">
  /**
   * App lock screen (v1.0, docs/02 §29). Covers the window while the backend says the app is
   * locked (at start, after the idle timeout, "lock now", the tray). While locked the IPC
   * bridge only lets the lock screen's methods through, so nothing behind it can load. User
   * input restarts the backend's idle timer (`lock.activity`, at most every 30 s).
   *
   * When the lock's hash is gone (`recovery_required`) the backend stays locked (fail
   * closed): instead of the PIN field the screen offers the explicit recovery, which removes
   * the lock after the user confirms (`lock.recover`).
   *
   * `flashcard://` links that arrive while locked are held by the shell and run after
   * unlocking; the screen lists them first and can drop them, so unlocking never runs a link
   * the user did not see.
   */
  import { onMount } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type { LockState, ProfileDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { defaultShell, DEEP_LINK_HELD_EVENT, type Shell } from '$lib/features/inbox/shell';
  import Avatar from '$lib/ui/Avatar.svelte';
  import Button from '$lib/ui/Button.svelte';
  import Checkbox from '$lib/ui/Checkbox.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import Input from '$lib/ui/Input.svelte';
  import { Lock, TriangleAlert } from '$lib/ui/icons';

  interface Props {
    /** Called after unlocking (the shell reloads what it could not load while locked). */
    onunlock?: () => void;
    /** Desktop shell (held links); tests pass a fake. */
    shell?: Shell;
    /** Told whenever the lock state changes: the shell mounts no screen (lazy or bundled) while locked. */
    onlockchange?: (locked: boolean) => void;
  }
  let { onunlock, shell = defaultShell(), onlockchange }: Props = $props();

  const ACTIVITY_EVERY_MS = 30_000;

  let lock = $state<LockState | null>(null);
  let secret = $state('');
  let error = $state<string | null>(null);
  let others = $state<ProfileDto[]>([]);
  let confirmRecovery = $state(false);
  /** Links held while locked; they run after unlock. */
  let held = $state<string[]>([]);
  let lastActivity = 0;
  let field = $state<HTMLInputElement | null>(null);

  const locked = $derived(lock?.available === true && lock.locked);
  $effect(() => onlockchange?.(locked));
  // The PIN field takes focus as soon as the lock screen shows.
  $effect(() => {
    if (locked && field) field.focus();
  });

  async function refresh() {
    try {
      const before = lock?.locked ?? false;
      lock = await api.call('lock.status', {});
      if (lock.locked) {
        const list = await api.call('profiles.list', {});
        others = list.profiles.filter((p) => !p.active);
        await refreshHeld();
      } else if (before) {
        onunlock?.();
      }
    } catch {
      lock = null;
    }
  }

  async function refreshHeld() {
    try {
      held = await shell.heldDeepLinks();
    } catch {
      held = [];
    }
  }

  async function discardHeld() {
    await shell.discardHeldDeepLinks().catch(() => {});
    await refreshHeld();
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
      } else if (a.details['reason'] === 'recovery_required') {
        await refresh();
      } else {
        error = td(a.messageKey ?? 'errors.internal.unexpected');
      }
    }
  }

  async function recover(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    try {
      lock = await api.call('lock.recover', {});
      confirmRecovery = false;
      if (!lock.locked) onunlock?.();
    } catch (err) {
      error = td((err as ApiError).messageKey ?? 'errors.internal.unexpected');
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
    const unlisten = shell.listen(DEEP_LINK_HELD_EVENT, () => void refreshHeld());
    window.addEventListener('keydown', activity, { passive: true });
    window.addEventListener('pointerdown', activity, { passive: true });
    return () => {
      void unsub.then((u) => u());
      void unlisten.then((u) => u());
      window.removeEventListener('keydown', activity);
      window.removeEventListener('pointerdown', activity);
    };
  });
</script>

{#if locked && lock}
  {@const profile = lock.profile_name ?? t('profiles.default_name')}
  <div class="lock" role="dialog" aria-modal="true" aria-labelledby="lock-title">
    <div class="panel">
      <form class="card" onsubmit={lock.recovery_required ? recover : unlock}>
        <span class="badge" class:warn={lock.recovery_required} aria-hidden="true">
          <Icon icon={lock.recovery_required ? TriangleAlert : Lock} size={24} />
        </span>
        <h1 id="lock-title">{t('lock.screen_title')}</h1>
        <p class="who">
          <Avatar name={profile} size="sm" />
          <span>{t('lock.screen_profile', { name: profile })}</span>
        </p>
        {#if lock.recovery_required}
          <div class="recovery">
            <h2>{t('lock.recovery_title')}</h2>
            <p>{t('lock.recovery_body')}</p>
            <Checkbox label={t('lock.recovery_confirm')} bind:checked={confirmRecovery} />
          </div>
          {#if error}<p class="error" role="alert">{error}</p>{/if}
          <Button variant="danger" type="submit" fullWidth disabled={!confirmRecovery}>{t('lock.recover')}</Button>
        {:else}
          <Input
            type="password"
            size="lg"
            autocomplete="current-password"
            inputmode={lock.kind === 'pin' ? 'numeric' : 'text'}
            label={t('lock.enter')}
            error={error}
            bind:value={secret}
            bind:element={field}
          />
          <Button variant="primary" size="lg" type="submit" fullWidth disabled={secret === ''}>{t('lock.unlock')}</Button>
        {/if}
      </form>
      {#if held.length > 0}
        <section class="card held" aria-labelledby="lock-held">
          <h2 id="lock-held">{t('lock.held_title')}</h2>
          <p class="muted">{t('lock.held_body')}</p>
          <ul>
            {#each held as url (url)}
              <li><code>{url}</code></li>
            {/each}
          </ul>
          <Button size="sm" onclick={() => void discardHeld()}>{t('lock.held_discard')}</Button>
        </section>
      {/if}
      {#if others.length > 0}
        <nav class="others" aria-labelledby="lock-others">
          <h2 id="lock-others">{t('lock.other_profile')}</h2>
          <ul>
            {#each others as p (p.id)}
              {@const name = p.name ?? t('profiles.default_name')}
              <li>
                <button type="button" class="other" aria-label={name} onclick={() => void open(p)}>
                  <Avatar {name} size="sm" />
                  <span>{name}</span>
                </button>
              </li>
            {/each}
          </ul>
        </nav>
      {/if}
    </div>
  </div>
{/if}

<style>
  .lock {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: grid;
    place-items: center;
    overflow: auto;
    padding: var(--space-6) var(--space-4);
    background: var(--surface-0);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: min(400px, 100%);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-8) var(--space-6) var(--space-6);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-1);
  }
  form.card {
    align-items: stretch;
    text-align: center;
  }
  .badge {
    align-self: center;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 56px;
    height: 56px;
    border-radius: var(--radius-pill);
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .badge.warn {
    background: var(--warning-soft);
    color: var(--warning);
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    line-height: var(--leading-tight);
  }
  h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .who {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    margin: calc(-1 * var(--space-2)) 0 var(--space-2);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  form.card :global(.fc-field) {
    text-align: start;
  }
  .recovery {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    text-align: start;
  }
  .recovery p {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
    line-height: var(--leading);
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .held {
    gap: var(--space-2);
    padding: var(--space-4) var(--space-6);
    align-items: flex-start;
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .held ul {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
    max-height: 10rem;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .held code {
    word-break: break-all;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .others {
    text-align: center;
  }
  .others h2 {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-2);
    margin-bottom: var(--space-2);
  }
  .others ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--space-2);
  }
  .other {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3) var(--space-1) var(--space-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }
  .other:hover {
    border-color: var(--border-strong);
  }
  .other:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
</style>
