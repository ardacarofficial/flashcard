<!--
  Card info of the Search screen (`browse.card_info`) as a side panel next to the table (v1.2:
  not a modal sheet, so the table stays usable and the panel follows the active row): facts in
  dense label/value rows with mono numbers, the review history, and "Edit note" (Enter).
-->
<script lang="ts">
  import { formatDate, formatDecimal, formatPercent } from '$lib/format';
  import type { CardInfo } from '$lib/api/types';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Chip, IconButton, Pencil, X } from '$lib/ui';
  import { stateLabel, stateTone } from './columns';

  interface Props {
    info: CardInfo | null;
    onclose: () => void;
    onedit: (info: CardInfo) => void;
  }
  let { info, onclose, onedit }: Props = $props();

  const date = (ms: number | null) => (ms === null ? t('common.none') : formatDate(ms));
  const num = (v: number | null | undefined) => (v === null || v === undefined ? t('common.none') : formatDecimal(v, 1));

  const facts = $derived(
    info
      ? [
          { label: t('browse.info.due'), value: date(info.card.due) },
          { label: t('browse.info.last_review'), value: date(info.last_review) },
          { label: t('browse.info.added'), value: date(info.card.created_at) },
          { label: t('browse.info.retrievability'), value: info.retrievability === null ? t('common.none') : formatPercent(info.retrievability) },
          { label: t('browse.info.stability'), value: num(info.card.stability) },
          { label: t('browse.info.difficulty'), value: num(info.card.difficulty) },
          { label: t('browse.col.reps'), value: String(info.card.reps) },
          { label: t('browse.col.lapses'), value: String(info.card.lapses) },
        ]
      : [],
  );
</script>

{#if info}
  <aside class="info-panel" aria-label={t('browse.info.heading')} data-testid="card-info">
    <header>
      <div class="titles">
        <span class="eyebrow">{t('browse.info.heading')}</span>
        <h2 title={info.card.sort_field}>{info.card.sort_field || t('common.none')}</h2>
      </div>
      <IconButton size="sm" icon={X} label={t('browse.info.close')} onclick={onclose} />
    </header>

    <div class="chips">
      <Chip size="sm" tone={stateTone(info.card)} label={stateLabel(info.card)} />
      <Chip size="sm" label={info.card.deck_path} />
      {#each info.card.tags as tag (tag)}<Chip size="sm" label={`#${tag}`} />{/each}
    </div>

    <Button size="sm" icon={Pencil} kbd="Enter" fullWidth onclick={() => info && onedit(info)}>{t('browse.info.edit')}</Button>

    <dl class="facts">
      {#each facts as f (f.label)}
        <div><dt>{f.label}</dt><dd class="num">{f.value}</dd></div>
      {/each}
    </dl>

    <section>
      <h3>{t('browse.info.reviews')}</h3>
      {#if info.reviews.length === 0}
        <p class="muted">{t('browse.info.no_reviews')}</p>
      {:else}
        <ol class="reviews">
          {#each info.reviews as rv, i (i)}
            <li>
              <span class="num">{date(rv.at)}</span>
              <!-- An Anki "set due date" / "forget" brought in by import: not an answer. -->
              {#if rv.mode === 'manual'}
                <span class="rating r-manual">{t('browse.info.rating.manual')}</span>
              {:else}
                <span class="rating r-{rv.rating}">{td(`browse.info.rating.${rv.rating}`)}</span>
              {/if}
              <span class="num muted">{t('browse.info.interval', { days: rv.scheduled_days })}</span>
            </li>
          {/each}
        </ol>
      {/if}
    </section>
  </aside>
{/if}

<style>
  .info-panel {
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    max-height: calc(100vh - 224px);
    overflow: auto;
    padding: var(--space-3) var(--space-4) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .eyebrow {
    display: block;
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
  h2 {
    margin: 2px 0 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .facts {
    margin: 0;
    border-top: 1px solid var(--border);
  }
  .facts div {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 28px;
    border-bottom: 1px solid var(--border);
  }
  dt {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  dd {
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-1);
  }
  .num {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  h3 {
    margin: 0 0 var(--space-1);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  .reviews {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .reviews li {
    display: grid;
    grid-template-columns: 1fr auto 3.5rem;
    gap: var(--space-3);
    align-items: center;
    min-height: 26px;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
  }
  .reviews li span:last-child {
    text-align: end;
  }
  .rating {
    font-weight: var(--weight-medium);
  }
  .r-again {
    color: var(--danger);
  }
  .r-good,
  .r-easy {
    color: var(--success);
  }
  .muted {
    color: var(--text-2);
    margin: 0;
    font-size: var(--text-xs);
  }
</style>
