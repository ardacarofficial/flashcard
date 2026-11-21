<!--
  A deck's thin New / Learning / Mastered bar with the mastered share in mono (v1.2 rows and
  cards). Numbers are the backend's (`decks.summaries`); `summary` null = still loading.
-->
<script lang="ts">
  import { formatPercent100 } from '$lib/format';
  import type { DeckSummary } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { ProgressBar } from '$lib/ui';

  interface Props {
    summary: DeckSummary | null;
    name: string;
    /** Spell the share out ("61% mastered") instead of the bare figure. */
    long?: boolean;
  }
  let { summary, name, long = false }: Props = $props();

  const pct = $derived(summary ? formatPercent100(summary.mastered_pct) : '');
</script>

<span class="mastery" class:long>
  {#if summary}
    <ProgressBar
      size="sm"
      label={t('library.mastery', { name })}
      max={Math.max(1, summary.new + summary.learning + summary.mastered)}
      segments={[
        { value: summary.mastered, label: t('deck_page.mastered'), tone: 'success' },
        { value: summary.learning, label: t('deck_page.learning'), tone: 'warning' },
        { value: summary.new, label: t('deck_page.new'), tone: 'neutral' },
      ]}
    />
    <span class="pct num" title={long ? undefined : t('library.mastered', { pct })}>{long ? t('library.mastered', { pct }) : pct}</span>
  {:else}
    <span class="placeholder" aria-hidden="true"></span>
  {/if}
</span>

<style>
  .mastery {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .mastery :global(.fc-progress) {
    flex: 1;
    min-width: 0;
  }
  .pct {
    flex: none;
    min-width: 4ch;
    color: var(--text-2);
    font-size: var(--text-xs);
    text-align: end;
    white-space: nowrap;
  }
  .long .pct {
    min-width: 0;
    font-family: inherit;
  }
  .placeholder {
    flex: 1;
    height: 4px;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
  }
</style>
