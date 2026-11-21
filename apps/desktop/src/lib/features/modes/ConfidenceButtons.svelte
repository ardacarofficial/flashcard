<script lang="ts">
  /** The five "how sure was I?" buttons (keys 1–5); each shows the interval of its rating. */
  import type { ConfidenceLevelDto } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { intervalLabel } from '../study/format';

  let { levels, onrate }: { levels: ConfidenceLevelDto[]; onrate: (level: number) => void } = $props();

  function interval(ms: number): string {
    const l = intervalLabel(ms);
    return t(l.key, { n: l.n });
  }
</script>

<div class="confidence" role="group" aria-labelledby="fc-confidence-q">
  <p id="fc-confidence-q" class="q">{t('modes.confidence.question')}</p>
  <div class="levels">
    {#each levels as l (l.level)}
      <button type="button" class="level l{l.level}" onclick={() => onrate(l.level)} data-rating={l.rating} aria-keyshortcuts={String(l.level)}>
        <span class="name">{td(`modes.confidence.level.${l.level}`)}</span>
        <span class="meta"><span class="ivl num">{interval(l.interval_ms)}</span><span class="key num" aria-hidden="true">{l.level}</span></span>
      </button>
    {/each}
  </div>
</div>

<style>
  .confidence {
    width: 100%;
  }
  .q {
    margin: 0 0 var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .levels {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .level {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-height: 52px;
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), background var(--motion-fast) var(--ease);
  }
  .level:hover {
    border-color: var(--border-strong);
    background: var(--surface-2);
  }
  .level:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .meta {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
    color: var(--text-2);
  }
  .key {
    color: var(--text-2);
    font-size: var(--text-2xs);
  }
</style>
