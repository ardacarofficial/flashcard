<script lang="ts">
  import type { RenderedSides } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import CardView from './CardView.svelte';

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
    <strong>{t('cards.preview')}</strong>
    {#if cards.length > 1}
      <div class="tabs" role="tablist">
        {#each cards as c, i (c.template_key)}
          <button role="tab" aria-selected={i === index} class:active={i === index} onclick={() => ((index = i), (showAnswer = false))}>
            {c.template_name}
          </button>
        {/each}
      </div>
    {/if}
  </header>
  {#if current}
    <CardView sides={current} {showAnswer} {onopennote} />
    <button class="flip" onclick={() => (showAnswer = !showAnswer)}>
      {showAnswer ? t('cards.show_question') : t('cards.show_answer')}
    </button>
  {:else}
    <p class="muted">{t('cards.no_cards')}</p>
  {/if}
</section>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-2);
    align-items: center;
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-1);
  }
  .tabs button.active {
    background: var(--fc-selected);
  }
  .flip {
    align-self: flex-start;
  }
</style>
