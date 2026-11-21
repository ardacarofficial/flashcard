<!--
  The app sidebar (docs/11 §2, v1.2 brief "Shell"): 216 px, the sections as compact rows
  (Create is an ordinary item: adding a card is the top bar's "Add card"), then Inbox (mono
  count when items wait), Settings and the profile menu. Narrow (< 1024 px): icons only.
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
      <img class="mark" src="/favicon.svg" alt="" aria-hidden="true" />
      <span class="name">{t('app.title')}</span>
    </div>
  {/snippet}
  {#each nav.sections as s (s.id)}
    <NavItem
      label={t(s.label_key)}
      icon={iconFor(s.icon)}
      active={current === s.id}
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
    min-width: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    letter-spacing: -0.01em;
  }
  .mark {
    width: 20px;
    height: 20px;
    flex: none;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
