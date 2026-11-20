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

<div class="confidence" role="group" aria-label={t('modes.confidence.question')}>
  <p class="muted">{t('modes.confidence.question')}</p>
  <div class="levels">
    {#each levels as l (l.level)}
      <button onclick={() => onrate(l.level)} data-rating={l.rating}>
        <small>{interval(l.interval_ms)}</small>
        {td(`modes.confidence.level.${l.level}`)}
        <kbd>{l.level}</kbd>
      </button>
    {/each}
  </div>
</div>

<style>
  .levels {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: var(--fc-space-2);
  }
  .levels button {
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  kbd {
    color: var(--fc-text-muted);
    font-size: 0.8em;
  }
  p {
    margin: 0 0 var(--fc-space-2);
  }
</style>
