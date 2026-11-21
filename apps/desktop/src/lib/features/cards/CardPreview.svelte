<!--
  Live preview of a note's cards (note editor, note types): the current card as a small
  flashcard (centred text, the study card's radius), a template switch when the note makes
  several cards, and a front/back toggle.
-->
<script lang="ts">
  import type { RenderedSides } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import CardView from './CardView.svelte';
  import { Button, Repeat, SegmentedControl } from '$lib/ui';

  interface Props {
    cards: RenderedSides[];
    onopennote?: (id: string) => void;
    /** Inside a section that already says "Preview": no heading of its own. */
    hideTitle?: boolean;
  }
  let { cards, onopennote, hideTitle = false }: Props = $props();

  let index = $state(0);
  let showAnswer = $state(false);
  const current = $derived(cards[Math.min(index, cards.length - 1)]);

  $effect(() => {
    if (index >= cards.length) index = 0;
  });
</script>

<section class="preview" aria-label={t('cards.preview')}>
  {#if !hideTitle}
    <header>
      <h3>{t('cards.preview')}</h3>
    </header>
  {/if}
  {#if cards.length > 1}
    <SegmentedControl
      size="sm"
      label={t('cards.preview')}
      options={cards.map((c, i) => ({ value: i, label: c.template_name }))}
      value={Math.min(index, cards.length - 1)}
      onchange={(i) => ((index = i), (showAnswer = false))}
    />
  {/if}
  {#if current}
    <div class="frame" class:answer={showAnswer}>
      <CardView sides={current} {showAnswer} {onopennote} />
    </div>
    <Button variant="ghost" size="sm" icon={Repeat} onclick={() => (showAnswer = !showAnswer)}>
      {showAnswer ? t('cards.show_question') : t('cards.show_answer')}
    </Button>
  {:else}
    <div class="frame empty">
      <p>{t('cards.no_cards')}</p>
    </div>
  {/if}
</section>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    align-items: flex-start;
    min-width: 0;
  }
  header {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    justify-content: space-between;
    width: 100%;
  }
  h3 {
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .frame {
    box-sizing: border-box;
    width: 100%;
  }
  .frame :global(.card) {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-height: 140px;
    padding: var(--space-5) var(--space-4);
    border-radius: var(--radius-flashcard);
    font-size: var(--text-xl);
    line-height: var(--leading-tight);
    text-align: center;
  }
  .frame.answer :global(.card) {
    font-size: var(--text-lg);
  }
  .frame.empty {
    min-height: 140px;
    padding: var(--space-6);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-flashcard);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
  }
  .frame.empty p {
    margin: 0;
    font-size: var(--text-xs);
  }
  .preview :global(.fc-btn) {
    margin-inline-start: calc(-1 * var(--space-2));
  }
</style>
