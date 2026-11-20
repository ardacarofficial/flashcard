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
      {#snippet trigger()}<Avatar {name} size="md" />{/snippet}
    </Menu>
  </li>
{/if}

<style>
  .profile {
    display: flex;
    padding: var(--space-2) var(--space-1) 0;
  }
</style>
