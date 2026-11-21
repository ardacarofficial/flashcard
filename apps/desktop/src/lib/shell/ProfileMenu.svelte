<!-- Sidebar profile avatar: a menu with "Switch profile" (opens Profiles) and "Lock now" (when a lock is set). -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { LockState } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import Avatar from '$lib/ui/Avatar.svelte';
  import Menu from '$lib/ui/Menu.svelte';
  import { Lock, Users } from '$lib/ui/icons';
  import type { MenuItem } from '$lib/ui/types';

  interface Props {
    /** Opens the Profiles page; `null` when the profiles module is off. */
    openProfiles: (() => void) | null;
    guarded: (fn: () => Promise<unknown>) => Promise<boolean>;
  }

  let { openProfiles, guarded }: Props = $props();
  let lock = $state<LockState | null>(null);

  const name = $derived(lock?.profile_name ?? t('shell.profile.default_name'));
  const items = $derived.by((): MenuItem[] => {
    const out: MenuItem[] = [];
    const go = openProfiles;
    if (go) out.push({ id: 'switch', label: t('shell.profile.switch'), icon: Users, onselect: go });
    if (lock?.enabled) {
      out.push({ id: 'lock', label: t('shell.profile.lock'), icon: Lock, onselect: () => void guarded(() => api.call('lock.lock', {})) });
    }
    return out;
  });

  async function load() {
    try {
      lock = await api.call('lock.status', {});
    } catch {
      lock = null; // profiles module off / locked: default name, no lock item
    }
  }

  onMount(() => {
    void load();
    const unsub = api.on(['lock_changed', 'profile_', 'module_toggled'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

{#if items.length > 0}
  <li class="profile">
    <Menu {items} label={t('shell.profile.menu', { name })} align="start" placement="top">
      {#snippet trigger()}<span class="who"><Avatar {name} size="sm" /><span class="name">{name}</span></span>{/snippet}
    </Menu>
  </li>
{/if}

<style>
  .profile {
    display: flex;
    padding-top: 2px;
  }
  .profile :global(.fc-menu) {
    width: 100%;
  }
  .profile :global(.trigger.custom) {
    justify-content: flex-start;
    width: 100%;
    height: var(--nav-row-h);
    padding: 0 6px;
    border-radius: var(--radius-control);
  }
  .who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .name {
    overflow: hidden;
    color: var(--text-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 1023.98px) {
    .name {
      display: none;
    }
    .profile :global(.trigger.custom) {
      justify-content: center;
      width: 36px;
      padding: 0;
    }
  }
</style>
