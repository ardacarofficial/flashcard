<script lang="ts">
  /** Built-in starter decks (v1.0): add one as an ordinary deck (one undo step). */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Locale, StarterDeckDto } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import { BookOpen, Button, Card, Check, CircleCheck, EmptyState, Icon, Layers, Plus, SegmentedControl, Skeleton } from '$lib/ui';
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
    <SegmentedControl label={t('starters.language')} options={languages} bind:value={locale} onchange={() => void load()} />
  {/snippet}

  {#if notice}
    <p class="notice" role="status"><Icon icon={CircleCheck} size={16} /><span>{notice}</span></p>
  {/if}

  {#if failed}
    <LoadState state="error" retry={() => void load()} />
  {:else if starters === null}
    <ul class="grid" aria-hidden="true">
      {#each { length: 4 } as _, i (i)}
        <li><Skeleton shape="card" height="168px" /></li>
      {/each}
    </ul>
  {:else if starters.length === 0}
    <Card padding="lg"><EmptyState icon={Layers} title={t('starters.none')} /></Card>
  {:else}
    <ul class="grid">
      {#each starters as d (d.id)}
        {@const done = added.includes(d.id)}
        <Card as="li" padding="lg" class="starter">
          <div class="top">
            <span class="ic" aria-hidden="true"><Icon icon={BookOpen} size={20} /></span>
            <span class="count">{t('onboarding.notes', { count: d.note_count })}</span>
          </div>
          <h2 class="name">{d.name}</h2>
          {#if d.description}<p class="desc">{d.description}</p>{/if}
          <div class="act">
            <Button
              variant={done ? 'ghost' : 'secondary'}
              icon={done ? Check : Plus}
              loading={busy === d.id}
              disabled={busy !== null}
              onclick={() => void install(d)}
            >{done ? t('starters.added') : t('starters.install')}</Button>
          </div>
        </Card>
      {/each}
    </ul>
  {/if}
</SettingsPage>

<style>
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
    gap: var(--space-4);
  }
  .grid :global(.starter) {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-bottom: var(--space-1);
  }
  .ic {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .count {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
    padding: 2px var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--surface-2);
  }
  .name {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    color: var(--text-1);
  }
  .desc {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading);
    flex: 1;
  }
  .act {
    margin-top: var(--space-2);
  }
  .notice {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    margin: 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-control);
    background: var(--success-soft);
    color: var(--success);
    font-size: var(--text-sm);
  }
</style>
