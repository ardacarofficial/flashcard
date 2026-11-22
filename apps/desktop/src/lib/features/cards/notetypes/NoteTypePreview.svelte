<!--
  Live preview of a note type being edited (v1.3): which card (when the type makes several),
  the side shown (front / back) and a flip button. The cards come from `cards.preview` with
  sample values; this only shows them.
-->
<script lang="ts">
  import type { RenderedSides } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Button, Repeat, SegmentedControl, Skeleton } from '$lib/ui';
  import CardView from '../CardView.svelte';

  interface Props {
    cards: RenderedSides[];
    /** Why there is no preview (the definition does not render), if known. */
    error?: string | null;
    /** The first render has not arrived yet. */
    loading?: boolean;
  }
  let { cards, error = null, loading = false }: Props = $props();

  let index = $state(0);
  let back = $state(false);
  const current = $derived(cards[Math.min(index, cards.length - 1)]);

  $effect(() => {
    if (index >= cards.length) index = 0;
  });
</script>

<div class="nt-preview" data-testid="notetype-preview">
  {#if cards.length > 1}
    <SegmentedControl
      size="sm"
      label={t('notetype.preview_which')}
      options={cards.map((c, i) => ({ value: i, label: c.template_name }))}
      value={Math.min(index, cards.length - 1)}
      onchange={(i) => ((index = i), (back = false))}
    />
  {/if}
  {#if loading}
    <Skeleton shape="card" height="120px" />
  {:else if error}
    <p class="problem" role="status">{error}</p>
  {:else if current}
    <p class="side" aria-live="polite">{back ? t('notetype.side_back') : t('notetype.side_front')}</p>
    <div class="frame" class:answer={back}>
      <CardView sides={current} showAnswer={back} interactive={false} />
    </div>
    <Button size="sm" icon={Repeat} aria-pressed={back} onclick={() => (back = !back)}>
      {back ? t('notetype.flip_to_front') : t('notetype.flip_to_back')}
    </Button>
  {:else}
    <p class="problem">{t('cards.no_cards')}</p>
  {/if}
</div>

<style>
  .nt-preview {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    min-width: 0;
  }
  .side {
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
    min-height: 180px;
    padding: var(--space-5) var(--space-4);
    border-radius: var(--radius-flashcard);
    font-size: var(--text-xl);
    line-height: var(--leading-tight);
    text-align: center;
  }
  .frame.answer :global(.card) {
    font-size: var(--text-lg);
  }
  .problem {
    box-sizing: border-box;
    width: 100%;
    min-height: 120px;
    margin: 0;
    padding: var(--space-5);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-flashcard);
    color: var(--text-2);
    font-size: var(--text-sm);
    line-height: var(--leading);
  }
</style>
