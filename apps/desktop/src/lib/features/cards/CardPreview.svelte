<script lang="ts">
  import type { RenderedSides } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import CardView from './CardView.svelte';
  import { Button, Repeat, SegmentedControl } from '$lib/ui';

  interface Props {
    cards: RenderedSides[];
    onopennote?: (id: string) => void;
  }
  let { cards, onopennote }: Props = $props();

  let index = $state(0);
  let showAnswer = $state(false);
  const current = $derived(cards[Math.min(index, cards.length - 1)]);

  $effect(() => {
    if (index >= cards.length) index = 0;
  });
</script>

<section class="preview" aria-label={t('cards.preview')}>
  <header>
    <h3>{t('cards.preview')}</h3>
    {#if cards.length > 1}
      <SegmentedControl
        size="sm"
        label={t('cards.preview')}
        options={cards.map((c, i) => ({ value: i, label: c.template_name }))}
        value={Math.min(index, cards.length - 1)}
        onchange={(i) => ((index = i), (showAnswer = false))}
      />
    {/if}
  </header>
  {#if current}
    <div class="card">
      <CardView sides={current} {showAnswer} {onopennote} />
    </div>
    <Button variant="secondary" size="sm" icon={Repeat} onclick={() => (showAnswer = !showAnswer)}>
      {showAnswer ? t('cards.show_question') : t('cards.show_answer')}
    </Button>
  {:else}
    <div class="card empty">
      <p>{t('cards.no_cards')}</p>
    </div>
  {/if}
</section>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    align-items: flex-start;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    justify-content: space-between;
    width: 100%;
  }
  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  .card {
    width: 100%;
  }
  .card.empty {
    box-sizing: border-box;
    min-height: 160px;
    padding: var(--space-6);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
  }
  .card.empty p {
    margin: 0;
    font-size: var(--text-sm);
  }
</style>
