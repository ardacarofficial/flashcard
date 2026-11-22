<!--
  Model picker (v1.3): a search box over the provider's model list (fetched from the provider,
  cached on it) with a free-text fallback — any typed id can be used, the list is only help.
  ↑/↓ move through the matches, Enter picks the highlighted one; the typed text itself is the
  last option (the only one when nothing matches).
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Icon, Input, RefreshCw, Search, Spinner, TriangleAlert, Check } from '$lib/ui';
  import { filterModels } from './ai';

  interface Props {
    models: readonly string[];
    /** The chosen model (`""` = none). */
    value: string;
    onpick: (model: string) => void;
    /** Fetches the list again; `null` hides the button. */
    onrefresh?: (() => void) | null;
    loading?: boolean;
    /** Why the list could not be fetched (shown; typing a name still works). */
    error?: string | null;
  }

  let { models, value, onpick, onrefresh = null, loading = false, error = null }: Props = $props();

  const uid = $props.id();
  let query = $state('');
  let active = $state(0);
  /** At most this many rows are drawn (lists of hundreds stay fast; search narrows them). */
  const SHOWN = 60;

  const matches = $derived(filterModels(models, query));
  const shown = $derived(matches.slice(0, SHOWN));
  const typed = $derived(query.trim());
  /** The typed text is offered as-is when it is not exactly in the list. */
  const offerTyped = $derived(typed !== '' && !models.includes(typed));

  $effect(() => {
    void query;
    active = 0;
  });

  function pick(m: string) {
    onpick(m);
    query = '';
  }

  function onkeydown(e: KeyboardEvent) {
    const count = shown.length + (offerTyped ? 1 : 0);
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      active = count ? (active + 1) % count : 0;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      active = count ? (active - 1 + count) % count : 0;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const m = active < shown.length ? shown[active] : offerTyped ? typed : undefined;
      if (m) pick(m);
    }
  }

  const optionId = (i: number) => `models-${uid}-${i}`;
</script>

<div class="picker">
  <div class="search">
    <Input
      type="search"
      icon={Search}
      label={t('ai.setup.model_search')}
      placeholder={t('ai.setup.model_placeholder')}
      bind:value={query}
      {onkeydown}
      autocomplete="off"
      role="combobox"
      aria-controls={`models-${uid}`}
      aria-expanded="true"
      aria-activedescendant={shown.length || offerTyped ? optionId(active) : undefined}
    />
    {#if onrefresh}
      <Button size="md" variant="ghost" icon={RefreshCw} disabled={loading} onclick={onrefresh}>
        {t('ai.setup.refresh_models')}
      </Button>
    {/if}
  </div>

  <p class="meta" aria-live="polite">
    {#if loading}
      <Spinner size={14} /> {t('ai.setup.models_loading')}
    {:else if error}
      <Icon icon={TriangleAlert} size={14} /> {t('ai.setup.models_failed', { reason: error })}
    {:else}
      {t('ai.setup.models_count', { count: models.length })}
    {/if}
    {#if value}<span class="chosen">{t('ai.setup.chosen', { model: value })}</span>{/if}
  </p>

  <ul class="list" id={`models-${uid}`} role="listbox" aria-label={t('ai.setup.model_search')}>
    {#each shown as m, i (m)}
      <li id={optionId(i)} role="option" aria-selected={m === value} class:active={active === i}>
        <button type="button" tabindex="-1" class="model" onclick={() => pick(m)}>
          <span>{m}</span>
          {#if m === value}<Icon icon={Check} size={14} />{/if}
        </button>
      </li>
    {/each}
    {#if offerTyped}
      <li id={optionId(shown.length)} role="option" aria-selected={false} class:active={active === shown.length}>
        <button type="button" tabindex="-1" onclick={() => pick(typed)}>
          {t('ai.setup.use_typed', { model: typed })}
        </button>
      </li>
    {/if}
    {#if !offerTyped && shown.length === 0 && !loading}
      <li class="empty" role="presentation">{t('ai.setup.no_match')}</li>
    {/if}
    {#if matches.length > SHOWN}
      <li class="empty" role="presentation">{t('ai.setup.more_models', { count: matches.length - SHOWN })}</li>
    {/if}
  </ul>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .search {
    display: flex;
    gap: var(--space-2);
    align-items: flex-end;
  }
  .search :global(.fc-field) {
    flex: 1;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .chosen::before {
    content: '·';
    margin-inline-end: var(--space-1);
  }
  .chosen {
    font-family: var(--font-mono);
    color: var(--text-1);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: var(--space-1) 0;
    max-height: 15rem;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-0);
  }
  .list button {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    min-height: var(--row-h);
    padding: 0 var(--space-3);
    border: 0;
    background: none;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    text-align: start;
    cursor: pointer;
  }
  .list .model {
    font-family: var(--font-mono);
  }
  .list li.active button,
  .list button:hover {
    background: var(--surface-2);
  }
  .list li[aria-selected='true'] button {
    color: var(--accent-text);
  }
  .empty {
    padding: var(--space-2) var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
</style>
