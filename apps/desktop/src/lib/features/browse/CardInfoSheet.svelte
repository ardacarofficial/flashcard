<!-- Card info side sheet of the Search screen (`browse.card_info`): facts, review history, edit. -->
<script lang="ts">
  import type { CardInfo } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { Button, Chip, Dialog, Pencil } from '$lib/ui';
  import { stateLabel, stateTone } from './columns';

  interface Props {
    info: CardInfo | null;
    onclose: () => void;
    onedit: (info: CardInfo) => void;
  }
  let { info, onclose, onedit }: Props = $props();

  let open = $state(false);
  $effect(() => {
    open = info !== null;
  });

  const date = (ms: number | null) => (ms === null ? t('common.none') : new Date(ms).toLocaleDateString(i18n.locale));
  const num = (v: number | null | undefined) => (v === null || v === undefined ? t('common.none') : v.toFixed(1));
</script>

<Dialog bind:open variant="sheet" title={t('browse.info.heading')} description={info?.card.sort_field ?? ''} {onclose}>
  {#if info}
    <div class="info">
      <div class="chips">
        <Chip size="sm" tone={stateTone(info.card)} label={stateLabel(info.card)} />
        <Chip size="sm" label={info.card.deck_path} />
        {#each info.card.tags as tag (tag)}<Chip size="sm" label={`#${tag}`} />{/each}
      </div>

      <dl class="facts">
        <div><dt>{t('browse.info.added')}</dt><dd>{date(info.card.created_at)}</dd></div>
        <div><dt>{t('browse.info.due')}</dt><dd>{date(info.card.due)}</dd></div>
        <div><dt>{t('browse.info.stability')}</dt><dd>{num(info.card.stability)}</dd></div>
        <div><dt>{t('browse.info.difficulty')}</dt><dd>{num(info.card.difficulty)}</dd></div>
        <div>
          <dt>{t('browse.info.retrievability')}</dt>
          <dd>{info.retrievability === null ? t('common.none') : `${Math.round(info.retrievability * 100)}%`}</dd>
        </div>
        <div><dt>{t('browse.col.reps')}</dt><dd>{info.card.reps}</dd></div>
        <div><dt>{t('browse.col.lapses')}</dt><dd>{info.card.lapses}</dd></div>
        <div><dt>{t('browse.info.last_review')}</dt><dd>{date(info.last_review)}</dd></div>
      </dl>

      <section>
        <h3>{t('browse.info.reviews')}</h3>
        {#if info.reviews.length === 0}
          <p class="muted">{t('browse.info.no_reviews')}</p>
        {:else}
          <ol class="reviews">
            {#each info.reviews as rv, i (i)}
              <li>
                <span>{date(rv.at)}</span>
                <span class="rating">{td(`browse.info.rating.${rv.rating}`)}</span>
                <span class="muted">{t('browse.info.interval', { days: rv.scheduled_days })}</span>
              </li>
            {/each}
          </ol>
        {/if}
      </section>

      <Button icon={Pencil} onclick={() => info && onedit(info)}>{t('browse.info.edit')}</Button>
    </div>
  {/if}
</Dialog>

<style>
  .info {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
    align-items: flex-start;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3) var(--space-6);
    margin: 0;
    width: 100%;
  }
  dt {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  dd {
    margin: 0;
    font-weight: var(--weight-semibold);
    font-variant-numeric: tabular-nums;
  }
  section {
    width: 100%;
  }
  h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .reviews {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .reviews li {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .rating {
    font-weight: var(--weight-medium);
  }
  .muted {
    color: var(--text-2);
    margin: 0;
  }
</style>
