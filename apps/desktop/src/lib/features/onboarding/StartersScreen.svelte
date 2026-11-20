<script lang="ts">
  /** Built-in starter decks (v1.0): add one as an ordinary deck (one undo step). */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { Locale, StarterDeckDto } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let locale = $state<Locale>(i18n.locale);
  let starters = $state<StarterDeckDto[]>([]);
  let notice = $state<string | null>(null);

  async function load() {
    await ctx.guarded(async () => {
      starters = (await api.call('starter.list', { locale })).starters;
    });
  }

  async function install(d: StarterDeckDto) {
    notice = null;
    await ctx.guarded(async () => {
      const r = await api.call('starter.install', { id: d.id, parent_id: null, locale });
      notice = t('starters.installed', { name: d.name, count: r.notes_created });
    });
  }

  onMount(() => void load());
</script>

<section>
  <h1>{t('starters.heading')}</h1>
  <p class="muted">{t('starters.intro')}</p>
  <label>
    {t('starters.language')}
    <select bind:value={locale} onchange={() => void load()}>
      <option value="tr">{t('starters.lang.tr')}</option>
      <option value="en">{t('starters.lang.en')}</option>
    </select>
  </label>
  {#if notice}<p role="status">{notice}</p>{/if}
  <ul class="starters">
    {#each starters as d (d.id)}
      <li>
        <div>
          <strong>{d.name}</strong>
          <span class="muted">{t('onboarding.notes', { count: d.note_count })}</span>
          {#if d.description}<p class="muted">{d.description}</p>{/if}
        </div>
        <button onclick={() => void install(d)}>{t('starters.install')}</button>
      </li>
    {/each}
  </ul>
</section>

<style>
  .starters {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  .starters li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--fc-space-4);
    padding: var(--fc-space-2) 0;
    border-bottom: var(--fc-border-width) solid var(--fc-border);
  }
  .starters p {
    margin: var(--fc-space-1) 0 0;
  }
</style>
