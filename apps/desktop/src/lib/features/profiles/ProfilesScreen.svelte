<script lang="ts">
  /**
   * Profiles (separate collections) and the app lock (v1.0, docs/02 §28–§29). Opening another
   * profile restarts the app on its collection (the backend stops background work first).
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { LockKind, LockState, ProfileDto, ProfileList } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<ProfileList | null>(null);
  let lock = $state<LockState | null>(null);
  let newName = $state('');
  let editing = $state<string | null>(null);
  let editName = $state('');
  let deleting = $state<ProfileDto | null>(null);
  let deleteSecret = $state('');
  let switching = $state(false);

  let kind = $state<LockKind>('pin');
  let secret = $state('');
  let current = $state('');
  let idle = $state(15);

  const nameOf = (p: ProfileDto) => p.name ?? t('profiles.default_name');

  async function load() {
    await ctx.guarded(async () => {
      list = await api.call('profiles.list', {});
      lock = await api.call('lock.status', {});
      idle = lock.idle_minutes;
    });
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    if (await ctx.guarded(() => api.call('profiles.create', { name: newName }))) newName = '';
    await load();
  }

  async function rename(p: ProfileDto) {
    if (await ctx.guarded(() => api.call('profiles.rename', { id: p.id, name: editName }))) editing = null;
    await load();
  }

  async function remove() {
    const p = deleting;
    deleting = null;
    if (!p) return;
    await ctx.guarded(() => api.call('profiles.delete', { id: p.id, secret: p.locked ? deleteSecret : null }));
    deleteSecret = '';
    await load();
  }

  async function open(p: ProfileDto) {
    await ctx.guarded(async () => {
      const r = await api.call('profiles.switch', { id: p.id });
      switching = r.restart_required;
    });
  }

  async function setLock(e: SubmitEvent) {
    e.preventDefault();
    const ok = await ctx.guarded(async () => {
      lock = await api.call('lock.set', {
        kind,
        secret,
        current: lock?.enabled ? current : null,
        idle_minutes: idle,
      });
    });
    if (ok) {
      secret = '';
      current = '';
    }
    await load();
  }

  async function configure() {
    await ctx.guarded(async () => {
      lock = await api.call('lock.configure', { idle_minutes: idle });
    });
  }

  async function clear() {
    if (await ctx.guarded(() => api.call('lock.clear', { current }))) current = '';
    await load();
  }

  onMount(() => void load());
</script>

<section>
  <h1>{t('profiles.heading')}</h1>
  {#if list && !list.available}
    <p class="muted">{t('profiles.unavailable')}</p>
  {:else if list}
    <p class="muted">{t('profiles.intro')}</p>
    {#if switching}<p role="status">{t('profiles.switching')}</p>{/if}
    <ul class="profiles">
      {#each list.profiles as p (p.id)}
        <li>
          {#if editing === p.id}
            <input aria-label={t('profiles.rename')} bind:value={editName} />
            <button onclick={() => void rename(p)}>{t('profiles.save')}</button>
            <button onclick={() => (editing = null)}>{t('profiles.cancel')}</button>
          {:else}
            <strong>{nameOf(p)}</strong>
            {#if p.active}<span class="badge">{t('profiles.active')}</span>{/if}
            {#if p.locked}<span class="muted">{t('profiles.locked')}</span>{/if}
            <span class="spacer"></span>
            {#if !p.active}<button onclick={() => void open(p)}>{t('profiles.switch')}</button>{/if}
            <button onclick={() => ((editing = p.id), (editName = nameOf(p)))}>{t('profiles.rename')}</button>
            {#if !p.active && p.id !== 'default'}
              {#if p.locked}
                <input type="password" aria-label={t('profiles.delete_secret')} placeholder={t('profiles.delete_secret')} bind:value={deleteSecret} />
              {/if}
              <button class="danger" onclick={() => (deleting = p)}>{t('profiles.delete')}</button>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
    <form class="row" onsubmit={create}>
      <input aria-label={t('profiles.new_name')} placeholder={t('profiles.new_name')} bind:value={newName} />
      <button type="submit" disabled={newName.trim() === ''}>{t('profiles.create')}</button>
    </form>
  {/if}

  {#if lock?.available}
    <h2>{t('lock.heading')}</h2>
    <p class="muted">{t('lock.intro')}</p>
    <p>
      {#if !lock.enabled}{t('lock.off')}{:else if lock.kind === 'pin'}{t('lock.on_pin')}{:else}{t('lock.on_password')}{/if}
    </p>
    <form class="lock" onsubmit={setLock}>
      <label>
        {t('lock.idle')}
        <select bind:value={idle} onchange={() => lock?.enabled && void configure()}>
          {#each lock.idle_choices as m (m)}
            <option value={m}>{m === 0 ? t('lock.idle_never') : t('lock.idle_minutes', { count: m })}</option>
          {/each}
        </select>
      </label>
      {#if lock.enabled}
        <label>
          {t('lock.current_secret')}
          <input type="password" autocomplete="current-password" bind:value={current} />
        </label>
      {/if}
      <label>
        {t('lock.new_secret')}
        <span class="row">
          <select aria-label={t('lock.new_secret')} bind:value={kind}>
            <option value="pin">{t('lock.kind.pin')}</option>
            <option value="password">{t('lock.kind.password')}</option>
          </select>
          <input
            type="password"
            autocomplete="new-password"
            inputmode={kind === 'pin' ? 'numeric' : 'text'}
            aria-label={t('lock.new_secret')}
            bind:value={secret}
          />
        </span>
      </label>
      <div class="row">
        <button type="submit" disabled={secret === ''}>{lock.enabled ? t('lock.change') : t('lock.set')}</button>
        {#if lock.enabled}
          <button type="button" onclick={() => void ctx.guarded(() => api.call('lock.lock', {}))}>{t('lock.lock_now')}</button>
          <button type="button" class="danger" disabled={current === ''} onclick={() => void clear()}>{t('lock.clear')}</button>
        {/if}
      </div>
    </form>
    {#if lock.enabled}
      <p class="muted">{lock.secure_storage ? t('lock.storage_secure') : t('lock.storage_file')}</p>
    {/if}
  {/if}
</section>

{#if deleting}
  <ConfirmDialog
    message={t('profiles.confirm_delete', { name: nameOf(deleting) })}
    confirmLabel={t('profiles.delete')}
    onconfirm={() => void remove()}
    oncancel={() => (deleting = null)}
  />
{/if}

<style>
  .profiles {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  .profiles li,
  .row {
    display: flex;
    gap: var(--fc-space-2);
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
  .badge {
    background: var(--fc-selected);
    border-radius: var(--fc-radius);
    padding: 0 var(--fc-space-2);
  }
  .lock {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    max-width: 420px;
  }
  .lock label {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-1);
  }
</style>
