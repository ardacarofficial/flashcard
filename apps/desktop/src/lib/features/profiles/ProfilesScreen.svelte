<script lang="ts">
  /**
   * Profiles (separate collections) and the app lock (v1.0, docs/02 §28–§29). Opening another
   * profile restarts the app on its collection (the backend stops background work first).
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { LockKind, LockState, ProfileDto, ProfileList } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Avatar,
    Button,
    Chip,
    Dialog,
    EmptyState,
    Icon,
    Input,
    IconButton,
    Lock,
    LockOpen,
    Pencil,
    Plus,
    SegmentedControl,
    Select,
    Trash2,
    Users,
  } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let list = $state<ProfileList | null>(null);
  let lock = $state<LockState | null>(null);
  let failed = $state(false);
  let newName = $state('');
  let editing = $state<string | null>(null);
  let editName = $state('');
  let deleting = $state<ProfileDto | null>(null);
  let deleteOpen = $state(false);
  let deleteSecret = $state('');
  let switching = $state(false);

  let kind = $state<LockKind>('pin');
  let secret = $state('');
  let current = $state('');
  let clearSecret = $state('');
  let idle = $state(15);

  const nameOf = (p: ProfileDto) => p.name ?? t('profiles.default_name');
  const kinds = $derived([
    { value: 'pin' as LockKind, label: t('lock.kind.pin') },
    { value: 'password' as LockKind, label: t('lock.kind.password') },
  ]);
  const idleOptions = $derived(
    (lock?.idle_choices ?? []).map((m) => ({ value: m, label: m === 0 ? t('lock.idle_never') : t('lock.idle_minutes', { count: m }) })),
  );

  async function load() {
    const ok = await ctx.guarded(async () => {
      list = await api.call('profiles.list', {});
      lock = await api.call('lock.status', {});
      idle = lock.idle_minutes;
    });
    failed = !ok && list === null;
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

  function askDelete(p: ProfileDto) {
    deleting = p;
    deleteSecret = '';
    deleteOpen = true;
  }

  async function remove() {
    const p = deleting;
    deleteOpen = false;
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
    if (await ctx.guarded(() => api.call('lock.clear', { current: clearSecret }))) clearSecret = '';
    await load();
  }

  onMount(() => void load());
</script>

<SettingsPage title={t('nav.profiles')} subtitle={t('profiles.intro')}>
  {#if list === null}
    <SettingsSection title={t('profiles.heading')} icon={Users}>
      <LoadState state={failed ? 'error' : 'loading'} retry={() => void load()} />
    </SettingsSection>
  {:else if !list.available}
    <SettingsSection title={t('profiles.heading')} icon={Users}>
      <EmptyState size="sm" icon={Users} title={t('profiles.unavailable')} />
    </SettingsSection>
  {:else}
    <SettingsSection title={t('profiles.heading')} icon={Users} body="rows">
      {#if switching}<p class="status" role="status">{t('profiles.switching')}</p>{/if}
      <ul class="profiles">
        {#each list.profiles as p (p.id)}
          <li class="profile" class:current={p.active}>
            <Avatar size="sm" name={nameOf(p)} />
            {#if editing === p.id}
              <form
                class="rename"
                onsubmit={(e) => {
                  e.preventDefault();
                  void rename(p);
                }}
              >
                <Input size="sm" label={t('profiles.rename')} hideLabel bind:value={editName} />
                <Button size="sm" variant="primary" type="submit" disabled={editName.trim() === ''}>{t('profiles.save')}</Button>
                <Button size="sm" variant="ghost" onclick={() => (editing = null)}>{t('profiles.cancel')}</Button>
              </form>
            {:else}
              <div class="who">
                <span class="name">{nameOf(p)}</span>
                {#if p.active}<Chip size="sm" tone="accent" label={t('profiles.active')} />{/if}
                {#if p.locked}<Chip size="sm" icon={Lock} label={t('profiles.locked')} />{/if}
              </div>
              <div class="actions">
                <IconButton
                  icon={Pencil}
                  size="sm"
                  label={t('profiles.rename')}
                  onclick={() => ((editing = p.id), (editName = nameOf(p)))}
                />
                {#if !p.active && p.id !== 'default'}
                  <IconButton icon={Trash2} size="sm" label={t('profiles.delete')} onclick={() => askDelete(p)} />
                {/if}
                {#if !p.active}<Button size="sm" onclick={() => void open(p)}>{t('profiles.switch')}</Button>{/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      <form class="add" onsubmit={create}>
        <Input size="sm" icon={Plus} label={t('profiles.new_name')} hideLabel placeholder={t('profiles.new_name')} bind:value={newName} />
        <Button size="sm" type="submit" disabled={newName.trim() === ''}>{t('profiles.create')}</Button>
      </form>
    </SettingsSection>
  {/if}

  {#if lock?.available}
    <SettingsSection title={t('lock.heading')} icon={Lock} description={t('lock.intro')} body="rows">
      <div class="lock-status" class:on={lock.enabled}>
        <span class="lock-icon" aria-hidden="true">
          <Icon icon={lock.enabled ? Lock : LockOpen} size={16} />
        </span>
        <p>
          {#if !lock.enabled}{t('lock.off')}{:else if lock.kind === 'pin'}{t('lock.on_pin')}{:else}{t('lock.on_password')}{/if}
        </p>
        {#if lock.enabled}
          <Button size="sm" icon={Lock} onclick={() => void ctx.guarded(() => api.call('lock.lock', {}))}>{t('lock.lock_now')}</Button>
        {/if}
      </div>
      <FormRow label={t('lock.idle')} description={t('lock.idle_hint')}>
        <Select
          size="sm"
          label={t('lock.idle')}
          hideLabel
          options={idleOptions}
          bind:value={idle}
          onchange={() => lock?.enabled && void configure()}
        />
      </FormRow>
      <form onsubmit={setLock}>
        <FormRow label={t('lock.kind_label')} description={t('lock.kind_hint')}>
          <SegmentedControl size="sm" label={t('lock.kind_label')} options={kinds} bind:value={kind} />
        </FormRow>
        {#if lock.enabled}
          <FormRow label={t('lock.current_secret')} description={t('lock.current_hint')}>
            <Input size="sm" type="password" autocomplete="current-password" label={t('lock.current_secret')} hideLabel bind:value={current} />
          </FormRow>
        {/if}
        <FormRow label={t('lock.new_secret')}>
          <Input
            size="sm"
            type="password"
            autocomplete="new-password"
            inputmode={kind === 'pin' ? 'numeric' : 'text'}
            label={t('lock.new_secret')}
            hideLabel
            bind:value={secret}
          />
        </FormRow>
        <div class="lock-actions">
          <Button size="sm" type="submit" variant="primary" disabled={secret === ''}>{lock.enabled ? t('lock.change') : t('lock.set')}</Button>
        </div>
      </form>
      {#if lock.enabled}
        <div class="notes">
          <p>{lock.secure_storage ? t('lock.storage_secure') : t('lock.storage_file')}</p>
          <p>{t('lock.scope_note')}</p>
        </div>
      {/if}
    </SettingsSection>

    {#if lock.enabled}
      <SettingsSection title={t('lock.danger_heading')} tone="danger" body="rows">
        <FormRow label={t('lock.clear')} description={t('lock.clear_desc')}>
          <Input size="sm" type="password" autocomplete="current-password" label={t('lock.current_secret')} hideLabel placeholder={t('lock.current_secret')} bind:value={clearSecret} />
          <Button size="sm" variant="danger" disabled={clearSecret === ''} onclick={() => void clear()}>{t('lock.clear')}</Button>
        </FormRow>
      </SettingsSection>
    {/if}
  {/if}
</SettingsPage>

<Dialog
  bind:open={deleteOpen}
  role="alertdialog"
  size="sm"
  title={t('profiles.delete_title')}
  description={deleting ? t('profiles.confirm_delete', { name: nameOf(deleting) }) : undefined}
  onclose={() => (deleting = null)}
>
  {#if deleting?.locked}
    <Input type="password" label={t('profiles.delete_secret')} bind:value={deleteSecret} />
  {/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={() => ((deleteOpen = false), (deleting = null))}>{t('profiles.cancel')}</Button>
    <Button variant="danger" icon={Trash2} disabled={!!deleting?.locked && deleteSecret === ''} onclick={() => void remove()}>
      {t('profiles.delete')}
    </Button>
  {/snippet}
</Dialog>

<style>
  .status {
    margin: 0;
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .profiles {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .profile {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 44px;
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .name {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
    overflow-wrap: anywhere;
  }
  .actions,
  .rename {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }
  .actions {
    flex: none;
  }
  .rename {
    flex: 1;
    gap: var(--space-2);
  }
  .rename :global(.fc-field) {
    flex: 1;
  }
  .add {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    padding-top: var(--space-3);
  }
  .add :global(.fc-field) {
    flex: 1;
    max-width: 22rem;
  }
  .lock-status {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: 44px;
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .lock-status.on .lock-icon {
    color: var(--success);
  }
  .lock-status p {
    flex: 1;
    margin: 0;
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  .lock-icon {
    display: inline-flex;
  }
  .lock-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    flex-wrap: wrap;
    padding-top: var(--space-3);
  }
  .notes {
    padding-top: var(--space-3);
    color: var(--text-2);
    font-size: var(--text-xs);
    line-height: var(--leading);
  }
  .notes p {
    margin: 0 0 var(--space-1);
  }
</style>
