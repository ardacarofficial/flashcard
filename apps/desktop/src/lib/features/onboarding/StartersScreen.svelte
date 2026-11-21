<script lang="ts">
  /**
   * Built-in starter decks (v1.0): add one as an ordinary deck (one undo step). v1.2: a dense
   * list (name and what it is, card count, Add) instead of a grid of big cards.
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Locale, StarterDeckDto } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import { BookOpen, Button, Check, CircleCheck, EmptyState, Icon, Layers, Plus, SegmentedControl, Skeleton } from '$lib/ui';
  import { LoadState, SettingsPage } from '../core/kit';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let locale = $state<Locale>(i18n.locale);
  let starters = $state<StarterDeckDto[] | null>(null);
  let failed = $state(false);
  let notice = $state<string | null>(null);
  let busy = $state<string | null>(null);
  /** Starters added in this visit (the button turns into "Added"; adding again is still allowed). */
  let added = $state<string[]>([]);

  const languages = $derived([
    { value: 'tr' as Locale, label: t('starters.lang.tr') },
    { value: 'en' as Locale, label: t('starters.lang.en') },
  ]);

  async function load() {
    failed = false;
    starters = null;
    const ok = await ctx.guarded(async () => {
      starters = (await api.call('starter.list', { locale })).starters;
    });
    if (!ok) failed = true;
  }

  async function install(d: StarterDeckDto) {
    notice = null;
    busy = d.id;
    await ctx.guarded(async () => {
      const r = await api.call('starter.install', { id: d.id, parent_id: null, locale });
      notice = t('starters.installed', { name: d.name, count: r.notes_created });
      added = [...added, d.id];
    });
    busy = null;
  }

  onMount(() => void load());
</script>

<SettingsPage title={t('starters.heading')} subtitle={t('starters.intro')} width="wide">
  {#snippet actions()}
    <SegmentedControl size="sm" label={t('starters.language')} options={languages} bind:value={locale} onchange={() => void load()} />
  {/snippet}

  {#if notice}
    <p class="notice" role="status"><Icon icon={CircleCheck} size={16} /><span>{notice}</span></p>
  {/if}

  {#if failed}
    <LoadState state="error" retry={() => void load()} />
  {:else if starters === null}
    <ul class="list" aria-hidden="true">
      {#each { length: 4 } as _, i (i)}
        <li class="row"><Skeleton lines={2} /></li>
      {/each}
    </ul>
  {:else if starters.length === 0}
    <EmptyState icon={Layers} title={t('starters.none')} />
  {:else}
    <ul class="list">
      {#each starters as d (d.id)}
        {@const done = added.includes(d.id)}
        <li class="row">
          <span class="ic" aria-hidden="true"><Icon icon={BookOpen} size={16} /></span>
          <div class="text">
            <h2 class="name">{d.name}</h2>
            {#if d.description}<p class="desc">{d.description}</p>{/if}
          </div>
          <span class="count num">{t('onboarding.notes', { count: d.note_count })}</span>
          <Button
            size="sm"
            variant={done ? 'ghost' : 'secondary'}
            icon={done ? Check : Plus}
            loading={busy === d.id}
            disabled={busy !== null}
            onclick={() => void install(d)}
          >{done ? t('starters.added') : t('starters.install')}</Button>
        </li>
      {/each}
    </ul>
  {/if}
</SettingsPage>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr) auto 96px;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--row-h);
    padding: 3px var(--space-2) 3px var(--space-3);
  }
  .row + .row {
    border-top: 1px solid var(--border);
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row :global(.fc-btn) {
    justify-self: end;
  }
  .ic {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
  }
  .text {
    min-width: 0;
  }
  .name {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .desc {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .count {
    font-size: var(--text-xs);
    color: var(--text-2);
    white-space: nowrap;
  }
  .notice {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin: 0;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    background: var(--success-soft);
    color: var(--success);
    font-size: var(--text-sm);
  }
</style>
