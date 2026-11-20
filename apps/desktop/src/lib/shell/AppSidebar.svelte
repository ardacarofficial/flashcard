<!--
  The app sidebar (docs/11 §2): the 5 sections (Create highlighted), then Inbox (count badge
  when items wait), Settings and the profile menu. Narrow (< 1024 px): icons only.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import NavItem from '$lib/ui/NavItem.svelte';
  import Sidebar from '$lib/ui/Sidebar.svelte';
  import { Settings, iconFor, Inbox } from '$lib/ui/icons';
  import type { Navigation, SectionId } from './navigation';

  interface Props {
    nav: Navigation;
    /** Highlighted item: a section id, `inbox` or `settings`. */
    current: SectionId | 'inbox' | 'settings' | null;
    inboxCount: number;
    openSection: (id: SectionId) => void;
    openInbox: () => void;
    openSettings: () => void;
    /** Profile menu (`<li>`). */
    profile?: Snippet;
    /** Extra panels under the sections (plugin sidebar panels). */
    panels?: Snippet;
  }

  let { nav, current, inboxCount, openSection, openInbox, openSettings, profile, panels }: Props = $props();
</script>

<Sidebar label={t('shell.nav_label')}>
  {#snippet header()}
    <div class="brand">
      <span class="mark" aria-hidden="true"></span>
      <span class="name">{t('app.title')}</span>
    </div>
  {/snippet}
  {#each nav.sections as s (s.id)}
    <NavItem
      label={t(s.label_key)}
      icon={iconFor(s.icon)}
      active={current === s.id}
      accent={s.id === 'create'}
      onclick={() => openSection(s.id)}
    />
  {/each}
  {#if panels}<li class="panels">{@render panels()}</li>{/if}
  {#snippet footer()}
    {#if nav.inbox}
      <NavItem
        label={t('shell.inbox')}
        icon={Inbox}
        active={current === 'inbox'}
        badge={inboxCount}
        badgeLabel={t('shell.inbox_count', { count: inboxCount })}
        onclick={openInbox}
      />
    {/if}
    {#if nav.settings.length > 0}
      <NavItem label={t('shell.settings')} icon={Settings} active={current === 'settings'} onclick={openSettings} />
    {/if}
    {#if profile}{@render profile()}{/if}
  {/snippet}
</Sidebar>

<style>
  .brand {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-md);
    font-weight: var(--weight-bold);
    letter-spacing: -0.01em;
  }
  .mark {
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: 7px;
    background: var(--accent);
    box-shadow: inset -6px -6px 0 color-mix(in srgb, var(--accent-fg) 22%, transparent);
  }
  .panels {
    margin-top: var(--space-4);
    font-size: var(--text-sm);
  }
  @media (max-width: 1023.98px) {
    .name,
    .panels {
      display: none;
    }
  }
</style>
